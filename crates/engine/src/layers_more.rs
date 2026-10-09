//! More of the Layers panel: the layer hierarchy (the default configuration's `/Order`, nested
//! arrays making children), saved layer configurations (`/OCProperties /Configs`: named
//! visibility presets), the layers used on a page, export states, previewing the print or
//! export layers, and importing another PDF's page as a layer or exporting one layer to a PDF.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, Stream};
use markupcraft_revu::layers::{
    default_config, ensure_group, find_group, group_name, groups, ocprops, ref_array, ref_list, set_ocprops,
};

use crate::docutil::page_objs;
use crate::{Result, Session, invalid};

const MAX_DEPTH: usize = 16;

/// One layer in the hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerNode {
    pub name: String,
    pub depth: usize,
    pub parent: Option<String>,
}

/// What a layer preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPreview {
    /// Only the layers set to print.
    Print,
    /// Only the layers set to export.
    Export,
}

#[derive(Debug, Clone)]
struct Node {
    r: ObjRef,
    kids: Vec<Node>,
}

/// The `/Order` tree (layers missing from it appended at the top level).
fn order_tree(cos: &CosDoc) -> Vec<Node> {
    fn parse(cos: &CosDoc, items: &[Object], depth: usize, seen: &mut HashSet<ObjRef>) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        if depth > MAX_DEPTH {
            return out;
        }
        for it in items {
            match &*cos.resolve(it) {
                Object::Array(a) => {
                    // A labelled group (first element a string) stands alone; else the children
                    // of the layer before it.
                    let labelled = a.first().is_some_and(|f| matches!(&*cos.resolve(f), Object::String(_)));
                    let kids = parse(
                        cos,
                        if labelled { a.get(1..).unwrap_or(&[]) } else { a },
                        depth + 1,
                        seen,
                    );
                    match out.last_mut() {
                        Some(last) if !labelled => last.kids.extend(kids),
                        _ => out.extend(kids),
                    }
                }
                _ => {
                    if let Some(r) = it.as_ref()
                        && group_name(cos, r).is_some()
                        && seen.insert(r)
                    {
                        out.push(Node { r, kids: Vec::new() });
                    }
                }
            }
        }
        out
    }
    let (props, _) = ocprops(cos);
    let d = default_config(cos, &props);
    let order = d
        .get(b"Order")
        .map(|o| cos.resolve(o))
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut tree = parse(cos, &order, 0, &mut seen);
    for (r, _) in groups(cos) {
        if seen.insert(r) {
            tree.push(Node { r, kids: Vec::new() });
        }
    }
    tree
}

fn order_objects(tree: &[Node]) -> Vec<Object> {
    let mut out = Vec::new();
    for n in tree {
        out.push(Object::Ref(n.r));
        if !n.kids.is_empty() {
            out.push(Object::Array(order_objects(&n.kids)));
        }
    }
    out
}

fn flatten_tree(cos: &CosDoc, tree: &[Node], depth: usize, parent: Option<&str>, out: &mut Vec<LayerNode>) {
    for n in tree {
        let name = group_name(cos, n.r).unwrap_or_default();
        out.push(LayerNode {
            name: name.clone(),
            depth,
            parent: parent.map(str::to_string),
        });
        flatten_tree(cos, &n.kids, depth + 1, Some(&name), out);
    }
}

/// Remove `r`'s node (with its subtree) from the tree.
fn take(tree: &mut Vec<Node>, r: ObjRef) -> Option<Node> {
    if let Some(i) = tree.iter().position(|n| n.r == r) {
        return Some(tree.remove(i));
    }
    for n in tree.iter_mut() {
        if let Some(x) = take(&mut n.kids, r) {
            return Some(x);
        }
    }
    None
}

fn find_mut(tree: &mut [Node], r: ObjRef) -> Option<&mut Node> {
    for n in tree.iter_mut() {
        if n.r == r {
            return Some(n);
        }
        if let Some(x) = find_mut(&mut n.kids, r) {
            return Some(x);
        }
    }
    None
}

fn contains(n: &Node, r: ObjRef) -> bool {
    n.r == r || n.kids.iter().any(|k| contains(k, r))
}

fn set_order(cos: &mut CosDoc, tree: &[Node]) {
    let (mut props, holder) = ocprops(cos);
    let mut d = default_config(cos, &props);
    d.set(b"Order".to_vec(), Object::Array(order_objects(tree)));
    props.set(b"D".to_vec(), Object::Dict(d));
    set_ocprops(cos, props, holder);
}

fn usage_state(cos: &CosDoc, r: ObjRef, usage: &[u8], key: &[u8]) -> Option<bool> {
    let g = cos.get(r);
    let u = g.as_dict()?.get(b"Usage").and_then(|u| cos.dict(u))?;
    let x = u.get(usage).and_then(|p| cos.dict(p))?;
    match x.name(key)? {
        b"ON" => Some(true),
        b"OFF" => Some(false),
        _ => None,
    }
}

/// The name of the configuration that holds the visibility a preview replaced.
const RESTORE: &str = "MarkupCraft: before preview";

impl Session {
    /// The layers as a tree, in order (parents before their children).
    pub fn layer_tree(&self) -> Vec<LayerNode> {
        let mut out = Vec::new();
        flatten_tree(&self.file.cos, &order_tree(&self.file.cos), 0, None, &mut out);
        out
    }

    /// Move layer `name` under `parent` (`None` = the top level) at `index` (default last):
    /// Layer hierarchy by drag. Undoable.
    pub fn nest_layer(&mut self, name: &str, parent: Option<&str>, index: Option<usize>) -> Result<()> {
        let cos = &self.file.cos;
        let r = find_group(cos, name).ok_or_else(|| invalid(format!("no layer named {name:?} in the file")))?;
        let p = match parent {
            Some(p) => Some(find_group(cos, p).ok_or_else(|| invalid(format!("no layer named {p:?} in the file")))?),
            None => None,
        };
        self.cos_edit("Arrange Layers", |cos| {
            let mut tree = order_tree(cos);
            let node = take(&mut tree, r).ok_or_else(|| invalid("the layer is not in the list"))?;
            if let Some(p) = p
                && contains(&node, p)
            {
                return Err(invalid("a layer cannot go inside itself"));
            }
            let list = match p {
                Some(p) => {
                    &mut find_mut(&mut tree, p)
                        .ok_or_else(|| invalid("the parent layer is not in the list"))?
                        .kids
                }
                None => &mut tree,
            };
            let at = index.unwrap_or(list.len()).min(list.len());
            list.insert(at, node);
            set_order(cos, &tree);
            Ok(((), true))
        })
    }

    /// Saved layer configurations (named visibility presets).
    pub fn layer_configs(&self) -> Vec<String> {
        let cos = &self.file.cos;
        let (props, _) = ocprops(cos);
        props
            .get(b"Configs")
            .map(|c| cos.resolve(c))
            .and_then(|c| c.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|c| cos.dict(c))
            .map(|d| crate::docutil::text_of(cos, d.get(b"Name")))
            .filter(|n| n != RESTORE)
            .collect()
    }

    fn put_configs(cos: &mut CosDoc, f: impl FnOnce(&mut Vec<Object>, &Dict)) {
        let (mut props, holder) = ocprops(cos);
        let d = default_config(cos, &props);
        let mut list = props
            .get(b"Configs")
            .map(|c| cos.resolve(c))
            .and_then(|c| c.as_array().cloned())
            .unwrap_or_default();
        f(&mut list, &d);
        if list.is_empty() {
            props.remove(b"Configs");
        } else {
            props.set(b"Configs".to_vec(), Object::Array(list));
        }
        set_ocprops(cos, props, holder);
    }

    /// Save the current visibility as configuration `name` (replacing one of that name). Undoable.
    pub fn save_layer_config(&mut self, name: &str) -> Result<()> {
        let name = crate::layers::check_name(name)?;
        self.cos_edit("Save Layer Configuration", |cos| {
            let named = |cos: &CosDoc, c: &Object, n: &str| {
                cos.dict(c)
                    .is_some_and(|d| crate::docutil::text_of(cos, d.get(b"Name")) == n)
            };
            let snapshot: Vec<Object> = {
                let probe = cos.clone();
                let (props, _) = ocprops(&probe);
                props
                    .get(b"Configs")
                    .map(|c| probe.resolve(c))
                    .and_then(|c| c.as_array().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| !named(&probe, c, &name))
                    .collect()
            };
            Self::put_configs(cos, |list, d| {
                *list = snapshot;
                let mut c = Dict::new();
                c.set(b"Name".to_vec(), Object::String(PdfString::text(&name)));
                for k in [b"ON".as_slice(), b"OFF", b"Locked"] {
                    if let Some(v) = d.get(k) {
                        c.set(k.to_vec(), v.clone());
                    }
                }
                list.push(Object::Dict(c));
            });
            Ok(((), true))
        })
    }

    /// Switch to configuration `name`: its visibility becomes the current one. Undoable.
    pub fn apply_layer_config(&mut self, name: &str) -> Result<()> {
        let cos = &self.file.cos;
        let (props, _) = ocprops(cos);
        let cfg = props
            .get(b"Configs")
            .map(|c| cos.resolve(c))
            .and_then(|c| c.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|c| cos.dict(c))
            .find(|d| crate::docutil::text_of(cos, d.get(b"Name")) == name)
            .ok_or_else(|| invalid(format!("no layer configuration {name:?}")))?;
        self.cos_edit("Apply Layer Configuration", |cos| {
            let (mut props, holder) = ocprops(cos);
            let mut d = default_config(cos, &props);
            for k in [b"ON".as_slice(), b"OFF", b"Locked"] {
                match cfg.get(k) {
                    Some(v) => d.set(k.to_vec(), v.clone()),
                    None => {
                        d.remove(k);
                    }
                }
            }
            props.set(b"D".to_vec(), Object::Dict(d));
            set_ocprops(cos, props, holder);
            Ok(((), true))
        })
    }

    pub fn delete_layer_config(&mut self, name: &str) -> Result<()> {
        if !self.layer_configs().iter().any(|n| n == name) && name != RESTORE {
            return Err(invalid(format!("no layer configuration {name:?}")));
        }
        let name = name.to_string();
        self.cos_edit("Delete Layer Configuration", |cos| {
            let probe = cos.clone();
            Self::put_configs(cos, |list, _| {
                list.retain(|c| {
                    probe
                        .dict(c)
                        .is_none_or(|d| crate::docutil::text_of(&probe, d.get(b"Name")) != name)
                });
            });
            Ok(((), true))
        })
    }

    /// The layers used on `page`: by its markups and by its page content.
    pub fn layers_on_page(&self, page: usize) -> Result<Vec<String>> {
        self.page(page)?;
        let mut out: Vec<String> = self
            .doc
            .markups
            .iter()
            .filter(|m| m.page == page && !m.layer.is_empty())
            .map(|m| m.layer.clone())
            .collect();
        let cos = &self.file.cos;
        if let Some(p) = page_objs(cos)?.get(page).copied() {
            let props = cos
                .dict(&Object::Ref(p))
                .and_then(|d| d.get(b"Resources").and_then(|r| cos.dict(r)))
                .and_then(|r| r.get(b"Properties").and_then(|x| cos.dict(x)))
                .unwrap_or_default();
            for (_, v) in props.iter() {
                if let Some(r) = v.as_ref()
                    && let Some(n) = group_name(cos, r)
                {
                    out.push(n);
                }
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// A layer's export state (`/Usage /Export /ExportState`; default: shown).
    pub fn layer_export(&self, name: &str) -> Result<bool> {
        let cos = &self.file.cos;
        let r = find_group(cos, name).ok_or_else(|| invalid(format!("no layer named {name:?} in the file")))?;
        Ok(usage_state(cos, r, b"Export", b"ExportState").unwrap_or(true))
    }

    pub fn set_layer_export(&mut self, name: &str, on: bool) -> Result<()> {
        let r =
            find_group(&self.file.cos, name).ok_or_else(|| invalid(format!("no layer named {name:?} in the file")))?;
        self.cos_edit("Layer Export", |cos| {
            cos.update_dict(r, |g| {
                let mut ex = Dict::new();
                ex.set(b"ExportState".to_vec(), Object::name(if on { "ON" } else { "OFF" }));
                let mut u = match g.get(b"Usage") {
                    Some(Object::Dict(u)) => u.clone(),
                    _ => Dict::new(),
                };
                u.set(b"Export".to_vec(), Object::Dict(ex));
                g.set(b"Usage".to_vec(), Object::Dict(u));
            })?;
            Ok(((), true))
        })
    }

    /// Show Print / Export layers: only the layers set to print (or export) are shown; the
    /// visibility before is kept so [`Self::end_layer_preview`] brings it back.
    pub fn preview_layers(&mut self, what: LayerPreview) -> Result<()> {
        if !self.layer_configs_all().iter().any(|n| n == RESTORE) {
            self.save_layer_config(RESTORE)?;
        }
        let cos = &self.file.cos;
        let (props, _) = ocprops(cos);
        let d = default_config(cos, &props);
        let off_now = ref_list(cos, &d, b"OFF");
        let all = groups(cos);
        let show: Vec<ObjRef> = all
            .iter()
            .filter(|(r, _)| match what {
                LayerPreview::Print => usage_state(cos, *r, b"Print", b"PrintState").unwrap_or(!off_now.contains(r)),
                LayerPreview::Export => usage_state(cos, *r, b"Export", b"ExportState").unwrap_or(true),
            })
            .map(|(r, _)| *r)
            .collect();
        let hide: Vec<ObjRef> = all.iter().map(|(r, _)| *r).filter(|r| !show.contains(r)).collect();
        self.cos_edit("Preview Layers", |cos| {
            let (mut props, holder) = ocprops(cos);
            let mut d = default_config(cos, &props);
            d.set(b"ON".to_vec(), ref_array(&show));
            d.set(b"OFF".to_vec(), ref_array(&hide));
            props.set(b"D".to_vec(), Object::Dict(d));
            set_ocprops(cos, props, holder);
            Ok(((), true))
        })
    }

    /// End a print or export preview: the visibility before it comes back.
    pub fn end_layer_preview(&mut self) -> Result<()> {
        if !self.layer_configs_all().iter().any(|n| n == RESTORE) {
            return Ok(());
        }
        self.apply_layer_config(RESTORE)?;
        self.delete_layer_config(RESTORE)
    }

    fn layer_configs_all(&self) -> Vec<String> {
        let cos = &self.file.cos;
        let (props, _) = ocprops(cos);
        props
            .get(b"Configs")
            .map(|c| cos.resolve(c))
            .and_then(|c| c.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|c| cos.dict(c))
            .map(|d| crate::docutil::text_of(cos, d.get(b"Name")))
            .collect()
    }

    /// Import Layer: page `src_page` of the PDF at `src` drawn on `page` as layer `name` (its
    /// lower-left corner on the page's). Undoable.
    pub fn import_layer(&mut self, src: &Path, src_page: usize, page: usize, name: &str) -> Result<()> {
        self.page(page)?;
        let name = crate::layers::check_name(name)?;
        let bytes = crate::raster::read_pdf(src)?;
        let src_cos = CosDoc::open(Arc::new(bytes.as_ref().clone()))?;
        let pages = crate::overlay::source_pages(&src_cos)?;
        let sp = pages
            .get(src_page)
            .ok_or_else(|| invalid(format!("{} has no page {}", src.display(), src_page + 1)))?;
        let data = crate::overlay::page_content(&src_cos, &sp.dict)?;
        let dst_box = self.page(page)?.crop.normalized();
        self.graph_edit("Import Layer", |cos, _| {
            let mut imp = crate::overlay::Importer::new(&src_cos);
            let mut fd = Dict::new();
            fd.set(b"Type".to_vec(), Object::name("XObject"));
            fd.set(b"Subtype".to_vec(), Object::name("Form"));
            let b = sp.box_;
            fd.set(
                b"BBox".to_vec(),
                Object::Array(vec![
                    Object::Real(b.x0),
                    Object::Real(b.y0),
                    Object::Real(b.x1),
                    Object::Real(b.y1),
                ]),
            );
            if let Some(res) = &sp.resources {
                let r = imp.rewrite(cos, res, 0);
                fd.set(b"Resources".to_vec(), r);
            }
            imp.drain(cos)?;
            let form = cos.add(Object::Stream(Stream::flate(fd, &data)));
            let oc = ensure_group(cos, &name).ok_or_else(|| invalid("the document has no catalog"))?;
            let pref = *page_objs(cos)?.get(page).ok_or_else(|| invalid("no such page"))?;
            let pd = cos.dict(&Object::Ref(pref)).unwrap_or_default();
            let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
            let mut xo = res.get(b"XObject").and_then(|x| cos.dict(x)).unwrap_or_default();
            let mut props = res.get(b"Properties").and_then(|x| cos.dict(x)).unwrap_or_default();
            let mut k = 0;
            while xo.contains(format!("MCImp{k}").as_bytes()) || props.contains(format!("MCImpL{k}").as_bytes()) {
                k += 1;
            }
            xo.set(format!("MCImp{k}").into_bytes(), Object::Ref(form));
            props.set(format!("MCImpL{k}").into_bytes(), Object::Ref(oc));
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            res.set(b"Properties".to_vec(), Object::Dict(props));
            let content = format!(
                "\nQ\nq /OC /MCImpL{k} BDC 1 0 0 1 {} {} cm /MCImp{k} Do EMC Q\n",
                dst_box.x0 - b.x0,
                dst_box.y0 - b.y0
            );
            let open = cos.add(Object::Stream(Stream::from_raw(Dict::new(), b"q\n".to_vec())));
            let close = cos.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
            let mut list = match pd.get(b"Contents").map(|c| cos.resolve(c)).as_deref() {
                Some(Object::Array(a)) => a.clone(),
                Some(_) => pd.get(b"Contents").cloned().into_iter().collect(),
                None => Vec::new(),
            };
            list.insert(0, Object::Ref(open));
            list.push(Object::Ref(close));
            cos.update_dict(pref, |d| {
                d.set(b"Resources".to_vec(), Object::Dict(res));
                d.set(b"Contents".to_vec(), Object::Array(list));
            })?;
            Ok(())
        })
    }

    /// Export Layer: a PDF at `out` holding only layer `name`: its page content (drawing on
    /// other layers and on no layer is left out) and its markups. Returns the page count.
    pub fn export_layer(&self, name: &str, out: &Path) -> Result<usize> {
        if crate::docutil::same_file(out, self.path()) {
            return Err(invalid("export the layer to a different file"));
        }
        if !self.layers().iter().any(|l| l.name == name) {
            return Err(invalid(format!("no layer named {name:?}")));
        }
        let mut t = Session::from_bytes(self.current_bytes()?.as_ref().clone(), out)?;
        let others: Vec<String> = t
            .doc()
            .markups
            .iter()
            .filter(|m| m.layer != name)
            .map(|m| m.id.clone())
            .collect();
        if !others.is_empty() {
            t.delete_markups(&others, true)?;
        }
        let target = find_group(&t.file.cos, name);
        t.graph_edit("Export Layer", |cos, _| {
            for p in page_objs(cos)? {
                let pd = cos.dict(&Object::Ref(p)).unwrap_or_default();
                let props = pd
                    .get(b"Resources")
                    .and_then(|r| cos.dict(r))
                    .and_then(|r| r.get(b"Properties").and_then(|x| cos.dict(x)))
                    .unwrap_or_default();
                let data = crate::overlay::page_content(cos, &pd)?;
                let parsed = pdfcraft_content::parse(&data);
                let mut stack: Vec<bool> = Vec::new();
                let mut ops = Vec::with_capacity(parsed.ops.len());
                for op in parsed.ops {
                    let keep_here = stack.last().copied().unwrap_or(false);
                    match op.op.as_slice() {
                        b"BDC" => {
                            let mine = op.name(0) == Some(b"OC")
                                && op
                                    .name(1)
                                    .and_then(|n| props.get(n))
                                    .and_then(Object::as_ref)
                                    .is_some_and(|r| Some(r) == target);
                            let other_oc = op.name(0) == Some(b"OC") && !mine;
                            stack.push(if other_oc { false } else { mine || keep_here });
                            ops.push(op);
                        }
                        b"BMC" => {
                            stack.push(keep_here);
                            ops.push(op);
                        }
                        b"EMC" => {
                            stack.pop();
                            ops.push(op);
                        }
                        // Painting outside the layer: end the path without painting.
                        b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" if !keep_here => {
                            ops.push(pdfcraft_content::Op::new("n", vec![]));
                        }
                        b"Tj" | b"TJ" | b"'" | b"\"" | b"Do" | b"sh" | b"BI" if !keep_here => {}
                        _ if op.inline.is_some() && !keep_here => {}
                        _ => ops.push(op),
                    }
                }
                let s = cos.add(Object::Stream(Stream::flate(
                    Dict::new(),
                    &pdfcraft_content::serialize_ops(&ops),
                )));
                cos.update_dict(p, |d| d.set(b"Contents".to_vec(), Object::Ref(s)))?;
            }
            Ok(())
        })?;
        let n = t.page_count();
        t.save_as(out, true)?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::LayerState;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    fn session() -> Session {
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(10.0, 10.0, 20.0, 20.0))]),
            "l.pdf",
        )
        .unwrap();
        for n in ["Arch", "Doors", "Mech"] {
            s.create_layer(n).unwrap();
        }
        s
    }

    #[test]
    fn hierarchy_and_configurations() {
        let mut s = session();
        s.nest_layer("Doors", Some("Arch"), None).unwrap();
        let t = s.layer_tree();
        assert_eq!(
            t[1],
            LayerNode {
                name: "Doors".into(),
                depth: 1,
                parent: Some("Arch".into())
            }
        );
        assert!(s.nest_layer("Arch", Some("Doors"), None).is_err(), "not into itself");
        s.nest_layer("Mech", None, Some(0)).unwrap();
        assert_eq!(s.layer_tree()[0].name, "Mech");
        s.nest_layer("Doors", None, None).unwrap();
        assert!(s.layer_tree().iter().all(|n| n.depth == 0));
        // Configurations.
        s.set_layer_state(
            "Mech",
            LayerState {
                visible: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        s.save_layer_config("No Mech").unwrap();
        s.show_all_layers().unwrap();
        assert!(s.layers().iter().all(|l| l.visible));
        s.apply_layer_config("No Mech").unwrap();
        assert!(!s.layers().iter().find(|l| l.name == "Mech").unwrap().visible);
        assert_eq!(s.layer_configs(), vec!["No Mech".to_string()]);
        s.delete_layer_config("No Mech").unwrap();
        assert!(s.layer_configs().is_empty());
        // Print preview: Doors does not print.
        s.show_all_layers().unwrap();
        s.set_layer_state(
            "Doors",
            LayerState {
                print: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        s.set_layer_export("Arch", false).unwrap();
        assert!(!s.layer_export("Arch").unwrap());
        s.preview_layers(LayerPreview::Print).unwrap();
        let vis = |s: &Session, n: &str| s.layers().iter().find(|l| l.name == n).unwrap().visible;
        assert!(!vis(&s, "Doors") && vis(&s, "Arch"));
        s.preview_layers(LayerPreview::Export).unwrap();
        assert!(!vis(&s, "Arch") && vis(&s, "Doors"));
        s.end_layer_preview().unwrap();
        assert!(vis(&s, "Arch") && vis(&s, "Doors"));
        assert!(s.layer_configs().is_empty());
    }

    #[test]
    fn import_a_page_as_a_layer_and_export_it_alone() {
        let d = std::env::temp_dir().join(format!("markupcraft-layersmore-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("src.pdf");
        std::fs::write(
            &src,
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(300.0, 300.0, 100.0, 100.0))]),
        )
        .unwrap();
        let mut s = session();
        s.import_layer(&src, 0, 0, "Imported").unwrap();
        assert!(s.layers().iter().any(|l| l.name == "Imported"));
        assert_eq!(s.layers_on_page(0).unwrap(), vec!["Imported".to_string()]);
        let px = |s: &Session, x: f64, y: f64| {
            s.renderable(true)
                .unwrap()
                .render(0, 0.5, 2000.0)
                .unwrap()
                .gray
                .get((x * 0.5) as usize, ((792.0 - y) * 0.5) as usize)
        };
        assert!(px(&s, 350.0, 350.0) < 100, "imported content drawn");
        s.set_layer_state(
            "Imported",
            LayerState {
                visible: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(px(&s, 350.0, 350.0) > 200, "hidden with its layer");
        s.set_layer_state(
            "Imported",
            LayerState {
                visible: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let out = d.join("layer.pdf");
        s.export_layer("Imported", &out).unwrap();
        let e = Session::open(&out).unwrap();
        assert!(px(&e, 350.0, 350.0) < 100, "the layer's content");
        assert!(px(&e, 20.0, 20.0) > 200, "content on no layer left out");
        assert!(s.export_layer("Nope", &out).is_err());
    }
}
