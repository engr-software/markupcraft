//! More measurement annotations: Volume, Diameter, Radius, Angle; and what every measurement
//! appearance shares: arcs drawn as curves and Show Segment Values.
//!
//! Storage (Volume follows the Area layout Revu writes with code 132; the others are
//! MarkupCraft's guesses until checked against a real Revu file):
//!   Volume   `/Polygon` `/IT /PolygonDimension` code 132, depth in `/PCDepth` (first `/D` unit),
//!            next to the `/DepthUnit` Revu writes on every measurement
//!   Diameter `/Circle` code 384, `/Vertices` = the two ends of a diameter, `/RD` = circle in `/Rect`
//!   Radius   `/Circle` `/IT /CircleRadius` code 384, `/Vertices` = [centre, point on the circle]
//!   Angle    `/PolyLine` `/IT /PolyLineAngle` code 1152, `/Vertices` = [arm end, vertex, arm end]

use std::f64::consts::PI;

use markupcraft_geom::{Point, Rect};
use markupcraft_model::measure_extras::{
    angle_degrees, arc_handle, arc_sweep, circle_of, circle_through, control_segments, segment_values,
};
use markupcraft_model::{Kind, Markup, code};
use pdfcraft_cos::{Dict, Document as CosDoc};

use super::AnnotKind;
use super::measure::{area_geometry, geometry_by_subtype, measurement_keys, polish_keys, polygon_keys};
use crate::ap::Ap;
use crate::pdf::{self, points_arr, real, rect_arr};

/// Our `/IT` for a Radius (a guess: Revu's is not known yet).
pub const RADIUS_INTENT: &str = "CircleRadius";
/// Our `/IT` for an Angle (a guess).
pub const ANGLE_INTENT: &str = "PolyLineAngle";

// ---- shared drawing -----------------------------------------------------------------------

/// Cubic Bezier pieces (at most 90 degrees each) of the arc about `c` with radius `r`, from
/// angle `start` sweeping `sweep` (radians, signed). The pen is at the arc's start.
pub fn arc_curves(ap: &mut Ap, c: Point, r: f64, start: f64, sweep: f64) {
    if !(r.is_finite() && sweep.is_finite()) {
        return;
    }
    let pieces = (sweep.abs() / (PI / 2.0)).ceil().clamp(1.0, 8.0) as usize;
    let d = sweep / pieces as f64;
    let k = 4.0 / 3.0 * (d / 4.0).tan();
    for i in 0..pieces {
        let (t0, t1) = (start + d * i as f64, start + d * (i + 1) as f64);
        let (p0, p3) = (
            Point::new(c.x + r * t0.cos(), c.y + r * t0.sin()),
            Point::new(c.x + r * t1.cos(), c.y + r * t1.sin()),
        );
        let p1 = Point::new(p0.x - k * r * t0.sin(), p0.y + k * r * t0.cos());
        let p2 = Point::new(p3.x + k * r * t1.sin(), p3.y - k * r * t1.cos());
        ap.curve_to(p1, p2, p3);
    }
}

/// The outline through `m.pts` with every arc drawn as curves (not painted). Closed shapes
/// end with `h`.
pub fn trace(ap: &mut Ap, m: &Markup, closed: bool) {
    let Some(first) = m.pts.first() else { return };
    if m.arcs.is_empty() {
        ap.path(&m.pts, closed);
        return;
    }
    ap.move_to(*first);
    for seg in control_segments(m) {
        let (Some(a), Some(b)) = (
            seg.path.first().and_then(|&i| m.pts.get(i)),
            seg.path.last().and_then(|&i| m.pts.get(i)),
        ) else {
            continue;
        };
        let through = seg.arc.and_then(|j| arc_handle(m, j)).and_then(|h| m.pts.get(h));
        match through.and_then(|t| circle_through(*a, *t, *b).map(|c| (c, *t))) {
            Some(((c, r), t)) => {
                let (start, sweep) = arc_sweep(c, *a, t, *b);
                arc_curves(ap, c, r, start, sweep);
            }
            None => {
                // a straight segment (or a flat arc): through the stored points
                for &i in seg.path.iter().skip(1) {
                    if let Some(p) = m.pts.get(i) {
                        ap.line_to(*p);
                    }
                }
            }
        }
    }
    if closed {
        ap.op("h ");
    }
}

/// Show Segment Values: each segment's length along it, upright, just above the line.
pub fn draw_segment_values(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if !m.segment_values {
        return;
    }
    let size = 8.0;
    for v in segment_values(m) {
        let mut ang = v.angle;
        if ang > PI / 2.0 + 1e-9 || ang <= -PI / 2.0 + 1e-9 {
            ang += if ang > 0.0 { -PI } else { PI }; // keep text upright
        }
        let (c, s) = (ang.cos(), ang.sin());
        let w = 0.55 * size * v.text.chars().count() as f64;
        let start = Point::new(v.at.x - c * w / 2.0 - s * 2.0, v.at.y - s * w / 2.0 + c * 2.0);
        ap.op("BT /Helv ")
            .nums(&[size], "Tf")
            .fill_rgb(&m.color)
            .nums(&[c, s, -s, c, start.x, start.y], "Tm");
        ap.op(&format!("({}) Tj ET\n", Ap::text_literal(&v.text)));
        for dx in [0.0, w] {
            for dy in [0.0, size] {
                extent.push(Point::new(start.x + c * dx - s * dy, start.y + s * dx + c * dy));
            }
        }
    }
}

/// A full circle as four Bezier quarters, closed (not painted).
fn circle_path(ap: &mut Ap, c: Point, r: f64) {
    ap.move_to(Point::new(c.x + r, c.y));
    arc_curves(ap, c, r, 0.0, 2.0 * PI);
    ap.op("h ");
}

// ---- Volume -------------------------------------------------------------------------------

fn volume_geometry(a: &mut Dict, m: &Markup) {
    area_geometry(a, m);
    if m.depth != 0.0 && m.depth.is_finite() {
        pdf::set(a, "PCDepth", real(m.depth));
    } else {
        pdf::remove(a, "PCDepth");
    }
}

fn draw_volume(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if m.pts.is_empty() {
        return;
    }
    trace(ap, m, false);
    let holes: Vec<&Vec<Point>> = m.holes.iter().filter(|h| h.len() >= 3).collect();
    for h in &holes {
        ap.op("h ");
        ap.path(h, false);
    }
    let even_odd = if holes.is_empty() { "B" } else { "B*" };
    ap.op(&format!("h {}\n", if m.fill.is_some() { even_odd } else { "S" }));
    draw_segment_values(ap, m, extent);
}

/// `/PCDepth`, and the moved caption of the kinds here (`/PCCaptionOffset`).
fn read_more_keys(_doc: &CosDoc, a: &Dict, m: &mut Markup) {
    if let Some(d) = pdf::num(a.get(b"PCDepth")) {
        m.depth = d;
    }
    let off = pdf::points(a.get(b"PCCaptionOffset"));
    if let [o] = off.as_slice() {
        m.caption_offset = Some(*o);
    }
}

pub static VOLUME: AnnotKind = AnnotKind {
    create_keys: Some(polygon_keys),
    draw_shape: Some(draw_volume),
    write_geometry: Some(volume_geometry),
    read_keys: Some(read_more_keys),
    ..AnnotKind::new(Kind::Volume, "Polygon", Some("PolygonDimension"), code::VOLUME, true)
};

// ---- Diameter / Radius --------------------------------------------------------------------

fn circle_geometry(a: &mut Dict, m: &Markup) {
    let two: Vec<Point> = m.pts.iter().take(2).copied().collect();
    pdf::set(a, "Vertices", points_arr(&two));
    polish_keys(a, m);
}

/// The circle, and the measured diameter or radius as a line.
fn draw_circle(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let Some((c, r)) = circle_of(m) else {
        if !m.pts.is_empty() {
            ap.path(&m.pts, false).op("S\n");
        }
        return;
    };
    circle_path(ap, c, r);
    ap.op(if m.fill.is_some() { "B\n" } else { "S\n" });
    if let (Some(a), Some(b)) = (m.pts.first(), m.pts.get(1)) {
        ap.move_to(*a).line_to(*b).op("S\n");
    }
    extent.push(Point::new(c.x - r, c.y - r));
    extent.push(Point::new(c.x + r, c.y + r));
}

/// `/RD`: where the circle sits inside `/Rect` (ISO 32000-1 §12.5.6.8), for other readers.
fn finish_circle(_cos: &mut CosDoc, a: &mut Dict, m: &mut Markup) {
    let Some((c, r)) = circle_of(m) else { return };
    let half = m.line_width / 2.0;
    let rect = m.rect.normalized();
    let rd = Rect::new(
        (c.x - r - half - rect.x0).max(0.0),
        (c.y - r - half - rect.y0).max(0.0),
        (rect.x1 - c.x - r - half).max(0.0),
        (rect.y1 - c.y - r - half).max(0.0),
    );
    pdf::set(a, "RD", rect_arr(&rd));
}

/// A circle written without `/Vertices` (another program): the measured line from `/Rect`
/// inset by `/RD`.
fn read_circle_keys(doc: &CosDoc, a: &Dict, m: &mut Markup) {
    read_more_keys(doc, a, m);
    if a.contains(b"Vertices") && m.pts.len() >= 2 {
        return;
    }
    let rd = pdf::rect(a.get(b"RD")).unwrap_or_default();
    let r = m.rect.normalized();
    let b = Rect::new(r.x0 + rd.x0, r.y0 + rd.y0, r.x1 - rd.x1, r.y1 - rd.y1);
    let c = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    m.pts = match m.kind {
        Kind::Radius => vec![c, Point::new(b.x1, c.y)],
        _ => vec![Point::new(b.x0, c.y), Point::new(b.x1, c.y)],
    };
}

pub static DIAMETER: AnnotKind = AnnotKind {
    create_keys: Some(measurement_keys),
    draw_shape: Some(draw_circle),
    write_geometry: Some(circle_geometry),
    read_keys: Some(read_circle_keys),
    finish: Some(finish_circle),
    ..AnnotKind::new(Kind::Diameter, "Circle", None, code::DIAMETER, true)
};

pub static RADIUS: AnnotKind = AnnotKind {
    create_keys: Some(measurement_keys),
    draw_shape: Some(draw_circle),
    write_geometry: Some(circle_geometry),
    read_keys: Some(read_circle_keys),
    finish: Some(finish_circle),
    ..AnnotKind::new(Kind::Radius, "Circle", Some(RADIUS_INTENT), code::DIAMETER, true)
};

// ---- Angle --------------------------------------------------------------------------------

fn angle_geometry(a: &mut Dict, m: &Markup) {
    geometry_by_subtype(a, m);
    polish_keys(a, m);
}

/// The two arms and an arc between them at the vertex.
fn draw_angle(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if m.pts.is_empty() {
        return;
    }
    ap.path(&m.pts, false).op("S\n");
    let [a, v, b, ..] = m.pts.as_slice() else { return };
    if angle_degrees(*a, *v, *b).is_none() {
        return;
    }
    let r = (a.dist(*v).min(b.dist(*v)) * 0.3).clamp(1.0, 15.0);
    let (ta, tb) = ((a.y - v.y).atan2(a.x - v.x), (b.y - v.y).atan2(b.x - v.x));
    let mut sweep = tb - ta;
    if sweep > PI {
        sweep -= 2.0 * PI;
    } else if sweep < -PI {
        sweep += 2.0 * PI;
    }
    ap.move_to(Point::new(v.x + r * ta.cos(), v.y + r * ta.sin()));
    arc_curves(ap, *v, r, ta, sweep);
    ap.op("S\n");
    extent.push(Point::new(v.x - r, v.y - r));
    extent.push(Point::new(v.x + r, v.y + r));
}

fn read_angle_keys(doc: &CosDoc, a: &Dict, m: &mut Markup) {
    read_more_keys(doc, a, m);
}

pub static ANGLE: AnnotKind = AnnotKind {
    create_keys: Some(measurement_keys),
    draw_shape: Some(draw_angle),
    write_geometry: Some(angle_geometry),
    read_keys: Some(read_angle_keys),
    ..AnnotKind::new(Kind::Angle, "PolyLine", Some(ANGLE_INTENT), code::ANGLE, false)
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use markupcraft_measure::Scale;
    use markupcraft_model::measure_extras::{convert_to_arc, ellipse_ring};
    use markupcraft_model::{Document, Point};
    use pdfcraft_cos::Object;

    use super::*;
    use crate::kinds::{classify, kind_for};
    use crate::{SaveMode, open, open_bytes, save};

    /// A one-page blank PDF with a correct cross-reference table.
    fn blank_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> >>",
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    fn temp_pdf(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "markupcraft_measure_more_{}_{}_{tag}.pdf",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn new_markup(kind: Kind, subject: &str, pts: Vec<Point>) -> Markup {
        Markup {
            subject: subject.into(),
            scale: Some(Scale::architectural(0.125, 1.0)),
            ..Markup::new(kind, 0, pts)
        }
    }

    /// Save `markups` on a blank page, reload, return (document, raw annotation dicts by subject).
    fn round_trip(markups: Vec<Markup>, tag: &str) -> (Document, Vec<(String, Dict)>) {
        let (mut file, mut doc) = open_bytes(Arc::new(blank_pdf()), &PathBuf::from("blank.pdf")).expect("blank opens");
        doc.markups.extend(markups);
        let out = temp_pdf(tag);
        save(&mut file, &mut doc, &out, SaveMode::Incremental).expect("save");
        let (file2, back) = open(&out).expect("reopen");
        let mut raw = Vec::new();
        for pg in pdf::pages(&file2.cos) {
            let annots = file2.cos.resolve(pg.dict.get(b"Annots").unwrap_or(&Object::Null));
            for o in annots.as_array().cloned().unwrap_or_default() {
                if let Some(d) = file2.cos.resolve(&o).as_dict() {
                    raw.push((pdf::text(d.get(b"Subj")), d.clone()));
                }
            }
        }
        drop(file2);
        let _ = std::fs::remove_file(&out);
        (back, raw)
    }

    fn find<'a>(doc: &'a Document, subject: &str) -> &'a Markup {
        doc.markups
            .iter()
            .find(|m| m.subject == subject)
            .expect("markup reloads")
    }

    fn raw<'a>(raw: &'a [(String, Dict)], subject: &str) -> &'a Dict {
        &raw.iter().find(|(s, _)| s == subject).expect("annotation").1
    }

    #[test]
    fn registry_and_classify() {
        assert_eq!(kind_for(Kind::Volume).code, code::VOLUME);
        assert_eq!(kind_for(Kind::Diameter).subtype, "Circle");
        assert_eq!(kind_for(Kind::Radius).intent, Some(RADIUS_INTENT));
        assert_eq!(kind_for(Kind::Angle).code, code::ANGLE);
        assert_eq!(classify("Polygon", "PolygonDimension", code::VOLUME), Kind::Volume);
        assert_eq!(classify("Circle", "", code::DIAMETER), Kind::Diameter);
        assert_eq!(classify("Circle", RADIUS_INTENT, code::DIAMETER), Kind::Radius);
        assert_eq!(classify("PolyLine", ANGLE_INTENT, code::ANGLE), Kind::Angle);
        assert_eq!(classify("Circle", "", 0), Kind::Ellipse);
    }

    #[test]
    fn new_kinds_round_trip() {
        let mut vol = new_markup(
            Kind::Volume,
            "vol",
            vec![p(100.0, 100.0), p(190.0, 100.0), p(190.0, 190.0), p(100.0, 190.0)],
        );
        vol.depth = 2.5;
        vol.holes = vec![vec![p(110.0, 110.0), p(119.0, 110.0), p(119.0, 119.0), p(110.0, 119.0)]];
        vol.caption_offset = Some(p(5.0, -3.0));
        let dia = new_markup(Kind::Diameter, "dia", vec![p(300.0, 300.0), p(390.0, 300.0)]);
        let rad = new_markup(Kind::Radius, "rad", vec![p(300.0, 500.0), p(300.0, 545.0)]);
        let mut ang = new_markup(
            Kind::Angle,
            "ang",
            vec![p(500.0, 100.0), p(450.0, 100.0), p(500.0, 150.0)],
        );
        ang.scale = None;
        let want_vol = vol.quantity_text();
        let (back, raw_annots) = round_trip(vec![vol, dia, rad, ang], "kinds");

        let v = find(&back, "vol");
        assert_eq!(v.kind, Kind::Volume);
        assert!((v.depth - 2.5).abs() < 1e-9);
        assert_eq!(v.holes.len(), 1);
        assert_eq!(v.caption_offset, Some(p(5.0, -3.0)));
        // 90 pt square = 10 ft x 10 ft, minus 1 ft x 1 ft, x 2.5 ft
        assert_eq!(want_vol, "247.5 cu ft");
        assert_eq!(v.quantity_text(), want_vol);
        assert_eq!(v.contents, want_vol);

        let d = find(&back, "dia");
        assert_eq!(d.kind, Kind::Diameter);
        assert_eq!(d.quantity_text(), "10'-0\"");
        assert_eq!(d.contents, "10'-0\"");
        let r = find(&back, "rad");
        assert_eq!(r.kind, Kind::Radius);
        assert_eq!(r.quantity_text(), "5'-0\"");
        let a = find(&back, "ang");
        assert_eq!(a.kind, Kind::Angle);
        assert_eq!(a.contents, "45\u{b0}");

        let vd = raw(&raw_annots, "vol");
        assert_eq!(vd.int(b"MeasurementTypes"), Some(code::VOLUME));
        assert!(vd.contains(b"DepthUnit") && vd.contains(b"PCDepth") && vd.contains(b"PCCaptionOffset"));
        let dd = raw(&raw_annots, "dia");
        assert_eq!(pdf::name(dd.get(b"Subtype")), "Circle");
        assert!(dd.contains(b"RD") && dd.contains(b"Vertices"));
        let rd = raw(&raw_annots, "rad");
        assert_eq!(pdf::name(rd.get(b"IT")), RADIUS_INTENT);
        let ad = raw(&raw_annots, "ang");
        assert_eq!(ad.int(b"MeasurementTypes"), Some(code::ANGLE));
        assert!(!ad.contains(b"PCDepth"));
    }

    #[test]
    fn circle_without_vertices_reads_from_rect() {
        let mut cos_dict = Dict::new();
        pdf::set(&mut cos_dict, "Subtype", pdf::n("Circle"));
        pdf::set(&mut cos_dict, "MeasurementTypes", Object::Int(code::DIAMETER));
        pdf::set(&mut cos_dict, "Rect", rect_arr(&Rect::new(10.0, 10.0, 112.0, 112.0)));
        pdf::set(&mut cos_dict, "RD", rect_arr(&Rect::new(1.0, 1.0, 1.0, 1.0)));
        let (file, _) = open_bytes(Arc::new(blank_pdf()), &PathBuf::from("blank.pdf")).expect("blank opens");
        let m = crate::read::read_annot(&file.cos, &cos_dict, 0, (0, 0));
        assert_eq!(m.kind, Kind::Diameter);
        assert_eq!(m.pts, vec![p(11.0, 61.0), p(111.0, 61.0)]);
    }

    #[test]
    fn segment_values_and_arcs_round_trip() {
        let mut pl = new_markup(
            Kind::Polylength,
            "run",
            vec![p(100.0, 100.0), p(200.0, 100.0), p(200.0, 200.0), p(300.0, 200.0)],
        );
        pl.segment_values = true;
        pl.rise_drop = 8.5;
        pl.caption_offset = Some(p(12.0, -7.0));
        convert_to_arc(&mut pl, 2, 0.0);
        let plain = new_markup(Kind::Polylength, "plain", vec![p(100.0, 300.0), p(200.0, 300.0)]);
        let mut area = new_markup(
            Kind::Area,
            "area",
            vec![p(300.0, 400.0), p(500.0, 400.0), p(500.0, 550.0), p(300.0, 550.0)],
        );
        area.holes = vec![ellipse_ring(p(400.0, 475.0), 30.0, 20.0, 48)];
        area.caption_offset = Some(p(-20.0, 30.0));
        area.segment_values = true;
        convert_to_arc(&mut area, 0, 0.0);
        let (want_pl, want_area, arcs_pl, arcs_area) = (
            pl.quantity_text(),
            area.quantity_text(),
            pl.arcs.clone(),
            area.arcs.clone(),
        );
        let n_pl = pl.pts.len();
        let (back, raw_annots) = round_trip(vec![pl, plain, area], "polish");

        let r = find(&back, "run");
        assert!(r.segment_values && (r.rise_drop - 8.5).abs() < 1e-6);
        assert_eq!(r.arcs, arcs_pl);
        assert_eq!(r.pts.len(), n_pl);
        assert_eq!(
            r.caption_offset.map(|o| ((o.x * 1e4).round(), (o.y * 1e4).round())),
            Some((120000.0, -70000.0))
        );
        assert_eq!(r.quantity_text(), want_pl);
        assert_eq!(r.contents, want_pl);
        let pln = find(&back, "plain");
        assert!(!pln.segment_values && pln.rise_drop == 0.0 && pln.caption_offset.is_none() && pln.arcs.is_empty());
        let a = find(&back, "area");
        assert_eq!(a.arcs, arcs_area);
        assert_eq!(a.holes.len(), 1);
        assert_eq!(a.quantity_text(), want_area);

        // Arcs are drawn as curves and segment values as rotated text in the appearance.
        let mut ap = Ap::new();
        let mut extent = Vec::new();
        let mut m = a.clone();
        m.segment_values = true;
        trace(&mut ap, &m, true);
        draw_segment_values(&mut ap, &m, &mut extent);
        assert!(ap.s.contains(" c "), "arc drawn with curves");
        assert_eq!(ap.s.matches(" l ").count(), 3, "the three straight sides");
        assert_eq!(ap.s.matches(" Tm ").count(), 4, "one value per control segment");
        assert_eq!(extent.len(), 16);

        let pd = raw(&raw_annots, "plain");
        for k in [
            "PCSegmentValues",
            "PCRiseDrop",
            "PCCaptionOffset",
            "PCArcs",
            "CO",
            "PCDepth",
        ] {
            assert!(!pd.contains(k.as_bytes()), "plain polylength has no /{k}");
        }
        let ad = raw(&raw_annots, "area");
        assert!(ad.contains(b"CO") && !ad.contains(b"PCCaptionOffset"));
    }

    #[test]
    fn arc_curves_follow_the_circle() {
        let mut ap = Ap::new();
        ap.move_to(p(10.0, 0.0));
        arc_curves(&mut ap, p(0.0, 0.0), 10.0, 0.0, PI);
        // two quarter curves ending at (0, 10) and (-10, 0)
        assert_eq!(ap.s.matches(" c ").count(), 2);
        assert!(ap.s.contains("0.000 10.000 c") && ap.s.contains("-10.000 0.000 c"));
        let mut bad = Ap::new();
        arc_curves(&mut bad, p(0.0, 0.0), f64::NAN, 0.0, 1.0);
        assert!(bad.s.is_empty());
    }
}
