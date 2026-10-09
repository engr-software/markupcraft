//! Layers: PDF optional content groups (ISO 32000-1 §8.11). A markup on a layer carries
//! `/OC` pointing at the layer's group; the groups are listed in the catalog's
//! `/OCProperties`:
//!
//! ```text
//! Catalog /OCProperties << /OCGs [ 12 0 R ... ]
//!                          /D << /Name (Default) /Order [ 12 0 R ... ] /ON [...] /OFF [...]
//!                                /Locked [...] /AS [ << /Event /Print /Category [/Print] /OCGs [...] >> ] >> >>
//! Group   << /Type /OCG /Name (Electrical) /Usage << /Print << /PrintState /OFF >> >> >>
//! ```
//!
//! [`write_oc`] runs for every markup the writer writes: it finds (or creates) the group named
//! by `Markup::layer` and points `/OC` at it. Membership dictionaries (`/OCMD`) written by
//! other software are left alone.

use markupcraft_model::Markup;
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString};

/// Most groups read from one file.
pub const MAX_LAYERS: usize = 10_000;

/// The catalog's `/OCProperties` (resolved; empty when missing) and the object holding it when
/// it is indirect.
pub fn ocprops(cos: &CosDoc) -> (Dict, Option<ObjRef>) {
    let Some(root) = cos.root() else {
        return (Dict::new(), None);
    };
    let cat = cos.get(root);
    let Some(cat) = cat.as_dict() else {
        return (Dict::new(), None);
    };
    match cat.get(b"OCProperties") {
        Some(Object::Ref(r)) => (cos.get(*r).as_dict().cloned().unwrap_or_default(), Some(*r)),
        Some(Object::Dict(d)) => (d.clone(), None),
        _ => (Dict::new(), None),
    }
}

/// Store `/OCProperties` back where it lives (inline in the catalog when it was not indirect).
pub fn set_ocprops(cos: &mut CosDoc, props: Dict, holder: Option<ObjRef>) {
    match holder {
        Some(r) => cos.set(r, Object::Dict(props)),
        None => {
            if let Some(root) = cos.root() {
                let _ = cos.update_dict(root, |d| d.set(b"OCProperties".to_vec(), Object::Dict(props)));
            }
        }
    }
}

/// The default configuration `/D` of `props` (resolved).
pub fn default_config(cos: &CosDoc, props: &Dict) -> Dict {
    props
        .get(b"D")
        .map(|d| cos.resolve(d))
        .and_then(|d| d.as_dict().cloned())
        .unwrap_or_default()
}

/// References in an array entry of `d` (resolved array).
pub fn ref_list(cos: &CosDoc, d: &Dict, key: &[u8]) -> Vec<ObjRef> {
    d.get(key)
        .map(|a| cos.resolve(a))
        .and_then(|a| a.as_array().map(|a| a.iter().filter_map(Object::as_ref).collect()))
        .unwrap_or_default()
}

pub fn ref_array(refs: &[ObjRef]) -> Object {
    Object::Array(refs.iter().map(|r| Object::Ref(*r)).collect())
}

/// The name of group `r` ("" when it is not a group).
pub fn group_name(cos: &CosDoc, r: ObjRef) -> Option<String> {
    let g = cos.get(r);
    let d = g.as_dict()?;
    if d.name(b"Type").is_some_and(|t| t != b"OCG") {
        return None;
    }
    Some(crate::pdf::text(d.get(b"Name")))
}

/// Every group the document lists, in `/OCGs` order: (object, name).
pub fn groups(cos: &CosDoc) -> Vec<(ObjRef, String)> {
    let (props, _) = ocprops(cos);
    ref_list(cos, &props, b"OCGs")
        .into_iter()
        .take(MAX_LAYERS)
        .filter_map(|r| Some((r, group_name(cos, r)?)))
        .collect()
}

/// The group named `name`, if the document has one.
pub fn find_group(cos: &CosDoc, name: &str) -> Option<ObjRef> {
    groups(cos).into_iter().find(|(_, n)| n == name).map(|(r, _)| r)
}

/// The group named `name`, created (listed in `/OCGs` and the default `/Order`, visible) when
/// the document has none. `None` without a catalog.
pub fn ensure_group(cos: &mut CosDoc, name: &str) -> Option<ObjRef> {
    if let Some(r) = find_group(cos, name) {
        return Some(r);
    }
    cos.root()?;
    let mut g = Dict::new();
    g.set(b"Type".to_vec(), Object::name("OCG"));
    g.set(b"Name".to_vec(), Object::String(PdfString::text(name)));
    let r = cos.add(Object::Dict(g));
    let (mut props, holder) = ocprops(cos);
    let mut all = ref_list(cos, &props, b"OCGs");
    all.push(r);
    props.set(b"OCGs".to_vec(), ref_array(&all));
    let mut d = default_config(cos, &props);
    if !d.contains(b"Name") {
        d.set(b"Name".to_vec(), Object::String(PdfString::text("Default")));
    }
    let mut order: Vec<Object> = d
        .get(b"Order")
        .map(|a| cos.resolve(a))
        .and_then(|a| a.as_array().cloned())
        .unwrap_or_default();
    order.push(Object::Ref(r));
    d.set(b"Order".to_vec(), Object::Array(order));
    props.set(b"D".to_vec(), Object::Dict(d));
    set_ocprops(cos, props, holder);
    Some(r)
}

/// Point annotation `a`'s `/OC` at the group named by `m.layer` (none: remove a group `/OC`).
pub fn write_oc(cos: &mut CosDoc, a: &mut Dict, m: &Markup) {
    let current = a.get(b"OC").cloned();
    let current_dict = current.as_ref().and_then(|o| cos.dict(o));
    let is_ocmd = current_dict.as_ref().is_some_and(|d| d.name(b"Type") == Some(b"OCMD"));
    if is_ocmd {
        return;
    }
    if m.layer.is_empty() {
        if current.is_some() {
            a.remove(b"OC");
        }
        return;
    }
    let same = current_dict
        .as_ref()
        .is_some_and(|d| crate::pdf::text(d.get(b"Name")) == m.layer);
    if same {
        return;
    }
    if let Some(r) = ensure_group(cos, &m.layer) {
        a.set(b"OC".to_vec(), Object::Ref(r));
    }
}
