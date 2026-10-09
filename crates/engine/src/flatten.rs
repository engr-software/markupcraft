//! Flatten markups: each chosen markup's appearance (`/AP /N`) is burned into the page content
//! and the annotation goes away, with its pop-up and replies. Which markups: all, or those a
//! [`FlattenFilter`] picks (ids, pages, kinds, layers, authors).
//!
//! The appearance is placed with ISO 32000-1 Â§12.5.5 (Algorithm 8.1: the form's bounding box,
//! transformed by its `/Matrix`, mapped onto `/Rect`), following PdfCraft's flattener
//! (`pdfcraft-edit`, MIT OR Apache-2.0) with a filter added. The drawing is appended after the
//! page's own content, which is wrapped in `q ... Q` so its graphics state cannot leak. Hidden
//! markups are left alone. The next save rewrites the file in full, so the flattened
//! annotations do not survive in an earlier revision.

use std::collections::HashSet;

use markupcraft_model::{Document, Markup};
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, Stream};

use crate::docutil::{annots_of, page_objs, set_annots, text_of};
use crate::{Result, Session, invalid};

const HIDDEN: i64 = 2;
const NO_VIEW: i64 = 32;

/// Which markups to flatten; empty lists do not filter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlattenFilter {
    pub ids: Vec<String>,
    /// 0-based pages.
    pub pages: Vec<usize>,
    /// Kind names as `markup_list` shows them (case-insensitive).
    pub kinds: Vec<String>,
    pub layers: Vec<String>,
    pub authors: Vec<String>,
}

impl FlattenFilter {
    fn picks(&self, m: &Markup) -> bool {
        let any = |list: &[String], v: &str| list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(v));
        (self.ids.is_empty() || self.ids.contains(&m.id))
            && (self.pages.is_empty() || self.pages.contains(&m.page))
            && any(&self.kinds, m.kind.name())
            && any(&self.layers, &m.layer)
            && any(&self.authors, &m.author)
    }
}

fn n(v: f64) -> String {
    let s = format!("{:.4}", if v.abs() < 5e-5 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn nums(cos: &CosDoc, o: Option<&Object>) -> Option<Vec<f64>> {
    let o = cos.resolve(o?);
    o.as_array()?
        .iter()
        .map(|x| cos.resolve(x).as_f64().filter(|v| v.is_finite()))
        .collect()
}

/// The normal appearance stream (the `/AS` state's, for appearances with states).
fn appearance(cos: &CosDoc, d: &Dict) -> Option<ObjRef> {
    let ap = cos.dict(d.get(b"AP")?)?;
    let normal = ap.get(b"N")?.clone();
    let is_stream = |r: ObjRef| matches!(&*cos.get(r), Object::Stream(_));
    match normal {
        Object::Ref(r) if is_stream(r) => Some(r),
        other => {
            let states = cos.dict(&other)?;
            let state = d.name(b"AS")?;
            states.get(state)?.as_ref().filter(|r| is_stream(*r))
        }
    }
}

/// Algorithm 8.1: the matrix mapping the form's box onto the annotation rectangle.
fn placement(cos: &CosDoc, form: &Dict, rect: [f64; 4]) -> Option<[f64; 6]> {
    let bbox = nums(cos, form.get(b"BBox")).filter(|b| b.len() == 4)?;
    let m = nums(cos, form.get(b"Matrix"))
        .filter(|m| m.len() == 6)
        .unwrap_or_else(|| vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let (b0, b1, b2, b3) = (*bbox.first()?, *bbox.get(1)?, *bbox.get(2)?, *bbox.get(3)?);
    let (m0, m1, m2, m3, m4, m5) = (*m.first()?, *m.get(1)?, *m.get(2)?, *m.get(3)?, *m.get(4)?, *m.get(5)?);
    let pts = [(b0, b1), (b2, b1), (b0, b3), (b2, b3)].map(|(x, y)| (m0 * x + m2 * y + m4, m1 * x + m3 * y + m5));
    let (x0, x1) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    if x1 - x0 < 1e-9 || y1 - y0 < 1e-9 {
        return None;
    }
    let (sx, sy) = ((rect[2] - rect[0]) / (x1 - x0), (rect[3] - rect[1]) / (y1 - y0));
    Some([sx, 0.0, 0.0, sy, rect[0] - x0 * sx, rect[1] - y0 * sy])
}

/// The page's content streams as a list.
fn contents(cos: &CosDoc, page: ObjRef) -> Vec<Object> {
    let p = cos.get(page);
    let Some(c) = p.as_dict().and_then(|d| d.get(b"Contents")).cloned() else {
        return Vec::new();
    };
    match &*cos.resolve(&c) {
        Object::Array(a) => a.clone(),
        _ => vec![c],
    }
}

/// Burn the annotations whose object or `/NM` is chosen into their pages. Returns how many.
fn flatten(cos: &mut CosDoc, objs: &HashSet<ObjRef>, ids: &HashSet<String>) -> Result<usize> {
    let pages = page_objs(cos)?;
    let mut drawn = 0usize;
    for page in pages {
        let list = annots_of(cos, page);
        if list.is_empty() {
            continue;
        }
        let mut content = String::new();
        let mut xobjects: Vec<(String, ObjRef)> = Vec::new();
        let mut gone: HashSet<ObjRef> = HashSet::new();
        for entry in &list {
            let Some(r) = entry.as_ref() else { continue };
            let Some(d) = cos.dict(entry) else { continue };
            if matches!(d.name(b"Subtype"), Some(b"Link" | b"Popup" | b"Widget")) {
                continue;
            }
            let nm = text_of(cos, d.get(b"NM"));
            if !objs.contains(&r) && !(!nm.is_empty() && ids.contains(&nm)) {
                continue;
            }
            let flags = d.get(b"F").and_then(|f| cos.resolve(f).as_int()).unwrap_or(0);
            if flags & (HIDDEN | NO_VIEW) != 0 {
                continue;
            }
            let rect = nums(cos, d.get(b"Rect"))
                .filter(|r| r.len() == 4)
                .map(|r| [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]);
            if let (Some(ap), Some(rect)) = (appearance(cos, &d), rect) {
                let form = cos.dict(&Object::Ref(ap)).unwrap_or_default();
                if let Some(m) = placement(cos, &form, rect) {
                    let name = format!("MCFlat{}_{}", ap.num, xobjects.len());
                    content.push_str(&format!(
                        "q {} {} {} {} {} {} cm /{name} Do Q\n",
                        n(m[0]),
                        n(m[1]),
                        n(m[2]),
                        n(m[3]),
                        n(m[4]),
                        n(m[5])
                    ));
                    xobjects.push((name, ap));
                }
            }
            gone.insert(r);
            drawn += 1;
        }
        if gone.is_empty() {
            continue;
        }
        // Pop-ups and replies of what was flattened go too.
        for entry in &list {
            let Some(r) = entry.as_ref() else { continue };
            let Some(d) = cos.dict(entry) else { continue };
            let points_at = |k: &[u8]| d.reference(k).is_some_and(|t| gone.contains(&t));
            if (points_at(b"Parent") && d.name(b"Subtype") == Some(b"Popup")) || points_at(b"IRT") {
                gone.insert(r);
            }
        }
        let kept: Vec<Object> = list
            .iter()
            .filter(|e| !e.as_ref().is_some_and(|r| gone.contains(&r)))
            .cloned()
            .collect();
        if !content.is_empty() {
            let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
            let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
            let mut xo = res.get(b"XObject").and_then(|x| cos.dict(x)).unwrap_or_default();
            for (k, v) in &xobjects {
                xo.set(k.clone().into_bytes(), Object::Ref(*v));
            }
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            let open = cos.add(Object::Stream(Stream::from_raw(Dict::new(), b"q\n".to_vec())));
            let mut body = b"\nQ\n".to_vec();
            body.extend_from_slice(content.as_bytes());
            let close = cos.add(Object::Stream(Stream::flate(Dict::new(), &body)));
            let mut list = vec![Object::Ref(open)];
            list.extend(contents(cos, page));
            list.push(Object::Ref(close));
            cos.update_dict(page, |d| {
                d.set(b"Resources".to_vec(), Object::Dict(res));
                d.set(b"Contents".to_vec(), Object::Array(list));
            })?;
        }
        set_annots(cos, page, kept)?;
    }
    Ok(drawn)
}

impl Session {
    /// Flatten the markups `filter` picks; returns how many were flattened. Undoable until
    /// saved; the next save rewrites the file in full.
    pub fn flatten_markups(&mut self, filter: &FlattenFilter) -> Result<usize> {
        for p in &filter.pages {
            self.page(*p)?;
        }
        for id in &filter.ids {
            self.markup(id)?;
        }
        let chosen: Vec<String> = self
            .doc
            .markups
            .iter()
            .filter(|m| filter.picks(m))
            .map(|m| m.id.clone())
            .collect();
        if chosen.is_empty() {
            return Err(invalid("no markup matches (markup_list shows them)"));
        }
        if let Some(m) = self.doc.markups.iter().find(|m| chosen.contains(&m.id) && m.locked()) {
            return Err(crate::EngineError::Locked(m.id.clone()));
        }
        self.graph_edit("Flatten", |cos, doc: &Document| {
            let ids: HashSet<String> = chosen.iter().cloned().collect();
            let objs: HashSet<ObjRef> = doc
                .markups
                .iter()
                .filter(|m| ids.contains(&m.id) && m.obj.0 != 0)
                .map(|m| ObjRef::new(m.obj.0, m.obj.1))
                .collect();
            let n = flatten(cos, &objs, &ids)?;
            cos.require_full_save();
            Ok(n)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_maps_bbox_to_rect() {
        let cos = CosDoc::new_empty();
        let mut form = Dict::new();
        form.set(
            b"BBox".to_vec(),
            Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(10), Object::Int(20)]),
        );
        let m = placement(&cos, &form, [100.0, 100.0, 120.0, 140.0]);
        assert_eq!(m, Some([2.0, 0.0, 0.0, 2.0, 100.0, 100.0]));
        form.set(
            b"BBox".to_vec(),
            Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(0), Object::Int(0)]),
        );
        assert_eq!(placement(&cos, &form, [0.0, 0.0, 1.0, 1.0]), None);
    }
}
