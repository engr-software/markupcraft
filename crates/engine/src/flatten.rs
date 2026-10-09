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

/// How flattened markups are written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlattenOptions {
    /// Keep the markups so Document > Unflatten can restore them (Revu's recovery option).
    pub recoverable: bool,
    /// Put the flattened drawing on this layer (an optional content group, made if needed).
    pub layer: Option<String>,
}

/// The tag on a stream of flattened markups, and the page key listing what can be restored.
const FLAT_TAG: &[u8] = b"PCFlattened";
const UNFLATTEN_KEY: &[u8] = b"PCUnflatten";

/// Burn the annotations whose object or `/NM` is chosen into their pages. Returns how many.
fn flatten(
    cos: &mut CosDoc,
    objs: &HashSet<ObjRef>,
    ids: &HashSet<String>,
    opts: &FlattenOptions,
    layer: Option<ObjRef>,
) -> Result<usize> {
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
        // what Unflatten needs to bring each one back: its dictionary and its drawing
        let mut undo: Vec<(Dict, String)> = Vec::new();
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
            let mut line = String::new();
            if let (Some(ap), Some(rect)) = (appearance(cos, &d), rect) {
                let form = cos.dict(&Object::Ref(ap)).unwrap_or_default();
                if let Some(m) = placement(cos, &form, rect) {
                    let name = format!("MCFlat{}_{}", ap.num, xobjects.len());
                    line = format!(
                        "q {} {} {} {} {} {} cm /{name} Do Q\n",
                        n(m[0]),
                        n(m[1]),
                        n(m[2]),
                        n(m[3]),
                        n(m[4]),
                        n(m[5])
                    );
                    content.push_str(&line);
                    xobjects.push((name, ap));
                }
            }
            undo.push((d.clone(), line));
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
        let list_order: Vec<ObjRef> = list.iter().filter_map(Object::as_ref).collect();
        if !content.is_empty() {
            let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
            let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
            let mut xo = res.get(b"XObject").and_then(|x| cos.dict(x)).unwrap_or_default();
            for (k, v) in &xobjects {
                xo.set(k.clone().into_bytes(), Object::Ref(*v));
            }
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            if let Some(oc) = layer {
                let mut props = res.get(b"Properties").and_then(|x| cos.dict(x)).unwrap_or_default();
                props.set(b"MCFlatLayer".to_vec(), Object::Ref(oc));
                res.set(b"Properties".to_vec(), Object::Dict(props));
                content = format!("/OC /MCFlatLayer BDC\n{content}EMC\n");
            }
            let mut list = contents(cos, page);
            let wrapped = |c: &Object| matches!(&*cos.resolve(c), Object::Stream(s) if s.dict.get(b"PCWrap").is_some());
            if !list.first().is_some_and(wrapped) {
                let mut wrap = Dict::new();
                wrap.set(b"PCWrap".to_vec(), Object::Bool(true));
                let open = cos.add(Object::Stream(Stream::from_raw(wrap.clone(), b"q\n".to_vec())));
                let close = cos.add(Object::Stream(Stream::from_raw(wrap, b"\nQ\n".to_vec())));
                list.insert(0, Object::Ref(open));
                list.push(Object::Ref(close));
            }
            let mut tag = Dict::new();
            tag.set(FLAT_TAG.to_vec(), Object::Bool(true));
            let flat = cos.add(Object::Stream(Stream::flate(tag, content.as_bytes())));
            list.push(Object::Ref(flat));
            let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
            let mut records: Vec<Object> = pd
                .get(UNFLATTEN_KEY)
                .map(|r| cos.resolve(r))
                .and_then(|r| r.as_array().cloned())
                .unwrap_or_default();
            if opts.recoverable {
                let mut rec = Dict::new();
                rec.set(b"S".to_vec(), Object::Ref(flat));
                let refs: Vec<Object> = list_order
                    .iter()
                    .filter(|r| gone.contains(r))
                    .map(|r| Object::Ref(*r))
                    .collect();
                rec.set(b"Annots".to_vec(), Object::Array(refs));
                records.push(Object::Dict(rec));
            }
            cos.update_dict(page, |d| {
                d.set(b"Resources".to_vec(), Object::Dict(res));
                d.set(b"Contents".to_vec(), Object::Array(list));
                if !records.is_empty() {
                    d.set(UNFLATTEN_KEY.to_vec(), Object::Array(records));
                }
            })?;
            crate::unflatten::record(cos, page, (flat, flat), std::mem::take(&mut undo))?;
        }
        set_annots(cos, page, kept)?;
    }
    Ok(drawn)
}

/// Unflatten records of a page: (flattened stream, annotations).
fn records(cos: &CosDoc, page: ObjRef) -> Vec<(Option<ObjRef>, Vec<Object>)> {
    cos.dict(&Object::Ref(page))
        .and_then(|d| d.get(UNFLATTEN_KEY).map(|r| cos.resolve(r)))
        .and_then(|r| r.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|r| cos.dict(r))
        .map(|d| {
            let annots = d
                .get(b"Annots")
                .map(|a| cos.resolve(a))
                .and_then(|a| a.as_array().cloned())
                .unwrap_or_default();
            (d.reference(b"S"), annots)
        })
        .collect()
}

/// Remove recoverable flattened markups from the page contents.
fn strip_flattened(cos: &mut CosDoc) -> Result<usize> {
    let mut n = 0;
    for page in page_objs(cos)? {
        let recoverable: HashSet<ObjRef> = records(cos, page).into_iter().filter_map(|r| r.0).collect();
        if recoverable.is_empty() {
            continue;
        }
        let list = contents(cos, page);
        let kept: Vec<Object> = list
            .iter()
            .filter(|c| !c.as_ref().is_some_and(|r| recoverable.contains(&r)))
            .cloned()
            .collect();
        n += list.len() - kept.len();
        cos.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(kept)))?;
    }
    Ok(n)
}

/// A PDF without its recoverable flattened markups (what Compare and Overlay see unless asked
/// to include them). The bytes come back unchanged when there are none or the file cannot be
/// read.
pub fn without_flattened(bytes: std::sync::Arc<Vec<u8>>) -> std::sync::Arc<Vec<u8>> {
    let Ok(mut cos) = CosDoc::open(bytes.clone()) else {
        return bytes;
    };
    match strip_flattened(&mut cos) {
        Ok(n) if n > 0 => match markupcraft_revu::cos::write_full(&cos, &crate::docutil::save_options()) {
            Ok(b) => std::sync::Arc::new(b),
            Err(_) => bytes,
        },
        _ => bytes,
    }
}

impl Session {
    /// Flatten the markups `filter` picks; returns how many were flattened. Undoable until
    /// saved; the next save rewrites the file in full.
    pub fn flatten_markups(&mut self, filter: &FlattenFilter) -> Result<usize> {
        self.flatten_markups_with(filter, &FlattenOptions::default())
    }

    /// How many flattened markups Document > Unflatten could restore on `pages` (empty = all).
    pub fn recoverable_flattened(&self, pages: &[usize]) -> usize {
        let Ok(objs) = page_objs(&self.file.cos) else { return 0 };
        objs.iter()
            .enumerate()
            .filter(|(i, _)| pages.is_empty() || pages.contains(i))
            .map(|(_, p)| records(&self.file.cos, *p).iter().map(|r| r.1.len()).sum::<usize>())
            .sum()
    }

    /// Document > Unflatten: restore markups flattened with recovery on `pages` (0-based;
    /// empty = all). Returns how many annotations came back. Undoable.
    pub fn unflatten(&mut self, pages: &[usize]) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        if self.recoverable_flattened(pages) == 0 {
            return Err(invalid("there are no recoverable flattened markups on those pages"));
        }
        let pages = pages.to_vec();
        self.graph_edit("Unflatten", |cos, _| {
            let mut restored = 0;
            for (i, page) in page_objs(cos)?.into_iter().enumerate() {
                if !pages.is_empty() && !pages.contains(&i) {
                    continue;
                }
                let recs = records(cos, page);
                if recs.is_empty() {
                    continue;
                }
                let streams: HashSet<ObjRef> = recs.iter().filter_map(|r| r.0).collect();
                let mut annots = annots_of(cos, page);
                for (_, list) in &recs {
                    for a in list {
                        if a.as_ref().is_some() && !annots.contains(a) {
                            annots.push(a.clone());
                            restored += 1;
                        }
                    }
                }
                let kept: Vec<Object> = contents(cos, page)
                    .into_iter()
                    .filter(|c| !c.as_ref().is_some_and(|r| streams.contains(&r)))
                    .collect();
                cos.update_dict(page, |d| {
                    d.set(b"Contents".to_vec(), Object::Array(kept));
                    d.remove(UNFLATTEN_KEY);
                    d.remove(b"PCFlattened");
                })?;
                set_annots(cos, page, annots)?;
            }
            cos.require_full_save();
            Ok(restored)
        })
    }

    /// Flatten with options: recoverable (Unflatten can restore them) and onto a layer.
    pub fn flatten_markups_with(&mut self, filter: &FlattenFilter, opts: &FlattenOptions) -> Result<usize> {
        let layer_name = match &opts.layer {
            Some(l) => Some(crate::layers::check_name(l)?),
            None => None,
        };
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
            let layer = match &layer_name {
                Some(n) => Some(
                    markupcraft_revu::layers::ensure_group(cos, n)
                        .ok_or_else(|| invalid("the layer could not be made"))?,
                ),
                None => None,
            };
            let n = flatten(cos, &objs, &ids, opts, layer)?;
            cos.require_full_save();
            Ok(n)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recoverable_flatten_unflattens_and_hides_from_compare() {
        use crate::synthetic::{SyntheticPage, pdf};
        use markupcraft_model::{Kind, Point};
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), "f.pdf").unwrap();
        let mut m = Markup::new(
            Kind::Rectangle,
            0,
            vec![
                Point::new(100.0, 100.0),
                Point::new(300.0, 100.0),
                Point::new(300.0, 300.0),
                Point::new(100.0, 300.0),
            ],
        );
        m.line_width = 6.0;
        s.add_markup(m).unwrap();
        let opts = FlattenOptions {
            recoverable: true,
            layer: Some("Flattened".into()),
        };
        assert_eq!(s.flatten_markups_with(&FlattenFilter::default(), &opts).unwrap(), 1);
        assert!(s.doc().markups.is_empty());
        assert_eq!(s.recoverable_flattened(&[]), 1);
        assert!(s.layers().iter().any(|l| l.name == "Flattened"));
        // drawn into the page, and gone again for Compare
        let ink = |b: std::sync::Arc<Vec<u8>>| {
            crate::raster::Renderable::new(b, true)
                .unwrap()
                .render(0, 0.5, 2000.0)
                .unwrap()
                .gray
                .get(50, 300)
        };
        let bytes = s.current_bytes().unwrap();
        assert!(ink(bytes.clone()) < 128, "flattened line drawn");
        assert!(ink(without_flattened(bytes)) > 200, "and stripped");
        assert_eq!(s.unflatten(&[]).unwrap(), 1);
        assert_eq!(s.doc().markups.len(), 1);
        assert!(s.unflatten(&[]).is_err(), "nothing left to restore");
        s.undo().unwrap();
        assert!(s.doc().markups.is_empty());
        // not recoverable: nothing to restore
        let mut t = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), "g.pdf").unwrap();
        t.add_markup(Markup::new(
            Kind::Line,
            0,
            vec![Point::new(1.0, 1.0), Point::new(90.0, 90.0)],
        ))
        .unwrap();
        t.flatten_markups(&FlattenFilter::default()).unwrap();
        assert_eq!(t.recoverable_flattened(&[]), 0);
    }

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
