//! Dimension and Arc markups, and the caption, slope and centroid keys every measurement can
//! carry.
//!
//! Storage (MarkupCraft's own until Revu's is checked against a real file):
//!   Dimension `/Line` `/IT /PCDimension`, `/L` = the two measured points, `/LL` = the offset of
//!             the dimension line (ISO 32000-1 §12.5.6.7 leader length), `/LLE` = how far the
//!             extension lines run past it, `/LE` the two arrowheads, `/Contents` the text
//!   Arc       `/PolyLine` `/IT /PCArc`, `/Vertices` = [start, a point on the arc, end]
//!   Measurements: `/SlopeType` (Revu's key) with its value in `/PCSlope`, `/PCCaption` (the
//!             caption's contents template), `/PCCapLeader`, `/PCCentroid`

use std::f64::consts::PI;

use markupcraft_geom::{Point, Rect};
use markupcraft_model::measure_extras::{arc_sweep, circle_through};
use markupcraft_model::{Kind, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use super::AnnotKind;
use super::common::{draw_ending, set_or_remove};
use super::measure_more::arc_curves;
use crate::ap::Ap;
use crate::pdf::{self, n, points_arr, real, s};

/// Our `/IT` for a Dimension.
pub const DIMENSION_INTENT: &str = "PCDimension";
/// Our `/IT` for an Arc.
pub const ARC_INTENT: &str = "PCArc";

// ---- Dimension ----------------------------------------------------------------------------

/// The geometry of a dimension: (dimension line start, end, extension line ends at a and b).
pub fn dimension_lines(m: &Markup) -> Option<(Point, Point, [Point; 2], [Point; 2])> {
    let (a, b) = (*m.pts.first()?, *m.pts.get(1)?);
    let l = a.dist(b);
    if l <= 0.0 || !l.is_finite() {
        return None;
    }
    let nrm = Point::new(-(b.y - a.y) / l, (b.x - a.x) / l);
    let off = |p: Point, d: f64| Point::new(p.x + nrm.x * d, p.y + nrm.y * d);
    let (da, db) = (off(a, m.leader), off(b, m.leader));
    let ext = m.leader + m.leader_ext.copysign(if m.leader == 0.0 { 1.0 } else { m.leader });
    // a small gap between the measured point and the extension line, as drafting does
    let gap = if m.leader == 0.0 {
        0.0
    } else {
        2.0_f64.copysign(m.leader).clamp(-m.leader.abs(), m.leader.abs())
    };
    Some((da, db, [off(a, gap), off(a, ext)], [off(b, gap), off(b, ext)]))
}

fn draw_dimension(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let Some((da, db, ea, eb)) = dimension_lines(m) else {
        if !m.pts.is_empty() {
            ap.path(&m.pts, false).op("S\n");
        }
        return;
    };
    ap.op("0 J 0 j\n");
    if m.leader != 0.0 {
        ap.move_to(ea[0]).line_to(ea[1]).move_to(eb[0]).line_to(eb[1]);
    }
    ap.move_to(da).line_to(db).op("S\n");
    extent.extend([da, db, ea[0], ea[1], eb[0], eb[1]]);
    draw_ending(ap, m, db, da, &m.line_start, m.line_width, extent);
    draw_ending(ap, m, da, db, &m.line_end, m.line_width, extent);
    let text = m.contents.lines().next().unwrap_or_default();
    if text.is_empty() {
        return;
    }
    // The text centred on the dimension line, upright, just above it.
    let mut ang = (db.y - da.y).atan2(db.x - da.x);
    if ang > PI / 2.0 + 1e-9 || ang <= -PI / 2.0 + 1e-9 {
        ang += if ang > 0.0 { -PI } else { PI };
    }
    let (c, sn) = (ang.cos(), ang.sin());
    let size = m.text.size.clamp(2.0, 144.0);
    let w = 0.55 * size * text.chars().count() as f64;
    let mid = da.mid(db);
    let start = Point::new(mid.x - c * w / 2.0 - sn * 2.0, mid.y - sn * w / 2.0 + c * 2.0);
    ap.op("BT /Helv ")
        .nums(&[size], "Tf")
        .fill_rgb(&m.text.color)
        .nums(&[c, sn, -sn, c, start.x, start.y], "Tm");
    ap.op(&format!("({}) Tj ET\n", Ap::text_literal(text)));
    for dx in [0.0, w] {
        for dy in [0.0, size] {
            extent.push(Point::new(start.x + c * dx - sn * dy, start.y + sn * dx + c * dy));
        }
    }
}

fn dimension_geometry(a: &mut Dict, m: &Markup) {
    let two: Vec<Point> = m.pts.iter().take(2).copied().collect();
    pdf::set(a, "L", points_arr(&two));
    pdf::set(a, "LL", real(m.leader));
    pdf::set(a, "LLE", real(m.leader_ext.abs()));
    let end = |v: &str| n(if v.is_empty() { "None" } else { v });
    pdf::set(a, "LE", Object::Array(vec![end(&m.line_start), end(&m.line_end)]));
    pdf::set(a, "Cap", Object::Bool(!m.contents.is_empty()));
}

fn read_dimension(_cos: &CosDoc, a: &Dict, m: &mut Markup) {
    m.leader = pdf::num_or(a.get(b"LL"), 0.0);
    m.leader_ext = pdf::num_or(a.get(b"LLE"), 0.0).abs();
}

pub static DIMENSION: AnnotKind = AnnotKind {
    draw_shape: Some(draw_dimension),
    write_geometry: Some(dimension_geometry),
    read_keys: Some(read_dimension),
    ..AnnotKind::new(Kind::Dimension, "Line", Some(DIMENSION_INTENT), 0, false)
};

// ---- Arc ----------------------------------------------------------------------------------

/// The arc through `[start, through, end]` as points (`n` chords), or the points themselves
/// when they are in a line.
pub fn arc_polyline(m: &Markup, n_chords: usize) -> Vec<Point> {
    let [a, t, b] = match m.pts.as_slice() {
        [a, t, b, ..] => [*a, *t, *b],
        _ => return m.pts.clone(),
    };
    match circle_through(a, t, b) {
        Some((c, r)) => {
            let (start, sweep) = arc_sweep(c, a, t, b);
            let k = n_chords.clamp(2, 720);
            (0..=k)
                .map(|i| {
                    let ang = start + sweep * i as f64 / k as f64;
                    Point::new(c.x + r * ang.cos(), c.y + r * ang.sin())
                })
                .collect()
        }
        None => vec![a, t, b],
    }
}

fn draw_arc(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let [a, t, b] = match m.pts.as_slice() {
        [a, t, b, ..] => [*a, *t, *b],
        _ => {
            if !m.pts.is_empty() {
                ap.path(&m.pts, false).op("S\n");
            }
            return;
        }
    };
    ap.op("0 J 0 j\n");
    match circle_through(a, t, b) {
        Some((c, r)) => {
            let (start, sweep) = arc_sweep(c, a, t, b);
            ap.move_to(a);
            arc_curves(ap, c, r, start, sweep);
            ap.op("S\n");
            extent.extend(arc_polyline(m, 32));
        }
        None => {
            ap.path(&[a, t, b], false).op("S\n");
        }
    }
    let pts = arc_polyline(m, 32);
    let k = pts.len();
    if k >= 2
        && let (Some(a0), Some(a1), Some(b0), Some(b1)) = (pts.first(), pts.get(1), pts.get(k - 2), pts.last())
    {
        draw_ending(ap, m, *a1, *a0, &m.line_start, m.line_width, extent);
        draw_ending(ap, m, *b0, *b1, &m.line_end, m.line_width, extent);
    }
}

fn arc_geometry(a: &mut Dict, m: &Markup) {
    let three: Vec<Point> = m.pts.iter().take(3).copied().collect();
    pdf::set(a, "Vertices", points_arr(&three));
    let set = |v: &str| !v.is_empty() && v != "None";
    if set(&m.line_start) || set(&m.line_end) {
        let end = |v: &str| n(if v.is_empty() { "None" } else { v });
        pdf::set(a, "LE", Object::Array(vec![end(&m.line_start), end(&m.line_end)]));
    } else {
        pdf::remove(a, "LE");
    }
}

pub static ARC: AnnotKind = AnnotKind {
    draw_shape: Some(draw_arc),
    write_geometry: Some(arc_geometry),
    ..AnnotKind::new(Kind::Arc, "PolyLine", Some(ARC_INTENT), 0, false)
};

// ---- File Attachment ----------------------------------------------------------------------

/// The icons a File Attachment can show (`/Name`).
pub const ATTACHMENT_ICONS: &[&str] = &["PushPin", "Paperclip", "Graph", "Tag"];

/// Largest file embedded in a File Attachment markup.
pub const MAX_ATTACHMENT: usize = 50 << 20;

fn attachment_geometry(a: &mut Dict, m: &Markup) {
    let icon = if ATTACHMENT_ICONS.contains(&m.icon.as_str()) {
        m.icon.as_str()
    } else {
        "PushPin"
    };
    pdf::set(a, "Name", n(icon));
}

fn attachment_rect(m: &Markup) -> Rect {
    super::common::box_of(m)
}

/// A new attachment embeds its file: `/FS << /Type /Filespec /F /UF /EF << /F stream >> >>`.
fn attachment_file(cos: &mut CosDoc, a: &mut Dict, m: &mut Markup) {
    if a.contains(b"FS") {
        return;
    }
    let data = m.attachment_data.take().unwrap_or_default();
    let name = if m.attachment_name.trim().is_empty() {
        "attachment".to_string()
    } else {
        m.attachment_name.clone()
    };
    let mut params = Dict::new();
    pdf::set(&mut params, "Size", Object::Int(data.len() as i64));
    let mut sd = pdf::dict(&[("Type", n("EmbeddedFile"))]);
    pdf::set(&mut sd, "Params", Object::Dict(params));
    let stream = cos.add(Object::Stream(pdfcraft_cos::Stream::flate(sd, &data)));
    let ef = pdf::dict(&[("F", Object::Ref(stream)), ("UF", Object::Ref(stream))]);
    let spec = pdf::dict(&[
        ("Type", n("Filespec")),
        ("F", s(&name)),
        ("UF", s(&name)),
        ("EF", Object::Dict(ef)),
    ]);
    pdf::set(a, "FS", Object::Dict(spec));
}

fn read_attachment(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    m.icon = pdf::name(a.get(b"Name"));
    let fs = cos.resolve(a.get(b"FS").unwrap_or(&Object::Null));
    if let Some(fs) = fs.as_dict() {
        let uf = pdf::text(fs.get(b"UF"));
        m.attachment_name = if uf.is_empty() { pdf::text(fs.get(b"F")) } else { uf };
    }
}

pub static ATTACHMENT: AnnotKind = AnnotKind {
    draw_shape: Some(super::textmarkup::draw_marks),
    write_geometry: Some(attachment_geometry),
    rect_of: Some(attachment_rect),
    read_keys: Some(read_attachment),
    finish: Some(attachment_file),
    ..AnnotKind::new(Kind::Attachment, "FileAttachment", None, 0, false)
};

/// The bytes of a File Attachment markup's embedded file (`None` without one).
pub fn attachment_bytes(cos: &CosDoc, a: &Dict) -> Option<Vec<u8>> {
    let fs = cos.resolve(a.get(b"FS")?);
    let fs = fs.as_dict()?;
    let ef = cos.resolve(fs.get(b"EF")?);
    let ef = ef.as_dict()?;
    let st = cos.resolve(ef.get(b"UF").or(ef.get(b"F"))?);
    match &*st {
        Object::Stream(s) => s.decoded_within(MAX_ATTACHMENT).ok(),
        _ => None,
    }
}

// ---- measurement extras -------------------------------------------------------------------

/// Slope, caption contents, caption leader and centroid keys of a measurement (written only
/// when used, so markups without them keep exactly the keys they had).
pub fn write_measure_extras(a: &mut Dict, m: &Markup) {
    let slope = m.slope_of();
    if slope.type_code() != 0 {
        pdf::set(a, "SlopeType", Object::Int(slope.type_code()));
        pdf::set(a, "PCSlope", real(slope.value()));
    } else if a.contains(b"PCSlope") {
        pdf::set(a, "SlopeType", Object::Int(0));
        pdf::remove(a, "PCSlope");
    }
    set_or_remove(a, "PCCaption", !m.caption_template.is_empty(), s(&m.caption_template));
    set_or_remove(a, "PCCapLeader", m.caption_leader, Object::Bool(true));
    set_or_remove(a, "PCCentroid", m.show_centroid, Object::Bool(true));
    set_or_remove(a, "PCCaptionLastSeg", m.caption_last_segment, Object::Bool(true));
    let dims = m.kind == Kind::Count && (m.item_width > 0.0 || m.item_height > 0.0 || m.depth > 0.0);
    set_or_remove(
        a,
        "PCCountDims",
        dims,
        Object::Array(vec![real(m.item_width), real(m.item_height), real(m.depth)]),
    );
}

/// Append the caption's bold, italic, underline, strike-through and super / subscript to a
/// measurement's `/DS` (CSS, as text boxes write them); nothing when none is set.
pub fn caption_style_css(t: &markupcraft_model::TextStyle, ds: &mut String) {
    if t.bold {
        ds.push_str("; font-weight:bold");
    }
    if t.italic {
        ds.push_str("; font-style:italic");
    }
    match (t.underline, t.strike) {
        (true, true) => ds.push_str("; text-decoration:underline line-through"),
        (true, false) => ds.push_str("; text-decoration:underline"),
        (false, true) => ds.push_str("; text-decoration:line-through"),
        (false, false) => {}
    }
    match t.script {
        1 => ds.push_str("; vertical-align:super"),
        -1 => ds.push_str("; vertical-align:sub"),
        _ => {}
    }
}

/// Underline and strike-through lines of one caption line whose baseline starts at `at`, `w`
/// wide, in colour `c`.
pub fn caption_decorations(
    ap: &mut Ap,
    t: &markupcraft_model::TextStyle,
    c: &markupcraft_model::Color,
    at: (f64, f64),
    w: f64,
    size: f64,
) {
    let lw = (size / 14.0).max(0.5);
    for (on, dy) in [(t.underline, -size * 0.12), (t.strike, size * 0.3)] {
        if on {
            ap.op("q ")
                .stroke_rgb(c)
                .nums(&[lw], "w")
                .nums(&[at.0, at.1 + dy], "m")
                .nums(&[at.0 + w, at.1 + dy], "l")
                .op("S Q\n");
        }
    }
}

/// Read what [`write_measure_extras`] writes. A Revu `/SlopeType` without our value is left
/// alone (its value is not known).
pub fn read_measure_extras(a: &Dict, m: &mut Markup) {
    if let Some(v) = pdf::num(a.get(b"PCSlope")) {
        m.slope_type = a.int(b"SlopeType").unwrap_or(0);
        m.slope = v;
    }
    m.caption_template = pdf::text(a.get(b"PCCaption"));
    m.caption_leader = pdf::boolean(a.get(b"PCCapLeader")).unwrap_or(false);
    m.show_centroid = pdf::boolean(a.get(b"PCCentroid")).unwrap_or(false);
    m.caption_last_segment = pdf::boolean(a.get(b"PCCaptionLastSeg")).unwrap_or(false);
    // the caption's styles written after Revu's string (bold, italic, decorations, script)
    {
        let mut st = m.text.clone();
        super::text::parse_text_css(&pdf::text(a.get(b"DS")), &mut st);
        m.text.bold = st.bold;
        m.text.italic = st.italic;
        m.text.underline = st.underline;
        m.text.strike = st.strike;
        m.text.script = st.script;
    }
    // the caption's font size, from /DS (`font: Helvetica 12pt; ...`)
    let ds = pdf::text(a.get(b"DS"));
    if let Some(sz) = ds
        .split(';')
        .find_map(|p| p.trim().strip_prefix("font:"))
        .and_then(|f| {
            f.split_whitespace()
                .find_map(|w| w.strip_suffix("pt")?.parse::<f64>().ok())
        })
        .filter(|v| v.is_finite() && (2.0..=144.0).contains(v))
    {
        m.text.size = sz;
    }
    if m.kind == Kind::Count
        && let Some(d) = a.get(b"PCCountDims").and_then(Object::as_array)
    {
        let v = |i: usize| {
            d.get(i)
                .and_then(|o| pdf::num(Some(o)))
                .filter(|x| x.is_finite() && *x >= 0.0)
        };
        m.item_width = v(0).unwrap_or(0.0);
        m.item_height = v(1).unwrap_or(0.0);
        m.depth = v(2).unwrap_or(0.0);
    }
}

/// The centroid mark (a small cross in a circle) of an Area or Volume.
pub fn draw_centroid(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    if !m.show_centroid {
        return;
    }
    let Some(c) = markupcraft_model::measure_extras::centroid(m) else {
        return;
    };
    let r = 4.0;
    ap.move_to(Point::new(c.x - r, c.y))
        .line_to(Point::new(c.x + r, c.y))
        .move_to(Point::new(c.x, c.y - r))
        .line_to(Point::new(c.x, c.y + r))
        .op("S\n");
    ap.move_to(Point::new(c.x + r * 0.6, c.y));
    arc_curves(ap, c, r * 0.6, 0.0, 2.0 * PI);
    ap.op("S\n");
    extent.push(Point::new(c.x - r, c.y - r));
    extent.push(Point::new(c.x + r, c.y + r));
}

/// The leader from a moved caption back to its markup.
pub fn draw_caption_leader(ap: &mut Ap, m: &Markup, at: Point, extent: &mut Vec<Point>) {
    if !m.caption_leader || m.caption_offset.is_none() {
        return;
    }
    let from = markupcraft_model::caption::default_caption_anchor(m);
    ap.op("q ")
        .stroke_rgb(&m.color)
        .nums(&[m.line_width.clamp(0.25, 2.0)], "w")
        .move_to(at)
        .line_to(from)
        .op("S Q\n");
    extent.push(from);
}

/// A rectangle around `pts` (for the tests and callers sizing appearances).
pub fn extent_of(pts: &[Point]) -> Option<Rect> {
    markupcraft_geom::bbox(pts)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use markupcraft_measure::Scale;
    use markupcraft_model::{Document, Point};

    use super::*;
    use crate::kinds::{classify, kind_for};
    use crate::{SaveMode, open, open_bytes, save};

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

    fn round_trip(markups: Vec<Markup>) -> (Document, Vec<Dict>) {
        static N: AtomicUsize = AtomicUsize::new(0);
        let out = std::env::temp_dir().join(format!(
            "markupcraft_more_{}_{}.pdf",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let (mut file, mut doc) = open_bytes(Arc::new(blank_pdf()), &PathBuf::from("blank.pdf")).expect("opens");
        doc.markups.extend(markups);
        save(&mut file, &mut doc, &out, SaveMode::Incremental).expect("save");
        let (file2, back) = open(&out).expect("reopen");
        let mut raw = Vec::new();
        for pg in pdf::pages(&file2.cos) {
            let annots = file2.cos.resolve(pg.dict.get(b"Annots").unwrap_or(&Object::Null));
            for o in annots.as_array().cloned().unwrap_or_default() {
                if let Some(d) = file2.cos.resolve(&o).as_dict() {
                    raw.push(d.clone());
                }
            }
        }
        drop(file2);
        let _ = std::fs::remove_file(&out);
        (back, raw)
    }

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn dimension_and_arc_round_trip() {
        assert_eq!(classify("Line", DIMENSION_INTENT, 0), Kind::Dimension);
        assert_eq!(classify("PolyLine", ARC_INTENT, 0), Kind::Arc);
        assert_eq!(kind_for(Kind::Dimension).subtype, "Line");
        let mut d = Markup::new(Kind::Dimension, 0, vec![p(100.0, 100.0), p(300.0, 100.0)]);
        d.subject = "dim".into();
        d.leader = 24.0;
        d.leader_ext = 6.0;
        d.line_start = "OpenArrow".into();
        d.line_end = "OpenArrow".into();
        d.contents = "12'-0\"".into();
        let mut arc = Markup::new(Kind::Arc, 0, vec![p(100.0, 400.0), p(150.0, 450.0), p(200.0, 400.0)]);
        arc.subject = "arc".into();
        let (back, raw) = round_trip(vec![d, arc]);
        let d = back
            .markups
            .iter()
            .find(|m| m.subject == "dim")
            .expect("dimension reloads");
        assert_eq!(d.kind, Kind::Dimension);
        assert_eq!(d.pts, vec![p(100.0, 100.0), p(300.0, 100.0)]);
        assert!((d.leader - 24.0).abs() < 1e-9 && (d.leader_ext - 6.0).abs() < 1e-9);
        assert_eq!((d.line_start.as_str(), d.line_end.as_str()), ("OpenArrow", "OpenArrow"));
        assert_eq!(d.contents, "12'-0\"");
        // The appearance covers the dimension line 24 pt above the points and its text.
        assert!(d.rect.y1 > 124.0 + 6.0, "{:?}", d.rect);
        let a = back.markups.iter().find(|m| m.subject == "arc").expect("arc reloads");
        assert_eq!(a.kind, Kind::Arc);
        assert_eq!(a.pts.len(), 3);
        // the arc bulges to y = 450 and its rect covers it
        assert!(a.rect.y1 >= 450.0 && a.rect.y0 <= 400.0, "{:?}", a.rect);
        let dd = raw
            .iter()
            .find(|r| pdf::name(r.get(b"IT")) == DIMENSION_INTENT)
            .expect("raw");
        assert!(dd.contains(b"LL") && dd.contains(b"LLE") && dd.contains(b"L"));
        let poly = arc_polyline(a, 8);
        assert_eq!(poly.len(), 9);
        assert!(poly.iter().all(|q| (q.dist(p(150.0, 400.0)) - 50.0).abs() < 1e-6));
    }

    #[test]
    fn text_margin_and_line_spacing_round_trip() {
        let r = Rect::new(100.0, 500.0, 300.0, 560.0);
        let mut t = Markup::new(Kind::Text, 0, r.corners().to_vec());
        t.rect = r;
        t.subject = "spaced".into();
        t.contents = "one\rtwo".into();
        t.text.margin = 6.0;
        t.text.line_spacing = 1.5;
        let mut single = t.clone();
        single.subject = "single".into();
        single.text.margin = 0.0;
        single.text.line_spacing = 1.0;
        // wider margins and spacing need a taller box
        let (mut a, mut b) = (t.clone(), single.clone());
        crate::kinds::text::fit_text_box(&mut a);
        crate::kinds::text::fit_text_box(&mut b);
        let h = |m: &Markup| markupcraft_geom::bbox(&m.pts).map_or(0.0, |r| r.height());
        assert!(h(&a) > h(&b) + 12.0, "{} vs {}", h(&a), h(&b));
        let lines = crate::kinds::text::markup_lines(&t, r);
        assert!((lines[0].y - lines[1].y - 1.15 * 12.0 * 1.5).abs() < 1e-9);
        let (back, raw) = round_trip(vec![t, single]);
        let s = back.markups.iter().find(|m| m.subject == "spaced").expect("reloads");
        assert_eq!((s.text.margin, s.text.line_spacing), (6.0, 1.5));
        let p = back.markups.iter().find(|m| m.subject == "single").expect("reloads");
        assert_eq!((p.text.margin, p.text.line_spacing), (0.0, 1.0));
        let plain = raw.iter().find(|d| pdf::text(d.get(b"Subj")) == "single").expect("raw");
        assert!(!plain.contains(b"PCTextMargin") && !plain.contains(b"PCLineSpacing"));
    }

    #[test]
    fn file_attachment_embeds_its_file() {
        assert_eq!(classify("FileAttachment", "", 0), Kind::Attachment);
        let r = Rect::new(100.0, 600.0, 118.0, 624.0);
        let mut m = Markup::new(Kind::Attachment, 0, r.corners().to_vec());
        m.rect = r;
        m.subject = "att".into();
        m.icon = "Paperclip".into();
        m.attachment_name = "notes.txt".into();
        m.attachment_data = Some(std::sync::Arc::new(b"hello attachment".to_vec()));
        let (back, raw) = round_trip(vec![m]);
        let a = back.markups.iter().find(|m| m.subject == "att").expect("reloads");
        assert_eq!(a.kind, Kind::Attachment);
        assert_eq!(
            (a.icon.as_str(), a.attachment_name.as_str()),
            ("Paperclip", "notes.txt")
        );
        assert!(a.attachment_data.is_none(), "the file lives in the PDF now");
        let d = raw
            .iter()
            .find(|d| pdf::name(d.get(b"Subtype")) == "FileAttachment")
            .expect("raw");
        assert!(d.contains(b"FS") && d.contains(b"AP"));
    }

    #[test]
    fn measurement_extras_round_trip() {
        let sq = vec![p(100.0, 100.0), p(190.0, 100.0), p(190.0, 190.0), p(100.0, 190.0)];
        let mut area = Markup::new(Kind::Area, 0, sq.clone());
        area.subject = "sloped".into();
        area.scale = Some(Scale::architectural(0.125, 1.0));
        area.slope_type = 1;
        area.slope = 12.0;
        area.caption_template = "{subject}: {value}".into();
        area.caption_leader = true;
        area.caption_offset = Some(p(80.0, 0.0));
        area.show_centroid = true;
        area.text.size = 16.0;
        let mut plain = Markup::new(Kind::Area, 0, sq);
        plain.subject = "plain".into();
        plain.scale = Some(Scale::architectural(0.125, 1.0));
        let want = area.quantity_text();
        let mut count = Markup::new(Kind::Count, 0, vec![p(300.0, 300.0), p(320.0, 300.0)]);
        count.subject = "diffusers".into();
        (count.item_width, count.item_height, count.depth) = (2.0, 2.0, 0.5);
        let (back, raw) = round_trip(vec![area, plain, count]);
        let c = back
            .markups
            .iter()
            .find(|m| m.subject == "diffusers")
            .expect("count reloads");
        assert_eq!((c.item_width, c.item_height, c.depth), (2.0, 2.0, 0.5));
        let a = back.markups.iter().find(|m| m.subject == "sloped").expect("reloads");
        assert_eq!((a.slope_type, a.slope), (1, 12.0));
        // 12:12 pitch: 100 sf of plan is 141.42 sf of roof
        assert_eq!(want, "141.42 sf");
        assert_eq!(a.quantity_text(), want);
        assert_eq!(a.caption_template, "{subject}: {value}");
        assert!(a.caption_leader && a.show_centroid);
        assert_eq!(a.text.size, 16.0, "the caption size comes back from /DS");
        let pl = back.markups.iter().find(|m| m.subject == "plain").expect("reloads");
        assert_eq!((pl.slope_type, pl.caption_leader, pl.show_centroid), (0, false, false));
        for r in &raw {
            let sloped = pdf::text(r.get(b"Subj")) == "sloped";
            assert_eq!(r.contains(b"PCSlope"), sloped);
            assert_eq!(r.contains(b"PCCaption"), sloped);
            if sloped {
                assert_eq!(r.int(b"SlopeType"), Some(1));
            }
        }
        assert!(extent_of(&[p(0.0, 0.0), p(1.0, 2.0)]).is_some());
    }
}
