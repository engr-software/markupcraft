//! Snapping while drawing and reshaping: Snap to Content (the page's vector linework, indexed
//! per page on a worker thread), Snap to Markup (vertices, segment midpoints and centres of the
//! page's markups) and Snap to Grid, with Revu-style indicator glyphs at the snapped point.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, Painter, Pos2, Stroke, pos2, vec2};
use markupcraft_geom::snap::{SnapIndex, SnapKind};
use markupcraft_geom::{Point, dist_to_segment};
use markupcraft_model::{Kind, Markup};

use crate::Snaps;
use crate::actions::{box_of, uses_rect};

/// Grid spacing in PDF points (a quarter inch).
pub const GRID: f64 = 18.0;
/// How far the pointer reaches for a snap, in screen points.
pub const REACH: f32 = 10.0;

/// What a snapped point is, for its indicator glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Endpoint,
    Midpoint,
    Intersection,
    Nearest,
    Center,
    Grid,
}

impl Glyph {
    fn of(k: SnapKind) -> Glyph {
        match k {
            SnapKind::Endpoint => Glyph::Endpoint,
            SnapKind::Intersection => Glyph::Intersection,
            SnapKind::Midpoint => Glyph::Midpoint,
            SnapKind::Nearest => Glyph::Nearest,
            SnapKind::Center => Glyph::Center,
        }
    }

    /// Lower is preferred.
    fn rank(self) -> u8 {
        match self {
            Glyph::Endpoint => 0,
            Glyph::Intersection => 1,
            Glyph::Center => 2,
            Glyph::Midpoint => 3,
            Glyph::Nearest => 4,
            Glyph::Grid => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Glyph::Endpoint => "Endpoint",
            Glyph::Midpoint => "Midpoint",
            Glyph::Intersection => "Intersection",
            Glyph::Nearest => "Nearest",
            Glyph::Center => "Center",
            Glyph::Grid => "Grid",
        }
    }
}

/// A point after snapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapped {
    pub pt: Point,
    pub glyph: Option<Glyph>,
}

impl Snapped {
    pub fn raw(pt: Point) -> Self {
        Self { pt, glyph: None }
    }
}

enum Slot {
    Building,
    Ready(Arc<SnapIndex>),
}

/// Per-document cache of page linework indexes.
pub struct SnapCache {
    pages: HashMap<usize, Slot>,
    /// bumped when the document's bytes change, so stale builds are dropped
    generation: u64,
    tx: Sender<(u64, usize, SnapIndex)>,
    rx: Receiver<(u64, usize, SnapIndex)>,
    /// build on the calling thread (tests, headless runs without workers)
    pub inline: bool,
}

impl Default for SnapCache {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            pages: HashMap::new(),
            generation: 0,
            tx,
            rx,
            inline: false,
        }
    }
}

impl SnapCache {
    /// A cache that builds on worker threads, or on the calling thread when `inline`.
    pub fn new(inline: bool) -> Self {
        Self {
            inline,
            ..Default::default()
        }
    }

    /// Forget every index (the document's pages changed).
    pub fn invalidate(&mut self) {
        self.pages.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Whether a page is still being indexed.
    pub fn building(&self) -> bool {
        self.pages.values().any(|s| matches!(s, Slot::Building))
    }

    /// The index of `page`, starting its build when there is none. `None` while it builds.
    pub fn index(&mut self, page: usize, bytes: &Arc<Vec<u8>>) -> Option<Arc<SnapIndex>> {
        while let Ok((g, p, idx)) = self.rx.try_recv() {
            if g == self.generation {
                self.pages.insert(p, Slot::Ready(Arc::new(idx)));
            }
        }
        match self.pages.get(&page) {
            Some(Slot::Ready(i)) => return Some(i.clone()),
            Some(Slot::Building) => return None,
            None => {}
        }
        if self.inline {
            let idx = Arc::new(build(bytes.clone(), page));
            self.pages.insert(page, Slot::Ready(idx.clone()));
            return Some(idx);
        }
        self.pages.insert(page, Slot::Building);
        let bytes_for_inline = bytes.clone();
        let (tx, g, bytes) = (self.tx.clone(), self.generation, bytes.clone());
        let spawned = std::thread::Builder::new()
            .name("markupcraft-snap".into())
            .spawn(move || {
                let _ = tx.send((g, page, build(bytes, page)));
            });
        if spawned.is_err() {
            // No threads (the browser build): index here from now on.
            self.inline = true;
            let idx = Arc::new(build(bytes_for_inline, page));
            self.pages.insert(page, Slot::Ready(idx.clone()));
            return Some(idx);
        }
        None
    }
}

fn build(bytes: Arc<Vec<u8>>, page: usize) -> SnapIndex {
    SnapIndex::new(markupcraft_render::snap::linework_of(bytes, page).segments)
}

/// Snap `raw` (PDF user space) with the toggles in `snaps`. `reach` is the snap distance in PDF
/// units; `markups` are the page's markups (minus the one being drawn or reshaped).
pub fn snap_point(raw: Point, snaps: Snaps, reach: f64, content: Option<&SnapIndex>, markups: &[&Markup]) -> Snapped {
    let mut best: Option<(Glyph, f64, Point)> = None;
    let mut offer = |g: Glyph, p: Point| {
        let d = p.dist(raw);
        if d > reach || !d.is_finite() {
            return;
        }
        let better = match best {
            None => true,
            Some((bg, bd, _)) => g.rank() < bg.rank() || (g.rank() == bg.rank() && d < bd),
        };
        if better {
            best = Some((g, d, p));
        }
    };
    if snaps.content
        && let Some(idx) = content
        && let Some(h) = idx.query(raw, reach)
    {
        offer(Glyph::of(h.kind), h.pt);
    }
    if snaps.markup {
        for m in markups {
            markup_snaps(m, raw, reach, &mut offer);
        }
    }
    if let Some((g, _, p)) = best {
        return Snapped { pt: p, glyph: Some(g) };
    }
    if snaps.grid {
        let g = Point::new((raw.x / GRID).round() * GRID, (raw.y / GRID).round() * GRID);
        return Snapped {
            pt: g,
            glyph: Some(Glyph::Grid),
        };
    }
    Snapped::raw(raw)
}

/// Snap candidates of one markup: vertices, segment midpoints, the nearest point on its
/// outline, and the centre of box shapes.
fn markup_snaps(m: &Markup, raw: Point, reach: f64, offer: &mut impl FnMut(Glyph, Point)) {
    let ring: Vec<Point> = if uses_rect(m.kind) {
        let b = box_of(m);
        offer(Glyph::Center, Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0));
        let mut c = b.corners().to_vec();
        if let Some(f) = c.first().copied() {
            c.push(f);
        }
        c
    } else {
        let mut v: Vec<Point> = m.pts.iter().take(20_000).copied().collect();
        if matches!(
            m.kind,
            Kind::Area | Kind::Perimeter | Kind::Polygon | Kind::Cloud | Kind::Volume
        ) && let Some(f) = v.first().copied()
        {
            v.push(f);
        }
        v
    };
    if m.kind == Kind::Count || ring.len() == 1 {
        for p in &ring {
            offer(Glyph::Endpoint, *p);
        }
        return;
    }
    for w in ring.windows(2) {
        let (a, b) = (w[0], w[1]);
        if dist_to_segment(raw, a, b) > reach {
            continue;
        }
        offer(Glyph::Endpoint, a);
        offer(Glyph::Endpoint, b);
        offer(Glyph::Midpoint, a.mid(b));
        offer(Glyph::Nearest, nearest_on(raw, a, b));
    }
}

fn nearest_on(p: Point, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Point::new(a.x + t * dx, a.y + t * dy)
}

/// Draw the indicator for a snap at `at` (screen).
pub fn paint_glyph(p: &Painter, at: Pos2, g: Glyph) {
    let col = Color32::from_rgb(0xE0, 0x10, 0xC0);
    let st = Stroke::new(1.6, col);
    let r = 6.0;
    match g {
        Glyph::Endpoint => {
            p.rect_stroke(
                egui::Rect::from_center_size(at, vec2(2.0 * r, 2.0 * r)),
                0.0,
                st,
                egui::StrokeKind::Middle,
            );
        }
        Glyph::Midpoint => {
            p.add(egui::Shape::closed_line(
                vec![at + vec2(0.0, -r), at + vec2(r, r * 0.8), at + vec2(-r, r * 0.8)],
                st,
            ));
        }
        Glyph::Intersection => {
            p.line_segment([at + vec2(-r, -r), at + vec2(r, r)], st);
            p.line_segment([at + vec2(-r, r), at + vec2(r, -r)], st);
        }
        Glyph::Nearest => {
            p.add(egui::Shape::closed_line(
                vec![at + vec2(-r, -r), at + vec2(r, -r), at + vec2(-r, r), at + vec2(r, r)],
                st,
            ));
        }
        Glyph::Center => {
            p.circle_stroke(at, r, st);
            p.circle_filled(at, 1.5, col);
        }
        Glyph::Grid => {
            p.line_segment([at + vec2(-r, 0.0), at + vec2(r, 0.0)], st);
            p.line_segment([at + vec2(0.0, -r), at + vec2(0.0, r)], st);
        }
    }
}

/// Grid lines over a page (screen rect `page`, `k` screen points per PDF point), when the
/// spacing is large enough to read.
pub fn paint_grid(p: &Painter, page: egui::Rect, k: f32) {
    let step = GRID as f32 * k;
    if step < 6.0 {
        return;
    }
    let col = Color32::from_rgba_unmultiplied(0x40, 0x80, 0xE0, 40);
    let clip = p.clip_rect().intersect(page);
    let mut x = page.left() + ((clip.left() - page.left()) / step).ceil() * step;
    let mut n = 0;
    while x <= clip.right() && n < 4000 {
        p.line_segment([pos2(x, clip.top()), pos2(x, clip.bottom())], Stroke::new(1.0, col));
        x += step;
        n += 1;
    }
    let mut y = page.top() + ((clip.top() - page.top()) / step).ceil() * step;
    while y <= clip.bottom() && n < 8000 {
        p.line_segment([pos2(clip.left(), y), pos2(clip.right(), y)], Stroke::new(1.0, col));
        y += step;
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_geom::snap::SnapSegment;

    fn snaps(grid: bool, content: bool, markup: bool) -> Snaps {
        Snaps { grid, content, markup }
    }

    #[test]
    fn content_markup_and_grid() {
        let idx = SnapIndex::new(vec![SnapSegment::new(Point::new(0.0, 0.0), Point::new(100.0, 0.0))]);
        let raw = Point::new(98.0, 3.0);
        let s = snap_point(raw, snaps(false, true, false), 6.0, Some(&idx), &[]);
        assert_eq!(s.glyph, Some(Glyph::Endpoint));
        assert_eq!(s.pt, Point::new(100.0, 0.0));
        // Off: the raw point.
        assert_eq!(
            snap_point(raw, snaps(false, false, false), 6.0, Some(&idx), &[]).glyph,
            None
        );

        let line = Markup::new(Kind::Line, 0, vec![Point::new(200.0, 0.0), Point::new(300.0, 0.0)]);
        let s = snap_point(Point::new(251.0, 2.0), snaps(false, false, true), 6.0, None, &[&line]);
        assert_eq!(s.glyph, Some(Glyph::Midpoint));
        assert_eq!(s.pt, Point::new(250.0, 0.0));

        let s = snap_point(
            Point::new(40.0, 40.0),
            snaps(true, true, true),
            6.0,
            Some(&idx),
            &[&line],
        );
        assert_eq!(s.glyph, Some(Glyph::Grid));
        assert_eq!(s.pt, Point::new(36.0, 36.0));
    }

    #[test]
    fn the_cache_indexes_the_sample_page() {
        let bytes = Arc::new(markupcraft_render::synthetic::sample_pdf());
        let mut c = SnapCache {
            inline: true,
            ..Default::default()
        };
        let idx = c.index(0, &bytes).unwrap();
        assert!(!idx.is_empty());
        c.invalidate();
        assert!(!c.building());
    }
}
