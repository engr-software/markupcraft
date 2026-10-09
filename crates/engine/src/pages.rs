//! Page operations on the PDF: rotate, delete, move, insert blank pages, insert pages from
//! another PDF, extract pages to a new PDF.
//!
//! Every operation is a [`PagePlan`]: the output pages, in order, each taken from this file,
//! another file or blank. [`apply_plan`] rebuilds the page tree from it on the object graph.
//! Markups are annotations in each page's `/Annots`, so they follow their page; page scales
//! (`/VP`) live on the page too, and page labels are recomputed per page from where it came
//! from. Pages that leave the file are replaced by `null`, so links and bookmarks to them stop
//! pulling their content into the saved file.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, SaveOptions, Stream, write_full};

use crate::labels::{self, LabelSpec};
use crate::{EngineError, Result, Session, blank, invalid};

/// Objects copied from another file per insert, at most.
const MAX_IMPORT: usize = 2_000_000;
const MAX_DEPTH: usize = 64;
const INHERITED: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// One page as it sits in a page tree, with the attributes it inherits.
#[derive(Debug, Clone)]
pub(crate) struct FlatPage {
    id: ObjRef,
    /// `MediaBox`, `CropBox`, `Resources`, `Rotate` from the page or its nearest ancestor
    inherited: [Option<Object>; 4],
}

impl FlatPage {
    fn rotate(&self, cos: &CosDoc) -> i64 {
        self.inherited[3]
            .as_ref()
            .and_then(|o| cos.resolve(o).as_f64())
            .map_or(0, |r| (r as i64).rem_euclid(360))
    }
}

/// The page tree root and every page, in the order `markupcraft-revu` reads them.
pub(crate) fn flatten(cos: &CosDoc) -> Result<(ObjRef, Vec<FlatPage>)> {
    let root = cos.root().ok_or_else(|| invalid("the PDF has no catalog"))?;
    let pages = cos
        .get(root)
        .as_dict()
        .and_then(|d| d.reference(b"Pages"))
        .ok_or_else(|| invalid("the PDF has no page tree"))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk(cos, pages, Default::default(), &mut seen, &mut out, 0);
    Ok((pages, out))
}

fn walk(
    cos: &CosDoc,
    node: ObjRef,
    inh: [Option<Object>; 4],
    seen: &mut HashSet<ObjRef>,
    out: &mut Vec<FlatPage>,
    depth: usize,
) {
    if depth > MAX_DEPTH || !seen.insert(node) {
        return;
    }
    let obj = cos.get(node);
    let Some(d) = obj.as_dict() else { return };
    let mut inh = inh;
    for (slot, key) in inh.iter_mut().zip(INHERITED) {
        if let Some(v) = d.get(key) {
            *slot = Some(v.clone());
        }
    }
    if d.name(b"Type") == Some(b"Pages") || d.contains(b"Kids") {
        let kids = cos.resolve(d.get(b"Kids").unwrap_or(&Object::Null));
        if let Some(arr) = kids.as_array() {
            for k in arr {
                if let Some(r) = k.as_ref() {
                    walk(cos, r, inh.clone(), seen, out, depth + 1);
                }
            }
        }
        return;
    }
    out.push(FlatPage {
        id: node,
        inherited: inh,
    });
}

/// Another PDF that pages are inserted from.
pub struct ForeignPdf {
    pub path: PathBuf,
    cos: CosDoc,
    pages: Vec<FlatPage>,
    labels: Option<Vec<Option<LabelSpec>>>,
}

impl ForeignPdf {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let cos = CosDoc::open(Arc::new(data))?;
        let (_, pages) = flatten(&cos)?;
        let labels = labels::read(&cos, pages.len());
        Ok(Self {
            path: path.to_path_buf(),
            cos,
            pages,
            labels,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// Where an output page comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSource {
    /// A page of this document (0-based, before the operation).
    Existing(usize),
    /// A new blank page of this size in points.
    Blank { width: f64, height: f64 },
    /// Page `page` of `PagePlan::foreign[doc]`.
    Foreign { doc: usize, page: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanPage {
    pub source: PageSource,
    /// Degrees added to the page's `/Rotate` (a multiple of 90).
    pub rotate: i64,
}

/// The output pages of a page operation, in order.
pub struct PagePlan {
    pub pages: Vec<PlanPage>,
    pub foreign: Vec<ForeignPdf>,
    /// false: drop the bookmarks (extracting to a new file)
    pub keep_outline: bool,
}

/// Sorted, de-duplicated, in range.
fn clean(pages: &[usize], size: usize) -> Vec<usize> {
    let mut v: Vec<usize> = pages.iter().copied().filter(|p| *p < size).collect();
    v.sort_unstable();
    v.dedup();
    v
}

impl PagePlan {
    /// Every page of an `n`-page document, unchanged.
    pub fn identity(n: usize) -> Self {
        Self {
            pages: (0..n)
                .map(|i| PlanPage {
                    source: PageSource::Existing(i),
                    rotate: 0,
                })
                .collect(),
            foreign: Vec::new(),
            keep_outline: true,
        }
    }

    /// Positions in `pages` are positions in the current plan.
    pub fn rotate(&mut self, pages: &[usize], degrees: i64) {
        for i in clean(pages, self.pages.len()) {
            if let Some(p) = self.pages.get_mut(i) {
                p.rotate = (p.rotate.rem_euclid(360) + degrees.rem_euclid(360)).rem_euclid(360);
            }
        }
    }

    pub fn delete(&mut self, pages: &[usize]) {
        for i in clean(pages, self.pages.len()).into_iter().rev() {
            self.pages.remove(i);
        }
    }

    /// Move `pages` (any order) so they sit together, in document order, before position
    /// `before` (0..=len, counted before the move).
    pub fn move_pages(&mut self, pages: &[usize], before: usize) {
        let pages = clean(pages, self.pages.len());
        let before = before.min(self.pages.len());
        let moved: Vec<PlanPage> = pages.iter().filter_map(|i| self.pages.get(*i).copied()).collect();
        let less = pages.iter().filter(|i| **i < before).count();
        for i in pages.iter().rev() {
            self.pages.remove(*i);
        }
        let at = (before - less).min(self.pages.len());
        self.pages.splice(at..at, moved);
    }

    pub fn insert_blank(&mut self, at: usize, count: usize, width: f64, height: f64) {
        let at = at.min(self.pages.len());
        let add = std::iter::repeat_n(
            PlanPage {
                source: PageSource::Blank { width, height },
                rotate: 0,
            },
            count,
        );
        self.pages.splice(at..at, add);
    }

    /// Insert `pages` of `file` (all when `None`) before position `at`.
    pub fn insert_file(&mut self, at: usize, file: ForeignPdf, pages: Option<&[usize]>) -> Result<()> {
        let n = file.page_count();
        let pages: Vec<usize> = match pages {
            Some(p) => {
                if let Some(bad) = p.iter().find(|p| **p >= n) {
                    return Err(invalid(format!(
                        "page {} is not in {} ({n} pages)",
                        bad + 1,
                        file.path.display()
                    )));
                }
                p.to_vec()
            }
            None => (0..n).collect(),
        };
        if pages.is_empty() {
            return Err(invalid(format!("{} has no pages to insert", file.path.display())));
        }
        let doc = self.foreign.len();
        self.foreign.push(file);
        let at = at.min(self.pages.len());
        let add: Vec<PlanPage> = pages
            .into_iter()
            .map(|page| PlanPage {
                source: PageSource::Foreign { doc, page },
                rotate: 0,
            })
            .collect();
        self.pages.splice(at..at, add);
        Ok(())
    }

    /// Only `pages` (positions in this plan), without bookmarks: the plan of an extract.
    pub fn extract(&self, pages: &[usize]) -> PagePlan {
        PagePlan {
            pages: clean(pages, self.pages.len())
                .into_iter()
                .filter_map(|i| self.pages.get(i).copied())
                .collect(),
            foreign: Vec::new(),
            keep_outline: false,
        }
    }
}

/// Copies objects from another document, following references.
struct Importer<'a> {
    src: &'a CosDoc,
    map: HashMap<ObjRef, ObjRef>,
    /// pages of the source that are not inserted: references to them become null
    skip: HashSet<ObjRef>,
    /// inserted pages: their `/Parent` is dropped
    pages: HashSet<ObjRef>,
    queue: Vec<ObjRef>,
    copied: usize,
}

impl<'a> Importer<'a> {
    fn new(src: &'a ForeignPdf, used: &HashSet<usize>) -> Self {
        let mut skip = HashSet::new();
        let mut pages = HashSet::new();
        for (i, p) in src.pages.iter().enumerate() {
            if used.contains(&i) {
                pages.insert(p.id);
            } else {
                skip.insert(p.id);
            }
        }
        Self {
            src: &src.cos,
            map: HashMap::new(),
            skip,
            pages,
            queue: Vec::new(),
            copied: 0,
        }
    }

    fn reference(&mut self, dst: &mut CosDoc, r: ObjRef) -> Object {
        if self.skip.contains(&r) {
            return Object::Null;
        }
        if let Some(n) = self.map.get(&r) {
            return Object::Ref(*n);
        }
        let n = dst.add(Object::Null);
        self.map.insert(r, n);
        self.queue.push(r);
        Object::Ref(n)
    }

    fn rewrite(&mut self, dst: &mut CosDoc, o: &Object, depth: usize) -> Object {
        if depth > MAX_DEPTH {
            return Object::Null;
        }
        match o {
            Object::Ref(r) => self.reference(dst, *r),
            Object::Array(a) => Object::Array(a.iter().map(|x| self.rewrite(dst, x, depth + 1)).collect()),
            Object::Dict(d) => Object::Dict(self.rewrite_dict(dst, d, depth)),
            Object::Stream(s) => Object::Stream(Stream {
                dict: self.rewrite_dict(dst, &s.dict, depth),
                raw: s.raw.clone(),
            }),
            other => other.clone(),
        }
    }

    fn rewrite_dict(&mut self, dst: &mut CosDoc, d: &Dict, depth: usize) -> Dict {
        d.iter()
            .map(|(k, v)| (k.clone(), self.rewrite(dst, v, depth + 1)))
            .collect()
    }

    /// Copy everything queued.
    fn drain(&mut self, dst: &mut CosDoc) -> Result<()> {
        while let Some(r) = self.queue.pop() {
            self.copied += 1;
            if self.copied > MAX_IMPORT {
                return Err(invalid(format!(
                    "the inserted pages reference more than {MAX_IMPORT} objects"
                )));
            }
            let Some(new) = self.map.get(&r).copied() else { continue };
            let obj = self.src.get(r);
            let mut out = self.rewrite(dst, &obj, 0);
            if self.pages.contains(&r)
                && let Some(d) = out.as_dict_mut()
            {
                d.remove(b"Parent");
            }
            dst.set(new, out);
        }
        Ok(())
    }
}

/// Rebuild `cos`'s page tree as `plan` says.
pub fn apply_plan(cos: &mut CosDoc, plan: &PagePlan) -> Result<()> {
    if plan.pages.is_empty() {
        return Err(invalid("a PDF needs at least one page"));
    }
    let (root_pages, flat) = flatten(cos)?;
    let n = flat.len();
    for p in &plan.pages {
        match p.source {
            PageSource::Existing(i) if i >= n => {
                return Err(EngineError::NoPage { page: i + 1, count: n });
            }
            PageSource::Foreign { doc, page } => {
                let f = plan
                    .foreign
                    .get(doc)
                    .ok_or_else(|| invalid("page plan names a file it does not hold"))?;
                if page >= f.page_count() {
                    return Err(invalid(format!(
                        "page {} is not in {} ({} pages)",
                        page + 1,
                        f.path.display(),
                        f.page_count()
                    )));
                }
            }
            PageSource::Blank { width, height } => blank::check_size(width, height)?,
            PageSource::Existing(_) => {}
        }
        if p.rotate % 90 != 0 {
            return Err(invalid(format!("rotation {} is not a multiple of 90", p.rotate)));
        }
    }

    let own_labels = labels::read(cos, n);
    let any_labels = own_labels.is_some() || plan.foreign.iter().any(|f| f.labels.is_some());

    // Which pages of each foreign file are inserted.
    let mut used: Vec<HashSet<usize>> = vec![HashSet::new(); plan.foreign.len()];
    for p in &plan.pages {
        if let PageSource::Foreign { doc, page } = p.source
            && let Some(u) = used.get_mut(doc)
        {
            u.insert(page);
        }
    }
    let mut importers: Vec<Importer> = plan
        .foreign
        .iter()
        .zip(&used)
        .map(|(f, u)| Importer::new(f, u))
        .collect();

    let mut kids = Vec::with_capacity(plan.pages.len());
    let mut specs = Vec::with_capacity(plan.pages.len());
    let mut kept = vec![false; n];
    let mut foreign_out = Vec::new();
    for (i, p) in plan.pages.iter().enumerate() {
        let (page_ref, label, base_rotate) = match p.source {
            PageSource::Existing(k) => {
                let Some(fp) = flat.get(k) else { continue };
                let mut d = cos.dict(&Object::Ref(fp.id)).unwrap_or_default();
                let page_ref = if kept.get(k).copied().unwrap_or(false) {
                    // The same page twice: a copy without its markups.
                    d.remove(b"Annots");
                    cos.add(Object::Dict(Dict::new()))
                } else {
                    fp.id
                };
                if let Some(slot) = kept.get_mut(k) {
                    *slot = true;
                }
                push_inherited(&mut d, &fp.inherited, |o| o.clone());
                d.set(b"Parent".to_vec(), Object::Ref(root_pages));
                cos.set(page_ref, Object::Dict(d));
                let label = own_labels.as_ref().and_then(|l| l.get(k).cloned().flatten());
                (page_ref, label, fp.rotate(cos))
            }
            PageSource::Blank { width, height } => {
                let mut d = blank::page_dict(cos, width, height);
                d.set(b"Parent".to_vec(), Object::Ref(root_pages));
                (cos.add(Object::Dict(d)), None, 0)
            }
            PageSource::Foreign { doc, page } => {
                let (Some(f), Some(imp)) = (plan.foreign.get(doc), importers.get_mut(doc)) else {
                    continue;
                };
                let Some(fp) = f.pages.get(page) else { continue };
                let new = imp.reference(cos, fp.id);
                imp.drain(cos)?;
                let mut d = cos.dict(&new).unwrap_or_default();
                let inherited: Vec<Option<Object>> = fp
                    .inherited
                    .iter()
                    .map(|o| o.as_ref().map(|o| imp.rewrite(cos, o, 0)))
                    .collect();
                imp.drain(cos)?;
                let inh: [Option<Object>; 4] = [
                    inherited.first().cloned().flatten(),
                    inherited.get(1).cloned().flatten(),
                    inherited.get(2).cloned().flatten(),
                    inherited.get(3).cloned().flatten(),
                ];
                push_inherited(&mut d, &inh, |o| o.clone());
                d.set(b"Parent".to_vec(), Object::Ref(root_pages));
                let Some(page_ref) = new.as_ref() else { continue };
                cos.set(page_ref, Object::Dict(d));
                foreign_out.push(page_ref);
                let label = f.labels.as_ref().and_then(|l| l.get(page).cloned().flatten());
                (page_ref, label, fp.rotate(&f.cos))
            }
        };
        let rot = (base_rotate.rem_euclid(360) + p.rotate.rem_euclid(360)).rem_euclid(360);
        let _ = cos.update_dict(page_ref, |d| {
            if rot == 0 {
                d.remove(b"Rotate");
            } else {
                d.set(b"Rotate".to_vec(), Object::Int(rot));
            }
        });
        kids.push(Object::Ref(page_ref));
        specs.push(label.unwrap_or_else(|| LabelSpec::decimal(i as i64 + 1)));
    }

    let count = kids.len() as i64;
    cos.update_dict(root_pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(kids));
        d.set(b"Count".to_vec(), Object::Int(count));
        for k in INHERITED {
            d.remove(k);
        }
    })?;
    for (k, fp) in flat.iter().enumerate() {
        if !kept.get(k).copied().unwrap_or(true) {
            cos.set(fp.id, Object::Null);
        }
    }
    rename_duplicate_ids(cos, &foreign_out);
    if any_labels {
        labels::write(cos, &specs);
    }
    if !plan.keep_outline
        && let Some(root) = cos.root()
    {
        let _ = cos.update_dict(root, |d| {
            d.remove(b"Outlines");
        });
    }
    Ok(())
}

/// Give the page its inherited attributes itself (it now hangs directly under the root).
fn push_inherited(d: &mut Dict, inherited: &[Option<Object>; 4], map: impl Fn(&Object) -> Object) {
    for (key, v) in INHERITED.iter().zip(inherited) {
        if *key == b"Rotate" || d.contains(key) {
            continue;
        }
        if let Some(v) = v {
            d.set(key.to_vec(), map(v));
        }
    }
    if !d.contains(b"MediaBox") {
        d.set(
            b"MediaBox".to_vec(),
            Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(612), Object::Int(792)]),
        );
    }
}

/// Markups on pages copied from another file keep their data but get a new `/NM` when the id
/// is already used in this file (inserting a file into itself, or two copies of a set).
fn rename_duplicate_ids(cos: &mut CosDoc, foreign_pages: &[ObjRef]) {
    let Ok((_, flat)) = flatten(cos) else { return };
    let foreign: HashSet<ObjRef> = foreign_pages.iter().copied().collect();
    let mut ids: HashSet<String> = HashSet::new();
    let annots_of = |cos: &CosDoc, page: ObjRef| -> Vec<ObjRef> {
        let p = cos.get(page);
        let Some(d) = p.as_dict() else { return Vec::new() };
        let arr = cos.resolve(d.get(b"Annots").unwrap_or(&Object::Null));
        arr.as_array()
            .map(|a| a.iter().filter_map(Object::as_ref).collect())
            .unwrap_or_default()
    };
    let nm = |cos: &CosDoc, a: ObjRef| -> Option<String> {
        let o = cos.get(a);
        o.as_dict()
            .and_then(|d| d.get(b"NM"))
            .and_then(Object::as_string)
            .map(PdfString::to_text)
    };
    for p in flat.iter().filter(|p| !foreign.contains(&p.id)) {
        for a in annots_of(cos, p.id) {
            if let Some(id) = nm(cos, a) {
                ids.insert(id);
            }
        }
    }
    for p in flat.iter().filter(|p| foreign.contains(&p.id)) {
        for a in annots_of(cos, p.id) {
            let Some(id) = nm(cos, a) else { continue };
            if ids.insert(id) {
                continue;
            }
            let fresh = loop {
                let id = markupcraft_revu::new_markup_id();
                if ids.insert(id.clone()) {
                    break id;
                }
            };
            let _ = cos.update_dict(a, |d| d.set(b"NM".to_vec(), Object::String(PdfString::text(&fresh))));
        }
    }
}

/// What a page operation did.
#[derive(Debug, Clone, PartialEq)]
pub struct PageReport {
    pub pages_before: usize,
    pub pages_after: usize,
    pub markups_before: usize,
    pub markups_after: usize,
}

impl Session {
    fn check_pages(&self, pages: &[usize]) -> Result<()> {
        let n = self.page_count();
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        match pages.iter().find(|p| **p >= n) {
            Some(p) => Err(EngineError::NoPage { page: p + 1, count: n }),
            None => Ok(()),
        }
    }

    /// Apply a page plan as one undoable step: flush the markups into the PDF, rebuild the page
    /// tree, reload the markups from it.
    pub fn apply_page_plan(&mut self, label: &str, plan: &PagePlan) -> Result<PageReport> {
        let (pages_before, markups_before) = (self.page_count(), self.doc.markups.len());
        self.edit(label, |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            apply_plan(&mut s.file.cos, plan)?;
            if plan.pages.len() < pages_before
                || plan.pages.iter().any(|p| !matches!(p.source, PageSource::Existing(_)))
            {
                // Removed pages must not survive in an earlier revision of the file.
                s.file.cos.require_full_save();
            }
            s.reload();
            Ok((
                PageReport {
                    pages_before,
                    pages_after: s.page_count(),
                    markups_before,
                    markups_after: s.doc.markups.len(),
                },
                true,
            ))
        })
    }

    /// Rotate pages by a multiple of 90 degrees (clockwise, as `/Rotate`).
    pub fn rotate_pages(&mut self, pages: &[usize], degrees: i64) -> Result<PageReport> {
        self.check_pages(pages)?;
        if degrees % 90 != 0 {
            return Err(invalid(format!("degrees must be a multiple of 90 (got {degrees})")));
        }
        let mut plan = PagePlan::identity(self.page_count());
        plan.rotate(pages, degrees);
        self.apply_page_plan("Rotate Pages", &plan)
    }

    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<PageReport> {
        self.check_pages(pages)?;
        let mut plan = PagePlan::identity(self.page_count());
        plan.delete(pages);
        if plan.pages.is_empty() {
            return Err(invalid("cannot delete every page: a PDF needs at least one page"));
        }
        self.apply_page_plan("Delete Pages", &plan)
    }

    /// Move `pages` so they sit together before position `before` (0..=page count).
    pub fn move_pages(&mut self, pages: &[usize], before: usize) -> Result<PageReport> {
        self.check_pages(pages)?;
        if before > self.page_count() {
            return Err(invalid(format!(
                "position {} is past the end (1 to {})",
                before + 1,
                self.page_count() + 1
            )));
        }
        let mut plan = PagePlan::identity(self.page_count());
        plan.move_pages(pages, before);
        self.apply_page_plan("Move Pages", &plan)
    }

    /// Insert `count` blank pages so the first becomes page `at`. Size defaults to the page
    /// before (or after) the insertion point.
    pub fn insert_blank_pages(&mut self, at: usize, count: usize, size: Option<(f64, f64)>) -> Result<PageReport> {
        if at > self.page_count() {
            return Err(invalid(format!(
                "position {} is past the end (1 to {})",
                at + 1,
                self.page_count() + 1
            )));
        }
        if count == 0 || count > blank::MAX_PAGES {
            return Err(invalid(format!("count must be 1 to {}", blank::MAX_PAGES)));
        }
        let (w, h) = match size {
            Some(s) => s,
            None => {
                let near = self.doc.pages.get(at.saturating_sub(1)).or(self.doc.pages.first());
                near.map_or((612.0, 792.0), |p| (p.media.width(), p.media.height()))
            }
        };
        blank::check_size(w, h)?;
        let mut plan = PagePlan::identity(self.page_count());
        plan.insert_blank(at, count, w, h);
        self.apply_page_plan("Insert Blank Pages", &plan)
    }

    /// Insert pages of another PDF (all when `pages` is `None`) so the first becomes page `at`.
    /// Their markups come along.
    pub fn insert_file_pages(&mut self, at: usize, path: &Path, pages: Option<&[usize]>) -> Result<PageReport> {
        if at > self.page_count() {
            return Err(invalid(format!(
                "position {} is past the end (1 to {})",
                at + 1,
                self.page_count() + 1
            )));
        }
        let file = ForeignPdf::open(path)?;
        let mut plan = PagePlan::identity(self.page_count());
        plan.insert_file(at, file, pages)?;
        self.apply_page_plan("Insert Pages", &plan)
    }

    /// Write `pages` (with their markups, including unsaved ones) to a new PDF at `out`; with
    /// `delete`, also remove them from this document (undoable). Returns the new file's page
    /// count.
    pub fn extract_pages(&mut self, pages: &[usize], out: &Path, delete: bool) -> Result<usize> {
        self.check_pages(pages)?;
        if same_file(out, self.path()) {
            return Err(invalid("extract to a different file than the open document"));
        }
        let mut cos = self.file.cos.clone();
        let mut doc = self.doc.clone();
        Session::flush(&mut cos, &mut doc, &self.vp_changed);
        let plan = PagePlan::identity(self.page_count()).extract(pages);
        apply_plan(&mut cos, &plan)?;
        let opts = SaveOptions {
            mod_date: Some(markupcraft_revu::pdf_date_now()),
            ..Default::default()
        };
        let bytes = write_full(&cos, &opts)?;
        crate::write_atomic(out, &bytes)?;
        if delete {
            let mut del = PagePlan::identity(self.page_count());
            del.delete(pages);
            if !del.pages.is_empty() {
                self.apply_page_plan("Extract Pages", &del)?;
            }
        }
        Ok(plan.pages.len())
    }

    /// Each page's label text ("" where the file has none).
    pub fn page_labels(&self) -> Vec<String> {
        self.doc.pages.iter().map(|p| p.label.clone()).collect()
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
