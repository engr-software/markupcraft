//! Modifier keys while drawing and editing (Revu's conventions):
//! - Alt while dragging a box tool (rectangle, ellipse...): draw from the centre;
//! - Alt+click while drawing a polyline, polygon or measurement: an arc passes through the
//!   point to the next click (a three-point arc; `engine::extras6::arcs_through`);
//! - Shift+click a vertex of the selected markup: delete it; Shift+click a segment: add one;
//! - Ctrl+click a vertex: the segment it starts becomes an arc, or an arc's handle goes back
//!   to straight (Ctrl+drag a segment bends it, `more::ctrl_curve`);
//! - resizing an image or stamp from a corner keeps its aspect ratio; Shift breaks it;
//! - Alt+drag a callout's handle: the callout moves as a whole.

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup, measure_extras};

use crate::DocTab;
use crate::actions;

/// Alt: `a` is the centre, so the box runs from the mirror of `b` to `b`.
pub fn from_center(alt: bool, a: Point, b: Point) -> (Point, Point) {
    if alt {
        (Point::new(2.0 * a.x - b.x, 2.0 * a.y - b.y), b)
    } else {
        (a, b)
    }
}

/// Shift+click (`shift`) or Ctrl+click on the selected markup's vertex or segment at `at`
/// (within `reach` PDF points). Returns the status when it changed the markup.
pub fn vertex_click(doc: &mut DocTab, page: usize, at: Point, reach: f64, shift: bool) -> Option<String> {
    let [id] = doc.selection() else { return None };
    let m = doc.session.doc().find(id)?.clone();
    if m.page != page || actions::uses_rect(m.kind) || m.locked() || !markupcraft_engine::props::geometry_editable(&m) {
        return None;
    }
    let vertex = m
        .pts
        .iter()
        .enumerate()
        .filter(|(i, _)| measure_extras::is_editable_vertex(&m, *i))
        .map(|(i, p)| (i, p.dist(at)))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i);
    let closed = measure_extras::closed_shape(m.kind) || matches!(m.kind, Kind::Polygon | Kind::Cloud);
    if shift {
        if let Some(i) = vertex.filter(|i| measure_extras::arc_containing(&m, *i).is_none()) {
            let r = doc.session.delete_vertex(&m.id, i);
            return Some(actions::report(r, |_| "Vertex deleted".into()));
        }
        if vertex.is_none() && measure_extras::can_add_vertices(m.kind) {
            let (after, p) = measure_extras::nearest_segment(&m.pts, closed, at, reach)?;
            let r = doc.session.insert_vertex(&m.id, after + 1, p);
            return Some(actions::report(r, |_| "Vertex added".into()));
        }
        return None;
    }
    let i = vertex?;
    if !measure_extras::can_have_arcs(m.kind) {
        return None;
    }
    if let Some(a) = measure_extras::arc_containing(&m, i) {
        let r = doc.session.straighten_arc(&m.id, a);
        return Some(actions::report(r, |_| "Arc straightened".into()));
    }
    let r = doc
        .session
        .convert_segment_to_arc(&m.id, i)
        .or_else(|e| match i.checked_sub(1) {
            Some(prev) => doc.session.convert_segment_to_arc(&m.id, prev),
            None => Err(e),
        });
    Some(actions::report(r, |_| "Segment curved into an arc".into()))
}

/// Markups that keep their aspect ratio when resized from a corner (images and stamps).
pub fn keeps_aspect(m: &Markup) -> bool {
    m.kind == Kind::Stamp
}

/// `new` (a corner-resized copy of a markup whose points were `old`) adjusted so the box keeps
/// `old`'s aspect ratio, anchored at the corner that did not move.
pub fn keep_aspect(old: &[Point], new: &[Point]) -> Vec<Point> {
    let (Some(ob), Some(nb)) = (bbox(old), bbox(new)) else {
        return new.to_vec();
    };
    let (ow, oh) = (ob.width(), ob.height());
    if ow <= 1e-9 || oh <= 1e-9 || nb.width() <= 1e-9 || nb.height() <= 1e-9 {
        return new.to_vec();
    }
    let k = (nb.width() / ow).max(nb.height() / oh);
    let (w, h) = (ow * k, oh * k);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    // The fixed corner: the one both boxes share.
    let left = close(nb.x0, ob.x0) || !close(nb.x1, ob.x1);
    let bottom = close(nb.y0, ob.y0) || !close(nb.y1, ob.y1);
    let x0 = if left { nb.x0 } else { nb.x1 - w };
    let y0 = if bottom { nb.y0 } else { nb.y1 - h };
    let to = Rect::new(x0, y0, x0 + w, y0 + h);
    let (nw, nh) = (nb.width(), nb.height());
    new.iter()
        .map(|p| {
            Point::new(
                to.x0 + (p.x - nb.x0) / nw * to.width(),
                to.y0 + (p.y - nb.y0) / nh * to.height(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_draws_from_the_centre() {
        let (a, b) = from_center(true, Point::new(10.0, 10.0), Point::new(15.0, 20.0));
        assert_eq!((a, b), (Point::new(5.0, 0.0), Point::new(15.0, 20.0)));
        let (a, _) = from_center(false, Point::new(10.0, 10.0), Point::new(15.0, 20.0));
        assert_eq!(a, Point::new(10.0, 10.0));
    }

    #[test]
    fn corner_resizes_keep_the_aspect_ratio() {
        let old = Rect::new(0.0, 0.0, 40.0, 20.0).corners().to_vec();
        // Dragged the top-right corner to (100, 25): 2.5x wide, 1.25x high -> 2.5x both.
        let new = Rect::new(0.0, 0.0, 100.0, 25.0).corners().to_vec();
        let kept = keep_aspect(&old, &new);
        let b = bbox(&kept).unwrap();
        assert!(
            (b.width() - 100.0).abs() < 1e-9 && (b.height() - 50.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!((b.x0).abs() < 1e-9 && (b.y0).abs() < 1e-9);
        // Dragged the bottom-left corner: anchored at the top right.
        let new = Rect::new(-40.0, -10.0, 40.0, 20.0).corners().to_vec();
        let b = bbox(&keep_aspect(&old, &new)).unwrap();
        assert!((b.x1 - 40.0).abs() < 1e-9 && (b.y1 - 20.0).abs() < 1e-9, "{b:?}");
        assert!((b.width() / b.height() - 2.0).abs() < 1e-9);
    }
}
