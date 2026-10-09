//! Measurement annotations: Area, Perimeter, Polylength, Length, Count.

use markupcraft_geom::Point;
use markupcraft_model::{CountSymbol, Kind, Markup, caption, code};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use super::{AnnotKind, measure_more};
use crate::ap::Ap;
use crate::pdf::{self, n, points_arr, real, s};

/// Keys Revu writes on every measurement (seen in Revu 21 files).
pub fn measurement_keys(a: &mut Dict, m: &Markup) {
    pdf::set(a, "Cap", Object::Bool(!m.hide_caption));
    pdf::set(a, "SlopeType", Object::Int(0));
    pdf::set(a, "PitchRun", Object::Int(12));
    let mut depth = Dict::new();
    pdf::set(&mut depth, "Type", n("NumberFormat"));
    pdf::set(&mut depth, "U", s("'"));
    pdf::set(&mut depth, "C", real(0.001157407));
    pdf::set(&mut depth, "D", Object::Int(100));
    pdf::set(&mut depth, "FD", Object::Bool(true));
    pdf::set(&mut depth, "SS", s(""));
    pdf::set(a, "DepthUnit", Object::Array(vec![Object::Dict(depth)]));
}

pub(super) fn polygon_keys(a: &mut Dict, m: &Markup) {
    measurement_keys(a, m);
    pdf::set(a, "AlignOnSegment", Object::Bool(true));
}

fn line_keys(a: &mut Dict, m: &Markup) {
    measurement_keys(a, m);
    pdf::set(a, "LE", Object::Array(vec![n("ClosedArrow"), n("ClosedArrow")]));
    pdf::set(a, "LL", Object::Int(0));
    pdf::set(a, "LLE", Object::Int(0));
}

fn set_or_remove(a: &mut Dict, key: &str, on: bool, v: Object) {
    if on {
        pdf::set(a, key, v);
    } else {
        pdf::remove(a, key);
    }
}

/// Same as the default geometry by subtype.
pub fn geometry_by_subtype(a: &mut Dict, m: &Markup) {
    match m.subtype.as_str() {
        "Line" => pdf::set(a, "L", points_arr(&m.pts)),
        "Square" | "Circle" => {}
        _ => pdf::set(a, "Vertices", points_arr(&m.pts)),
    }
}

/// Written only when used, so markups without them keep exactly the keys they had:
/// `/PCSegmentValues`, `/PCRiseDrop`, `/CO` (Area, Perimeter, Length), `/PCCaptionOffset`
/// (Polylength, Count), `/PCArcs`.
pub(super) fn polish_keys(a: &mut Dict, m: &Markup) {
    set_or_remove(a, "PCSegmentValues", m.segment_values, Object::Bool(true));
    set_or_remove(
        a,
        "PCRiseDrop",
        m.kind == Kind::Polylength && m.rise_drop != 0.0,
        real(m.rise_drop),
    );
    let off = m.caption_offset.unwrap_or_default();
    if caption::uses_co(m.kind) {
        let o = if m.kind == Kind::Length {
            let (along, perp) = caption::offset_to_line_co(m, off);
            Point::new(along, perp)
        } else {
            off
        };
        set_or_remove(a, "CO", m.caption_offset.is_some(), points_arr(&[o]));
    } else {
        set_or_remove(a, "PCCaptionOffset", m.caption_offset.is_some(), points_arr(&[off]));
    }
    let arcs = Object::Array(
        m.arcs
            .iter()
            .flat_map(|(f, k)| [Object::Int(*f as i64), Object::Int(*k as i64)])
            .collect(),
    );
    set_or_remove(a, "PCArcs", !m.arcs.is_empty(), arcs);
}

fn line_geometry(a: &mut Dict, m: &Markup) {
    geometry_by_subtype(a, m);
    polish_keys(a, m);
}

pub(super) fn area_geometry(a: &mut Dict, m: &Markup) {
    geometry_by_subtype(a, m);
    polish_keys(a, m);
    let holes = Object::Array(m.holes.iter().map(|h| points_arr(h)).collect());
    set_or_remove(a, "PCCutouts", !m.holes.is_empty(), holes);
}

fn count_geometry(a: &mut Dict, m: &Markup) {
    geometry_by_subtype(a, m);
    let sym = match m.count_symbol {
        CountSymbol::Circle => None,
        CountSymbol::Square => Some("Square"),
        CountSymbol::Check => Some("Check"),
        CountSymbol::Cross => Some("Cross"),
        CountSymbol::Custom => Some("Custom"),
    };
    set_or_remove(a, "PCCountSymbol", sym.is_some(), n(sym.unwrap_or("Circle")));
    let shape = Object::Array(m.symbol_paths.iter().map(|p| points_arr(p)).collect());
    set_or_remove(
        a,
        "PCCountShape",
        m.count_symbol == CountSymbol::Custom && !m.symbol_paths.is_empty(),
        shape,
    );
    polish_keys(a, m);
    set_or_remove(a, "PCSymbolScale", m.symbol_scale != 1.0, real(m.symbol_scale));
}

fn draw_open(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if !m.pts.is_empty() {
        measure_more::trace(ap, m, false);
        ap.op("S\n");
    }
    measure_more::draw_segment_values(ap, m, extent);
}

fn draw_closed(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if !m.pts.is_empty() {
        measure_more::trace(ap, m, true);
        ap.op(if m.fill.is_some() { "B\n" } else { "S\n" });
    }
    measure_more::draw_segment_values(ap, m, extent);
}

/// The outline and, with cutouts, every hole as its own subpath filled even-odd.
fn draw_area(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if m.pts.is_empty() {
        return;
    }
    measure_more::draw_segment_values(ap, m, extent);
    measure_more::trace(ap, m, false);
    if m.holes.is_empty() {
        ap.op(if m.fill.is_some() { "h B\n" } else { "h S\n" });
        return;
    }
    for h in m.holes.iter().filter(|h| h.len() >= 3) {
        ap.op("h ");
        ap.path(h, false);
    }
    ap.op(if m.fill.is_some() { "h B*\n" } else { "h S\n" });
}

/// The count symbol at each counted point: a filled circle (four Bezier quarters) by default,
/// or a square, check mark or cross, sized by `symbol_scale`.
fn draw_count(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let r = 6.0 * if m.symbol_scale > 0.0 { m.symbol_scale } else { 1.0 };
    let k = 0.5523 * r;
    for p in &m.pts {
        let (x, y) = (p.x, p.y);
        match m.count_symbol {
            CountSymbol::Custom => {
                for path in &m.symbol_paths {
                    let moved: Vec<Point> = path
                        .iter()
                        .map(|q| Point::new(x + q.x * m.symbol_scale, y + q.y * m.symbol_scale))
                        .collect();
                    let closed = moved.len() >= 3 && moved.first() == moved.last();
                    ap.path(&moved, false).op(if closed { "h B\n" } else { "S\n" });
                    extent.extend(moved);
                }
                continue;
            }
            CountSymbol::Square => {
                ap.nums(&[x - r, y - r, 2.0 * r, 2.0 * r], "re").op("B\n");
            }
            CountSymbol::Check => {
                ap.move_to(Point::new(x - r, y))
                    .line_to(Point::new(x - r / 3.0, y - r * 0.7))
                    .line_to(Point::new(x + r, y + r * 0.8))
                    .op("S\n");
            }
            CountSymbol::Cross => {
                ap.move_to(Point::new(x - r, y - r))
                    .line_to(Point::new(x + r, y + r))
                    .move_to(Point::new(x - r, y + r))
                    .line_to(Point::new(x + r, y - r))
                    .op("S\n");
            }
            CountSymbol::Circle => {
                ap.move_to(Point::new(x + r, y))
                    .curve_to(Point::new(x + r, y + k), Point::new(x + k, y + r), Point::new(x, y + r))
                    .curve_to(Point::new(x - k, y + r), Point::new(x - r, y + k), Point::new(x - r, y))
                    .curve_to(Point::new(x - r, y - k), Point::new(x - k, y - r), Point::new(x, y - r))
                    .curve_to(Point::new(x + k, y - r), Point::new(x + r, y - k), Point::new(x + r, y))
                    .op("B\n");
            }
        }
        extent.push(Point::new(x - r, y - r));
        extent.push(Point::new(x + r, y + r));
    }
}

/// Our keys for cutouts, count symbols and measurement polish, read back on load.
pub fn read_takeoff_keys(_doc: &CosDoc, a: &Dict, m: &mut Markup) {
    if m.kind.is_measurement() {
        m.hide_caption = pdf::boolean(a.get(b"Cap")) == Some(false);
    }
    if let Some(holes) = a.get(b"PCCutouts").and_then(Object::as_array) {
        for h in holes {
            let ring = pdf::points(Some(h));
            if ring.len() >= 3 {
                m.holes.push(ring);
            }
        }
    }
    m.count_symbol = match pdf::name(a.get(b"PCCountSymbol")).as_str() {
        "Square" => CountSymbol::Square,
        "Check" => CountSymbol::Check,
        "Cross" => CountSymbol::Cross,
        "Custom" => CountSymbol::Custom,
        _ => CountSymbol::Circle,
    };
    if let Some(sc) = pdf::num(a.get(b"PCSymbolScale")).filter(|v| *v > 0.0) {
        m.symbol_scale = sc;
    }
    m.segment_values = pdf::boolean(a.get(b"PCSegmentValues")).unwrap_or(false);
    if m.kind == Kind::Polylength {
        m.rise_drop = pdf::num(a.get(b"PCRiseDrop")).unwrap_or(0.0);
    }
    let key: &[u8] = if caption::uses_co(m.kind) {
        b"CO"
    } else {
        b"PCCaptionOffset"
    };
    let off = pdf::points(a.get(key));
    if off.len() == 1 && (caption::uses_co(m.kind) || matches!(m.kind, Kind::Polylength | Kind::Count)) {
        let o = off[0];
        m.caption_offset = Some(if m.kind == Kind::Length {
            caption::line_co_to_offset(m, o.x, o.y)
        } else {
            o
        });
    }
    if let Some(arcs) = a.get(b"PCArcs").and_then(Object::as_array) {
        for c in arcs.as_chunks::<2>().0 {
            if let (Some(f), Some(k)) = (c[0].as_int(), c[1].as_int())
                && f >= 0
                && k >= 0
                && (f as usize) < m.pts.len()
            {
                m.arcs.push((f as usize, k as usize));
            }
        }
    }
    if let Some(shape) = a.get(b"PCCountShape").and_then(Object::as_array) {
        for p in shape {
            let path = pdf::points(Some(p));
            if path.len() >= 2 {
                m.symbol_paths.push(path);
            }
        }
    }
    markupcraft_model::measure_extras::validate_arcs(m);
    if m.count_symbol == CountSymbol::Custom && m.symbol_paths.is_empty() {
        m.count_symbol = CountSymbol::Circle;
    }
}

pub static AREA: AnnotKind = AnnotKind {
    create_keys: Some(polygon_keys),
    draw_shape: Some(draw_area),
    write_geometry: Some(area_geometry),
    ..AnnotKind::new(Kind::Area, "Polygon", Some("PolygonDimension"), code::AREA, true)
};

/// Guess, to check against a real Revu file: Perimeter as a `/Polygon` with code 130.
pub static PERIMETER: AnnotKind = AnnotKind {
    create_keys: Some(polygon_keys),
    draw_shape: Some(draw_closed),
    write_geometry: Some(line_geometry),
    ..AnnotKind::new(Kind::Perimeter, "Polygon", Some("PolygonDimension"), code::LENGTH, true)
};

pub static POLYLENGTH: AnnotKind = AnnotKind {
    create_keys: Some(measurement_keys),
    draw_shape: Some(draw_open),
    write_geometry: Some(line_geometry),
    ..AnnotKind::new(
        Kind::Polylength,
        "PolyLine",
        Some("PolyLineDimension"),
        code::LENGTH,
        false,
    )
};

pub static LENGTH: AnnotKind = AnnotKind {
    create_keys: Some(line_keys),
    write_geometry: Some(line_geometry),
    ..AnnotKind::new(Kind::Length, "Line", Some("LineDimension"), code::LENGTH, false)
};

/// Guess, to check against a real Revu file: Count as `/PolyLine` + `/PolyLineCount`.
pub static COUNT: AnnotKind = AnnotKind {
    create_keys: Some(measurement_keys),
    draw_shape: Some(draw_count),
    write_geometry: Some(count_geometry),
    ..AnnotKind::new(Kind::Count, "PolyLine", Some("PolyLineCount"), code::COUNT, false)
};
