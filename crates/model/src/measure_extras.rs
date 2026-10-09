//! Measurement polish: arcs in a vertex shape, adding / deleting vertices, segment values,
//! custom count symbols, totals of a selection, counts per page, re-applying page scales,
//! cutouts (deductions), and the quantities of the Volume, Diameter, Radius and Angle tools,
//! plus slope and wall area.
//!
//! Arcs (Revu "Convert to Arc") are kept as a polyline approximation in [`Markup::pts`] plus
//! [`Markup::arcs`] = (start vertex, interior point count) per arc, so quantities, hit tests,
//! copy / paste, page operations and other PDF readers all see the curve without knowing about
//! arcs. The arc's middle interior point is its handle: dragging it bends the arc (the handle
//! stays on the chord's perpendicular bisector, so it remains the arc's midpoint).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use markupcraft_geom::{Point, bbox, dist_to_segment, point_in_polygon, polygon_area, polyline_length};
use markupcraft_measure::units::{self, LengthUnit};
use markupcraft_measure::{Fmt, NumberFormat, Scale, format_value};

use crate::{CountSymbol, Document, Kind, Markup};

// ---- arcs ---------------------------------------------------------------------------------

/// Interior points per arc: 32 chords, length error < 0.05%.
pub const ARC_INTERIOR: usize = 31;

/// Polylength, Perimeter, Area, Volume, Polyline, Polygon.
pub fn can_have_arcs(k: Kind) -> bool {
    matches!(
        k,
        Kind::Polylength | Kind::Perimeter | Kind::Area | Kind::Volume | Kind::Polyline | Kind::Polygon
    )
}

/// The last vertex joins the first.
pub fn closed_shape(k: Kind) -> bool {
    matches!(
        k,
        Kind::Area | Kind::Perimeter | Kind::Volume | Kind::Polygon | Kind::Cloud
    )
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

fn pt(m: &Markup, i: usize) -> Option<Point> {
    m.pts.get(i).copied()
}

/// Index in `pts` of arc `arc`'s end vertex (`pts[0]` for a closing arc).
pub fn arc_end(m: &Markup, arc: usize) -> Option<usize> {
    let (first, n) = *m.arcs.get(arc)?;
    let len = m.pts.len();
    if len == 0 {
        return None;
    }
    Some((first + n + 1) % len)
}

/// The arc whose INTERIOR holds `pts[i]`.
pub fn arc_containing(m: &Markup, i: usize) -> Option<usize> {
    m.arcs.iter().position(|&(first, n)| i > first && i <= first + n)
}

/// Index in `pts` of arc `arc`'s handle (its middle point).
pub fn arc_handle(m: &Markup, arc: usize) -> Option<usize> {
    let (first, n) = *m.arcs.get(arc)?;
    Some(first + n.div_ceil(2))
}

/// A vertex that is drawn and grabbed: control vertices and arc handles (not other arc points).
pub fn is_editable_vertex(m: &Markup, i: usize) -> bool {
    match arc_containing(m, i) {
        None => true,
        Some(a) => arc_handle(m, a) == Some(i),
    }
}

/// The circle through `a`, `t`, `b`: (centre, radius), `None` when collinear.
pub fn circle_through(a: Point, t: Point, b: Point) -> Option<(Point, f64)> {
    let (tx, ty, bx, by) = (t.x - a.x, t.y - a.y, b.x - a.x, b.y - a.y);
    let chord2 = bx * bx + by * by;
    let cross = tx * by - ty * bx;
    if chord2 <= 0.0 || cross.abs() < 1e-9 * chord2 || !cross.is_finite() {
        return None;
    }
    let d = 2.0 * cross;
    let t2 = tx * tx + ty * ty;
    let (ux, uy) = ((by * t2 - ty * chord2) / d, (tx * chord2 - bx * t2) / d);
    Some((Point::new(a.x + ux, a.y + uy), ux.hypot(uy)))
}

/// Start angle and signed sweep (radians) of the arc from `a` through `t` to `b` about `c`.
pub fn arc_sweep(c: Point, a: Point, t: Point, b: Point) -> (f64, f64) {
    let ang = |p: Point| (p.y - c.y).atan2(p.x - c.x);
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(2.0 * PI);
    let (aa, at, ab) = (ang(a), ang(t), ang(b));
    let (sweep_b, sweep_t) = (ccw(aa, ab), ccw(aa, at));
    let sweep = if sweep_t < sweep_b {
        sweep_b
    } else {
        -(2.0 * PI - sweep_b)
    };
    (aa, sweep)
}

/// Points on the arc from `a` through `t` to `b`, interior only (`n` points). Collinear = straight.
pub fn arc_through(a: Point, t: Point, b: Point, n: usize) -> Vec<Point> {
    let n = n.min(10_000);
    let Some((c, r)) = circle_through(a, t, b) else {
        return (1..=n)
            .map(|k| {
                let f = k as f64 / (n + 1) as f64;
                Point::new(a.x + f * (b.x - a.x), a.y + f * (b.y - a.y))
            })
            .collect();
    };
    let (start, sweep) = arc_sweep(c, a, t, b);
    (1..=n)
        .map(|k| {
            let ang = start + sweep * k as f64 / (n + 1) as f64;
            Point::new(c.x + r * ang.cos(), c.y + r * ang.sin())
        })
        .collect()
}

fn sort_arcs(m: &mut Markup) {
    m.arcs.sort_unstable();
}

/// Turn the segment that starts at control vertex `start` into an arc bulging by `sagitta`
/// (PDF units, + = to the left of a->b; 0 = a quarter of the chord, outward for closed shapes).
/// Returns the arc index; `None` when not a control vertex, already an arc, or no next vertex.
pub fn convert_to_arc(m: &mut Markup, start: usize, sagitta: f64) -> Option<usize> {
    let n = m.pts.len();
    if !can_have_arcs(m.kind) || start >= n || arc_containing(m, start).is_some() {
        return None;
    }
    if m.arcs.iter().any(|a| a.0 == start) {
        return None;
    }
    let closed = closed_shape(m.kind);
    let next = if start + 1 < n {
        start + 1
    } else if closed && n >= 3 {
        0
    } else {
        return None;
    };
    let (a, b) = (pt(m, start)?, pt(m, next)?);
    let l = a.dist(b);
    if l.is_nan() || l <= 0.0 {
        return None;
    }
    let mid = a.mid(b);
    let nrm = Point::new(-(b.y - a.y) / l, (b.x - a.x) / l);
    let mut sagitta = sagitta;
    if sagitta == 0.0 {
        sagitta = l / 4.0;
        if closed {
            // bulge outward: away from the shape's vertex mean
            let c = markupcraft_geom::vertex_mean(&m.pts);
            if (c.x - mid.x) * nrm.x + (c.y - mid.y) * nrm.y > 0.0 {
                sagitta = -sagitta;
            }
        }
    }
    let through = Point::new(mid.x + nrm.x * sagitta, mid.y + nrm.y * sagitta);
    let inner = arc_through(a, through, b, ARC_INTERIOR);
    for arc in &mut m.arcs {
        if arc.0 > start {
            arc.0 += ARC_INTERIOR;
        }
    }
    m.pts.splice(start + 1..start + 1, inner);
    m.arcs.push((start, ARC_INTERIOR));
    sort_arcs(m);
    m.dirty = true;
    m.arcs.iter().position(|a| a.0 == start)
}

/// Bend arc `arc` so its midpoint is `toward` projected onto the chord's perpendicular bisector.
pub fn bend_arc(m: &mut Markup, arc: usize, toward: Point) {
    let Some(&(first, n)) = m.arcs.get(arc) else { return };
    let (Some(a), Some(b)) = (pt(m, first), arc_end(m, arc).and_then(|e| pt(m, e))) else {
        return;
    };
    let l = a.dist(b);
    if l.is_nan() || l <= 0.0 {
        return;
    }
    let mid = a.mid(b);
    let nrm = Point::new(-(b.y - a.y) / l, (b.x - a.x) / l);
    let s = (toward.x - mid.x) * nrm.x + (toward.y - mid.y) * nrm.y;
    let inner = arc_through(a, Point::new(mid.x + nrm.x * s, mid.y + nrm.y * s), b, n);
    for (k, p) in inner.into_iter().enumerate() {
        if let Some(q) = m.pts.get_mut(first + 1 + k) {
            *q = p;
        }
    }
    m.dirty = true;
}

/// Back to a straight segment.
pub fn straighten_arc(m: &mut Markup, arc: usize) {
    let Some(&(first, n)) = m.arcs.get(arc) else { return };
    let end = (first + 1 + n).min(m.pts.len());
    let from = (first + 1).min(end);
    m.pts.drain(from..end);
    m.arcs.remove(arc);
    for a in &mut m.arcs {
        if a.0 > first {
            a.0 = a.0.saturating_sub(n);
        }
    }
    m.dirty = true;
}

/// Vertices that are not arc interior points.
pub fn control_count(m: &Markup) -> usize {
    let interior: usize = m.arcs.iter().map(|a| a.1).sum();
    m.pts.len().saturating_sub(interior)
}

fn min_controls(m: &Markup) -> usize {
    if closed_shape(m.kind) { 3 } else { 2 }
}

/// Drop arcs that do not fit the points (after a load or an edit by another program).
pub fn validate_arcs(m: &mut Markup) {
    if m.arcs.is_empty() {
        return;
    }
    sort_arcs(m);
    let n = m.pts.len();
    let mut ok = can_have_arcs(m.kind);
    let mut next_free = 0usize;
    let count = m.arcs.len();
    for (j, &(first, cnt)) in m.arcs.iter().enumerate() {
        if !ok {
            break;
        }
        // Interior points first+1 .. first+cnt must exist; the end vertex is the next point, or
        // pts[0] for the closing segment of a closed shape (then it is the last arc).
        let last = first.checked_add(cnt);
        match last {
            Some(last) if cnt >= 1 && first >= next_free && last < n => {
                if last + 1 == n && !(closed_shape(m.kind) && j + 1 == count) {
                    ok = false;
                }
                next_free = last + 1;
            }
            _ => ok = false,
        }
    }
    if ok && control_count(m) < min_controls(m) {
        ok = false;
    }
    // The points must still be the arc: another program may have moved or added vertices.
    for j in 0..m.arcs.len() {
        if !ok {
            break;
        }
        let Some(&(first, cnt)) = m.arcs.get(j) else { break };
        let (Some(a), Some(b), Some(h)) = (
            pt(m, first),
            arc_end(m, j).and_then(|e| pt(m, e)),
            arc_handle(m, j).and_then(|h| pt(m, h)),
        ) else {
            ok = false;
            break;
        };
        let want = arc_through(a, h, b, cnt);
        let tol = 0.5 + 0.01 * a.dist(b);
        ok = want
            .iter()
            .enumerate()
            .all(|(k, w)| pt(m, first + 1 + k).is_some_and(|q| w.dist(q) <= tol));
    }
    if !ok {
        m.arcs.clear();
    }
}

/// A control segment: its start, the arc (if any) and every `pts` index along it, start to end.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlSegment {
    pub start: usize,
    pub arc: Option<usize>,
    pub path: Vec<usize>,
}

/// The control segments of a vertex shape (an arc counts as one).
pub fn control_segments(m: &Markup) -> Vec<ControlSegment> {
    let n = m.pts.len();
    let closed = closed_shape(m.kind);
    let mut out = Vec::new();
    for i in 0..n {
        if arc_containing(m, i).is_some() {
            continue;
        }
        let arc = m.arcs.iter().rposition(|a| a.0 == i);
        let mut path = vec![i];
        if let Some(j) = arc {
            let cnt = m.arcs.get(j).map_or(0, |a| a.1);
            path.extend((1..=cnt).map(|k| i + k));
            match arc_end(m, j) {
                Some(e) => path.push(e),
                None => continue,
            }
        } else if i + 1 < n {
            path.push(i + 1);
        } else if closed && n >= 3 {
            path.push(0);
        } else {
            continue;
        }
        out.push(ControlSegment { start: i, arc, path });
    }
    out
}

/// The control segment under `p` (within `tol`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentHit {
    pub start: usize,
    pub arc: Option<usize>,
    /// nearest point on the segment
    pub foot: Point,
}

pub fn segment_at(m: &Markup, p: Point, tol: f64) -> Option<SegmentHit> {
    let mut best = None;
    let mut best_d = tol;
    for s in control_segments(m) {
        for w in s.path.windows(2) {
            let (Some(a), Some(b)) = (pt(m, w[0]), pt(m, w[1])) else {
                continue;
            };
            let q = closest_on(p, a, b);
            let d = p.dist(q);
            if d <= best_d {
                best_d = d;
                best = Some(SegmentHit {
                    start: s.start,
                    arc: s.arc,
                    foot: q,
                });
            }
        }
    }
    best
}

// ---- vertices -----------------------------------------------------------------------------

/// Kinds whose vertices can be added and deleted one at a time.
pub fn can_add_vertices(k: Kind) -> bool {
    matches!(
        k,
        Kind::Polylength | Kind::Perimeter | Kind::Area | Kind::Volume | Kind::Polyline | Kind::Polygon | Kind::Cloud
    )
}

/// Insert `p` after `pts[after]` (a straight segment's start). False inside an arc.
pub fn insert_vertex(m: &mut Markup, after: usize, p: Point) -> bool {
    let n = m.pts.len();
    if !can_add_vertices(m.kind) || after >= n || arc_containing(m, after).is_some() {
        return false;
    }
    if m.arcs.iter().any(|a| a.0 == after) {
        return false; // that segment is an arc
    }
    if after + 1 == n && !closed_shape(m.kind) {
        return false;
    }
    for a in &mut m.arcs {
        if a.0 > after {
            a.0 += 1;
        }
    }
    m.pts.insert(after + 1, p);
    m.dirty = true;
    true
}

/// Delete control vertex `idx` (arcs touching it are straightened first). Keeps at least 2
/// vertices (open) / 3 (closed).
pub fn erase_vertex(m: &mut Markup, idx: usize) -> bool {
    let mut idx = idx;
    if !can_add_vertices(m.kind) || idx >= m.pts.len() || arc_containing(m, idx).is_some() {
        return false;
    }
    if control_count(m) <= min_controls(m) {
        return false;
    }
    // Each pass removes one arc, so this ends after at most arcs.len() passes.
    while let Some(j) =
        (0..m.arcs.len()).find(|&j| m.arcs.get(j).is_some_and(|a| a.0 == idx) || arc_end(m, j) == Some(idx))
    {
        let Some(&(first, cnt)) = m.arcs.get(j) else { break };
        straighten_arc(m, j);
        if first < idx {
            idx = idx.saturating_sub(cnt);
        }
    }
    if idx >= m.pts.len() {
        return false;
    }
    m.pts.remove(idx);
    for a in &mut m.arcs {
        if a.0 > idx {
            a.0 -= 1;
        }
    }
    m.dirty = true;
    true
}

/// Insert a vertex into cutout `h` after `after`.
pub fn insert_hole_vertex(m: &mut Markup, h: usize, after: usize, p: Point) -> bool {
    let Some(ring) = m.holes.get_mut(h) else { return false };
    if after >= ring.len() {
        return false;
    }
    ring.insert(after + 1, p);
    m.dirty = true;
    true
}

/// Delete a vertex of cutout `h` (a cutout keeps 3).
pub fn erase_hole_vertex(m: &mut Markup, h: usize, idx: usize) -> bool {
    let Some(ring) = m.holes.get_mut(h) else { return false };
    if ring.len() <= 3 || idx >= ring.len() {
        return false;
    }
    ring.remove(idx);
    m.dirty = true;
    true
}

/// Nearest segment of a ring to `p` within `tol`: its start index and the foot.
pub fn nearest_segment(ring: &[Point], closed: bool, p: Point, tol: f64) -> Option<(usize, Point)> {
    let n = ring.len();
    let mut best = None;
    let mut best_d = tol;
    for i in 0..n {
        let j = if i + 1 == n {
            if !closed || n < 3 {
                break;
            }
            0
        } else {
            i + 1
        };
        let (Some(a), Some(b)) = (ring.get(i), ring.get(j)) else {
            continue;
        };
        let q = closest_on(p, *a, *b);
        let d = p.dist(q);
        if d <= best_d {
            best_d = d;
            best = Some((i, q));
        }
    }
    best
}

/// Cutout whose outline is within `tol` of `p`, or (with `inside_counts`) whose inside holds `p`.
pub fn hole_at(m: &Markup, p: Point, tol: f64, inside_counts: bool) -> Option<usize> {
    if let Some(h) = m.holes.iter().position(|r| nearest_segment(r, true, p, tol).is_some()) {
        return Some(h);
    }
    if inside_counts {
        return m.holes.iter().position(|r| r.len() >= 3 && point_in_polygon(p, r));
    }
    None
}

// ---- cutouts (deductions) -----------------------------------------------------------------

/// Kinds that take cutouts.
pub fn can_have_cutouts(k: Kind) -> bool {
    matches!(k, Kind::Area | Kind::Volume)
}

/// Add a cutout ring. False for kinds without cutouts, fewer than 3 points, a degenerate ring,
/// or a ring not inside the outline.
pub fn add_cutout(m: &mut Markup, ring: Vec<Point>) -> bool {
    if !can_have_cutouts(m.kind) || ring.len() < 3 || polygon_area(&ring) <= 0.0 {
        return false;
    }
    if !ring
        .iter()
        .all(|p| point_in_polygon(*p, &m.pts) || on_outline(&m.pts, *p))
    {
        return false;
    }
    m.holes.push(ring);
    m.dirty = true;
    true
}

fn on_outline(ring: &[Point], p: Point) -> bool {
    nearest_segment(ring, true, p, 1e-6).is_some()
}

/// Remove cutout `h`.
pub fn remove_cutout(m: &mut Markup, h: usize) -> bool {
    if h >= m.holes.len() {
        return false;
    }
    m.holes.remove(h);
    m.dirty = true;
    true
}

/// Each cutout's area in the first `/A` unit (empty without a scale).
pub fn cutout_areas(m: &Markup) -> Vec<f64> {
    match m.scale.as_ref().filter(|s| s.valid()) {
        Some(s) => m.holes.iter().map(|h| s.area_of(h)).collect(),
        None => Vec::new(),
    }
}

/// Area without cutouts minus the cutouts, never below zero (first `/A` unit).
pub fn net_area(m: &Markup) -> Option<f64> {
    let s = m.scale.as_ref().filter(|s| s.valid())?;
    let deductions: f64 = m.holes.iter().map(|h| s.area_of(h)).sum();
    Some((s.area_of(&m.pts) - deductions).max(0.0))
}

/// A cutout ring for an ellipse (implicitly closed, `n` points).
pub fn ellipse_ring(c: Point, rx: f64, ry: f64, n: usize) -> Vec<Point> {
    let mut r = ring(c, rx, ry, n);
    r.pop();
    r
}

// ---- segment values -----------------------------------------------------------------------

/// One segment's value (Show Segment Values).
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentValue {
    /// where the text goes (segment midpoint; an arc's handle)
    pub at: Point,
    /// the segment's direction, radians, PDF space (y up)
    pub angle: f64,
    /// length in the first `/D` unit
    pub value: f64,
    /// formatted like the caption
    pub text: String,
}

/// One value per control segment of a Polylength, Perimeter, Area or Volume; empty without a
/// scale.
pub fn segment_values(m: &Markup) -> Vec<SegmentValue> {
    let Some(s) = m.scale.as_ref().filter(|s| s.valid()) else {
        return Vec::new();
    };
    if !matches!(m.kind, Kind::Polylength | Kind::Perimeter | Kind::Area | Kind::Volume) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for seg in control_segments(m) {
        let path: Vec<Point> = seg.path.iter().filter_map(|&i| pt(m, i)).collect();
        let (Some(a), Some(b)) = (path.first().copied(), path.last().copied()) else {
            continue;
        };
        let value = s.length_of(&path, false);
        let at = seg
            .arc
            .and_then(|j| arc_handle(m, j))
            .and_then(|h| pt(m, h))
            .unwrap_or_else(|| a.mid(b));
        out.push(SegmentValue {
            at,
            angle: (b.y - a.y).atan2(b.x - a.x),
            value,
            text: format_value(value, &s.dist),
        });
    }
    out
}

// ---- custom count symbols -----------------------------------------------------------------

fn ring(c: Point, rx: f64, ry: f64, n: usize) -> Vec<Point> {
    let n = n.clamp(3, 4096);
    (0..=n)
        .map(|k| {
            let a = 2.0 * PI * k as f64 / n as f64;
            Point::new(c.x + rx * a.cos(), c.y + ry * a.sin())
        })
        .collect()
}

fn builtin_symbol(s: CountSymbol, c: Point, r: f64) -> Vec<Vec<Point>> {
    let p = Point::new;
    match s {
        CountSymbol::Square => vec![vec![
            p(c.x - r, c.y - r),
            p(c.x + r, c.y - r),
            p(c.x + r, c.y + r),
            p(c.x - r, c.y + r),
            p(c.x - r, c.y - r),
        ]],
        CountSymbol::Check => vec![vec![
            p(c.x - r, c.y),
            p(c.x - r / 3.0, c.y - r * 0.7),
            p(c.x + r, c.y + r * 0.8),
        ]],
        CountSymbol::Cross => vec![
            vec![p(c.x - r, c.y - r), p(c.x + r, c.y + r)],
            vec![p(c.x - r, c.y + r), p(c.x + r, c.y - r)],
        ],
        CountSymbol::Circle | CountSymbol::Custom => vec![ring(c, r, r, 24)],
    }
}

/// A count markup's symbol paths at point `c` (scaled by its symbol scale).
pub fn symbol_paths_at(m: &Markup, c: Point) -> Vec<Vec<Point>> {
    let s = if m.symbol_scale > 0.0 { m.symbol_scale } else { 1.0 };
    if m.count_symbol == CountSymbol::Custom && !m.symbol_paths.is_empty() {
        return m
            .symbol_paths
            .iter()
            .map(|path| path.iter().map(|p| Point::new(c.x + p.x * s, c.y + p.y * s)).collect())
            .collect();
    }
    builtin_symbol(m.count_symbol, c, 6.0 * s)
}

/// The outline of any markup as count-symbol paths: centred on (0, 0) and scaled so the larger
/// half-extent is 6 pt (the built-in symbols' radius). Closed outlines repeat their first point.
pub fn symbol_from_markup(m: &Markup) -> Vec<Vec<Point>> {
    if m.kind == Kind::Count {
        let mut one = m.clone();
        one.symbol_scale = 1.0;
        return symbol_paths_at(&one, Point::default());
    }
    let Some(b) = bbox(&m.pts) else { return Vec::new() };
    let closed_ring = |r: &[Point]| {
        let mut r = r.to_vec();
        if let (Some(f), Some(l)) = (r.first().copied(), r.last().copied())
            && r.len() >= 2
            && f != l
        {
            r.push(f);
        }
        r
    };
    let c = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let mut paths = Vec::new();
    match m.kind {
        Kind::Ellipse => paths.push(ring(c, b.width() / 2.0, b.height() / 2.0, 32)),
        Kind::Area | Kind::Perimeter | Kind::Volume | Kind::Polygon | Kind::Cloud | Kind::Rectangle => {
            paths.push(closed_ring(&m.pts));
            paths.extend(m.holes.iter().map(|h| closed_ring(h)));
        }
        Kind::Line | Kind::Arrow | Kind::Length | Kind::Polyline | Kind::Polylength | Kind::Angle => {
            paths.push(m.pts.clone());
        }
        Kind::Ink | Kind::Highlight => {
            let mut from = 0usize;
            for &s in m.strokes.iter().chain(std::iter::once(&m.pts.len())) {
                if s > from
                    && let Some(stroke) = m.pts.get(from..s)
                {
                    paths.push(stroke.to_vec());
                }
                from = s;
            }
        }
        _ => paths.push(vec![
            Point::new(b.x0, b.y0),
            Point::new(b.x1, b.y0),
            Point::new(b.x1, b.y1),
            Point::new(b.x0, b.y1),
            Point::new(b.x0, b.y0),
        ]),
    }
    let half = b.width().max(b.height()) / 2.0;
    let f = if half > 0.0 { 6.0 / half } else { 1.0 };
    for path in &mut paths {
        for p in path.iter_mut() {
            *p = Point::new((p.x - c.x) * f, (p.y - c.y) * f);
        }
    }
    paths
}

// ---- totals and counts --------------------------------------------------------------------

/// The total of one unit in a selection (Properties toolbar Totals).
#[derive(Debug, Clone, PartialEq)]
pub struct Total {
    /// `ft`, `sf`, `ea` ...
    pub unit: String,
    pub value: f64,
    pub items: usize,
    /// formatted with the first item's format
    pub text: String,
}

/// Totals per unit of the measurements at `indices` (others ignored).
pub fn totals(doc: &Document, indices: &[usize]) -> Vec<Total> {
    let mut out: Vec<Total> = Vec::new();
    let mut fmts: Vec<Option<Vec<NumberFormat>>> = Vec::new();
    for &i in indices {
        let Some(m) = doc.markups.get(i) else { continue };
        if !m.kind.is_measurement() {
            continue;
        }
        let Some(q) = m.quantity() else { continue };
        let unit = m.unit();
        let k = match out.iter().position(|t| t.unit == unit) {
            Some(k) => k,
            None => {
                out.push(Total {
                    unit: unit.clone(),
                    value: 0.0,
                    items: 0,
                    text: String::new(),
                });
                fmts.push(if m.kind == Kind::Count || m.kind == Kind::Angle {
                    None
                } else {
                    m.quantity_formats().cloned()
                });
                out.len() - 1
            }
        };
        if let Some(t) = out.get_mut(k) {
            t.value += q;
            t.items += 1;
        }
    }
    for (t, fa) in out.iter_mut().zip(&fmts) {
        t.text = match fa {
            Some(fa) if !fa.is_empty() => format_value(t.value, fa),
            _ => format!("{} {}", t.value.round() as i64, t.unit),
        };
    }
    out
}

/// (subject, page) -> items, for every Count on every page.
pub fn counts_by_page(doc: &Document) -> BTreeMap<(String, usize), usize> {
    let mut out = BTreeMap::new();
    for m in doc.markups.iter().filter(|m| m.kind == Kind::Count) {
        *out.entry((m.subject.clone(), m.page)).or_insert(0) += m.pts.len();
    }
    out
}

// ---- page scales --------------------------------------------------------------------------

fn same_formats(a: &[NumberFormat], b: &[NumberFormat]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.unit == y.unit
                && (x.conv - y.conv).abs() <= 1e-12 * x.conv.abs().max(1.0)
                && x.fmt == y.fmt
                && x.den == y.den
        })
}

fn same_scale(a: &Scale, b: &Scale) -> bool {
    a.ratio == b.ratio
        && same_formats(&a.x, &b.x)
        && same_formats(&a.y, &b.y)
        && same_formats(&a.dist, &b.dist)
        && same_formats(&a.area, &b.area)
}

/// Give every measurement on `pages` the scale now in effect where it sits (page scale or
/// viewport), keeping its own display unit, precision and Rise/Drop / depth. Returns how many
/// changed.
pub fn recalculate(doc: &mut Document, pages: &[usize]) -> usize {
    let mut changed = 0;
    let Document {
        markups, pages: infos, ..
    } = doc;
    for m in markups.iter_mut() {
        if !m.kind.is_measurement() || m.kind == Kind::Count || m.locked() || !pages.contains(&m.page) {
            continue;
        }
        let Some(first) = m.pts.first().copied() else { continue };
        let Some(s) = infos.get(m.page).and_then(|pi| pi.scale_at(first)) else {
            continue;
        };
        let mut next = s.clone();
        let (mut rise, mut depth) = (m.rise_drop, m.depth);
        if let Some(own) = &m.scale {
            if let Some(du) = units::scale_display_unit(own) {
                units::set_display_unit(&mut next, du);
            }
            units::set_precision(&mut next, units::scale_precision(own));
            let from = own.dist.first().and_then(|f| LengthUnit::from_label(&f.unit));
            let to = next.dist.first().and_then(|f| LengthUnit::from_label(&f.unit));
            if let (Some(from), Some(to)) = (from, to) {
                rise = from.convert(rise, to);
                depth = from.convert(depth, to);
            }
            if same_scale(own, &next) {
                continue;
            }
        }
        m.scale = Some(next);
        m.rise_drop = rise;
        m.depth = depth;
        m.dirty = true;
        changed += 1;
    }
    changed
}

/// Copy page `from`'s scale (page scale or its first viewport) to `pages` as their page scale.
/// Returns how many pages changed (0 when `from` has no scale).
pub fn copy_scale_to_pages(doc: &mut Document, from: usize, pages: &[usize]) -> usize {
    let Some(src) = doc.pages.get(from) else { return 0 };
    let s = src
        .scale
        .clone()
        .or_else(|| src.viewports.first().map(|v| v.scale.clone()));
    let Some(s) = s.filter(Scale::valid) else { return 0 };
    let mut n = 0;
    for &p in pages {
        if p == from {
            continue;
        }
        let Some(pi) = doc.pages.get_mut(p) else { continue };
        pi.scale = Some(s.clone());
        pi.scale_changed = true;
        pi.viewports.clear(); // the scale covers the whole page (as Set Scale does)
        n += 1;
    }
    n
}

// ---- Volume, Diameter, Radius, Angle, wall area, slope ------------------------------------

/// The angle at `vertex` between the rays to `a` and `b`, degrees in `[0, 180]`.
pub fn angle_degrees(a: Point, vertex: Point, b: Point) -> Option<f64> {
    let (ux, uy, vx, vy) = (a.x - vertex.x, a.y - vertex.y, b.x - vertex.x, b.y - vertex.y);
    if ux.hypot(uy) <= 0.0 || vx.hypot(vy) <= 0.0 {
        return None;
    }
    let ang = (ux * vy - uy * vx).atan2(ux * vx + uy * vy).abs().to_degrees();
    ang.is_finite().then_some(ang)
}

/// An Angle markup's angle: `pts` = [arm end, vertex, arm end].
pub fn angle_of(m: &Markup) -> Option<f64> {
    match m.pts.as_slice() {
        [a, v, b, ..] => angle_degrees(*a, *v, *b),
        _ => None,
    }
}

/// `45°`, `33.69°`: two decimals, trailing zeros stripped (like Revu's decimal labels).
pub fn format_angle(deg: f64) -> String {
    format_value(deg, &[angle_format()])
}

fn angle_format() -> NumberFormat {
    NumberFormat::new("\u{b0}", 1.0, Fmt::Decimal, 100, "", "")
}

/// Diameter: `pts` = the two ends of a diameter. Radius: `pts` = [centre, point on the circle].
/// Value in the first `/D` unit.
pub fn circle_measure(m: &Markup) -> Option<f64> {
    let s = m.scale.as_ref().filter(|s| s.valid())?;
    let (a, b) = (m.pts.first()?, m.pts.get(1)?);
    Some(s.length_of(&[*a, *b], false))
}

/// Centre and radius (PDF units) of a Diameter or Radius markup's circle.
pub fn circle_of(m: &Markup) -> Option<(Point, f64)> {
    let (a, b) = (*m.pts.first()?, *m.pts.get(1)?);
    match m.kind {
        Kind::Diameter => Some((a.mid(b), a.dist(b) / 2.0)),
        Kind::Radius => Some((a, a.dist(b))),
        _ => None,
    }
}

/// The circle's area in the first `/A` unit (Diameter / Radius).
pub fn circle_area(m: &Markup) -> Option<f64> {
    let s = m.scale.as_ref().filter(|s| s.valid())?;
    let (c, r) = circle_of(m)?;
    Some(s.area_of(&ellipse_ring(c, r, r, 256)) * circle_correction(256))
}

/// The circle's circumference in the first `/D` unit (Diameter / Radius).
pub fn circle_circumference(m: &Markup) -> Option<f64> {
    let d = match m.kind {
        Kind::Diameter => circle_measure(m)?,
        Kind::Radius => 2.0 * circle_measure(m)?,
        _ => return None,
    };
    Some(PI * d)
}

/// A regular n-gon inscribed in a unit circle has area n/2 sin(2 pi / n): scale to pi.
fn circle_correction(n: usize) -> f64 {
    let n = n as f64;
    PI / (n / 2.0 * (2.0 * PI / n).sin())
}

/// Volume = net area x depth (`/V`'s first unit; depth in the first `/D` unit).
pub fn volume_of(m: &Markup) -> Option<f64> {
    Some(net_area(m)? * m.depth)
}

/// Wall area = perimeter x depth (Perimeter, Area, Volume, Polylength with a depth).
pub fn wall_area(m: &Markup) -> Option<f64> {
    let s = m.scale.as_ref().filter(|s| s.valid())?;
    if m.depth == 0.0 {
        return None;
    }
    let closed = closed_shape(m.kind);
    if !closed && m.kind != Kind::Polylength {
        return None;
    }
    Some(s.length_of(&m.pts, closed) * m.depth.abs())
}

/// The plan length of the outline in PDF units (no scale), handy for drawing.
pub fn outline_length(m: &Markup) -> f64 {
    polyline_length(&m.pts, closed_shape(m.kind))
}

/// How a slope is given (Measurement Properties > Slope).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slope {
    None,
    /// rise per run, e.g. 4 in 12
    Pitch {
        rise: f64,
        run: f64,
    },
    Degrees(f64),
    /// percent grade (rise / run x 100)
    Grade(f64),
}

impl Slope {
    /// The slope angle, radians.
    pub fn angle(self) -> f64 {
        match self {
            Slope::None => 0.0,
            Slope::Pitch { rise, run } if run != 0.0 => (rise / run).atan(),
            Slope::Pitch { .. } => 0.0,
            Slope::Degrees(d) => d.to_radians(),
            Slope::Grade(g) => (g / 100.0).atan(),
        }
    }

    /// True (sloped) length or area per unit of plan: 1 / cos(angle). 1 for vertical or bad input.
    pub fn factor(self) -> f64 {
        let c = self.angle().cos();
        if c.abs() < 1e-9 || !c.is_finite() {
            1.0
        } else {
            1.0 / c.abs()
        }
    }

    /// Revu's `/SlopeType` (0 none, 1 pitch, 2 degrees, 3 grade).
    pub fn type_code(self) -> i64 {
        match self {
            Slope::None => 0,
            Slope::Pitch { .. } => 1,
            Slope::Degrees(_) => 2,
            Slope::Grade(_) => 3,
        }
    }
}

/// Distance from `p` to the outline of `m` (PDF units), for hit tests on the new kinds.
pub fn distance_to(m: &Markup, p: Point) -> Option<f64> {
    if let Some((c, r)) = circle_of(m) {
        return Some((p.dist(c) - r).abs());
    }
    let closed = closed_shape(m.kind);
    let n = m.pts.len();
    let segs = if closed && n >= 3 { n } else { n.saturating_sub(1) };
    (0..segs)
        .filter_map(|i| Some(dist_to_segment(p, *m.pts.get(i)?, *m.pts.get((i + 1) % n)?)))
        .reduce(f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageInfo;
    use markupcraft_measure::units::{DisplayUnit, Precision, custom_scale, metric_ratio};

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    /// 1 pt = 1 ft, decimal feet: lengths read directly.
    fn unit_scale() -> Scale {
        custom_scale(
            1.0,
            LengthUnit::Point,
            1.0,
            LengthUnit::Foot,
            Some(DisplayUnit::single(LengthUnit::Foot)),
            Some(Precision::decimals(2)),
        )
    }

    fn polylength(pts: Vec<Point>) -> Markup {
        Markup {
            kind: Kind::Polylength,
            pts,
            scale: Some(unit_scale()),
            ..Default::default()
        }
    }

    fn handle(m: &Markup, arc: usize) -> Point {
        arc_handle(m, arc).and_then(|h| pt(m, h)).unwrap_or_default()
    }

    #[test]
    fn arc_through_points() {
        let pts = arc_through(p(0.0, 0.0), p(50.0, 50.0), p(100.0, 0.0), ARC_INTERIOR);
        assert_eq!(pts.len(), ARC_INTERIOR);
        let worst = pts
            .iter()
            .map(|q| ((q.x - 50.0).hypot(q.y) - 50.0).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-9);
        let mid = pts[ARC_INTERIOR.div_ceil(2) - 1];
        assert!((mid.x - 50.0).abs() < 1e-9 && (mid.y - 50.0).abs() < 1e-9);
        let straight = arc_through(p(0.0, 0.0), p(50.0, 0.0), p(100.0, 0.0), 3);
        assert!((straight[1].x - 50.0).abs() < 1e-9);
    }

    #[test]
    fn convert_bend_and_edit_arcs() {
        let mut m = polylength(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)]);
        assert_eq!(convert_to_arc(&mut m, 0, 0.0), Some(0));
        assert_eq!(m.pts.len(), 3 + ARC_INTERIOR);
        assert_eq!(arc_end(&m, 0), Some(ARC_INTERIOR + 1));
        let h = handle(&m, 0);
        assert!((h.x - 50.0).abs() < 1e-9 && (h.y - 25.0).abs() < 1e-9);
        // r = (50^2 + 25^2) / 50 = 62.5; arc = 2 asin(50 / 62.5) r.
        let arc_len = 2.0 * (50.0f64 / 62.5).asin() * 62.5;
        let q = m.quantity().unwrap_or(0.0);
        assert!((q - (100.0 + arc_len)).abs() < (100.0 + arc_len) * 5e-4);
        assert_eq!(convert_to_arc(&mut m, 0, 0.0), None, "already an arc");
        assert_eq!(convert_to_arc(&mut m, 5, 0.0), None, "not a control vertex");
        let last = m.pts.len() - 1;
        assert_eq!(
            convert_to_arc(&mut m, last, 0.0),
            None,
            "no segment after the last vertex"
        );
        assert!(!is_editable_vertex(&m, 3) && is_editable_vertex(&m, 16) && is_editable_vertex(&m, 0));

        bend_arc(&mut m, 0, p(50.0, -40.0));
        let h = handle(&m, 0);
        assert!((h.x - 50.0).abs() < 1e-9 && (h.y + 40.0).abs() < 1e-9);
        bend_arc(&mut m, 0, p(80.0, -40.0));
        assert!((handle(&m, 0).x - 50.0).abs() < 1e-9, "handle stays on the bisector");

        m.segment_values = true;
        let sv = segment_values(&m);
        assert_eq!(sv.len(), 2);
        assert!((sv[1].value - 100.0).abs() < 1e-9);
        assert!((sv[0].value + sv[1].value - m.quantity().unwrap_or(0.0)).abs() < 1e-9);

        assert!(!insert_vertex(&mut m, 0, p(50.0, 0.0)), "no vertex inside an arc");
        let n = m.pts.len();
        assert!(insert_vertex(&mut m, ARC_INTERIOR + 1, p(100.0, 50.0)));
        assert_eq!(m.pts.len(), n + 1);
        assert_eq!(m.arcs[0].0, 0);
        let i = m.pts.len() - 2;
        assert!(erase_vertex(&mut m, i));
        assert_eq!(m.pts.len(), n);
        let last = m.pts.len() - 1;
        assert!(!insert_vertex(&mut m, last, p(200.0, 200.0)));
        assert!(erase_vertex(&mut m, 0), "erase the arc's start");
        assert!(m.arcs.is_empty() && m.pts.len() == 2);
        assert!(!erase_vertex(&mut m, 0), "an open run keeps 2 vertices");
    }

    #[test]
    fn closing_arc_and_validation() {
        let mut area = Markup {
            kind: Kind::Area,
            scale: Some(unit_scale()),
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            ..Default::default()
        };
        assert_eq!(convert_to_arc(&mut area, 3, 0.0), Some(0));
        assert_eq!(arc_end(&area, 0), Some(0));
        assert_eq!(area.pts.len(), 4 + ARC_INTERIOR);
        assert!(handle(&area, 0).x < 0.0, "closing arc bulges outward");
        assert!(area.quantity().unwrap_or(0.0) > 10000.0);
        let mut copy = area.clone();
        validate_arcs(&mut copy);
        assert_eq!(copy.arcs.len(), 1);
        let k = copy.arcs[0].0 + 3;
        copy.pts[k].x += 10.0;
        validate_arcs(&mut copy);
        assert!(copy.arcs.is_empty(), "bad arcs dropped");
        let mut hostile = area.clone();
        hostile.arcs = vec![(usize::MAX - 1, 5), (2, usize::MAX)];
        validate_arcs(&mut hostile);
        assert!(hostile.arcs.is_empty());
        assert!(erase_vertex(&mut area, 0), "erase the arc's end vertex");
        assert!(area.arcs.is_empty() && area.pts.len() == 3);
        assert!(!erase_vertex(&mut area, 0), "an area keeps 3 vertices");

        let mut pl = polylength(vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)]);
        convert_to_arc(&mut pl, 1, 0.0);
        let hit = segment_at(&pl, p(50.0, 1.0), 3.0);
        assert!(hit.is_some_and(|h| h.start == 0 && h.arc.is_none()));
        let hh = handle(&pl, 0);
        let hit = segment_at(&pl, hh, 3.0);
        assert!(hit.is_some_and(|h| h.start == 1 && h.arc == Some(0)));
    }

    #[test]
    fn holes_and_cutouts() {
        let mut a = Markup {
            kind: Kind::Area,
            scale: Some(Scale::engineering(10.0)),
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)],
            holes: vec![vec![p(20.0, 20.0), p(40.0, 20.0), p(40.0, 40.0), p(20.0, 40.0)]],
            ..Default::default()
        };
        assert_eq!(hole_at(&a, p(30.0, 20.5), 1.0, true), Some(0));
        assert_eq!(hole_at(&a, p(30.0, 30.0), 1.0, true), Some(0));
        assert_eq!(hole_at(&a, p(30.0, 30.0), 1.0, false), None);
        assert_eq!(hole_at(&a, p(60.0, 60.0), 1.0, true), None);
        assert!(insert_hole_vertex(&mut a, 0, 0, p(30.0, 18.0)) && a.holes[0].len() == 5);
        assert!(erase_hole_vertex(&mut a, 0, 1) && a.holes[0].len() == 4);
        assert!(!insert_hole_vertex(&mut a, 3, 0, p(0.0, 0.0)));
        let ring = ellipse_ring(p(0.0, 0.0), 10.0, 10.0, 48);
        assert_eq!(ring.len(), 48);
        assert!((polygon_area(&ring) / (PI * 100.0) - 1.0).abs() < 0.005);

        // Deductions: 1" = 10', 100 pt square = (1000/72)^2 sf.
        let gross = a.gross_area();
        let deductions = cutout_areas(&a);
        assert_eq!(deductions.len(), 1);
        assert!((net_area(&a).unwrap_or(0.0) - (gross - deductions[0])).abs() < 1e-9);
        assert!(add_cutout(&mut a, vec![p(60.0, 60.0), p(80.0, 60.0), p(80.0, 80.0)]));
        assert!(
            !add_cutout(&mut a, vec![p(60.0, 60.0), p(180.0, 60.0), p(80.0, 80.0)]),
            "outside"
        );
        assert!(
            !add_cutout(&mut a, vec![p(60.0, 60.0), p(70.0, 70.0)]),
            "too few points"
        );
        assert_eq!(cutout_areas(&a).len(), 2);
        assert!(remove_cutout(&mut a, 1) && !remove_cutout(&mut a, 5));
        let mut line = polylength(vec![p(0.0, 0.0), p(1.0, 1.0)]);
        assert!(!add_cutout(&mut line, ring));
    }

    #[test]
    fn custom_count_symbols() {
        let r = Markup {
            kind: Kind::Rectangle,
            pts: vec![p(100.0, 100.0), p(120.0, 100.0), p(120.0, 110.0), p(100.0, 110.0)],
            ..Default::default()
        };
        let paths = symbol_from_markup(&r);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].len(), 5);
        assert!((paths[0][0].x + 6.0).abs() < 1e-9 && (paths[0][0].y + 3.0).abs() < 1e-9);
        let e = Markup {
            kind: Kind::Ellipse,
            pts: vec![p(0.0, 0.0), p(40.0, 0.0), p(40.0, 40.0), p(0.0, 40.0)],
            ..Default::default()
        };
        let ep = symbol_from_markup(&e);
        assert!(ep.len() == 1 && ep[0].len() == 33);
        let ink = Markup {
            kind: Kind::Ink,
            pts: vec![p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0), p(10.0, 10.0)],
            strokes: vec![2],
            ..Default::default()
        };
        assert_eq!(symbol_from_markup(&ink).len(), 2);
        let mut c = Markup {
            kind: Kind::Count,
            count_symbol: CountSymbol::Custom,
            symbol_paths: paths,
            symbol_scale: 2.0,
            ..Default::default()
        };
        let at = symbol_paths_at(&c, p(50.0, 50.0));
        assert!((at[0][0].x - 38.0).abs() < 1e-9 && (at[0][0].y - 44.0).abs() < 1e-9);
        c.count_symbol = CountSymbol::Square;
        assert_eq!(symbol_from_markup(&c).len(), 1);
        assert!(symbol_from_markup(&Markup::default()).is_empty());
    }

    #[test]
    fn totals_counts_and_page_scales() {
        let mut doc = Document::default();
        doc.pages.resize(2, PageInfo::default());
        let ft = unit_scale();
        doc.pages[0].scale = Some(ft.clone());
        let pl = polylength(vec![p(0.0, 0.0), p(30.0, 0.0)]);
        let pl2 = polylength(vec![p(0.0, 0.0), p(0.0, 20.0)]);
        let a = Markup {
            kind: Kind::Area,
            scale: Some(ft.clone()),
            pts: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)],
            ..Default::default()
        };
        let c = Markup {
            kind: Kind::Count,
            page: 1,
            subject: "VAV".into(),
            pts: vec![p(1.0, 1.0), p(2.0, 2.0)],
            ..Default::default()
        };
        let c2 = Markup {
            page: 0,
            pts: vec![p(3.0, 3.0)],
            ..c.clone()
        };
        doc.markups = vec![pl, pl2, a, c, c2];
        let t = totals(&doc, &[0, 1, 2, 3, 4, 99]);
        assert_eq!(t.len(), 3);
        assert!(t[0].unit == "ft" && (t[0].value - 50.0).abs() < 1e-9 && t[0].items == 2);
        assert_eq!(t[0].text, "50 ft");
        assert!(t[1].unit == "sf" && (t[1].value - 100.0).abs() < 1e-9);
        assert!(t[2].unit == "ea" && t[2].text == "3 ea");
        let by_page = counts_by_page(&doc);
        assert_eq!(by_page.len(), 2);
        assert_eq!(by_page.get(&("VAV".to_string(), 1)), Some(&2));
        assert_eq!(by_page.get(&("VAV".to_string(), 0)), Some(&1));

        // Recalculate: page 0 now 1 pt = 2 ft; measurements keep decimal feet and 2 places.
        let two = custom_scale(
            1.0,
            LengthUnit::Point,
            2.0,
            LengthUnit::Foot,
            Some(DisplayUnit::single(LengthUnit::Foot)),
            Some(Precision::decimals(2)),
        );
        doc.pages[0].scale = Some(two.clone());
        doc.markups[0].rise_drop = 1.0;
        assert_eq!(recalculate(&mut doc, &[0]), 3);
        assert!((doc.markups[0].quantity().unwrap_or(0.0) - 61.0).abs() < 1e-9);
        assert!(doc.markups[0].dirty);
        assert_eq!(recalculate(&mut doc, &[0]), 0);

        // A metric page scale: the markup keeps its display unit (ft), Rise/Drop unchanged.
        let mut mm = polylength(vec![p(0.0, 0.0), p(10.0, 0.0)]);
        mm.page = 1;
        mm.rise_drop = 10.0;
        doc.markups.push(mm);
        doc.pages[1].scale = Some(metric_ratio(100.0, LengthUnit::Meter));
        recalculate(&mut doc, &[1]);
        let back = doc
            .markups
            .last()
            .and_then(|m| m.scale.as_ref())
            .and_then(units::scale_display_unit);
        assert_eq!(back, Some(DisplayUnit::single(LengthUnit::Foot)));
        assert!((doc.markups.last().map_or(0.0, |m| m.rise_drop) - 10.0).abs() < 1e-9);

        let mut d2 = Document::default();
        d2.pages.resize(4, PageInfo::default());
        d2.pages[1].scale = Some(two.clone());
        assert_eq!(copy_scale_to_pages(&mut d2, 1, &[0, 1, 2, 3, 9]), 3);
        assert!(d2.pages[3].scale.as_ref().is_some_and(|s| s.ratio == two.ratio) && d2.pages[3].scale_changed);
        assert_eq!(copy_scale_to_pages(&mut d2, 0, &[2]), 1);
        let mut d3 = Document::default();
        d3.pages.resize(2, PageInfo::default());
        assert_eq!(copy_scale_to_pages(&mut d3, 0, &[1]), 0);
        assert_eq!(copy_scale_to_pages(&mut d3, 7, &[1]), 0);
    }

    #[test]
    fn angle_diameter_radius_volume() {
        assert!((angle_degrees(p(10.0, 0.0), p(0.0, 0.0), p(0.0, 5.0)).unwrap_or(0.0) - 90.0).abs() < 1e-9);
        assert!((angle_degrees(p(10.0, 0.0), p(0.0, 0.0), p(-3.0, 0.0)).unwrap_or(0.0) - 180.0).abs() < 1e-9);
        assert!(angle_degrees(p(0.0, 0.0), p(0.0, 0.0), p(1.0, 0.0)).is_none());
        let ang = Markup {
            kind: Kind::Angle,
            pts: vec![p(10.0, 0.0), p(0.0, 0.0), p(10.0, 10.0)],
            ..Default::default()
        };
        assert!((angle_of(&ang).unwrap_or(0.0) - 45.0).abs() < 1e-9);
        assert_eq!(format_angle(45.0), "45\u{b0}");
        assert_eq!(format_angle(33.690067), "33.69\u{b0}");

        let dia = Markup {
            kind: Kind::Diameter,
            scale: Some(unit_scale()),
            pts: vec![p(0.0, 0.0), p(20.0, 0.0)],
            ..Default::default()
        };
        assert!((circle_measure(&dia).unwrap_or(0.0) - 20.0).abs() < 1e-9);
        assert!((circle_area(&dia).unwrap_or(0.0) - PI * 100.0).abs() < 1e-6);
        assert!((circle_circumference(&dia).unwrap_or(0.0) - PI * 20.0).abs() < 1e-9);
        let rad = Markup {
            kind: Kind::Radius,
            pts: vec![p(5.0, 5.0), p(5.0, 15.0)],
            ..dia.clone()
        };
        assert_eq!(circle_of(&rad), Some((p(5.0, 5.0), 10.0)));
        assert!((circle_circumference(&rad).unwrap_or(0.0) - PI * 20.0).abs() < 1e-9);
        assert!((distance_to(&rad, p(5.0, 17.0)).unwrap_or(0.0) - 2.0).abs() < 1e-9);

        let mut vol = Markup {
            kind: Kind::Volume,
            scale: Some(unit_scale()),
            pts: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)],
            holes: vec![vec![p(1.0, 1.0), p(3.0, 1.0), p(3.0, 3.0), p(1.0, 3.0)]],
            depth: 2.5,
            ..Default::default()
        };
        assert!((volume_of(&vol).unwrap_or(0.0) - 96.0 * 2.5).abs() < 1e-9);
        assert!((wall_area(&vol).unwrap_or(0.0) - 40.0 * 2.5).abs() < 1e-9);
        vol.depth = 0.0;
        assert!(wall_area(&vol).is_none());
        assert!((outline_length(&vol) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn slopes() {
        assert!((Slope::None.factor() - 1.0).abs() < 1e-12);
        assert!((Slope::Pitch { rise: 12.0, run: 12.0 }.factor() - 2f64.sqrt()).abs() < 1e-12);
        assert!((Slope::Degrees(60.0).factor() - 2.0).abs() < 1e-9);
        assert!((Slope::Grade(100.0).factor() - 2f64.sqrt()).abs() < 1e-12);
        assert!(
            (Slope::Degrees(90.0).factor() - 1.0).abs() < 1e-12,
            "vertical: no factor"
        );
        assert!((Slope::Pitch { rise: 1.0, run: 0.0 }.factor() - 1.0).abs() < 1e-12);
        assert_eq!(Slope::Grade(5.0).type_code(), 3);
    }
}
