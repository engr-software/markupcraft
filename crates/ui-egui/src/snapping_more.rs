//! Alignment guides of Snap to Markup: while drawing, when the pointer lines up (horizontally or
//! vertically, within the snap reach) with a vertex of another markup, a dashed guide joins
//! them and the point takes the aligned coordinate.

use egui::{Color32, Painter, Stroke};
use markupcraft_geom::Point;
use markupcraft_model::Markup;

use crate::painter::Xf;

/// The guides at a point: the vertex it lines up with vertically (same x) and horizontally
/// (same y), and the point moved onto them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Guides {
    pub page: usize,
    pub vertical: Option<Point>,
    pub horizontal: Option<Point>,
    pub at: Point,
}

/// Find the nearest aligned vertices (other than ones the pointer is on) within `reach`.
pub fn find(page: usize, raw: Point, reach: f64, markups: &[&Markup]) -> Option<Guides> {
    let mut bx: Option<(f64, Point)> = None;
    let mut by: Option<(f64, Point)> = None;
    for m in markups.iter().take(20_000) {
        for v in m.pts.iter().take(5_000) {
            if v.dist(raw) <= reach {
                continue;
            }
            let dx = (v.x - raw.x).abs();
            if dx <= reach && bx.is_none_or(|(d, _)| dx < d) {
                bx = Some((dx, *v));
            }
            let dy = (v.y - raw.y).abs();
            if dy <= reach && by.is_none_or(|(d, _)| dy < d) {
                by = Some((dy, *v));
            }
        }
    }
    if bx.is_none() && by.is_none() {
        return None;
    }
    let at = Point::new(bx.map_or(raw.x, |(_, v)| v.x), by.map_or(raw.y, |(_, v)| v.y));
    Some(Guides {
        page,
        vertical: bx.map(|(_, v)| v),
        horizontal: by.map(|(_, v)| v),
        at,
    })
}

fn id() -> egui::Id {
    egui::Id::new("alignment-guides")
}

pub fn store(ctx: &egui::Context, g: Option<Guides>) {
    ctx.data_mut(|d| match g {
        Some(g) => {
            d.insert_temp(id(), g);
        }
        None => d.remove::<Guides>(id()),
    });
}

pub fn current(ctx: &egui::Context) -> Option<Guides> {
    ctx.data(|d| d.get_temp::<Guides>(id()))
}

/// Draw the guides of `page`.
pub fn paint(p: &Painter, xf: &Xf, page: usize, col: Color32) {
    let Some(g) = current(p.ctx()) else { return };
    if g.page != page {
        return;
    }
    let to = xf.to_screen(g.at);
    let stroke = Stroke::new(1.0, col);
    for v in [g.vertical, g.horizontal].into_iter().flatten() {
        let from = xf.to_screen(v);
        p.extend(egui::Shape::dashed_line(&[from, to], stroke, 5.0, 4.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::Kind;

    #[test]
    fn lines_up_with_vertices() {
        let m = Markup::new(
            Kind::Rectangle,
            0,
            vec![Point::new(100.0, 100.0), Point::new(200.0, 200.0)],
        );
        let g = find(0, Point::new(102.0, 400.0), 5.0, &[&m]).unwrap();
        assert_eq!(g.vertical, Some(Point::new(100.0, 100.0)));
        assert_eq!(g.at, Point::new(100.0, 400.0));
        assert!(find(0, Point::new(150.0, 400.0), 5.0, &[&m]).is_none());
        let g = find(0, Point::new(300.0, 198.0), 5.0, &[&m]).unwrap();
        assert_eq!(g.at, Point::new(300.0, 200.0));
    }
}
