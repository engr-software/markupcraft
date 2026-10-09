//! Where a measurement's value caption sits, and Revu's `/CO` caption offset.
//!
//! Area / Perimeter: the vertex mean (Revu anchors there; `/CO` is an offset from it, seen on
//! 37 captions of the reference set). Length: `/CO` is `[along, perpendicular]` to the line
//! (ISO 32000-1 §12.5.6.7). With Show Segment Values the running total sits beside the last
//! segment, on the side away from its segment value; with arcs, at the middle control segment.
//! Angle: inside the angle on its bisector. Others: the middle of the middle segment.

use markupcraft_geom::{Point, vertex_mean};

use crate::measure_extras::{arc_handle, control_segments};
use crate::{Kind, Markup};

/// How far the Angle caption sits from the vertex, and the total from its segment (PDF units).
const ANGLE_CAPTION: f64 = 20.0;
const TOTAL_OFFSET: f64 = 10.0;

/// The caption anchor before the user moved it.
pub fn default_caption_anchor(m: &Markup) -> Point {
    let Some(first) = m.pts.first() else {
        return Point::default();
    };
    if matches!(m.kind, Kind::Area | Kind::Perimeter | Kind::Volume) {
        return vertex_mean(&m.pts);
    }
    if m.kind == Kind::Angle
        && let [a, v, b, ..] = m.pts.as_slice()
    {
        let unit = |p: &Point| {
            let l = p.dist(*v);
            if l > 0.0 {
                Point::new((p.x - v.x) / l, (p.y - v.y) / l)
            } else {
                Point::default()
            }
        };
        let (ua, ub) = (unit(a), unit(b));
        let (bx, by) = (ua.x + ub.x, ua.y + ub.y);
        let l = bx.hypot(by);
        let dir = if l > 1e-9 {
            Point::new(bx / l, by / l)
        } else {
            Point::new(-ua.y, ua.x)
        };
        return Point::new(v.x + dir.x * ANGLE_CAPTION, v.y + dir.y * ANGLE_CAPTION);
    }
    if m.segment_values || !m.arcs.is_empty() {
        let segs = control_segments(m);
        let seg = if m.segment_values {
            segs.last()
        } else {
            segs.get(segs.len() / 2)
        };
        if let Some(s) = seg
            && let (Some(a), Some(b)) = (
                s.path.first().and_then(|&i| m.pts.get(i)),
                s.path.last().and_then(|&i| m.pts.get(i)),
            )
        {
            let at = s
                .arc
                .and_then(|j| arc_handle(m, j))
                .and_then(|h| m.pts.get(h).copied())
                .unwrap_or_else(|| a.mid(*b));
            if !m.segment_values {
                return at;
            }
            let l = a.dist(*b);
            let mut n = if l > 0.0 {
                Point::new(-(b.y - a.y) / l, (b.x - a.x) / l)
            } else {
                Point::new(0.0, 1.0)
            };
            // segment values are drawn on the upper side
            if n.y > 0.0 || (n.y == 0.0 && n.x > 0.0) {
                n = Point::new(-n.x, -n.y);
            }
            return Point::new(at.x + n.x * TOTAL_OFFSET, at.y + n.y * TOTAL_OFFSET);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn caption_anchors() {
        let mut a = Markup {
            kind: Kind::Area,
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)],
            ..Default::default()
        };
        assert_eq!(default_caption_anchor(&a), p(50.0, 25.0));
        a.caption_offset = Some(p(10.0, -5.0));
        assert_eq!(caption_anchor(&a), p(60.0, 20.0));

        let l = Markup {
            kind: Kind::Length,
            pts: vec![p(100.0, 0.0), p(0.0, 0.0)],
            ..Default::default()
        };
        let o = line_co_to_offset(&l, 10.0, 5.0);
        assert!((o.x + 10.0).abs() < 1e-9 && (o.y + 5.0).abs() < 1e-9);
        let (along, perp) = offset_to_line_co(&l, o);
        assert!((along - 10.0).abs() < 1e-9 && (perp - 5.0).abs() < 1e-9);

        // With Show Segment Values the total sits under the last segment.
        let mut pl = Markup {
            kind: Kind::Polylength,
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)],
            segment_values: true,
            ..Default::default()
        };
        assert_eq!(default_caption_anchor(&pl), p(150.0, 90.0));
        pl.segment_values = false;
        assert_eq!(default_caption_anchor(&pl), p(100.0, 50.0));
        // With an arc: the middle control segment's handle.
        crate::measure_extras::convert_to_arc(&mut pl, 1, 0.0);
        let h = default_caption_anchor(&pl);
        assert!((h.y - 50.0).abs() < 1e-9 && (h.x - 75.0).abs() < 1e-9, "{h:?}");

        let ang = Markup {
            kind: Kind::Angle,
            pts: vec![p(10.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)],
            ..Default::default()
        };
        let c = default_caption_anchor(&ang);
        assert!((c.x - c.y).abs() < 1e-9 && (c.x - 20.0 / 2f64.sqrt()).abs() < 1e-9);
    }
}
