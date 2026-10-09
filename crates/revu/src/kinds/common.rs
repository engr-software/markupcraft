//! Helpers the markup kind files share: box geometry with `/RD`, line endings, paint operators.

use markupcraft_geom::path::path_points;
use markupcraft_geom::shapes::line_ending;
use markupcraft_geom::text::{Font, FontFamily};
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Markup, TextStyle};
use pdfcraft_cos::{Dict, Object};

use crate::ap::Ap;
use crate::pdf::{self, rect_arr};

/// The box of a box-shaped markup: the bounding box of `pts[0..4]` (zeros when empty).
pub fn box_of(m: &Markup) -> Rect {
    let corners: Vec<Point> = m.pts.iter().take(4).copied().collect();
    bbox(&corners).unwrap_or_default()
}

/// The centre line of a Rectangle / Ellipse border: the box inset by half the line width.
pub fn stroke_box(m: &Markup) -> Rect {
    let r = box_of(m);
    let h = (m.line_width / 2.0).min(r.width() / 2.0).min(r.height() / 2.0);
    if h > 0.0 { r.padded(-h) } else { r }
}

/// `/RD`: the differences between `/Rect` and the inner box.
pub fn write_rd(a: &mut Dict, rect: Rect, inner: Rect) {
    let rd = Rect::new(
        inner.x0 - rect.x0,
        inner.y0 - rect.y0,
        rect.x1 - inner.x1,
        rect.y1 - inner.y1,
    );
    pdf::set(a, "RD", rect_arr(&rd));
}

/// `m.pts` = the four corners of `/Rect` minus `/RD` (`m.rect` is read already).
pub fn box_from_rd(a: &Dict, m: &mut Markup) {
    let rd = pdf::rect(a.get(b"RD")).unwrap_or_default();
    let r = m.rect;
    m.pts = Rect::new(r.x0 + rd.x0, r.y0 + rd.y0, r.x1 - rd.x1, r.y1 - rd.y1)
        .corners()
        .to_vec();
}

/// Fill and / or stroke; a zero-width border is not stroked (PDF would draw a hairline).
pub fn paint_op(m: &Markup) -> &'static str {
    match (m.line_width > 0.0, m.fill.is_some()) {
        (true, true) => "B\n",
        (true, false) => "S\n",
        (false, true) => "f\n",
        (false, false) => "n\n",
    }
}

/// Whether a line has a start or end decoration.
pub fn has_endings(m: &Markup) -> bool {
    let set = |s: &str| !s.is_empty() && s != "None";
    set(&m.line_start) || set(&m.line_end)
}

/// A line ending at `tip`, stroked, or filled with the stroke colour (Revu fills closed arrows).
pub fn draw_ending(ap: &mut Ap, m: &Markup, from: Point, tip: Point, style: &str, w: f64, extent: &mut Vec<Point>) {
    let e = line_ending(from, tip, style, w);
    if e.path.is_empty() {
        return;
    }
    if e.filled {
        ap.op("q ").fill_rgb(&m.color).op("[] 0 d ");
    }
    ap.segs(&e.path);
    ap.op(if e.filled { "b Q\n" } else { "S\n" });
    extent.extend(path_points(&e.path));
}

pub fn set_or_remove(a: &mut Dict, key: &str, on: bool, v: Object) {
    if on {
        pdf::set(a, key, v);
    } else {
        pdf::remove(a, key);
    }
}

/// The layout font of a text style.
pub fn font_of(s: &TextStyle) -> Font {
    Font {
        family: FontFamily::of(&s.font),
        bold: s.bold,
        italic: s.italic,
        size: s.size,
    }
}
