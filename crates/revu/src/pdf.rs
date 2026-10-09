//! Small helpers over the PdfCraft object layer: typed getters, page tree walk, builders.

use std::collections::HashSet;

use markupcraft_geom::{Point, Rect};
use markupcraft_model::Color;
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString};

/// Number value of `o` (int or real), resolving nothing.
pub fn num(o: Option<&Object>) -> Option<f64> {
    match o? {
        Object::Int(i) => Some(*i as f64),
        Object::Real(r) if r.is_finite() => Some(*r),
        _ => None,
    }
}

pub fn num_or(o: Option<&Object>, dflt: f64) -> f64 {
    num(o).unwrap_or(dflt)
}

/// Text of a string or name.
pub fn text(o: Option<&Object>) -> String {
    match o {
        Some(Object::String(s)) => s.to_text(),
        Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

/// Name without the slash, "" if not a name.
pub fn name(o: Option<&Object>) -> String {
    match o {
        Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

pub fn boolean(o: Option<&Object>) -> Option<bool> {
    match o? {
        Object::Bool(b) => Some(*b),
        _ => None,
    }
}

pub fn rect(o: Option<&Object>) -> Option<Rect> {
    let a = o?.as_array()?;
    if a.len() < 4 {
        return None;
    }
    Some(Rect::new(
        num(a.first())?,
        num(a.get(1))?,
        num(a.get(2))?,
        num(a.get(3))?,
    ))
}

pub fn color(o: Option<&Object>) -> Option<Color> {
    let a = o?.as_array()?;
    let v: Vec<f64> = a.iter().map(|x| num(Some(x)).unwrap_or(0.0)).collect();
    match v.as_slice() {
        [r, g, b] => Some(Color::rgb(*r, *g, *b)),
        [g] => Some(Color::rgb(*g, *g, *g)),
        [c, m, y, k] => Some(Color::rgb(
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        )),
        _ => None,
    }
}

/// Flat number array to points.
pub fn points(o: Option<&Object>) -> Vec<Point> {
    let Some(a) = o.and_then(Object::as_array) else {
        return Vec::new();
    };
    a.as_chunks::<2>()
        .0
        .iter()
        .filter_map(|c| Some(Point::new(num(c.first())?, num(c.get(1))?)))
        .collect()
}

// ---- builders --------------------------------------------------------------------------

pub fn n(s: &str) -> Object {
    Object::name(s)
}

pub fn s(t: &str) -> Object {
    Object::String(PdfString::text(t))
}

pub fn real(v: f64) -> Object {
    // Shortest round-trip text (pdfcraft-cos writes reals exactly).
    if v.is_finite() { Object::Real(v) } else { Object::Int(0) }
}

pub fn color_arr(c: &Color) -> Object {
    Object::Array(vec![real(c.r), real(c.g), real(c.b)])
}

pub fn points_arr(pts: &[Point]) -> Object {
    Object::Array(pts.iter().flat_map(|p| [real(p.x), real(p.y)]).collect())
}

pub fn rect_arr(r: &Rect) -> Object {
    Object::Array(r.as_array().iter().map(|v| real(*v)).collect())
}

pub fn dict(entries: &[(&str, Object)]) -> Dict {
    let mut d = Dict::new();
    for (k, v) in entries {
        d.set(k.as_bytes().to_vec(), v.clone());
    }
    d
}

pub fn set(d: &mut Dict, key: &str, v: Object) {
    d.set(key.as_bytes().to_vec(), v);
}

pub fn get<'a>(d: &'a Dict, key: &str) -> Option<&'a Object> {
    d.get(key.as_bytes())
}

pub fn remove(d: &mut Dict, key: &str) {
    d.remove(key.as_bytes());
}

/// `d[key]`, following a reference.
pub fn resolved(doc: &CosDoc, d: &Dict, key: &str) -> Option<Object> {
    let o = d.get(key.as_bytes())?;
    Some(doc.resolve(o).as_ref().clone())
}

// ---- page tree -------------------------------------------------------------------------

/// One page with its inherited attributes resolved.
#[derive(Debug, Clone)]
pub struct PageRef {
    pub id: ObjRef,
    pub dict: Dict,
    pub media: Rect,
    pub crop: Option<Rect>,
    pub rotate: i64,
}

const MAX_DEPTH: usize = 64;

/// All pages in order. Tolerates cycles, missing kids and broken inheritance.
pub fn pages(doc: &CosDoc) -> Vec<PageRef> {
    let mut out = Vec::new();
    let Some(root) = doc.root() else { return out };
    let catalog = doc.get(root);
    let Some(cat) = catalog.as_dict() else { return out };
    let Some(pages_ref) = cat.reference(b"Pages") else {
        return out;
    };
    let mut seen = HashSet::new();
    walk(doc, pages_ref, Inherited::default(), &mut seen, &mut out, 0);
    out
}

#[derive(Clone, Default)]
struct Inherited {
    media: Option<Rect>,
    crop: Option<Rect>,
    rotate: Option<i64>,
}

fn walk(doc: &CosDoc, node: ObjRef, inh: Inherited, seen: &mut HashSet<ObjRef>, out: &mut Vec<PageRef>, depth: usize) {
    if depth > MAX_DEPTH || !seen.insert(node) {
        return;
    }
    let obj = doc.get(node);
    let Some(d) = obj.as_dict() else { return };
    let resolve_rect = |k: &str| rect(Some(&doc.resolve(d.get(k.as_bytes()).unwrap_or(&Object::Null))));
    let inh = Inherited {
        media: resolve_rect("MediaBox").or(inh.media),
        crop: resolve_rect("CropBox").or(inh.crop),
        rotate: num(Some(&doc.resolve(d.get(b"Rotate").unwrap_or(&Object::Null))))
            .map(|v| v as i64)
            .or(inh.rotate),
    };
    let is_pages = d.name(b"Type") == Some(b"Pages") || d.contains(b"Kids");
    if is_pages {
        let kids = doc.resolve(d.get(b"Kids").unwrap_or(&Object::Null));
        if let Some(arr) = kids.as_array() {
            for k in arr {
                if let Some(r) = k.as_ref() {
                    walk(doc, r, inh.clone(), seen, out, depth + 1);
                }
            }
        }
        return;
    }
    out.push(PageRef {
        id: node,
        dict: d.clone(),
        media: inh.media.unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0)),
        crop: inh.crop,
        rotate: inh.rotate.map(|r| r.rem_euclid(360)).unwrap_or(0),
    });
}
