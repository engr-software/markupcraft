//! Layers (Revu's Layers panel and markup layers): PDF optional content groups. A layer has a
//! name, a view state (on/off in the default configuration), a print state and a lock; markups
//! are put on a layer with `/OC` (written on save by `markupcraft_revu::layers::write_oc`).
//!
//! Layers markups use but the file does not list yet (a layer typed on a markup) show up too;
//! they become groups when the markups are saved.

use std::collections::BTreeMap;

use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object};
use markupcraft_revu::layers::{
    default_config, ensure_group, find_group, groups, ocprops, ref_array, ref_list, set_ocprops,
};

use crate::{EngineError, Result, Session, invalid};

/// Longest layer name.
pub const MAX_NAME: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    /// shown (not in the default configuration's `/OFF`)
    pub visible: bool,
    /// printed: the group's `/Usage /Print /PrintState`, else the view state
    pub print: bool,
    /// the print state was set explicitly
    pub print_set: bool,
    pub locked: bool,
    /// markups on this layer
    pub markups: usize,
    /// the file lists the group (false: only unsaved markups name it)
    pub in_file: bool,
}

/// Changes to a layer's states; `None` leaves one as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerState {
    pub visible: Option<bool>,
    pub print: Option<bool>,
    pub locked: Option<bool>,
}

pub(crate) fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) {
        return Err(invalid(format!(
            "a layer name has 1 to {MAX_NAME} characters and no control characters"
        )));
    }
    Ok(n.to_string())
}

fn print_state(cos: &CosDoc, r: ObjRef) -> Option<bool> {
    let g = cos.get(r);
    let usage = g.as_dict()?.get(b"Usage").and_then(|u| cos.dict(u))?;
    let print = usage.get(b"Print").and_then(|p| cos.dict(p))?;
    match print.name(b"PrintState")? {
        b"ON" => Some(true),
        b"OFF" => Some(false),
        _ => None,
    }
}

/// Write the default configuration back.
fn put_config(cos: &mut CosDoc, d: Dict) {
    let (mut props, holder) = ocprops(cos);
    props.set(b"D".to_vec(), Object::Dict(d));
    set_ocprops(cos, props, holder);
}

fn set_member(list: &mut Vec<ObjRef>, r: ObjRef, on: bool) {
    list.retain(|x| *x != r);
    if on {
        list.push(r);
    }
}

/// `/Order` without `r` (nested arrays too, bounded depth).
fn strip_order(items: &[Object], r: ObjRef, depth: usize) -> Vec<Object> {
    items
        .iter()
        .filter(|o| o.as_ref() != Some(r))
        .map(|o| match o {
            Object::Array(a) if depth < 16 => Object::Array(strip_order(a, r, depth + 1)),
            other => other.clone(),
        })
        .collect()
}

/// Rebuild the print auto-state (`/AS` event Print) from the groups' print usage.
fn sync_print_event(cos: &CosDoc, d: &mut Dict) {
    let printed: Vec<ObjRef> = groups(cos)
        .into_iter()
        .filter(|(r, _)| print_state(cos, *r).is_some())
        .map(|(r, _)| r)
        .collect();
    let mut events: Vec<Object> = d
        .get(b"AS")
        .map(|a| cos.resolve(a))
        .and_then(|a| a.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|e| cos.dict(e).is_none_or(|e| e.name(b"Event") != Some(b"Print")))
        .collect();
    if !printed.is_empty() {
        let mut e = Dict::new();
        e.set(b"Event".to_vec(), Object::name("Print"));
        e.set(b"Category".to_vec(), Object::Array(vec![Object::name("Print")]));
        e.set(b"OCGs".to_vec(), ref_array(&printed));
        events.push(Object::Dict(e));
    }
    if events.is_empty() {
        d.remove(b"AS");
    } else {
        d.set(b"AS".to_vec(), Object::Array(events));
    }
}

impl Session {
    /// Every layer: the file's groups in their order, then layers only unsaved markups name.
    pub fn layers(&self) -> Vec<LayerInfo> {
        let cos = &self.file.cos;
        let (props, _) = ocprops(cos);
        let d = default_config(cos, &props);
        let off = ref_list(cos, &d, b"OFF");
        let locked = ref_list(cos, &d, b"Locked");
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for m in self.doc.markups.iter().filter(|m| !m.layer.is_empty()) {
            *counts.entry(m.layer.as_str()).or_default() += 1;
        }
        let mut out: Vec<LayerInfo> = Vec::new();
        for (r, name) in groups(cos) {
            if out.iter().any(|l| l.name == name) {
                continue;
            }
            let visible = !off.contains(&r);
            let ps = print_state(cos, r);
            out.push(LayerInfo {
                markups: counts.get(name.as_str()).copied().unwrap_or(0),
                visible,
                print: ps.unwrap_or(visible),
                print_set: ps.is_some(),
                locked: locked.contains(&r),
                in_file: true,
                name,
            });
        }
        for (name, n) in counts {
            if !out.iter().any(|l| l.name == name) {
                out.push(LayerInfo {
                    name: name.to_string(),
                    visible: true,
                    print: true,
                    print_set: false,
                    locked: false,
                    markups: n,
                    in_file: false,
                });
            }
        }
        out
    }

    fn layer_exists(&self, name: &str) -> bool {
        self.layers().iter().any(|l| l.name == name)
    }

    fn need_layer(&self, name: &str) -> Result<()> {
        if self.layer_exists(name) {
            Ok(())
        } else {
            Err(invalid(format!("no layer named {name:?} (layer_list shows them)")))
        }
    }

    /// Add an empty layer (visible, printed). Undoable.
    pub fn create_layer(&mut self, name: &str) -> Result<()> {
        let name = check_name(name)?;
        if find_group(&self.file.cos, &name).is_some() {
            return Err(invalid(format!("a layer named {name:?} already exists")));
        }
        self.edit("New Layer", |s| {
            ensure_group(&mut s.file.cos, &name).ok_or_else(|| invalid("the document has no catalog"))?;
            Ok(((), true))
        })
    }

    /// Rename a layer (its markups follow). Returns how many markups are on it. Undoable.
    pub fn rename_layer(&mut self, old: &str, new: &str) -> Result<usize> {
        self.need_layer(old)?;
        let new = check_name(new)?;
        if new != old && self.layer_exists(&new) {
            return Err(invalid(format!("a layer named {new:?} already exists")));
        }
        let old = old.to_string();
        self.edit("Rename Layer", |s| {
            if let Some(r) = find_group(&s.file.cos, &old) {
                s.file.cos.update_dict(r, |d| {
                    d.set(
                        b"Name".to_vec(),
                        Object::String(markupcraft_revu::cos::PdfString::text(&new)),
                    )
                })?;
            }
            let mut n = 0;
            for m in s.doc.markups.iter_mut().filter(|m| m.layer == old) {
                m.layer = new.clone();
                n += 1;
            }
            if s.doc.markup_layer == old {
                s.doc.markup_layer = new.clone();
            }
            Ok((n, true))
        })
    }

    /// Delete a layer. Its markups are deleted with it (`delete_markups`) or left on no layer.
    /// Returns how many markups were on it. Undoable.
    pub fn delete_layer(&mut self, name: &str, delete_markups: bool) -> Result<usize> {
        self.need_layer(name)?;
        let name = name.to_string();
        self.edit("Delete Layer", |s| {
            if let Some(r) = find_group(&s.file.cos, &name) {
                let cos = &mut s.file.cos;
                let (mut props, holder) = ocprops(cos);
                let mut all = ref_list(cos, &props, b"OCGs");
                all.retain(|x| *x != r);
                props.set(b"OCGs".to_vec(), ref_array(&all));
                let mut d = default_config(cos, &props);
                for key in [&b"ON"[..], b"OFF", b"Locked"] {
                    if d.contains(key) {
                        let mut l = ref_list(cos, &d, key);
                        l.retain(|x| *x != r);
                        d.set(key.to_vec(), ref_array(&l));
                    }
                }
                for key in [&b"Order"[..], b"RBGroups"] {
                    if let Some(a) = d.get(key).map(|a| cos.resolve(a)).and_then(|a| a.as_array().cloned()) {
                        d.set(key.to_vec(), Object::Array(strip_order(&a, r, 0)));
                    }
                }
                props.set(b"D".to_vec(), Object::Dict(d));
                set_ocprops(cos, props, holder);
                let mut d = default_config(&s.file.cos, &ocprops(&s.file.cos).0);
                sync_print_event(&s.file.cos, &mut d);
                put_config(&mut s.file.cos, d);
            }
            let on: Vec<usize> = (0..s.doc.markups.len())
                .filter(|&i| s.doc.markups.get(i).is_some_and(|m| m.layer == name))
                .collect();
            let n = on.len();
            if s.doc.markup_layer == name {
                s.doc.markup_layer.clear();
            }
            if delete_markups {
                for i in on.into_iter().rev() {
                    let m = s.doc.markups.remove(i);
                    if m.in_file() {
                        s.doc.deleted.push(m.obj);
                    }
                }
            } else {
                for i in on {
                    if let Some(m) = s.doc.markups.get_mut(i) {
                        m.layer.clear();
                        m.dirty = true;
                    }
                }
            }
            Ok((n, true))
        })
    }

    /// Change a layer's view, print and lock states. A layer only unsaved markups name gets its
    /// group now. Undoable.
    pub fn set_layer_state(&mut self, name: &str, st: LayerState) -> Result<()> {
        self.need_layer(name)?;
        let name = name.to_string();
        self.edit("Layer State", |s| {
            let cos = &mut s.file.cos;
            let r = ensure_group(cos, &name).ok_or_else(|| invalid("the document has no catalog"))?;
            if let Some(p) = st.print {
                cos.update_dict(r, |g| {
                    let mut pr = Dict::new();
                    pr.set(b"PrintState".to_vec(), Object::name(if p { "ON" } else { "OFF" }));
                    let mut u = match g.get(b"Usage") {
                        Some(Object::Dict(u)) => u.clone(),
                        _ => Dict::new(),
                    };
                    u.set(b"Print".to_vec(), Object::Dict(pr));
                    g.set(b"Usage".to_vec(), Object::Dict(u));
                })?;
            }
            let (props, _) = ocprops(cos);
            let mut d = default_config(cos, &props);
            if let Some(v) = st.visible {
                let mut off = ref_list(cos, &d, b"OFF");
                set_member(&mut off, r, !v);
                d.set(b"OFF".to_vec(), ref_array(&off));
                if d.contains(b"ON") || v {
                    let mut on = ref_list(cos, &d, b"ON");
                    set_member(&mut on, r, v);
                    d.set(b"ON".to_vec(), ref_array(&on));
                }
            }
            if let Some(l) = st.locked {
                let mut lk = ref_list(cos, &d, b"Locked");
                set_member(&mut lk, r, l);
                d.set(b"Locked".to_vec(), ref_array(&lk));
            }
            sync_print_event(cos, &mut d);
            put_config(cos, d);
            Ok(((), true))
        })
    }

    /// Show every layer (Show All / Reset Layers). Undoable.
    pub fn show_all_layers(&mut self) -> Result<()> {
        self.edit("Show All Layers", |s| {
            let cos = &mut s.file.cos;
            let (props, _) = ocprops(cos);
            let mut d = default_config(cos, &props);
            let all: Vec<ObjRef> = groups(cos).into_iter().map(|(r, _)| r).collect();
            d.set(b"OFF".to_vec(), Object::Array(Vec::new()));
            d.set(b"ON".to_vec(), ref_array(&all));
            put_config(cos, d);
            Ok(((), true))
        })
    }

    /// Show only `name` (Isolate). Undoable.
    pub fn isolate_layer(&mut self, name: &str) -> Result<()> {
        self.need_layer(name)?;
        let name = name.to_string();
        self.edit("Isolate Layer", |s| {
            let cos = &mut s.file.cos;
            let keep = ensure_group(cos, &name).ok_or_else(|| invalid("the document has no catalog"))?;
            let (props, _) = ocprops(cos);
            let mut d = default_config(cos, &props);
            let others: Vec<ObjRef> = groups(cos).into_iter().map(|(r, _)| r).filter(|r| *r != keep).collect();
            d.set(b"OFF".to_vec(), ref_array(&others));
            d.set(b"ON".to_vec(), ref_array(&[keep]));
            put_config(cos, d);
            Ok(((), true))
        })
    }

    /// Put markups on layer `name` ("" = no layer); the layer is created when it is new.
    /// Returns how many changed. Undoable.
    pub fn assign_layer(&mut self, ids: &[String], name: &str) -> Result<usize> {
        if ids.is_empty() {
            return Err(invalid("no markups given"));
        }
        let name = if name.trim().is_empty() {
            String::new()
        } else {
            check_name(name)?
        };
        for id in ids {
            let m = self.markup(id)?;
            if m.locked() {
                return Err(EngineError::Locked(id.clone()));
            }
        }
        let ids = ids.to_vec();
        self.edit("Set Layer", |s| {
            if !name.is_empty() {
                ensure_group(&mut s.file.cos, &name).ok_or_else(|| invalid("the document has no catalog"))?;
            }
            let mut n = 0;
            for id in &ids {
                let m = s.doc.find_mut(id).ok_or_else(|| EngineError::NoMarkup(id.clone()))?;
                if m.layer != name {
                    m.layer = name.clone();
                    m.dirty = true;
                    n += 1;
                }
            }
            Ok((n, true))
        })
    }

    /// Markup ids on any of `layers` ("" = markups on no layer); with `visible_only`, only
    /// markups whose layer is shown (markups on no layer always are).
    pub fn layer_markups(&self, layers: &[String], visible_only: bool) -> Vec<String> {
        let info = self.layers();
        let hidden: Vec<&str> = info.iter().filter(|l| !l.visible).map(|l| l.name.as_str()).collect();
        self.doc
            .markups
            .iter()
            .filter(|m| layers.is_empty() || layers.iter().any(|l| l.trim() == m.layer))
            .filter(|m| !visible_only || !hidden.contains(&m.layer.as_str()))
            .map(|m| m.id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{Kind, Markup, Rect};

    use super::*;

    fn session() -> Session {
        Session::new_blank("layers.pdf", &[(612.0, 792.0)]).unwrap()
    }

    fn square(s: &mut Session) -> String {
        s.add_markup(Markup::new(
            Kind::Rectangle,
            0,
            Rect::new(10.0, 10.0, 60.0, 60.0).corners().to_vec(),
        ))
        .unwrap()
    }

    #[test]
    fn layers_create_assign_state_save_and_reopen() {
        let dir = std::env::temp_dir().join(format!("mc-layers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("layers.pdf");
        let mut s = session();
        s.create_layer("Electrical").unwrap();
        assert!(s.create_layer("Electrical").is_err());
        let a = square(&mut s);
        let b = square(&mut s);
        assert_eq!(s.assign_layer(std::slice::from_ref(&a), "Electrical").unwrap(), 1);
        assert_eq!(s.assign_layer(std::slice::from_ref(&b), "Plumbing").unwrap(), 1);
        s.set_layer_state(
            "Plumbing",
            LayerState {
                visible: Some(false),
                print: Some(false),
                locked: Some(true),
            },
        )
        .unwrap();
        assert_eq!(s.layer_markups(&[], true), vec![a.clone()]);
        s.save_as(&path, true).unwrap();

        let s2 = Session::open(&path).unwrap();
        let l = s2.layers();
        let names: Vec<&str> = l.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Electrical", "Plumbing"]);
        let p = &l[1];
        assert!(!p.visible && !p.print && p.print_set && p.locked && p.markups == 1);
        assert_eq!(s2.markup(&b).unwrap().layer, "Plumbing");
        assert_eq!(s2.layer_markups(&["Electrical".into()], false), vec![a]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_delete_isolate_and_undo() {
        let mut s = session();
        let a = square(&mut s);
        s.assign_layer(std::slice::from_ref(&a), "A").unwrap();
        s.create_layer("B").unwrap();
        assert_eq!(s.rename_layer("A", "Walls").unwrap(), 1);
        assert_eq!(s.markup(&a).unwrap().layer, "Walls");
        s.isolate_layer("B").unwrap();
        let l = s.layers();
        assert!(l.iter().find(|l| l.name == "Walls").is_some_and(|l| !l.visible));
        s.show_all_layers().unwrap();
        assert!(s.layers().iter().all(|l| l.visible));
        assert_eq!(s.delete_layer("Walls", false).unwrap(), 1);
        assert_eq!(s.markup(&a).unwrap().layer, "");
        s.undo().unwrap();
        assert_eq!(s.markup(&a).unwrap().layer, "Walls");
        assert_eq!(s.delete_layer("Walls", true).unwrap(), 1);
        assert!(s.markup(&a).is_err());
        assert!(s.delete_layer("Nope", false).is_err());
        assert!(s.create_layer("  ").is_err());
    }
}
