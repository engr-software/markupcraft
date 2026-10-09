//! Where a measurement's value caption sits, and Revu's `/CO` caption offset.
//!
//! Area / Perimeter: the vertex mean (Revu anchors there; `/CO` is an offset from it, seen on
//! 37 captions of the reference set). Length: `/CO` is `[along, perpendicular]` to the line
//! (ISO 32000-1 §12.5.6.7). Others: the middle of the middle segment.

use markupcraft_geom::{Point, vertex_mean};

use crate::{Kind, Markup};

/// The caption anchor before the user moved it.
pub fn default_caption_anchor(m: &Markup) -> Point {
    let Some(first) = m.pts.first() else {
        return Point::default();
    };
    if matches!(m.kind, Kind::Area | Kind::Perimeter | Kind::Volume) {
        return vertex_mean(&m.pts);
    }
    if m.pts.len() >= 2 {
        let k = m.pts.len() / 2;
        if let (Some(a), Some(b)) = (m.pts.get(k - 1), m.pts.get(k)) {
            return a.mid(*b);
        }
    }
    *first
}

/// Where the caption is drawn: the default anchor plus the user's offset.
pub fn caption_anchor(m: &Markup) -> Point {
    let a = default_caption_anchor(m);
    match m.caption_offset {
        Some(o) => a + o,
        None => a,
    }
}

fn line_frame(m: &Markup) -> Option<(Point, Point)> {
    let (a, b) = (*m.pts.first()?, *m.pts.last()?);
    let l = a.dist(b);
    if l <= 0.0 {
        return None;
    }
    let u = Point::new((b.x - a.x) / l, (b.y - a.y) / l);
    Some((u, Point::new(-u.y, u.x)))
}

/// A Length's `/CO [along perp]` to a page-space offset.
pub fn line_co_to_offset(m: &Markup, along: f64, perp: f64) -> Point {
    match line_frame(m) {
        Some((u, n)) => Point::new(u.x * along + n.x * perp, u.y * along + n.y * perp),
        None => Point::new(along, perp),
    }
}

/// A page-space offset to a Length's `/CO [along perp]`.
pub fn offset_to_line_co(m: &Markup, o: Point) -> (f64, f64) {
    match line_frame(m) {
        Some((u, n)) => (o.x * u.x + o.y * u.y, o.x * n.x + o.y * n.y),
        None => (o.x, o.y),
    }
}

/// Kinds whose moved caption is stored in Revu's `/CO`.
pub fn uses_co(k: Kind) -> bool {
    matches!(k, Kind::Area | Kind::Perimeter | Kind::Length)
}
