//! Plain shapes: Polygon, Cloud (`/Polygon /PolygonCloud` + `/BE`), Polyline, Line, Arrow
//! (`/Line /LineArrow`), Rectangle (`/Square` + `/RD`) and Ellipse (`/Circle` + `/RD`).
//!
//! From Revu 21 files: an Arrow is `/Line /IT /LineArrow`, `/LE [/None /ClosedArrow]` (head at
//! the second `/L` point), `/IC` = the stroke colour; Square boxes keep their geometry as `/Rect`
//! minus `/RD`; Clouds are `/Polygon /IT /PolygonCloud` with `/BE << /S /C /I 2 >>`.

use markupcraft_geom::path::path_points;
use markupcraft_geom::shapes::{cloud_bulge, cloud_path, ellipse_path};
use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Kind, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use super::AnnotKind;
use super::common::{box_from_rd, box_of, draw_ending, has_endings, paint_op, stroke_box, write_rd};
use crate::ap::Ap;
use crate::pdf::{self, color_arr, n, points_arr, real};

// ------------------------------------------------------------------------------ lines ---

/// The open path through `m.pts` (butt caps, mitred joins, like Revu), then the line endings.
fn draw_lines(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if m.pts.is_empty() {
        return;
    }
    ap.op("0 J 0 j\n").path(&m.pts, false).op("S\n");
    if m.pts.len() < 2 || !has_endings(m) {
        return;
    }
    let k = m.pts.len();
    if let (Some(a0), Some(a1), Some(b0), Some(b1)) = (m.pts.first(), m.pts.get(1), m.pts.get(k - 2), m.pts.last()) {
        draw_ending(ap, m, *a1, *a0, &m.line_start, m.line_width, extent);
        draw_ending(ap, m, *b0, *b1, &m.line_end, m.line_width, extent);
    }
}

fn end_name(s: &str) -> Object {
    n(if s.is_empty() { "None" } else { s })
}

fn write_endings(a: &mut Dict, m: &Markup) {
    if !has_endings(m) {
        if m.kind != Kind::Arrow {
            pdf::remove(a, "LE");
        }
        return;
    }
    pdf::set(
        a,
        "LE",
        Object::Array(vec![end_name(&m.line_start), end_name(&m.line_end)]),
    );
}

fn line_geometry(a: &mut Dict, m: &Markup) {
    pdf::set(a, "L", points_arr(&m.pts));
    write_endings(a, m);
    if m.kind == Kind::Arrow {
        pdf::set(a, "IC", color_arr(&m.fill.unwrap_or(m.color)));
    }
}

fn polyline_geometry(a: &mut Dict, m: &Markup) {
    pdf::set(a, "Vertices", points_arr(&m.pts));
    write_endings(a, m);
}

// --------------------------------------------------------------------- polygon / cloud ---

fn cloud_be(intensity: f64) -> Object {
    Object::Dict(pdf::dict(&[("S", n("C")), ("I", real(intensity))]))
}

fn draw_polygon(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if m.pts.is_empty() {
        return;
    }
    let intensity = if m.kind == Kind::Cloud && m.cloud <= 0.0 {
        1.0
    } else {
        m.cloud
    };
    if intensity > 0.0 && m.pts.len() >= 2 {
        let p = cloud_path(&m.pts, intensity);
        ap.segs(&p);
        extent.extend(path_points(&p));
    } else {
        ap.path(&m.pts, true);
    }
    ap.op(paint_op(m));
}

fn polygon_geometry(a: &mut Dict, m: &Markup) {
    pdf::set(a, "Vertices", points_arr(&m.pts));
    if m.cloud > 0.0 || m.kind == Kind::Cloud {
        pdf::set(a, "BE", cloud_be(if m.cloud > 0.0 { m.cloud } else { 1.0 }));
    } else {
        pdf::remove(a, "BE");
    }
}

// -------------------------------------------------------------------------- box shapes ---

fn box_pad(m: &Markup) -> f64 {
    m.line_width / 2.0 + if m.cloud > 0.0 { cloud_bulge(m.cloud) } else { 0.0 }
}

/// `/Rect` of a Rectangle / Ellipse: the box grown by half the line width (and a cloud's bulge).
pub fn box_rect(m: &Markup) -> Rect {
    box_of(m).padded(box_pad(m))
}

/// `/RD` (and `/BE` for a clouded box); the box itself is `/Rect` minus `/RD`.
pub fn box_geometry(a: &mut Dict, m: &Markup) {
    write_rd(a, box_rect(m), box_of(m));
    if m.cloud > 0.0 {
        pdf::set(a, "BE", cloud_be(m.cloud));
    } else {
        pdf::remove(a, "BE");
    }
}

pub fn read_box(_cos: &CosDoc, a: &Dict, m: &mut Markup) {
    box_from_rd(a, m);
}

fn draw_rectangle(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let c = stroke_box(m).corners();
    if m.cloud > 0.0 {
        let p = cloud_path(&c, m.cloud);
        ap.segs(&p);
        extent.extend(path_points(&p));
    } else {
        ap.op("0 j\n").path(&c, true); // mitred corners, as Revu
    }
    ap.op(paint_op(m));
}

fn draw_ellipse(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let p = ellipse_path(stroke_box(m));
    ap.segs(&p);
    extent.extend(path_points(&p));
    ap.op(paint_op(m));
}

// ------------------------------------------------------------------------------- table ---

pub static POLYGON: AnnotKind = AnnotKind {
    draw_shape: Some(draw_polygon),
    write_geometry: Some(polygon_geometry),
    ..AnnotKind::new(Kind::Polygon, "Polygon", None, 0, true)
};
pub static CLOUD: AnnotKind = AnnotKind {
    draw_shape: Some(draw_polygon),
    write_geometry: Some(polygon_geometry),
    ..AnnotKind::new(Kind::Cloud, "Polygon", Some("PolygonCloud"), 0, true)
};
pub static POLYLINE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_lines),
    write_geometry: Some(polyline_geometry),
    ..AnnotKind::new(Kind::Polyline, "PolyLine", None, 0, false)
};
pub static LINE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_lines),
    write_geometry: Some(line_geometry),
    ..AnnotKind::new(Kind::Line, "Line", None, 0, false)
};
pub static ARROW: AnnotKind = AnnotKind {
    draw_shape: Some(draw_lines),
    write_geometry: Some(line_geometry),
    ..AnnotKind::new(Kind::Arrow, "Line", Some("LineArrow"), 0, false)
};
pub static RECTANGLE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_rectangle),
    write_geometry: Some(box_geometry),
    rect_of: Some(box_rect),
    read_keys: Some(read_box),
    ..AnnotKind::new(Kind::Rectangle, "Square", None, 0, true)
};
pub static ELLIPSE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_ellipse),
    write_geometry: Some(box_geometry),
    rect_of: Some(box_rect),
    read_keys: Some(read_box),
    ..AnnotKind::new(Kind::Ellipse, "Circle", None, 0, true)
};
