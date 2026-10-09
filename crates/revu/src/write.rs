//! Writing markups back as annotations: the part every kind shares. Per-kind differences come
//! from the [`crate::kinds`] registry.

use std::collections::HashSet;

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Document, Kind, Markup, caption};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object, Stream};

use crate::ap::{Ap, f3};
use crate::kinds::{self, AnnotKind};
use crate::pdf::{self, color_arr, n, real, rect_arr, s};
use crate::{extras, new_markup_id, pdf_date_now, scale};

/// The font resource name and base font for a text style.
pub fn base_font(font: &str, bold: bool, italic: bool) -> String {
    use markupcraft_geom::text::{Font, FontFamily};
    Font {
        family: FontFamily::of(font),
        bold,
        italic,
        size: 0.0,
    }
    .base_name()
    .to_string()
}

fn type1_font(base: &str) -> Object {
    Object::Dict(pdf::dict(&[
        ("Type", n("Font")),
        ("Subtype", n("Type1")),
        ("BaseFont", n(base)),
        ("Encoding", n("WinAnsiEncoding")),
    ]))
}

/// Build the `/AP /N` stream for a markup and set its `/Rect`.
fn appearance(cos: &mut CosDoc, m: &mut Markup, ak: &AnnotKind) -> Object {
    let mut ap = Ap::new();
    let c = m.color;
    let f = m.fill.unwrap_or(c);
    ap.op("q /GS0 gs ")
        .stroke_rgb(&c)
        .fill_rgb(&f)
        .nums(&[m.line_width], "w")
        .op("1 j 1 J\n");
    if !m.dash.is_empty() {
        ap.op("[");
        for d in &m.dash {
            ap.op(&f3(*d)).op(" ");
        }
        ap.op("] 0 d\n");
    }
    let mut extent = m.pts.clone();
    if let Some(draw) = ak.draw_shape {
        draw(&mut ap, m, &mut extent);
    } else if !m.pts.is_empty() {
        ap.path(&m.pts, false);
        ap.op(if ak.closed {
            if m.fill.is_some() { "h B\n" } else { "h S\n" }
        } else {
            "S\n"
        });
    }
    ap.op("Q\n");

    // The quantity, drawn like Revu does, so other viewers show it too.
    let label = if m.kind.is_measurement() && m.kind != Kind::Count {
        m.quantity_text()
    } else {
        String::new()
    };
    if !label.is_empty() {
        let a = caption::caption_anchor(m);
        let size = 10.0;
        let w = 0.55 * size * label.chars().count() as f64;
        ap.op("BT /Helv ")
            .nums(&[size], "Tf")
            .fill_rgb(&c)
            .nums(&[a.x - w / 2.0, a.y + 3.0], "Td");
        ap.op(&format!("({}) Tj ET\n", Ap::text_literal(&label)));
        extent.push(Point::new(a.x - w / 2.0, a.y));
        extent.push(Point::new(a.x + w / 2.0, a.y + 3.0 + size));
    }

    let r = match ak.rect_of {
        Some(rect_of) => rect_of(m),
        None => bbox(&extent).unwrap_or_default().padded(m.line_width + 1.0),
    };
    m.rect = r;

    let gs = pdf::dict(&[
        ("Type", n("ExtGState")),
        ("CA", real(m.opacity)),
        ("ca", real(if ak.closed { m.fill_opacity } else { m.opacity })),
    ]);
    let mut fonts = pdf::dict(&[("Helv", type1_font("Helvetica"))]);
    if m.kind.is_text() || !m.stamp.is_empty() {
        // the markup's own font, under the resource name the kind's drawing uses
        let f = kinds::common::font_of(&m.text);
        pdf::set(&mut fonts, f.res_name(), type1_font(f.base_name()));
    }
    let res = pdf::dict(&[
        ("ExtGState", Object::Dict(pdf::dict(&[("GS0", Object::Dict(gs))]))),
        ("Font", Object::Dict(fonts)),
    ]);
    let mut d = pdf::dict(&[
        ("Type", n("XObject")),
        ("Subtype", n("Form")),
        ("FormType", Object::Int(1)),
        ("BBox", rect_arr(&r)),
        ("Resources", Object::Dict(res)),
    ]);
    if !m.multiply {
        return Object::Stream(Stream::flate(d, &ap.bytes()));
    }
    // Multiply blend (Highlight, Revu's multiply fills), the way Revu writes it: an outer form
    // sets /BM /Multiply and draws the shape as a transparency-group form.
    pdf::set(&mut d, "Group", Object::Dict(pdf::dict(&[("S", n("Transparency"))])));
    let inner = cos.add(Object::Stream(Stream::flate(d, &ap.bytes())));
    let outer = pdf::dict(&[
        ("Type", n("XObject")),
        ("Subtype", n("Form")),
        ("FormType", Object::Int(1)),
        ("BBox", rect_arr(&r)),
        (
            "Resources",
            Object::Dict(pdf::dict(&[
                (
                    "ExtGState",
                    Object::Dict(pdf::dict(&[(
                        "GSM",
                        Object::Dict(pdf::dict(&[("Type", n("ExtGState")), ("BM", n("Multiply"))])),
                    )])),
                ),
                ("XObject", Object::Dict(pdf::dict(&[("MForm", Object::Ref(inner))]))),
            ])),
        ),
    ]);
    Object::Stream(Stream::flate(outer, b"/GSM gs /MForm Do\n"))
}

/// `Helv`, `HeBo`, `TiRo` ... resource names for the base-14 fonts (as `/DA` and our
/// appearance streams use them).
pub fn font_res_name(base: &str) -> String {
    use markupcraft_geom::text::{Font, FontFamily};
    let f = Font {
        family: FontFamily::of(base),
        bold: base.contains("Bold"),
        italic: base.contains("Italic") || base.contains("Oblique"),
        size: 0.0,
    };
    f.res_name().to_string()
}

fn write_geometry_by_subtype(a: &mut Dict, m: &Markup) {
    kinds::measure::geometry_by_subtype(a, m);
}

/// Write every field we own onto annotation dictionary `a` (new or existing).
pub fn write_annot(cos: &mut CosDoc, a: &mut Dict, m: &mut Markup, page: ObjRef) {
    let ak = kinds::kind_for(m.kind);
    if m.subtype.is_empty() || !m.in_file() {
        pdf::set(a, "Type", n("Annot"));
        pdf::set(a, "Subtype", n(ak.subtype));
        if let Some(it) = ak.intent {
            pdf::set(a, "IT", n(it));
        }
        if ak.code != 0 {
            pdf::set(a, "MeasurementTypes", Object::Int(ak.code));
        }
        pdf::set(a, "F", Object::Int(m.flags));
        if m.id.is_empty() {
            m.id = new_markup_id();
        }
        pdf::set(a, "NM", s(&m.id));
        if m.created.is_empty() {
            m.created = pdf_date_now();
        }
        pdf::set(a, "CreationDate", s(&m.created));
        if let Some(ck) = ak.create_keys {
            ck(a, m);
        }
        m.subtype = ak.subtype.to_string();
        m.intent = ak.intent.unwrap_or("").to_string();
        m.measure_code = ak.code;
    }
    pdf::set(a, "P", Object::Ref(page));
    // Flags (Lock): written only when they differ from the file.
    if a.int(b"F").unwrap_or(0) != m.flags {
        pdf::set(a, "F", Object::Int(m.flags));
    }
    m.modified = pdf_date_now();
    pdf::set(a, "M", s(&m.modified));
    pdf::set(a, "Subj", s(&m.subject));
    pdf::set(a, "T", s(&m.author));
    pdf::set(a, "Label", s(&m.label));
    pdf::set(a, "C", color_arr(&m.color));
    if let Some(fill) = &m.fill {
        pdf::set(a, "IC", color_arr(fill));
    }
    pdf::set(a, "CA", real(m.opacity));
    pdf::set(a, "FillOpacity", real(m.fill_opacity));
    let mut bs = pdf::dict(&[
        ("Type", n("Border")),
        ("W", real(m.line_width)),
        ("S", n(if m.dash.is_empty() { "S" } else { "D" })),
    ]);
    if !m.dash.is_empty() {
        pdf::set(&mut bs, "D", Object::Array(m.dash.iter().map(|v| real(*v)).collect()));
    }
    pdf::set(a, "BS", Object::Dict(bs));

    // Annotations MarkupCraft does not draw itself keep their geometry keys, /AP and /Rect.
    let own = !m.in_file() || kinds::app_draws(&m.subtype, &m.intent, !m.stamp.is_empty(), m.foreign_look);
    if !own {
        if m.subtype == "FreeText" {
            // FreeText's /C is its fill
            pdf::set(a, "C", m.fill.as_ref().map_or(Object::Array(Vec::new()), color_arr));
            pdf::remove(a, "IC");
        }
        if let Some(sc) = &m.scale {
            pdf::set(a, "Measure", scale::object(sc));
        }
        pdf::set(a, "Contents", s(&m.contents));
        extras::write_markup(cos, a, m, page);
        return;
    }

    if matches!(m.subtype.as_str(), "Polygon" | "PolyLine" | "Line" | "Ink") {
        pdf::remove(a, "Rotation");
    }
    match ak.write_geometry {
        Some(wg) => wg(a, m),
        None => write_geometry_by_subtype(a, m),
    }
    if let Some(sc) = &m.scale {
        pdf::set(a, "Measure", scale::object(sc));
    }
    if m.kind.is_measurement() {
        m.contents = if m.kind == Kind::Count {
            format!("{} ea", m.quantity().unwrap_or(0.0) as i64)
        } else {
            m.quantity_text()
        };
    }
    pdf::set(a, "Contents", s(&m.contents));
    if m.kind.is_measurement() {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let ds = format!(
            "font: Helvetica 12pt; text-align:center; line-height:13.8pt; color:#{:02X}{:02X}{:02X}",
            c(m.color.r),
            c(m.color.g),
            c(m.color.b)
        );
        pdf::set(a, "DS", s(&ds));
        if m.kind == Kind::Length && m.fill.is_none() {
            pdf::set(a, "IC", color_arr(&m.color));
        }
    }
    let ap = appearance(cos, m, ak);
    let ap_ref = cos.add(ap);
    pdf::set(a, "AP", Object::Dict(pdf::dict(&[("N", Object::Ref(ap_ref))])));
    pdf::set(a, "Rect", rect_arr(&m.rect));
    if let Some(fin) = ak.finish {
        fin(cos, a, m);
    }
    extras::write_markup(cos, a, m, page);
}

/// The page's `/Annots` array (resolving a reference) and where it lives.
pub(crate) fn annots_of(cos: &CosDoc, page: ObjRef) -> (Vec<Object>, Option<ObjRef>) {
    let p = cos.get(page);
    let Some(pd) = p.as_dict() else {
        return (Vec::new(), None);
    };
    match pd.get(b"Annots") {
        Some(Object::Ref(r)) => (cos.get(*r).as_array().cloned().unwrap_or_default(), Some(*r)),
        Some(Object::Array(a)) => (a.clone(), None),
        _ => (Vec::new(), None),
    }
}

pub(crate) fn set_annots(cos: &mut CosDoc, page: ObjRef, arr: Vec<Object>, holder: Option<ObjRef>) {
    match holder {
        Some(r) => cos.set(r, Object::Array(arr)),
        None => {
            let _ = cos.update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(arr)));
        }
    }
}

/// Apply `doc` to the object layer: new/changed markups written back as annotations (with
/// appearance streams), deleted ones removed, calibrated page scales stored as full-page `/VP`.
pub fn apply(cos: &mut CosDoc, doc: &mut Document) {
    let pages: Vec<pdf::PageRef> = pdf::pages(cos);
    extras::write_document(cos, doc);

    // Remove deleted annotations (and a deleted note's /Popup) from every page's /Annots.
    let deleted: HashSet<(u32, u16)> = doc.deleted.iter().copied().collect();
    if !deleted.is_empty() {
        for pg in &pages {
            let (arr, holder) = annots_of(cos, pg.id);
            let before = arr.len();
            let kept: Vec<Object> = arr
                .into_iter()
                .filter(|o| {
                    let Some(r) = o.as_ref() else { return true };
                    if deleted.contains(&(r.num, r.generation)) {
                        return false;
                    }
                    let a = cos.get(r);
                    let popup_of_deleted = a.as_dict().is_some_and(|d| {
                        d.name(b"Subtype") == Some(b"Popup")
                            && d.reference(b"Parent")
                                .is_some_and(|p| deleted.contains(&(p.num, p.generation)))
                    });
                    !popup_of_deleted
                })
                .collect();
            if kept.len() != before {
                set_annots(cos, pg.id, kept, holder);
            }
        }
    }

    for i in 0..doc.markups.len() {
        let Some(m) = doc.markups.get_mut(i) else { continue };
        if !m.dirty {
            continue;
        }
        let Some(pg) = pages.get(m.page) else { continue };
        let page_id = pg.id;
        if m.in_file() {
            let r = ObjRef::new(m.obj.0, m.obj.1);
            let existing = cos.get(r);
            if let Some(d) = existing.as_dict() {
                let mut a = d.clone();
                write_annot(cos, &mut a, m, page_id);
                cos.set(r, Object::Dict(a));
                continue;
            }
        }
        let mut a = Dict::new();
        write_annot(cos, &mut a, m, page_id);
        let popup = a.reference(b"Popup");
        let r = cos.add(Object::Dict(a));
        // a new note's /Popup was made before the note had an object number
        if let Some(p) = popup {
            let _ = cos.update_dict(p, |d| d.set(b"Parent".to_vec(), Object::Ref(r)));
        }
        let (mut arr, holder) = annots_of(cos, page_id);
        arr.push(Object::Ref(r));
        set_annots(cos, page_id, arr, holder);
        m.obj = (r.num, r.generation);
    }

    // Z-order: each page's /Annots lists its markups in Document::markups order (later = on
    // top). Only the slots holding our markups are reordered.
    for (pi, pg) in pages.iter().enumerate() {
        let want: Vec<(u32, u16)> = doc
            .markups
            .iter()
            .filter(|m| m.page == pi && m.in_file())
            .map(|m| m.obj)
            .collect();
        if want.len() < 2 {
            continue;
        }
        let wanted: HashSet<(u32, u16)> = want.iter().copied().collect();
        let (arr, holder) = annots_of(cos, pg.id);
        let mut slots = Vec::new();
        let mut have = Vec::new();
        for (i, o) in arr.iter().enumerate() {
            if let Some(r) = o.as_ref()
                && wanted.contains(&(r.num, r.generation))
            {
                slots.push(i);
                have.push((r.num, r.generation));
            }
        }
        if have.len() != want.len() || have == want {
            continue;
        }
        let mut out = arr.clone();
        for (slot, id) in slots.iter().zip(&want) {
            if let Some(o) = out.get_mut(*slot) {
                *o = Object::Ref(ObjRef::new(id.0, id.1));
            }
        }
        set_annots(cos, pg.id, out, holder);
    }
    doc.order_changed = false;
    extras::finish_document(cos, doc);

    // Calibrated page scales become a full-page viewport, which Revu reads: box relative to the
    // media box corner, /NM id, no /Name (the shape of Revu's own page scale).
    for (i, pg) in pages.iter().enumerate() {
        let Some(info) = doc.pages.get_mut(i) else { continue };
        if !info.scale_changed {
            continue;
        }
        let Some(sc) = info.scale.clone() else { continue };
        let mut vp = Dict::new();
        pdf::set(&mut vp, "Type", n("Viewport"));
        pdf::set(
            &mut vp,
            "BBox",
            rect_arr(&Rect::new(0.0, 0.0, info.media.width(), info.media.height())),
        );
        pdf::set(&mut vp, "Measure", scale::object(&sc));
        let id = new_markup_id();
        pdf::set(&mut vp, "NM", s(&id));
        let _ = cos.update_dict(pg.id, |d| d.set(b"VP".to_vec(), Object::Array(vec![Object::Dict(vp)])));
        info.viewports = vec![markupcraft_model::Viewport {
            bbox: info.media,
            name: String::new(),
            id,
            scale: sc,
        }];
        info.scale_changed = false;
    }
}
