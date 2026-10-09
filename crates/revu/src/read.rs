//! Loading page geometry, viewports and every annotation into a [`Document`].

use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Document, Kind, Markup, PageInfo, Viewport};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use crate::kinds::{self, classify};
use crate::{extras, pdf, scale};

const NOT_MARKUPS: &[&str] = &[
    "Link",
    "Popup",
    "Widget",
    "Annot",
    "PrinterMark",
    "TrapNet",
    "Watermark",
];

/// Fill `doc` from the object layer.
pub fn load(cos: &CosDoc, path: &str) -> Document {
    let mut doc = Document {
        path: path.to_string(),
        ..Default::default()
    };
    for (pi, page) in pdf::pages(cos).iter().enumerate() {
        let mut info = PageInfo {
            media: page.media,
            crop: page.crop.unwrap_or(page.media),
            rotate: page.rotate as i32,
            ..Default::default()
        };
        read_viewports(cos, &page.dict, &mut info);
        doc.pages.push(info);

        let annots = cos.resolve(page.dict.get(b"Annots").unwrap_or(&Object::Null));
        let Some(arr) = annots.as_array() else { continue };
        for (i, item) in arr.iter().enumerate() {
            let obj = item.as_ref().map(|r| (r.num, r.generation)).unwrap_or((0, 0));
            let resolved = cos.resolve(item);
            let Some(a) = resolved.as_dict() else { continue };
            let sub = pdf::name(a.get(b"Subtype"));
            if NOT_MARKUPS.contains(&sub.as_str()) {
                continue;
            }
            let mut m = read_annot(cos, a, pi, obj);
            m.annot_index = Some(i);
            doc.markups.push(m);
        }
    }
    extras::read_document(cos, &mut doc);
    doc
}

fn read_viewports(cos: &CosDoc, page: &Dict, info: &mut PageInfo) {
    let vps = cos.resolve(page.get(b"VP").unwrap_or(&Object::Null));
    let Some(arr) = vps.as_array() else { return };
    for v in arr {
        let v = cos.resolve(v);
        let Some(v) = v.as_dict() else { continue };
        let Some(sc) = scale::read(cos, v.get(b"Measure")) else {
            continue;
        };
        let b = pdf::rect(Some(&cos.resolve(v.get(b"BBox").unwrap_or(&Object::Null)))).unwrap_or_default();
        // Revu writes viewport boxes relative to the media box corner (a full-page /VP on a
        // [-1728 -1296 1728 1296] page is [0 0 3456 2592]); annotations use plain user space.
        let (ox, oy) = (info.media.x0, info.media.y0);
        info.viewports.push(Viewport {
            bbox: Rect::new(b.x0 + ox, b.y0 + oy, b.x1 + ox, b.y1 + oy),
            name: pdf::text(v.get(b"Name")),
            id: pdf::text(v.get(b"NM")),
            scale: sc,
        });
    }
}

/// One annotation dictionary to a markup.
pub fn read_annot(cos: &CosDoc, a: &Dict, page: usize, obj: (u32, u16)) -> Markup {
    let sub = pdf::name(a.get(b"Subtype"));
    let intent = pdf::name(a.get(b"IT"));
    let code = a.int(b"MeasurementTypes").unwrap_or(0);
    let mut m = Markup {
        page,
        obj,
        kind: classify(&sub, &intent, code),
        subtype: sub.clone(),
        intent,
        measure_code: code,
        id: pdf::text(a.get(b"NM")),
        subject: pdf::text(a.get(b"Subj")),
        label: pdf::text(a.get(b"Label")),
        author: pdf::text(a.get(b"T")),
        contents: pdf::text(a.get(b"Contents")),
        created: pdf::text(a.get(b"CreationDate")),
        modified: pdf::text(a.get(b"M")),
        dirty: false,
        flags: a.int(b"F").unwrap_or(0),
        ..Default::default()
    };
    let ap = cos.resolve(a.get(b"AP").unwrap_or(&Object::Null));
    let ap_n = ap.as_dict().and_then(|d| d.get(b"N")).map(|n| cos.resolve(n));
    m.stored_look = obj.0 != 0
        && ap_n
            .as_ref()
            .is_some_and(|n| matches!(n.as_ref(), Object::Stream(_) | Object::Dict(_)));
    if let Some(c) = pdf::color(a.get(b"C")) {
        m.color = c;
    }
    m.fill = pdf::color(a.get(b"IC"));
    m.opacity = pdf::num_or(a.get(b"CA"), 1.0);
    m.fill_opacity = pdf::num_or(a.get(b"FillOpacity"), m.opacity);
    let bs = cos.resolve(a.get(b"BS").unwrap_or(&Object::Null));
    m.line_width = bs.as_dict().map_or(1.0, |d| pdf::num_or(d.get(b"W"), 1.0));
    m.rect = pdf::rect(a.get(b"Rect")).unwrap_or_default();
    m.pts = if a.contains(b"Vertices") {
        pdf::points(a.get(b"Vertices"))
    } else if a.contains(b"L") {
        pdf::points(a.get(b"L"))
    } else {
        m.rect.corners().to_vec()
    };
    if sub == "Square" && m.kind == Kind::Area {
        let rd = pdf::rect(a.get(b"RD")).unwrap_or_default();
        m.pts = Rect::new(
            m.rect.x0 + rd.x0,
            m.rect.y0 + rd.y0,
            m.rect.x1 - rd.x1,
            m.rect.y1 - rd.y1,
        )
        .corners()
        .to_vec();
    }
    m.scale = scale::read(cos, a.get(b"Measure"));
    kinds::measure::read_takeoff_keys(cos, a, &mut m);
    let oc = cos.resolve(a.get(b"OC").unwrap_or(&Object::Null));
    if let Some(oc) = oc.as_dict() {
        m.layer = pdf::text(oc.get(b"Name"));
    }
    extras::read_markup(cos, a, &mut m);
    read_markup_keys(cos, a, ap_n.as_deref(), &mut m);
    m
}

/// Keys several kinds share (`/BS /D`, `/LE`, `/BE`, `/BM`, `/PCStamp`), then the kind's own.
fn read_markup_keys(cos: &CosDoc, a: &Dict, ap_n: Option<&Object>, m: &mut Markup) {
    let bs = cos.resolve(a.get(b"BS").unwrap_or(&Object::Null));
    if let Some(bs) = bs.as_dict()
        && pdf::name(bs.get(b"S")) == "D"
    {
        if let Some(d) = bs.get(b"D").and_then(Object::as_array) {
            m.dash = d.iter().filter_map(|x| pdf::num(Some(x))).collect();
        }
        if m.dash.is_empty() {
            m.dash = vec![3.0];
        }
    }
    match a.get(b"LE") {
        Some(Object::Array(le)) if le.len() == 2 => {
            m.line_start = pdf::name(le.first());
            m.line_end = pdf::name(le.get(1));
        }
        Some(o @ Object::Name(_)) => m.line_end = pdf::name(Some(o)),
        _ => {}
    }
    let be = cos.resolve(a.get(b"BE").unwrap_or(&Object::Null));
    if let Some(be) = be.as_dict()
        && pdf::name(be.get(b"S")) == "C"
    {
        m.cloud = pdf::num_or(be.get(b"I"), 0.0);
    }
    m.multiply = pdf::name(a.get(b"BM")) == "Multiply";
    if m.kind == Kind::Ink && m.multiply {
        m.kind = Kind::Highlight;
    }
    if m.subtype == "Stamp"
        && let Some(Object::String(st)) = a.get(b"PCStamp")
    {
        m.stamp = st.to_text();
    }
    let ak = kinds::kind_for(m.kind);
    if ak.kind == m.kind
        && let Some(read) = ak.read_keys
    {
        read(cos, a, m);
    }
    let rc = pdf::text(a.get(b"RC"));
    m.foreign_look = a.contains(b"Pattern") || rc.contains("<span");
    if m.kind.is_text()
        && rc.contains("<span")
        && let Some(runs) = kinds::text::read_rich_runs(&rc, &m.text)
    {
        // Our own rich text: we draw it, so it stays editable.
        m.rich = runs;
        m.foreign_look = a.contains(b"Pattern");
    }

    // Revu's /Rotation on a vertex shape: the /AP /N /Matrix turns the stored vertices about the
    // centre of /Rect. The /AP is what Revu shows, so follow its /Matrix and keep page-space
    // points; the writer drops the key.
    let mut t = 0.0;
    if a.contains(b"Rotation")
        && let Some(Object::Stream(st)) = ap_n
        && let Some(mx) = st.dict.get(b"Matrix").and_then(Object::as_array)
        && mx.len() == 6
    {
        t = pdf::num_or(mx.get(1), 0.0).atan2(pdf::num_or(mx.first(), 1.0));
    }
    if t.abs() > 1e-6 && matches!(m.subtype.as_str(), "Polygon" | "PolyLine" | "Line" | "Ink") {
        let (c, s) = (t.cos(), t.sin());
        let (cx, cy) = ((m.rect.x0 + m.rect.x1) / 2.0, (m.rect.y0 + m.rect.y1) / 2.0);
        for p in &mut m.pts {
            let (dx, dy) = (p.x - cx, p.y - cy);
            *p = Point::new(cx + dx * c - dy * s, cy + dx * s + dy * c);
        }
    }
}
