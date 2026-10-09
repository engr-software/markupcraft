//! Insert Pages options (Revu's Insert Pages dialog) and Insert > Layered Pages.
//!
//! Options: carry the source's bookmarks (under a bookmark named after the file) and its
//! attachments, merge its document properties into empty ones here, keep its layers (the
//! optional content groups its pages use are registered in this document's layer list), label
//! the new pages from the file name, and interleave (rejoin a scan of odd pages with a scan of
//! even pages, the second optionally in reverse order). Everything is one undo step.
//!
//! Layered Pages draws another PDF's pages on existing pages as a new layer each.

use std::collections::BTreeSet;
use std::path::Path;

use markupcraft_revu::cos::{Document as CosDoc, ObjRef, Object};

use crate::pages::{ForeignPdf, PagePlan, PageReport, PageSource};
use crate::{Result, Session, invalid};

/// Options of Insert Pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InsertOptions {
    pub bookmarks: bool,
    pub attachments: bool,
    pub properties: bool,
    pub layers: bool,
    pub labels_from_name: bool,
    /// Interleave the source pages with this document's (page 1, source 1, page 2, ...).
    pub interleave: bool,
    /// With `interleave`: take the source pages last to first (a stack of even pages scanned
    /// from the back).
    pub reverse: bool,
}

const MERGE: &str = "insert-pages-with-options";

/// Register every optional content group the pages use that the layer list does not have.
fn adopt_groups(cos: &mut CosDoc) -> Result<usize> {
    let pages = crate::docutil::page_objs(cos)?;
    let mut used: BTreeSet<ObjRef> = BTreeSet::new();
    let is_ocg = |cos: &CosDoc, r: ObjRef| {
        cos.get(r)
            .as_dict()
            .and_then(|d| d.get(b"Type").cloned())
            .is_some_and(|t| t == Object::name("OCG"))
    };
    for p in pages {
        let Some(pd) = cos.dict(&Object::Ref(p)) else { continue };
        let Some(res) = pd.get(b"Resources").and_then(|r| cos.dict(r)) else {
            continue;
        };
        let mut cands: Vec<Object> = Vec::new();
        if let Some(props) = res.get(b"Properties").and_then(|x| cos.dict(x)) {
            cands.extend(props.iter().map(|(_, v)| v.clone()));
        }
        if let Some(xo) = res.get(b"XObject").and_then(|x| cos.dict(x)) {
            for (_, v) in xo.iter().take(10_000) {
                if let Some(oc) = cos.dict(v).and_then(|d| d.get(b"OC").cloned()) {
                    cands.push(oc);
                }
            }
        }
        for c in cands.into_iter().take(10_000) {
            if let Object::Ref(r) = c
                && is_ocg(cos, r)
            {
                used.insert(r);
            }
        }
    }
    let root = cos.root().ok_or_else(|| invalid("the document has no catalog"))?;
    let cat = cos.dict(&Object::Ref(root)).unwrap_or_default();
    let oc_obj = cat.get(b"OCProperties").cloned();
    let mut ocp = oc_obj.as_ref().and_then(|o| cos.dict(o)).unwrap_or_default();
    let mut ocgs = ocp
        .get(b"OCGs")
        .map(|o| cos.resolve(o))
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default();
    let have: BTreeSet<ObjRef> = ocgs.iter().filter_map(Object::as_ref).collect();
    let new: Vec<ObjRef> = used.into_iter().filter(|r| !have.contains(r)).collect();
    if new.is_empty() {
        return Ok(0);
    }
    let mut d = ocp.get(b"D").and_then(|o| cos.dict(o)).unwrap_or_default();
    let mut order = d
        .get(b"Order")
        .map(|o| cos.resolve(o))
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default();
    for r in &new {
        ocgs.push(Object::Ref(*r));
        order.push(Object::Ref(*r));
    }
    d.set(b"Order".to_vec(), Object::Array(order));
    ocp.set(b"OCGs".to_vec(), Object::Array(ocgs));
    ocp.set(b"D".to_vec(), Object::Dict(d));
    match oc_obj {
        Some(Object::Ref(r)) => cos.set(r, Object::Dict(ocp)),
        _ => cos.update_dict(root, |c| c.set(b"OCProperties".to_vec(), Object::Dict(ocp)))?,
    }
    Ok(new.len())
}

impl Session {
    /// Insert pages of the PDF at `path` (all when `pages` is `None`) so the first becomes page
    /// `at` (ignored when interleaving), with `o`. One undo step.
    pub fn insert_pages_with(
        &mut self,
        at: usize,
        path: &Path,
        pages: Option<&[usize]>,
        o: &InsertOptions,
    ) -> Result<PageReport> {
        let n = self.page_count();
        if at > n {
            return Err(invalid(format!("position {} is past the end (1 to {})", at + 1, n + 1)));
        }
        let file = ForeignPdf::open(path)?;
        let src_count = file.page_count();
        let chosen: Vec<usize> = match pages {
            Some(p) => p.to_vec(),
            None => (0..src_count).collect(),
        };
        let mut plan = PagePlan::identity(n);
        let at = if o.interleave { n } else { at };
        plan.insert_file(at, file, Some(&chosen))?;
        // Where each inserted page ends up (0-based, in source order).
        let mut placed: Vec<usize> = (at..at + chosen.len()).collect();
        if o.interleave {
            let existing: Vec<_> = plan.pages.iter().take(n).copied().collect();
            let mut foreign: Vec<_> = plan.pages.iter().skip(n).copied().collect();
            if o.reverse {
                foreign.reverse();
            }
            let mut out = Vec::with_capacity(existing.len() + foreign.len());
            let mut e = existing.into_iter();
            let mut f = foreign.into_iter();
            loop {
                let a = e.next();
                if let Some(a) = a {
                    out.push(a);
                }
                let b = f.next();
                if let Some(b) = b {
                    if let PageSource::Foreign { page, .. } = b.source
                        && let Some(i) = chosen.iter().position(|c| *c == page)
                        && let Some(slot) = placed.get_mut(i)
                    {
                        *slot = out.len();
                    }
                    out.push(b);
                }
                if a.is_none() && b.is_none() {
                    break;
                }
            }
            plan.pages = out;
        }
        self.set_merge_key(Some(MERGE));
        let r = self.insert_options_steps(&plan, path, &chosen, &placed, o);
        self.set_merge_key(None);
        self.seal();
        r
    }

    fn insert_options_steps(
        &mut self,
        plan: &PagePlan,
        path: &Path,
        chosen: &[usize],
        placed: &[usize],
        o: &InsertOptions,
    ) -> Result<PageReport> {
        let label = if o.interleave {
            "Insert Pages (Interleaved)"
        } else {
            "Insert Pages"
        };
        let report = self.apply_page_plan(label, plan)?;
        let src = Session::open(path)?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Inserted".into());
        let first = placed.iter().copied().min().unwrap_or(0);
        if o.bookmarks {
            let top = self.add_bookmark(&[], None, &stem, first)?;
            // Source page -> where it went.
            let map = |p: usize| chosen.iter().position(|c| *c == p).and_then(|i| placed.get(i).copied());
            let mut last_page = first;
            for b in src.bookmarks().into_iter().take(10_000) {
                let page = b.page.and_then(map).unwrap_or(last_page);
                last_page = page;
                let mut parent = top.clone();
                parent.extend(b.path.iter().take(b.path.len().saturating_sub(1)));
                self.add_bookmark(&parent, None, &b.title, page)?;
            }
        }
        if o.attachments {
            for a in src.attachments().into_iter().take(1_000) {
                let tmp = std::env::temp_dir().join(format!(
                    "markupcraft-insert-{}-{}",
                    std::process::id(),
                    a.file.replace(['/', '\\', ':'], "_")
                ));
                src.extract_attachment(&a.name, &tmp)?;
                let r = self.add_attachment(&tmp, Some(&a.name), &a.description);
                let _ = std::fs::remove_file(&tmp);
                r?;
            }
        }
        if o.properties {
            let have = self.doc_properties();
            let theirs = src.doc_properties();
            let mut changes = Vec::new();
            for (k, v) in theirs.standard.iter().chain(&theirs.custom) {
                let empty_here = have
                    .standard
                    .iter()
                    .chain(&have.custom)
                    .all(|(hk, hv)| hk != k || hv.is_empty());
                let kept = matches!(k.as_str(), "CreationDate" | "ModDate" | "Producer");
                if empty_here && !kept && !v.is_empty() && changes.len() < 100 {
                    changes.push((k.clone(), Some(v.clone())));
                }
            }
            if !changes.is_empty() {
                self.set_doc_properties(&changes)?;
            }
        }
        if o.labels_from_name {
            let labels: Vec<(usize, String)> = placed
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let l = if placed.len() == 1 {
                        stem.clone()
                    } else {
                        format!("{stem}-{}", i + 1)
                    };
                    (*p, l)
                })
                .collect();
            if !labels.is_empty() {
                self.set_page_labels(&labels)?;
            }
        }
        if o.layers {
            self.cos_edit("Keep Layers", |cos| {
                let n = adopt_groups(cos)?;
                Ok(((), n > 0))
            })?;
        }
        Ok(report)
    }

    /// Insert > Layered Pages: page `k` of `src_pages` (all pages when `None`) of the PDF at
    /// `path` is drawn on page `first + k` of this document as a layer named `name` (default:
    /// the file's name). Returns the pages layered. One undo step.
    pub fn insert_layered_pages(
        &mut self,
        path: &Path,
        src_pages: Option<&[usize]>,
        first: usize,
        name: Option<&str>,
    ) -> Result<usize> {
        self.page(first)?;
        let count = ForeignPdf::open(path)?.page_count();
        let list: Vec<usize> = match src_pages {
            Some(p) => p.to_vec(),
            None => (0..count).collect(),
        };
        if list.is_empty() {
            return Err(invalid("no pages to layer"));
        }
        if let Some(bad) = list.iter().find(|p| **p >= count) {
            return Err(invalid(format!("{} has no page {}", path.display(), bad + 1)));
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Layer".into());
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&stem)
            .to_string();
        let room = self.page_count().saturating_sub(first);
        self.set_merge_key(Some("insert-layered-pages"));
        let mut done = 0;
        let mut result = Ok(());
        for (k, sp) in list.iter().take(room).enumerate() {
            if let Err(e) = self.import_layer(path, *sp, first + k, &name) {
                result = Err(e);
                break;
            }
            done += 1;
        }
        self.set_merge_key(None);
        self.seal();
        result.map(|_| done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn pages(prefix: &str, n: usize) -> Vec<u8> {
        pdf(&(1..=n)
            .map(|i| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, &format!("{prefix}{i}"))))
            .collect::<Vec<_>>())
    }

    #[test]
    fn insert_options_interleave_bookmarks_labels_and_layers() {
        let d = std::env::temp_dir().join(format!("markupcraft-insertmore-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("evens.pdf"), pages("EVEN", 3)).unwrap();
        let mut src = Session::open(d.join("evens.pdf")).unwrap();
        src.add_bookmark(&[], None, "Second", 1).unwrap();
        src.set_doc_properties(&[("Title".into(), Some("Scans".into()))])
            .unwrap();
        src.save(true).unwrap();

        let mut s = Session::from_bytes(pages("ODD", 3), d.join("odds.pdf")).unwrap();
        let o = InsertOptions {
            bookmarks: true,
            properties: true,
            labels_from_name: true,
            interleave: true,
            reverse: true,
            ..Default::default()
        };
        let r = s.insert_pages_with(0, &d.join("evens.pdf"), None, &o).unwrap();
        assert_eq!(r.pages_after, 6);
        // ODD1, EVEN3 (reversed), ODD2, EVEN2, ODD3, EVEN1
        let order: Vec<String> = (0..6).map(|p| s.page_text(p).unwrap().trim().to_string()).collect();
        assert_eq!(order, ["ODD1", "EVEN3", "ODD2", "EVEN2", "ODD3", "EVEN1"]);
        let b = s.bookmarks();
        assert_eq!(b.first().map(|b| b.title.as_str()), Some("evens"));
        assert!(b.iter().any(|x| x.title == "Second" && x.page == Some(3)), "{b:?}");
        let props = s.doc_properties();
        assert_eq!(
            props
                .standard
                .iter()
                .find(|(k, _)| *k == "Title")
                .map(|(_, v)| v.as_str()),
            Some("Scans")
        );
        assert_eq!(s.page_labels().get(5).map(String::as_str), Some("evens-1"));
        // One undo step undoes it all.
        s.undo().unwrap();
        assert_eq!(s.page_count(), 3);

        // Layered pages: the evens drawn on the odd pages as a layer.
        let n = s
            .insert_layered_pages(&d.join("evens.pdf"), Some(&[0, 1]), 1, Some("Scan B"))
            .unwrap();
        assert_eq!(n, 2);
        assert!(s.layers().iter().any(|l| l.name == "Scan B"));
        assert_eq!(s.layers_on_page(1).unwrap(), ["Scan B"]);
        assert_eq!(s.layers_on_page(2).unwrap(), ["Scan B"]);
        assert!(s.layers_on_page(0).unwrap().is_empty());
        s.undo().unwrap();
        assert!(!s.layers().iter().any(|l| l.name == "Scan B"));
    }
}
