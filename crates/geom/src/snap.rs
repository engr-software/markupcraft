//! Snap to Content: a spatial index of a page's vector linework (PDF user space) and the query
//! that finds the snap point near the cursor: endpoints, intersections, midpoints and the
//! nearest point on a line, in that order of preference.
//!
//! Pure geometry: the renderer feeds it the page's flattened path segments once per page.

use crate::Point;

/// At most this many segments are indexed (the rest of a pathological page is ignored).
pub const MAX_SEGMENTS: usize = 2_000_000;
/// At most this many grid cells (about one segment per cell below that).
const MAX_CELLS: f64 = (1u32 << 20) as f64;
/// At most this many grid cells after coarsening.
const MAX_GRID: usize = 1 << 21;
/// At most this many (cell, segment) entries.
const MAX_ITEMS: usize = 1 << 25;
/// Intersections are tested among at most this many of the closest segments (dense hatching).
const MAX_PAIRS: usize = 96;

/// A straight piece of linework. A piece of a flattened curve has only the curve's real ends
/// (and no midpoint) as snap points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapSegment {
    pub a: Point,
    pub b: Point,
    /// [`SnapSegment::END_A`] | [`SnapSegment::END_B`] | [`SnapSegment::MID`]
    pub flags: u8,
}

impl SnapSegment {
    pub const END_A: u8 = 1;
    pub const END_B: u8 = 2;
    pub const MID: u8 = 4;
    pub const ALL: u8 = 7;

    pub const fn new(a: Point, b: Point) -> Self {
        Self { a, b, flags: Self::ALL }
    }
    pub const fn with_flags(a: Point, b: Point, flags: u8) -> Self {
        Self { a, b, flags }
    }
    fn finite(&self) -> bool {
        self.a.x.is_finite() && self.a.y.is_finite() && self.b.x.is_finite() && self.b.y.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SnapKind {
    Endpoint,
    Intersection,
    Midpoint,
    Nearest,
    /// a markup's centre (Snap to Markup)
    Center,
}

impl SnapKind {
    pub fn name(self) -> &'static str {
        match self {
            SnapKind::Endpoint => "Endpoint",
            SnapKind::Intersection => "Intersection",
            SnapKind::Midpoint => "Midpoint",
            SnapKind::Nearest => "Nearest",
            SnapKind::Center => "Center",
        }
    }
    /// This kind's bit in a kinds mask.
    pub const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// Every kind may snap.
pub const ALL_SNAP_KINDS: u32 = 0x1f;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapHit {
    pub pt: Point,
    pub kind: SnapKind,
    /// from the query point, PDF units
    pub dist: f64,
}

/// A uniform grid over the segments' bounding box (CSR layout: cell `c` holds
/// `items[start[c] .. start[c + 1]]`).
#[derive(Debug, Clone, Default)]
pub struct SnapIndex {
    segs: Vec<SnapSegment>,
    min_x: f64,
    min_y: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    start: Vec<u32>,
    items: Vec<u32>,
}

fn closest_on(p: Point, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Point::new(a.x + t * dx, a.y + t * dy)
}

/// Proper or touching intersection of two segments (not parallel).
pub fn segment_intersection(s: (Point, Point), t: (Point, Point)) -> Option<Point> {
    let (rx, ry) = (s.1.x - s.0.x, s.1.y - s.0.y);
    let (qx, qy) = (t.1.x - t.0.x, t.1.y - t.0.y);
    let den = rx * qy - ry * qx;
    let (lr, lq) = (rx.hypot(ry), qx.hypot(qy));
    if lr == 0.0 || lq == 0.0 || den.abs() < 1e-9 * lr * lq {
        return None;
    }
    let (ux, uy) = (t.0.x - s.0.x, t.0.y - s.0.y);
    let u = (ux * qy - uy * qx) / den;
    let v = (ux * ry - uy * rx) / den;
    let eps = 1e-9;
    if u < -eps || u > 1.0 + eps || v < -eps || v > 1.0 + eps {
        return None;
    }
    Some(Point::new(s.0.x + u * rx, s.0.y + u * ry))
}

impl SnapIndex {
    pub fn new(segs: Vec<SnapSegment>) -> Self {
        let mut s = Self::default();
        s.build(segs);
        s
    }

    pub fn len(&self) -> usize {
        self.segs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }
    pub fn segments(&self) -> &[SnapSegment] {
        &self.segs
    }

    fn cell_x(&self, x: f64) -> usize {
        let c = ((x - self.min_x) / self.cell).floor();
        if c.is_nan() || c < 0.0 {
            0
        } else {
            (c as usize).min(self.nx.saturating_sub(1))
        }
    }
    fn cell_y(&self, y: f64) -> usize {
        let c = ((y - self.min_y) / self.cell).floor();
        if c.is_nan() || c < 0.0 {
            0
        } else {
            (c as usize).min(self.ny.saturating_sub(1))
        }
    }

    /// The cells `s` actually crosses, row by row (long diagonals do not fill their box).
    fn for_each_cell(&self, s: &SnapSegment, mut f: impl FnMut(usize)) {
        let (sy0, sy1) = (s.a.y.min(s.b.y), s.a.y.max(s.b.y));
        let (r0, r1) = (self.cell_y(sy0), self.cell_y(sy1));
        let dy = s.b.y - s.a.y;
        for r in r0..=r1 {
            let ya = sy0.max(self.min_y + r as f64 * self.cell);
            let yb = sy1.min(self.min_y + (r + 1) as f64 * self.cell);
            let (xa, xb) = if dy.abs() < 1e-12 {
                (s.a.x, s.b.x)
            } else {
                let ta = ((ya - s.a.y) / dy).clamp(0.0, 1.0);
                let tb = ((yb - s.a.y) / dy).clamp(0.0, 1.0);
                (s.a.x + ta * (s.b.x - s.a.x), s.a.x + tb * (s.b.x - s.a.x))
            };
            let (c0, c1) = (self.cell_x(xa.min(xb)), self.cell_x(xa.max(xb)));
            for c in c0..=c1 {
                f(r * self.nx + c);
            }
        }
    }

    /// Index `segs` (non-finite ones dropped, at most [`MAX_SEGMENTS`]).
    pub fn build(&mut self, segs: Vec<SnapSegment>) {
        *self = Self::default();
        self.segs = segs
            .into_iter()
            .filter(SnapSegment::finite)
            .take(MAX_SEGMENTS)
            .collect();
        if self.segs.is_empty() {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for s in &self.segs {
            x0 = x0.min(s.a.x).min(s.b.x);
            y0 = y0.min(s.a.y).min(s.b.y);
            x1 = x1.max(s.a.x).max(s.b.x);
            y1 = y1.max(s.a.y).max(s.b.y);
        }
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let cells = (self.segs.len() as f64).clamp(1.0, MAX_CELLS);
        self.cell = (w * h / cells).sqrt().max(1e-3);
        self.min_x = x0;
        self.min_y = y0;
        // Coarsen until the grid and the cell lists are bounded (long diagonals on a fine grid
        // would otherwise list one segment in thousands of cells).
        let mut counts;
        loop {
            self.nx = ((w / self.cell).ceil() as usize).max(1);
            self.ny = ((h / self.cell).ceil() as usize).max(1);
            if self.nx.saturating_mul(self.ny) > MAX_GRID {
                self.cell *= 2.0;
                continue;
            }
            counts = vec![0u32; self.nx * self.ny + 1];
            let mut total = 0usize;
            for s in &self.segs {
                self.for_each_cell(s, |c| {
                    total += 1;
                    if let Some(v) = counts.get_mut(c + 1) {
                        *v = v.saturating_add(1);
                    }
                });
            }
            if total <= MAX_ITEMS || (self.nx == 1 && self.ny == 1) {
                break;
            }
            self.cell *= 2.0;
        }
        for c in 1..counts.len() {
            let prev = counts.get(c - 1).copied().unwrap_or(0);
            if let Some(v) = counts.get_mut(c) {
                *v = v.saturating_add(prev);
            }
        }
        self.start = counts.clone();
        let mut items = vec![0u32; counts.last().copied().unwrap_or(0) as usize];
        for (i, s) in self.segs.iter().enumerate() {
            self.for_each_cell(s, |c| {
                if let Some(slot) = counts.get_mut(c) {
                    if let Some(it) = items.get_mut(*slot as usize) {
                        *it = i as u32;
                    }
                    *slot += 1;
                }
            });
        }
        self.items = items;
    }

    /// Segment indices whose cells touch the box (may include some outside it), sorted, unique.
    pub fn candidates(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<u32> {
        let mut out = Vec::new();
        if self.segs.is_empty() {
            return out;
        }
        let (gx1, gy1) = (
            self.min_x + self.nx as f64 * self.cell,
            self.min_y + self.ny as f64 * self.cell,
        );
        if x1 < self.min_x || y1 < self.min_y || x0 > gx1 || y0 > gy1 {
            return out;
        }
        let (c0, c1, r0, r1) = (self.cell_x(x0), self.cell_x(x1), self.cell_y(y0), self.cell_y(y1));
        for r in r0..=r1 {
            for c in c0..=c1 {
                let k = r * self.nx + c;
                let (Some(&a), Some(&b)) = (self.start.get(k), self.start.get(k + 1)) else {
                    continue;
                };
                if let Some(s) = self.items.get(a as usize..b as usize) {
                    out.extend_from_slice(s);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The snap point within `radius` of `p`. Points (endpoints, intersections, midpoints) win
    /// over the nearest point on a line when both are within the radius.
    pub fn query(&self, p: Point, radius: f64) -> Option<SnapHit> {
        self.query_kinds(p, radius, ALL_SNAP_KINDS)
    }

    /// The same, offering only the kinds in `kinds` (a [`SnapKind::bit`] mask).
    pub fn query_kinds(&self, p: Point, radius: f64, kinds: u32) -> Option<SnapHit> {
        if self.segs.is_empty() || radius.is_nan() || radius <= 0.0 {
            return None;
        }
        let mut near: Vec<(&SnapSegment, f64, Point)> = self
            .candidates(p.x - radius, p.y - radius, p.x + radius, p.y + radius)
            .into_iter()
            .filter_map(|i| self.segs.get(i as usize))
            .filter_map(|s| {
                let q = closest_on(p, s.a, s.b);
                let d = p.dist(q);
                (d <= radius).then_some((s, d, q))
            })
            .collect();
        if near.is_empty() {
            return None;
        }
        near.sort_by(|a, b| a.1.total_cmp(&b.1));

        let offer = |best: &mut Option<SnapHit>, q: Point, k: SnapKind| {
            if kinds & k.bit() == 0 {
                return;
            }
            let d = p.dist(q);
            if d > radius {
                return;
            }
            // Endpoints beat intersections at (nearly) the same spot.
            let better = match best {
                None => true,
                Some(b) => d < b.dist - 1e-9 || ((d - b.dist).abs() <= 1e-9 && k < b.kind),
            };
            if better {
                *best = Some(SnapHit {
                    pt: q,
                    kind: k,
                    dist: d,
                });
            }
        };

        let mut point = None;
        for (s, _, _) in &near {
            if s.flags & SnapSegment::END_A != 0 {
                offer(&mut point, s.a, SnapKind::Endpoint);
            }
            if s.flags & SnapSegment::END_B != 0 {
                offer(&mut point, s.b, SnapKind::Endpoint);
            }
        }
        let m = near.len().min(MAX_PAIRS);
        for i in 0..m {
            for j in i + 1..m {
                let (Some((s, _, _)), Some((t, _, _))) = (near.get(i), near.get(j)) else {
                    continue;
                };
                if let Some(x) = segment_intersection((s.a, s.b), (t.a, t.b)) {
                    // A shared vertex of two segments of one path is an endpoint, not a crossing.
                    let at_end = [s, t].iter().any(|g| x.dist(g.a) < 1e-6 || x.dist(g.b) < 1e-6);
                    if !at_end {
                        offer(&mut point, x, SnapKind::Intersection);
                    }
                }
            }
        }
        if point.is_some() {
            return point;
        }

        let mut mid = None;
        for (s, _, _) in &near {
            if s.flags & SnapSegment::MID != 0 {
                offer(&mut mid, s.a.mid(s.b), SnapKind::Midpoint);
            }
        }
        if mid.is_some() {
            return mid;
        }
        if kinds & SnapKind::Nearest.bit() == 0 {
            return None;
        }
        near.first().map(|(_, d, q)| SnapHit {
            pt: *q,
            kind: SnapKind::Nearest,
            dist: *d,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn kind(h: Option<SnapHit>) -> &'static str {
        h.map_or("none", |h| h.kind.name())
    }

    #[test]
    fn snap_point_types() {
        let idx = SnapIndex::new(vec![
            SnapSegment::new(p(0.0, 0.0), p(100.0, 0.0)),
            SnapSegment::new(p(50.0, -50.0), p(50.0, 50.0)),
            SnapSegment::new(p(200.0, 0.0), p(300.0, 0.0)),
            SnapSegment::with_flags(p(400.0, 0.0), p(410.0, 5.0), SnapSegment::END_A),
            SnapSegment::with_flags(p(410.0, 5.0), p(420.0, 0.0), SnapSegment::END_B),
        ]);
        let hit = idx.query(p(50.5, 0.5), 3.0);
        assert_eq!(kind(hit), "Intersection");
        assert!((hit.map_or(0.0, |h| h.pt.x) - 50.0).abs() < 1e-9);
        let hit = idx.query(p(99.0, 1.0), 3.0);
        assert_eq!(kind(hit), "Endpoint");
        assert!((hit.map_or(0.0, |h| h.pt.x) - 100.0).abs() < 1e-9);
        assert_eq!(kind(idx.query(p(251.0, 1.0), 3.0)), "Midpoint");
        let hit = idx.query(p(220.0, 1.0), 3.0);
        assert_eq!(kind(hit), "Nearest");
        assert!(hit.map_or(1.0, |h| h.pt.y).abs() < 1e-9);
        assert_eq!(
            kind(idx.query(p(410.0, 6.0), 3.0)),
            "Nearest",
            "curve joint is not an endpoint"
        );
        assert_eq!(kind(idx.query(p(150.0, 30.0), 3.0)), "none");
        assert_eq!(kind(idx.query(p(50.0, 0.0), 0.0)), "none");
    }

    #[test]
    fn snap_kind_mask() {
        let idx = SnapIndex::new(vec![SnapSegment::new(p(0.0, 0.0), p(100.0, 0.0))]);
        assert_eq!(kind(idx.query(p(50.0, 2.0), 5.0)), "Midpoint");
        let no_mid = ALL_SNAP_KINDS & !SnapKind::Midpoint.bit();
        assert_eq!(kind(idx.query_kinds(p(50.0, 2.0), 5.0, no_mid)), "Nearest");
        assert_eq!(
            kind(idx.query_kinds(p(50.0, 2.0), 5.0, SnapKind::Endpoint.bit())),
            "none"
        );
        assert_eq!(
            kind(idx.query_kinds(p(1.0, 1.0), 5.0, SnapKind::Endpoint.bit())),
            "Endpoint"
        );
    }

    #[test]
    fn grid_agrees_with_brute_force() {
        let mut seed: u32 = 12345;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            f64::from((seed >> 8) & 0xFFFF) / 65536.0 * 1000.0
        };
        let mut many = Vec::new();
        for _ in 0..5000 {
            let a = p(rnd(), rnd());
            let b = p(a.x + rnd() / 50.0 - 10.0, a.y + rnd() / 50.0 - 10.0);
            many.push(SnapSegment::new(a, b));
        }
        let big = SnapIndex::new(many.clone());
        assert_eq!(big.len(), 5000);
        let mut mismatches = 0;
        for _ in 0..300 {
            let q = p(rnd(), rnd());
            let h = big.query(q, 4.0);
            let best = many
                .iter()
                .map(|s| q.dist(closest_on(q, s.a, s.b)))
                .fold(f64::MAX, f64::min);
            if (best <= 4.0) != h.is_some() {
                mismatches += 1;
            }
        }
        assert_eq!(mismatches, 0);
    }

    #[test]
    fn hostile_input() {
        let idx = SnapIndex::new(vec![
            SnapSegment::new(p(f64::NAN, 0.0), p(1.0, 1.0)),
            SnapSegment::new(p(0.0, 0.0), p(f64::INFINITY, 0.0)),
        ]);
        assert!(idx.is_empty());
        assert!(idx.query(p(0.0, 0.0), 5.0).is_none());
        // A huge sliver and a single point both index.
        let idx = SnapIndex::new(vec![
            SnapSegment::new(p(-1e12, 0.0), p(1e12, 0.0)),
            SnapSegment::new(p(5.0, 5.0), p(5.0, 5.0)),
        ]);
        assert_eq!(kind(idx.query(p(0.0, 1.0), 2.0)), "Midpoint");
        assert!(idx.query(p(f64::NAN, 0.0), 2.0).is_none());
        // Long diagonals coarsen the grid instead of listing each segment in every cell.
        let diag: Vec<SnapSegment> = (0..2000)
            .map(|i| SnapSegment::new(p(f64::from(i), 0.0), p(f64::from(i) + 1e6, 1e6)))
            .chain(
                (0..2000).map(|i| SnapSegment::new(p(f64::from(i) * 500.0, 5.0), p(f64::from(i) * 500.0 + 1.0, 5.0))),
            )
            .collect();
        let idx = SnapIndex::new(diag);
        assert!(idx.items.len() <= MAX_ITEMS);
        assert!(idx.query(p(1e6 + 10.0, 1e6), 1.0).is_some());
    }
}
