//! Shared plumbing for the document features (bookmarks, links, attachments, marks, security,
//! files): undoable edits of the object graph, a decrypted copy of the current state, page
//! objects, name trees, dates and randomness.

use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use markupcraft_model::Document;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, SaveOptions, write_full};

use crate::{EngineError, Result, Session, invalid};

/// Entries read from one name tree, at most.
const MAX_NAMES: usize = 100_000;
const MAX_DEPTH: usize = 32;

/// Any PdfCraft error as an engine error (their messages are written for people).
pub(crate) fn err(e: impl std::fmt::Display) -> EngineError {
    EngineError::Invalid(e.to_string())
}

impl Session {
    /// One undoable step that edits the object graph where markups live (annotation lists,
    /// page boxes, page content). Unsaved markups are written into the graph first and the
    /// model is reloaded afterwards, so markups and pages stay in step with what `f` changed.
    /// `f` sees the flushed model (markup ids and their objects).
    pub(crate) fn graph_edit<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut CosDoc, &Document) -> Result<T>,
    ) -> Result<T> {
        self.edit(label, |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let v = f(&mut s.file.cos, &s.doc)?;
            s.reload();
            Ok((v, true))
        })
    }

    /// One undoable step that edits document-level objects markups do not depend on (outline,
    /// names, info, security). `f` returns its result and whether it changed anything.
    pub(crate) fn cos_edit<T>(&mut self, label: &str, f: impl FnOnce(&mut CosDoc) -> Result<(T, bool)>) -> Result<T> {
        self.edit(label, |s| f(&mut s.file.cos))
    }

    /// The document as it is now (unsaved markups and scales included), unencrypted.
    pub(crate) fn current_copy(&self) -> CosDoc {
        let mut cos = self.file.cos.clone();
        let mut doc = self.doc.clone();
        Session::flush(&mut cos, &mut doc, &self.vp_changed);
        if cos.security().is_some() || cos.output_handler().is_some() {
            cos.remove_encryption();
        }
        cos
    }

    /// [`Self::current_copy`] as PDF bytes (the file's own bytes when nothing changed).
    pub(crate) fn current_bytes(&self) -> Result<Arc<Vec<u8>>> {
        let untouched = !self.file.cos.is_modified()
            && self.file.cos.security().is_none()
            && !self.doc.markups.iter().any(|m| m.dirty)
            && self.doc.deleted.is_empty()
            && self.vp_changed.is_empty();
        if untouched {
            return Ok(self.file.cos.bytes().clone());
        }
        Ok(Arc::new(write_full(&self.current_copy(), &save_options())?))
    }
}

pub(crate) fn save_options() -> SaveOptions {
    SaveOptions {
        mod_date: Some(markupcraft_revu::pdf_date_now()),
        ..Default::default()
    }
}

/// Write a whole document to `out` atomically; returns the byte count.
pub(crate) fn write_doc(cos: &CosDoc, out: &std::path::Path) -> Result<usize> {
    let bytes = write_full(cos, &save_options())?;
    crate::write_atomic(out, &bytes)?;
    Ok(bytes.len())
}

/// Do two paths name the same file?
pub(crate) fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The page objects in document order.
pub(crate) fn page_objs(cos: &CosDoc) -> Result<Vec<ObjRef>> {
    Ok(pdfcraft_organize::pages(cos)
        .map_err(err)?
        .into_iter()
        .map(|p| p.obj)
        .collect())
}

/// 32 bytes of entropy (keys, salts, identifiers): the standard library's randomly keyed
/// hasher mixed with the clock and the process id.
pub(crate) fn random_seed() -> [u8; 32] {
    let mut out = [0u8; 32];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        h.write_usize(i);
        for (d, s) in chunk.iter_mut().zip(h.finish().to_le_bytes()) {
            *d = s;
        }
    }
    out
}

/// Today's date (UTC) as (year, month, day).
pub(crate) fn today() -> (i64, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    civil_from_days(secs.div_euclid(86_400))
}

/// Days since 1970-01-01 to a proleptic Gregorian date (H. Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Text of a string or name object (resolved).
pub(crate) fn text_of(cos: &CosDoc, o: Option<&Object>) -> String {
    match o.map(|o| cos.resolve(o)).as_deref() {
        Some(Object::String(s)) => s.to_text(),
        Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

/// The catalog's `/Names` subtree `key` (e.g. `EmbeddedFiles`, `Dests`), if present.
pub(crate) fn names_tree(cos: &CosDoc, key: &[u8]) -> Option<Object> {
    let root = cos.root()?;
    let cat = cos.get(root);
    let names = cos.dict(cat.as_dict()?.get(b"Names")?)?;
    names.get(key).cloned()
}

/// Every `(key, value)` of a name tree, in tree order. Cycle- and size-safe.
pub(crate) fn name_tree_entries(cos: &CosDoc, tree: &Object) -> Vec<(Vec<u8>, Object)> {
    fn visit(cos: &CosDoc, node: &Object, out: &mut Vec<(Vec<u8>, Object)>, seen: &mut HashSet<ObjRef>, depth: usize) {
        if depth > MAX_DEPTH || out.len() >= MAX_NAMES {
            return;
        }
        if let Some(r) = node.as_ref()
            && !seen.insert(r)
        {
            return;
        }
        let Some(d) = cos.dict(node) else { return };
        if let Some(names) = d.get(b"Names").map(|n| cos.resolve(n))
            && let Some(a) = names.as_array()
        {
            for pair in a.as_chunks::<2>().0 {
                let [k, v] = pair;
                if let Some(k) = cos.resolve(k).as_string() {
                    out.push((k.bytes.clone(), v.clone()));
                }
            }
        }
        if let Some(kids) = d.get(b"Kids").map(|k| cos.resolve(k))
            && let Some(a) = kids.as_array()
        {
            for k in a {
                visit(cos, k, out, seen, depth + 1);
            }
        }
    }
    let mut out = Vec::new();
    visit(cos, tree, &mut out, &mut HashSet::new(), 0);
    out
}

/// Replace the catalog's `/Names` subtree `key` by one flat node holding `entries` (sorted by
/// key, as name trees must be); no entries removes the subtree.
pub(crate) fn write_name_tree(cos: &mut CosDoc, key: &[u8], mut entries: Vec<(Vec<u8>, Object)>) -> Result<()> {
    let root = cos.root().ok_or_else(|| invalid("the PDF has no catalog"))?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.dedup_by(|a, b| a.0 == b.0);
    let cat = cos.dict(&Object::Ref(root)).unwrap_or_default();
    let names_obj = cat.get(b"Names").cloned();
    let mut names = names_obj.as_ref().and_then(|n| cos.dict(n)).unwrap_or_default();
    if entries.is_empty() {
        names.remove(key);
    } else {
        let mut arr = Vec::with_capacity(entries.len() * 2);
        for (k, v) in entries {
            arr.push(Object::String(PdfString { bytes: k, hex: false }));
            arr.push(v);
        }
        let mut node = Dict::new();
        node.set(b"Names".to_vec(), Object::Array(arr));
        let node = cos.add(Object::Dict(node));
        names.set(key.to_vec(), Object::Ref(node));
    }
    match names_obj.and_then(|n| n.as_ref()) {
        Some(r) => cos.set(r, Object::Dict(names)),
        None => cos.update_dict(root, |d| {
            if names.is_empty() {
                d.remove(b"Names");
            } else {
                d.set(b"Names".to_vec(), Object::Dict(names));
            }
        })?,
    }
    Ok(())
}

/// The page (0-based) a destination goes to: an explicit `[page /Fit ...]` array, a named
/// destination, or a GoTo action's `/D`.
pub(crate) fn dest_page(cos: &CosDoc, dest: &Object, pages: &[ObjRef], depth: usize) -> Option<usize> {
    if depth > 4 {
        return None;
    }
    let d = cos.resolve(dest);
    match &*d {
        Object::Array(a) => match a.first()? {
            Object::Ref(r) => pages.iter().position(|p| p == r),
            // Remote destinations carry a page number.
            Object::Int(i) => usize::try_from(*i).ok().filter(|i| *i < pages.len()),
            _ => None,
        },
        Object::Dict(x) => dest_page(cos, x.get(b"D")?, pages, depth + 1),
        Object::String(s) => named_dest(cos, &s.bytes).and_then(|o| dest_page(cos, &o, pages, depth + 1)),
        Object::Name(n) => named_dest(cos, n).and_then(|o| dest_page(cos, &o, pages, depth + 1)),
        _ => None,
    }
}

/// A named destination from the catalog's `/Dests` dictionary or the `/Names /Dests` tree.
fn named_dest(cos: &CosDoc, name: &[u8]) -> Option<Object> {
    let root = cos.root()?;
    let cat = cos.get(root);
    if let Some(dests) = cat.as_dict().and_then(|c| c.get(b"Dests")).and_then(|d| cos.dict(d))
        && let Some(v) = dests.get(name)
    {
        return Some(v.clone());
    }
    let tree = names_tree(cos, b"Dests")?;
    name_tree_entries(cos, &tree)
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

/// The page an outline item or link goes to (`/Dest`, or a GoTo `/A`).
pub(crate) fn item_page(cos: &CosDoc, d: &Dict, pages: &[ObjRef]) -> Option<usize> {
    if let Some(dest) = d.get(b"Dest") {
        return dest_page(cos, dest, pages, 0);
    }
    let a = cos.dict(d.get(b"A")?)?;
    if a.name(b"S") == Some(b"GoTo") {
        return dest_page(cos, a.get(b"D")?, pages, 0);
    }
    None
}

/// The page's `/Annots` list (resolved).
pub(crate) fn annots_of(cos: &CosDoc, page: ObjRef) -> Vec<Object> {
    let p = cos.get(page);
    let Some(d) = p.as_dict() else { return Vec::new() };
    d.get(b"Annots")
        .map(|a| cos.resolve(a))
        .and_then(|a| a.as_array().cloned())
        .unwrap_or_default()
}

/// Replace the page's `/Annots` (in its own array object when it has one).
pub(crate) fn set_annots(cos: &mut CosDoc, page: ObjRef, list: Vec<Object>) -> Result<()> {
    let p = cos.get(page);
    let shared = p
        .as_dict()
        .and_then(|d| d.reference(b"Annots"))
        .filter(|r| cos.get(*r).as_array().is_some());
    match shared {
        Some(r) => cos.set(r, Object::Array(list)),
        None => cos.update_dict(page, |d| {
            if list.is_empty() {
                d.remove(b"Annots");
            } else {
                d.set(b"Annots".to_vec(), Object::Array(list));
            }
        })?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_735), (2026, 10, 9));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn seeds_differ() {
        assert_ne!(random_seed(), random_seed());
    }
}
