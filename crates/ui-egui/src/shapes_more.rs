//! Drawing the Dimension and Flag markups and the centroid mark on the canvas (the PDF
//! appearances are written by `markupcraft_revu::kinds::more`).

use egui::{Align2, Color32, FontId, Painter, Pos2, Stroke, vec2};
use markupcraft_geom::Point;
use markupcraft_model::Markup;

use crate::actions::box_of;
use crate::painter::{Xf, line_endings, stroke_path};
use crate::theme::color32;

/// The Note icon name the Flag tool places.
pub const FLAG_ICON: &str = "Flag";

/// A Dimension: extension lines, the dimension line with its arrowheads, and its text.
pub fn paint_dimension(p: &Painter, xf: &Xf, m: &Markup, stroke: Stroke) {
    let Some((a, b, ea, eb)) = markupcraft_revu::kinds::more::dimension_lines(m) else {
        let pts: Vec<Pos2> = m.pts.iter().map(|q| xf.to_screen(*q)).collect();
        stroke_path(p, &pts, false, stroke, &m.dash, xf);
        return;
    };
    let s = |q: Point| xf.to_screen(q);
    if m.leader != 0.0 {
        let thin = Stroke::new((stroke.width * 0.75).max(1.0), stroke.color);
        p.line_segment([s(ea[0]), s(ea[1])], thin);
        p.line_segment([s(eb[0]), s(eb[1])], thin);
    }
    let line = [s(a), s(b)];
    stroke_path(p, &line, false, stroke, &m.dash, xf);
    line_endings(p, &line, m, stroke);
    let text = m.contents.lines().next().unwrap_or_default();
    let size = xf.len(m.text.size.clamp(2.0, 144.0));
    if text.is_empty() || size < 4.0 {
        return;
    }
    let col = color32(&m.text.color, m.opacity);
    let mid = line[0].lerp(line[1], 0.5);
    let d = (line[1] - line[0]).normalized();
    // above the line (screen y grows downwards), upright
    let mut nrm = vec2(d.y, -d.x);
    if nrm.y > 0.0 {
        nrm = -nrm;
    }
    let galley = p.layout_no_wrap(text.to_string(), FontId::proportional(size), col);
    let at = mid + nrm * (galley.size().y / 2.0 + 2.0);
    let r = Align2::CENTER_CENTER.anchor_size(at, galley.size());
    p.rect_filled(r.expand(1.0), 1.0, Color32::from_white_alpha(160));
    p.galley(r.min, galley, col);
}

/// A small circled cross at an area's centroid.
pub fn paint_centroid(p: &Painter, at: Pos2, col: Color32) {
    let st = Stroke::new(1.2, col);
    p.line_segment([at - vec2(6.0, 0.0), at + vec2(6.0, 0.0)], st);
    p.line_segment([at - vec2(0.0, 6.0), at + vec2(0.0, 6.0)], st);
    p.circle_stroke(at, 3.5, st);
}

/// A File Attachment: its icon (a push pin by default) in the markup's colour.
pub fn paint_attachment(p: &Painter, xf: &Xf, m: &Markup) {
    let b = box_of(m);
    let r = xf.rect_of(b);
    let col = color32(&m.color, m.opacity);
    let ink = Stroke::new((r.width() / 12.0).max(1.0), Color32::from_gray(40));
    let at = |u: f32, v: f32| egui::pos2(r.left() + r.width() * u, r.bottom() - r.height() * v);
    match m.icon.as_str() {
        "Paperclip" => {
            let pts: Vec<Pos2> = [
                (0.42, 0.3),
                (0.42, 0.78),
                (0.52, 0.9),
                (0.62, 0.78),
                (0.62, 0.2),
                (0.47, 0.06),
                (0.32, 0.2),
                (0.32, 0.86),
            ]
            .iter()
            .map(|(u, v)| at(*u, *v))
            .collect();
            p.add(egui::Shape::line(pts, Stroke::new(ink.width, col)));
        }
        "Graph" | "Tag" => {
            p.rect_filled(r.shrink(r.width() * 0.08), 2.0, col);
            p.rect_stroke(r.shrink(r.width() * 0.08), 2.0, ink, egui::StrokeKind::Inside);
        }
        _ => {
            p.line_segment([at(0.5, 0.5), at(0.24, 0.06)], ink);
            p.circle_filled(at(0.58, 0.68), r.width() * 0.26, col);
            p.circle_stroke(at(0.58, 0.68), r.width() * 0.26, ink);
        }
    }
}

/// A Flag: a pennant on a pole at the note's corner.
pub fn paint_flag(p: &Painter, xf: &Xf, m: &Markup) {
    let b = box_of(m);
    let s = xf.len(20.0).clamp(10.0, 40.0);
    let top_left = xf.to_screen(Point::new(b.x0, b.y1));
    let col = color32(&m.color, m.opacity);
    let pole = Stroke::new((s / 10.0).max(1.5), Color32::from_gray(60));
    let x = top_left.x + s * 0.2;
    p.line_segment(
        [egui::pos2(x, top_left.y + s * 0.04), egui::pos2(x, top_left.y + s)],
        pole,
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(x, top_left.y + s * 0.04),
            egui::pos2(top_left.x + s * 0.9, top_left.y + s * 0.22),
            egui::pos2(x, top_left.y + s * 0.44),
        ],
        col,
        Stroke::new(1.0, Color32::from_black_alpha(140)),
    ));
}
