//! Dynamic Fill: click inside a room drawn with lines and get its outline, as an Area (or
//! Polygon, Perimeter, Space).
//!
//! The page's linework ([`crate::vectors`]) becomes a planar graph: endpoints closer than the
//! gap tolerance are merged, segments are split where they cross or where an endpoint stops
//! within the tolerance of another segment, and dangling edges are pruned. A ray from the seed
//! finds the nearest edge; walking the faces of the graph from there (always taking the next
//! edge clockwise) finds the smallest closed region around the seed. Islands inside it (columns,
//! fixtures drawn as closed shapes) become cutouts.
//!
//! The graph is built over a window around the seed that grows (×4) until the region does not
//! touch the window's border, so a small room on a large sheet stays cheap.

use std::collections::{HashMap, HashSet};

use markupcraft_geom::{Point, Rect, point_in_polygon, polygon_area};
use markupcraft_model::{Kind, Markup};

use crate::{EngineError, Result, Session, invalid};

/// Most segments one fill builds its graph from.
pub const MAX_FILL_SEGMENTS: usize = 400_000;
/// Most (cell, segment) entries in the intersection grid.
const MAX_GRID_ITEMS: usize = 8_000_000;
/// Most edges crossed by the seed's ray that are tried.
const MAX_HITS: usize = 512;
/// Most cutouts kept.
const MAX_HOLES: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub struct FillOptions {
    /// Gaps up to this many points between line ends (and between an end and another line)
    /// are closed.
    pub gap: f64,
    /// Islands inside the region become cutouts.
    pub cutouts: bool,
    /// Extra boundary lines (Add Boundary), each an open polyline in user space.
    pub boundaries: Vec<Vec<Point>>,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            gap: 0.5,
            cutouts: true,
            boundaries: Vec::new(),
        }
    }
}

/// A traced region.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FillRegion {
    /// counter-clockwise outline, user space
    pub outer: Vec<Point>,
    /// cutouts (clockwise outlines)
    pub holes: Vec<Vec<Point>>,
    /// area in PDF points², cutouts removed
    pub area: f64,
    /// segments the final graph was built from
    pub segments: usize,
}

/// What a fill makes.
#[derive(Debug, Clone, PartialEq)]
pub enum FillOutput {
    Area,
    Polygon,
    Perimeter,
    /// a Space with this name
    Space(String),
}

// ---- the planar graph ------------------------------------------------------------------------

struct Graph {
    pts: Vec<Point>,
    /// neighbours of each vertex, sorted by angle (counter-clockwise from +x)
    adj: Vec<Vec<usize>>,
}

/// Merges points closer than `tol` (a hash grid of `tol`-sized cells).
struct Merger {
    tol: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
    pts: Vec<Point>,
}

impl Merger {
    fn key(&self, p: Point) -> (i64, i64) {
        ((p.x / self.tol).floor() as i64, (p.y / self.tol).floor() as i64)
    }

    fn id(&mut self, p: Point) -> usize {
        let (cx, cy) = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &i in list {
                        if self.pts.get(i).is_some_and(|q| q.dist(p) <= self.tol) {
                            return i;
                        }
                    }
                }
            }
        }
        let i = self.pts.len();
        self.pts.push(p);
        self.cells.entry((cx, cy)).or_default().push(i);
        i
    }
}

fn seg_intersection(a: Point, b: Point, c: Point, d: Point) -> Option<(f64, f64)> {
    let r = (b.x - a.x, b.y - a.y);
    let s = (d.x - c.x, d.y - c.y);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-12 {
        return None;
    }
    let qp = (c.x - a.x, c.y - a.y);
    let t = (qp.0 * s.1 - qp.1 * s.0) / den;
    let u = (qp.0 * r.1 - qp.1 * r.0) / den;
    let eps = 1e-9;
    ((-eps..=1.0 + eps).contains(&t) && (-eps..=1.0 + eps).contains(&u))
        .then_some((t.clamp(0.0, 1.0), u.clamp(0.0, 1.0)))
}

/// Parameter of the point of segment `a b` nearest `p`, when it is within `tol` of it.
fn project(p: Point, a: Point, b: Point, tol: f64) -> Option<f64> {
    let d = (b.x - a.x, b.y - a.y);
    let len2 = d.0 * d.0 + d.1 * d.1;
    if len2 < 1e-18 {
        return None;
    }
    let t = (((p.x - a.x) * d.0 + (p.y - a.y) * d.1) / len2).clamp(0.0, 1.0);
    let q = Point::new(a.x + t * d.0, a.y + t * d.1);
    (q.dist(p) <= tol).then_some(t)
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn build_graph(segs: &[(Point, Point)], tol: f64) -> Graph {
    let n = segs.len();
    let mut cuts: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; n];
    // intersection grid
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (a, b) in segs {
        x0 = x0.min(a.x.min(b.x));
        y0 = y0.min(a.y.min(b.y));
        x1 = x1.max(a.x.max(b.x));
        y1 = y1.max(a.y.max(b.y));
    }
    if n > 0 {
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let cell = ((w * h / n as f64).sqrt() * 2.0).max(tol * 4.0).max(1e-3);
        let cols = ((w / cell).ceil() as usize).clamp(1, 4096);
        let rows = ((h / cell).ceil() as usize).clamp(1, 4096);
        let (cw, ch) = (w / cols as f64, h / rows as f64);
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); cols * rows];
        let mut items = 0usize;
        let mut first_cell: Vec<(usize, usize)> = Vec::with_capacity(n);
        let cell_of = |v: f64, o: f64, s: f64, m: usize| (((v - o) / s).floor().max(0.0) as usize).min(m - 1);
        for (i, (a, b)) in segs.iter().enumerate() {
            let (cx0, cx1) = (
                cell_of(a.x.min(b.x) - tol, x0, cw, cols),
                cell_of(a.x.max(b.x) + tol, x0, cw, cols),
            );
            let (cy0, cy1) = (
                cell_of(a.y.min(b.y) - tol, y0, ch, rows),
                cell_of(a.y.max(b.y) + tol, y0, ch, rows),
            );
            first_cell.push((cx0, cy0));
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    if items >= MAX_GRID_ITEMS {
                        break;
                    }
                    if let Some(c) = grid.get_mut(cy * cols + cx) {
                        c.push(i as u32);
                        items += 1;
                    }
                }
            }
        }
        for (ci, c) in grid.iter().enumerate() {
            let (cx, cy) = (ci % cols, ci / cols);
            for (k, &i) in c.iter().enumerate() {
                for &j in c.iter().skip(k + 1) {
                    // each pair once: in the first cell both segments cover
                    let (Some(fi), Some(fj)) = (first_cell.get(i as usize), first_cell.get(j as usize)) else {
                        continue;
                    };
                    if cx != fi.0.max(fj.0) || cy != fi.1.max(fj.1) {
                        continue;
                    }
                    let (Some(&(a, b)), Some(&(cc, d))) = (segs.get(i as usize), segs.get(j as usize)) else {
                        continue;
                    };
                    if let Some((t, u)) = seg_intersection(a, b, cc, d) {
                        if let Some(v) = cuts.get_mut(i as usize) {
                            v.push(t);
                        }
                        if let Some(v) = cuts.get_mut(j as usize) {
                            v.push(u);
                        }
                    }
                    // an end stopping short of (or just past) another line joins it
                    for (p, (s, e), idx) in [(a, (cc, d), j), (b, (cc, d), j), (cc, (a, b), i), (d, (a, b), i)] {
                        if let Some(t) = project(p, s, e, tol)
                            && let Some(v) = cuts.get_mut(idx as usize)
                        {
                            v.push(t);
                        }
                    }
                }
            }
        }
    }
    let mut merger = Merger {
        tol: tol.max(1e-6),
        cells: HashMap::new(),
        pts: Vec::new(),
    };
    let mut edges: HashSet<(usize, usize)> = HashSet::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        let Some(ts) = cuts.get_mut(i) else { continue };
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        let mut prev: Option<usize> = None;
        for &t in ts.iter() {
            let v = merger.id(lerp(*a, *b, t));
            if let Some(u) = prev
                && u != v
            {
                edges.insert((u.min(v), u.max(v)));
            }
            prev = Some(v);
        }
    }
    let pts = merger.pts;
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); pts.len()];
    for &(u, v) in &edges {
        if let Some(l) = adj.get_mut(u) {
            l.push(v);
        }
        if let Some(l) = adj.get_mut(v) {
            l.push(u);
        }
    }
    // prune dangling edges
    let mut stack: Vec<usize> = (0..adj.len())
        .filter(|&v| adj.get(v).is_some_and(|l| l.len() == 1))
        .collect();
    while let Some(v) = stack.pop() {
        let Some(l) = adj.get_mut(v) else { continue };
        if l.len() != 1 {
            continue;
        }
        let Some(w) = l.pop() else { continue };
        if let Some(lw) = adj.get_mut(w) {
            lw.retain(|x| *x != v);
            if lw.len() == 1 {
                stack.push(w);
            }
        }
    }
    for (v, l) in adj.iter_mut().enumerate() {
        let Some(&p) = pts.get(v) else { continue };
        l.sort_by(|a, b| {
            let ang = |q: usize| pts.get(q).map_or(0.0, |q| (q.y - p.y).atan2(q.x - p.x));
            ang(*a).total_cmp(&ang(*b))
        });
        l.dedup();
    }
    Graph { pts, adj }
}

impl Graph {
    fn pt(&self, v: usize) -> Point {
        self.pts.get(v).copied().unwrap_or(Point::new(0.0, 0.0))
    }

    /// The next half-edge of the face left of `u -> v`.
    fn next(&self, u: usize, v: usize) -> Option<usize> {
        let l = self.adj.get(v)?;
        let i = l.iter().position(|x| *x == u)?;
        let k = (i + l.len() - 1) % l.len();
        l.get(k).copied()
    }

    /// The face left of half-edge `u -> v`, as vertex ids.
    fn face(&self, u: usize, v: usize) -> Option<Vec<usize>> {
        let limit = self.pts.len().saturating_mul(4).saturating_add(8);
        let mut out = vec![u];
        let (mut a, mut b) = (u, v);
        for _ in 0..limit {
            let c = self.next(a, b)?;
            if b == u && c == v {
                return Some(out);
            }
            out.push(b);
            a = b;
            b = c;
        }
        None
    }

    fn polygon(&self, ids: &[usize]) -> Vec<Point> {
        ids.iter().map(|&i| self.pt(i)).collect()
    }

    /// Vertex sets of the connected components (only vertices with edges).
    fn components(&self) -> Vec<Vec<usize>> {
        let mut seen = vec![false; self.pts.len()];
        let mut out = Vec::new();
        for s in 0..self.pts.len() {
            if seen.get(s).copied().unwrap_or(true) || self.adj.get(s).is_none_or(Vec::is_empty) {
                continue;
            }
            let mut comp = Vec::new();
            let mut stack = vec![s];
            if let Some(x) = seen.get_mut(s) {
                *x = true;
            }
            while let Some(v) = stack.pop() {
                comp.push(v);
                for &w in self.adj.get(v).map(Vec::as_slice).unwrap_or(&[]) {
                    if let Some(x) = seen.get_mut(w)
                        && !*x
                    {
                        *x = true;
                        stack.push(w);
                    }
                }
            }
            out.push(comp);
        }
        out
    }

    /// The outer boundary of a component (clockwise), starting at its leftmost vertex.
    fn outer_boundary(&self, comp: &[usize]) -> Option<Vec<usize>> {
        let v0 = *comp.iter().min_by(|a, b| {
            let (p, q) = (self.pt(**a), self.pt(**b));
            p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y))
        })?;
        // the neighbour closest counter-clockwise to straight left: the outer face is on the
        // left of v0 -> w
        let w = *self.adj.get(v0)?.last()?;
        self.face(v0, w)
    }
}

fn signed_area(p: &[Point]) -> f64 {
    let n = p.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        s += a.x * b.y - b.x * a.y;
    }
    s / 2.0
}

/// Drop repeated and collinear vertices.
fn simplify(p: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(p.len());
    for q in p {
        if out.last().is_some_and(|l| l.dist(q) < 1e-9) {
            continue;
        }
        out.push(q);
    }
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| a.dist(*b) < 1e-9) {
        out.pop();
    }
    let mut changed = true;
    while changed && out.len() > 3 {
        changed = false;
        let n = out.len();
        for i in 0..n {
            let (a, b, c) = (out[(i + n - 1) % n], out[i], out[(i + 1) % n]);
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            let scale = a.dist(b).max(b.dist(c)).max(1e-9);
            if cross.abs() / scale < 1e-6 {
                out.remove(i);
                changed = true;
                break;
            }
        }
    }
    out
}

/// Trace the region around `seed` in `segs`. `window`: the box the segments were cut to (its
/// border is part of the linework); the result says whether the region touches it.
fn trace(segs: &[(Point, Point)], seed: Point, opts: &FillOptions) -> Option<(FillRegion, Vec<usize>, Graph)> {
    let g = build_graph(segs, opts.gap.max(1e-6));
    // edges crossing the ray from the seed towards +x, nearest first
    let mut hits: Vec<(f64, usize, usize)> = Vec::new();
    for (u, l) in g.adj.iter().enumerate() {
        for &v in l {
            if v <= u {
                continue;
            }
            let (a, b) = (g.pt(u), g.pt(v));
            if (a.y > seed.y) == (b.y > seed.y) {
                continue;
            }
            let x = a.x + (seed.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x > seed.x {
                // the face containing the seed is left of the upward half-edge
                let (lo, hi) = if a.y < b.y { (u, v) } else { (v, u) };
                hits.push((x, lo, hi));
            }
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    for &(_, lo, hi) in hits.iter().take(MAX_HITS) {
        let Some(ids) = g.face(lo, hi) else { continue };
        let poly = g.polygon(&ids);
        if signed_area(&poly) > 0.0 && point_in_polygon(seed, &poly) {
            let area = signed_area(&poly);
            let region = FillRegion {
                outer: poly,
                holes: Vec::new(),
                area,
                segments: segs.len(),
            };
            return Some((region, ids, g));
        }
    }
    None
}

/// Cutouts: the outer boundaries of components inside `region` that are not part of it and
/// not inside another cutout.
fn cutouts(g: &Graph, face: &[usize], region: &mut FillRegion) {
    let on_face: HashSet<usize> = face.iter().copied().collect();
    let mut holes: Vec<Vec<Point>> = Vec::new();
    let mut comps = g.components();
    // bigger islands first, so islands inside them are skipped
    comps.sort_by_key(|c| std::cmp::Reverse(c.len()));
    for comp in comps {
        if holes.len() >= MAX_HOLES {
            break;
        }
        if comp.iter().any(|v| on_face.contains(v)) {
            continue;
        }
        let Some(&v) = comp.first() else { continue };
        let p = g.pt(v);
        if !point_in_polygon(p, &region.outer) || holes.iter().any(|h| point_in_polygon(p, h)) {
            continue;
        }
        let Some(ids) = g.outer_boundary(&comp) else { continue };
        let h = simplify(g.polygon(&ids));
        if h.len() >= 3 && polygon_area(&h).abs() > 1e-6 {
            holes.push(h);
        }
    }
    // a bigger island processed later may contain one found earlier: keep only top-level ones
    let tops: Vec<Vec<Point>> = holes
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            !holes.iter().enumerate().any(|(j, o)| {
                j != *i
                    && h.first().is_some_and(|p| point_in_polygon(*p, o))
                    && polygon_area(o).abs() > polygon_area(h).abs()
            })
        })
        .map(|(_, h)| h.clone())
        .collect();
    for h in &tops {
        region.area -= polygon_area(h).abs();
    }
    region.holes = tops;
}

/// Find the closed region around `seed` in linework `segs`.
pub fn fill_region(segs: &[(Point, Point)], seed: Point, opts: &FillOptions) -> Result<FillRegion> {
    if !(seed.x.is_finite() && seed.y.is_finite()) {
        return Err(invalid("the seed point must be finite"));
    }
    if !(opts.gap.is_finite() && (0.0..=72.0).contains(&opts.gap)) {
        return Err(invalid("the gap tolerance must be 0 to 72 points"));
    }
    let mut all: Vec<(Point, Point)> = segs.to_vec();
    for b in &opts.boundaries {
        for w in b.windows(2) {
            if let [a, c] = w {
                all.push((*a, *c));
            }
        }
    }
    let Some(full) = markupcraft_geom::bbox(&all.iter().flat_map(|(a, b)| [*a, *b]).collect::<Vec<_>>()) else {
        return Err(invalid("the page has no linework to fill"));
    };
    let mut half = 128.0;
    loop {
        let w = Rect::new(seed.x - half, seed.y - half, seed.x + half, seed.y + half);
        let whole = w.x0 <= full.x0 && w.y0 <= full.y0 && w.x1 >= full.x1 && w.y1 >= full.y1;
        let mut segs: Vec<(Point, Point)> = all
            .iter()
            .filter(|(a, b)| {
                a.x.max(b.x) >= w.x0 && a.x.min(b.x) <= w.x1 && a.y.max(b.y) >= w.y0 && a.y.min(b.y) <= w.y1
            })
            .copied()
            .collect();
        if segs.len() > MAX_FILL_SEGMENTS {
            return Err(invalid(format!(
                "too much linework around the point ({} segments); zoom the fill with boundaries",
                segs.len()
            )));
        }
        if !whole {
            let c = w.corners();
            for i in 0..4 {
                segs.push((c[i], c[(i + 1) % 4]));
            }
        }
        let found = trace(&segs, seed, opts);
        let touches = |r: &FillRegion| {
            r.outer.iter().any(|p| {
                (p.x - w.x0).abs() < 1e-6
                    || (p.x - w.x1).abs() < 1e-6
                    || (p.y - w.y0).abs() < 1e-6
                    || (p.y - w.y1).abs() < 1e-6
            })
        };
        match found {
            Some((mut r, face, g)) if whole || !touches(&r) => {
                if opts.cutouts {
                    cutouts(&g, &face, &mut r);
                }
                r.outer = simplify(r.outer);
                if r.outer.len() < 3 {
                    return Err(invalid("the region around the point is degenerate"));
                }
                return Ok(r);
            }
            _ if whole => {
                return Err(invalid(
                    "no closed region around the point (the linework is open there; raise the gap tolerance or add a boundary)",
                ));
            }
            _ => half *= 4.0,
        }
    }
}

impl Session {
    /// The closed region of page linework around `seed` (page 0-based).
    pub fn dynamic_fill(&self, page: usize, seed: Point, opts: &FillOptions) -> Result<FillRegion> {
        let lw = self.page_linework(page)?;
        fill_region(&lw.segments, seed, opts)
    }

    /// Dynamic Fill and make the result: an Area (with cutouts), a Polygon, a Perimeter or a
    /// Space. `look` gives the new markup's subject, colours and so on (kind, page and points are
    /// set here). Returns the new markup's or space's id. Undoable.
    pub fn dynamic_fill_create(
        &mut self,
        page: usize,
        seed: Point,
        opts: &FillOptions,
        output: &FillOutput,
        look: Option<Markup>,
    ) -> Result<(String, FillRegion)> {
        let r = self.dynamic_fill(page, seed, opts)?;
        let kind = match output {
            FillOutput::Space(name) => {
                let id = self.add_space(page, name, r.outer.clone(), None, None)?;
                return Ok((id, r));
            }
            FillOutput::Area => Kind::Area,
            FillOutput::Polygon => Kind::Polygon,
            FillOutput::Perimeter => Kind::Perimeter,
        };
        let mut m = look.unwrap_or_default();
        m.kind = kind;
        m.page = page;
        m.pts = r.outer.clone();
        m.holes = if kind == Kind::Area {
            r.holes.clone()
        } else {
            Vec::new()
        };
        m.rect = Rect::default();
        if kind == Kind::Area && m.fill.is_none() {
            m.fill = Some(m.color);
            m.fill_opacity = 0.3;
        }
        let id = self.add_markup(m).map_err(|e| match e {
            EngineError::Invalid(s) => invalid(format!("the filled region could not be made a markup: {s}")),
            other => other,
        })?;
        Ok((id, r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf};

    fn segs(lines: &[(f64, f64, f64, f64)]) -> Vec<(Point, Point)> {
        lines
            .iter()
            .map(|&(a, b, c, d)| (Point::new(a, b), Point::new(c, d)))
            .collect()
    }

    /// Two rooms side by side sharing a wall, with a stub wall and a column in the left one.
    fn plan() -> Vec<(Point, Point)> {
        segs(&[
            (0.0, 0.0, 200.0, 0.0),
            (200.0, 0.0, 200.0, 100.0),
            (200.0, 100.0, 0.0, 100.0),
            (0.0, 100.0, 0.0, 0.0),
            (100.0, -10.0, 100.0, 110.0), // shared wall overshooting both ends
            (20.0, 50.0, 40.0, 50.0),     // dangling stub
            (60.0, 20.0, 70.0, 20.0),     // column
            (70.0, 20.0, 70.0, 30.0),
            (70.0, 30.0, 60.0, 30.0),
            (60.0, 30.0, 60.0, 20.0),
        ])
    }

    #[test]
    fn fill_finds_the_room_and_its_column() {
        let r = fill_region(&plan(), Point::new(30.0, 30.0), &FillOptions::default()).unwrap();
        assert_eq!(r.outer.len(), 4, "{:?}", r.outer);
        assert!((r.area - (100.0 * 100.0 - 100.0)).abs() < 1e-6, "{}", r.area);
        assert_eq!(r.holes.len(), 1);
        let right = fill_region(&plan(), Point::new(150.0, 50.0), &FillOptions::default()).unwrap();
        assert!((right.area - 10_000.0).abs() < 1e-6);
        let no_cut = FillOptions {
            cutouts: false,
            ..Default::default()
        };
        let r = fill_region(&plan(), Point::new(30.0, 30.0), &no_cut).unwrap();
        assert!((r.area - 10_000.0).abs() < 1e-6);
        // inside the column
        let col = fill_region(&plan(), Point::new(65.0, 25.0), &FillOptions::default()).unwrap();
        assert!((col.area - 100.0).abs() < 1e-6);
    }

    #[test]
    fn gaps_close_within_tolerance_and_open_rooms_fail() {
        // the right wall stops 2 points short of the top
        let open = segs(&[
            (0.0, 0.0, 100.0, 0.0),
            (100.0, 0.0, 100.0, 98.0),
            (100.0, 100.0, 0.0, 100.0),
            (0.0, 100.0, 0.0, 0.0),
        ]);
        let seed = Point::new(50.0, 50.0);
        assert!(fill_region(&open, seed, &FillOptions::default()).is_err());
        let wide = FillOptions {
            gap: 3.0,
            ..Default::default()
        };
        let r = fill_region(&open, seed, &wide).unwrap();
        assert!((r.area - 10_000.0).abs() < 150.0, "{}", r.area);
        // or close it with a boundary line
        let b = FillOptions {
            boundaries: vec![vec![Point::new(100.0, 90.0), Point::new(100.0, 110.0)]],
            ..Default::default()
        };
        assert!(fill_region(&open, seed, &b).is_ok());
    }

    #[test]
    fn big_rooms_grow_the_window() {
        let big = segs(&[
            (0.0, 0.0, 3000.0, 0.0),
            (3000.0, 0.0, 3000.0, 2000.0),
            (3000.0, 2000.0, 0.0, 2000.0),
            (0.0, 2000.0, 0.0, 0.0),
        ]);
        let r = fill_region(&big, Point::new(10.0, 10.0), &FillOptions::default()).unwrap();
        assert!((r.area - 6_000_000.0).abs() < 1e-3);
    }

    #[test]
    fn dynamic_fill_makes_an_area_with_a_cutout() {
        let mut c = String::new();
        for (a, b) in plan() {
            c.push_str(&line(a.x + 100.0, a.y + 100.0, b.x + 100.0, b.y + 100.0, 1.0));
        }
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, c)]), "fill.pdf").unwrap();
        let (id, r) = s
            .dynamic_fill_create(
                0,
                Point::new(130.0, 130.0),
                &FillOptions::default(),
                &FillOutput::Area,
                None,
            )
            .unwrap();
        let m = s.markup(&id).unwrap();
        assert_eq!(m.kind, Kind::Area);
        assert_eq!(m.holes.len(), 1);
        assert!((r.area - 9_900.0).abs() < 1e-6);
        let (sid, _) = s
            .dynamic_fill_create(
                0,
                Point::new(250.0, 150.0),
                &FillOptions::default(),
                &FillOutput::Space("Room 2".into()),
                None,
            )
            .unwrap();
        assert_eq!(s.spaces(Some(0)).first().map(|(_, sp)| sp.id.clone()), Some(sid));
    }
}
