//! markupcraft-engine: one open document as an editing [`Session`].
//!
//! A session holds the PDF object graph ([`PdfFile`], from `markupcraft-revu`) and the markup
//! model ([`Document`]). Every change goes through [`Session::edit`]: the state before it is
//! kept as a snapshot (cheap: the object layer is copy-on-write), so a failed command leaves the
//! document as it was and every successful one can be undone. Snapshots are capped by count and
//! by an estimate of their memory.
//!
//! - Markup edits (`edit.rs`): add, delete, duplicate, properties, move, rotate, resize,
//!   vertices, z-order, clipboard, groups.
//! - Scales (`scales.rs`): page scale, calibration, viewports, measuring a set of points.
//! - Page operations ([`pages`]): rotate, delete, move, insert blank, insert from another PDF,
//!   extract. Markups stay with their pages: unsaved markups are written into the object graph
//!   first, the page tree is rearranged, and the model is reloaded from it.
//! - The command table ([`commands`]): selection-based commands with stable ids, one line each.
//!
//! Pages are 0-based here; tools and the UI number them from 1.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod align;
pub mod archive;
pub mod attachments;
pub mod batch;
pub mod batch_compare;
pub mod blank;
pub mod bookmarks;
pub mod bookmarks_more;
pub mod boxes;
pub mod bulk;
pub mod capture;
pub mod combine;
pub mod commands;
pub mod compare;
pub mod compare_diff;
pub mod convert;
pub mod docfile;
pub mod docprops;
pub mod docs_more;
mod docutil;
mod edit;
pub mod export;
pub mod favorites;
pub mod fill;
pub mod finish;
pub mod flatten;
pub mod forms;
pub mod geometry;
pub mod hatch;
pub mod hf_tokens;
pub mod jobs;
pub mod labels;
pub mod layers;
pub mod layers_more;
pub mod legend;
pub mod links;
pub mod marks;
pub mod markup_layer;
pub mod markup_ops;
mod numbering;
pub mod ocr;
pub mod overlay;
pub mod pages;
pub mod pdfa1;
pub mod places;
pub mod prefs;
pub mod print_preview;
pub mod print_seam;
pub mod printing;
pub mod printout;
pub mod props;
pub mod quantity;
pub mod raster;
pub mod redact;
pub mod reduce;
mod scales;
pub mod search;
pub mod search_links;
pub mod search_more;
pub mod security;
mod session_ui;
pub mod sets_more;
pub mod signatures;
pub mod slip;
pub mod spaces;
pub mod spell;
pub mod stamp_apply;
pub mod stamps;
pub mod summary;
pub mod summary_cols;
pub mod summary_links;
pub mod summary_print;
pub mod synthetic;
pub mod unflatten;
pub mod vectors;
pub mod viewports;
pub mod visual;
pub mod xfdf;
// Packages, stitching, insert options, source bookmarks, devices, web capture, scripting,
// shell integration, batch signing, Smart Overlay, interactive stamps.
pub mod batch_sign;
pub mod devices;
pub mod extras6;
pub mod insert_more;
pub mod package;
pub mod prefs_pages;
pub mod profiles;
pub mod scripting;
pub mod shell_integration;
pub mod smart_overlay;
pub mod source_bookmarks;
pub mod stamp_fields;
pub mod stitch;
pub mod webtab;

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use edit::Arrange;
pub use markupcraft_model::{Color, Document, Kind, Markup, PageInfo, Point, Rect, Scale, Viewport};
use markupcraft_revu::cos::{CosError, Document as CosDoc};
use markupcraft_revu::{PdfFile, RevuError, SaveMode};
pub use props::MarkupPatch;
pub use scales::{Measurement, calibrated_scale, decimal_scale};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Revu(#[from] RevuError),
    #[error("PDF: {0}")]
    Pdf(#[from] CosError),
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("no markup with id {0:?} (markup_list shows the ids)")]
    NoMarkup(String),
    #[error("page {page} does not exist (the document has {count} pages)")]
    NoPage { page: usize, count: usize },
    #[error("markup {0} is locked; unlock it first (set locked to false)")]
    Locked(String),
    #[error("{0}")]
    Invalid(String),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
}

pub type Result<T> = std::result::Result<T, EngineError>;

pub(crate) fn invalid(msg: impl Into<String>) -> EngineError {
    EngineError::Invalid(msg.into())
}

/// How much undo history a session keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoLimits {
    /// Most undo steps kept.
    pub max_steps: usize,
    /// Approximate memory the undo history may use; the oldest steps go first.
    pub max_bytes: usize,
}

impl Default for UndoLimits {
    fn default() -> Self {
        Self {
            max_steps: 200,
            max_bytes: 512 << 20,
        }
    }
}

/// The state an undo step restores.
struct Snapshot {
    label: String,
    doc: Document,
    cos: CosDoc,
    vp_changed: BTreeSet<usize>,
    version: u64,
    bytes: usize,
}

/// One open document.
pub struct Session {
    file: PdfFile,
    doc: Document,
    /// Pages whose `/VP` (viewports, page scale) the next save writes.
    vp_changed: BTreeSet<usize>,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    undo_bytes: usize,
    selection: Vec<String>,
    clipboard: Vec<Markup>,
    /// Identifies the current state; `saved_version` is the state on disk.
    version: u64,
    saved_version: u64,
    counter: u64,
    limits: UndoLimits,
    author: String,
    /// Edits made under this key join the previous step made under it (`session_ui.rs`).
    merge: Option<String>,
    /// The merge key of the last undo step, until [`Session::seal`].
    last_merge: Option<String>,
    /// Page text read for search, kept while the bytes it came from are current.
    text_cache: std::sync::Mutex<search::TextCache>,
    /// Pictures placed as markups are stored as JPEG at this quality (else losslessly).
    image_jpeg: Option<u8>,
}

impl Session {
    /// How pictures placed as markups (Image, image stamps) are stored: JPEG at quality (1 to
    /// 100, lossy and small) or, with `None`, losslessly (Preferences > Tools > Markup).
    pub fn set_image_encoding(&mut self, quality: Option<u8>) {
        self.image_jpeg = quality.map(|q| q.clamp(1, 100));
    }

    /// The JPEG quality pictures are stored at, if not losslessly.
    pub fn image_jpeg_quality(&self) -> Option<u8> {
        self.image_jpeg
    }

    /// Open a PDF file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let (file, doc) = markupcraft_revu::open(path)?;
        Ok(Self::from_parts(file, doc))
    }

    /// Open a PDF already in memory; `path` is where it saves.
    pub fn from_bytes(data: Vec<u8>, path: impl AsRef<Path>) -> Result<Self> {
        let (file, doc) = markupcraft_revu::open_bytes(Arc::new(data), path.as_ref())?;
        Ok(Self::from_parts(file, doc))
    }

    /// A new document of blank pages (`(width, height)` in points), not yet on disk.
    pub fn new_blank(path: impl AsRef<Path>, sizes: &[(f64, f64)]) -> Result<Self> {
        let bytes = blank::pdf_bytes(sizes)?;
        let mut s = Self::from_bytes(bytes, path)?;
        s.saved_version = u64::MAX; // never saved
        Ok(s)
    }

    fn from_parts(file: PdfFile, mut doc: Document) -> Self {
        finish_load(&file.cos, &mut doc);
        Self {
            file,
            doc,
            vp_changed: BTreeSet::new(),
            undo: VecDeque::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            selection: Vec::new(),
            clipboard: Vec::new(),
            version: 0,
            saved_version: 0,
            counter: 0,
            limits: UndoLimits::default(),
            author: String::new(),
            merge: None,
            last_merge: None,
            text_cache: search::TextCache::new(),
            image_jpeg: None,
        }
    }

    pub fn with_undo_limits(mut self, limits: UndoLimits) -> Self {
        self.limits = limits;
        self
    }

    /// The author written on new markups (`/T`) when a markup does not name one.
    pub fn set_author(&mut self, author: &str) {
        self.author = author.to_string();
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    // ---- reading ---------------------------------------------------------------------------

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    pub fn pdf(&self) -> &PdfFile {
        &self.file
    }

    pub fn path(&self) -> &Path {
        &self.file.path
    }

    pub fn page_count(&self) -> usize {
        self.doc.pages.len()
    }

    pub fn page(&self, page: usize) -> Result<&PageInfo> {
        self.doc.pages.get(page).ok_or(EngineError::NoPage {
            page: page.saturating_add(1),
            count: self.doc.pages.len(),
        })
    }

    pub fn markup(&self, id: &str) -> Result<&Markup> {
        self.doc.find(id).ok_or_else(|| EngineError::NoMarkup(id.to_string()))
    }

    pub(crate) fn index_of(&self, id: &str) -> Result<usize> {
        self.doc
            .markups
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| EngineError::NoMarkup(id.to_string()))
    }

    /// A number that changes with every edit (for caches of derived facts).
    pub fn state_version(&self) -> u64 {
        self.version
    }

    /// Unsaved changes (a new blank document always counts as unsaved).
    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What undo would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    // ---- selection and clipboard -------------------------------------------------------------

    /// Selected markup ids; the last one is the primary selection.
    pub fn selection(&self) -> &[String] {
        &self.selection
    }

    /// Select these markups (replacing the selection). Unknown ids are an error.
    pub fn select(&mut self, ids: &[String]) -> Result<()> {
        for id in ids {
            self.index_of(id)?;
        }
        let mut seen = HashSet::new();
        self.selection = ids.iter().filter(|id| seen.insert(id.as_str())).cloned().collect();
        Ok(())
    }

    pub fn select_all(&mut self, page: Option<usize>) {
        self.selection = self
            .doc
            .markups
            .iter()
            .filter(|m| page.is_none_or(|p| m.page == p) && !m.id.is_empty())
            .map(|m| m.id.clone())
            .collect();
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    fn prune_selection(&mut self) {
        let ids: HashSet<&str> = self.doc.markups.iter().map(|m| m.id.as_str()).collect();
        self.selection.retain(|id| ids.contains(id.as_str()));
    }

    /// The markups copied last (shared by the caller between sessions with [`Self::set_clipboard`]).
    pub fn clipboard(&self) -> &[Markup] {
        &self.clipboard
    }

    pub fn set_clipboard(&mut self, items: Vec<Markup>) {
        self.clipboard = items;
    }

    // ---- editing -----------------------------------------------------------------------------

    /// Run `f` as one undoable step called `label`. `f` returns its result and whether it
    /// changed anything. On error the document is restored to the state before `f`.
    pub fn edit<T>(&mut self, label: &str, f: impl FnOnce(&mut Self) -> Result<(T, bool)>) -> Result<T> {
        let snap = self.snapshot(label);
        match f(self) {
            Ok((v, changed)) => {
                if changed {
                    let join = self.merge.is_some() && self.merge == self.last_merge && !self.undo.is_empty();
                    if !join {
                        self.push_undo(snap);
                    }
                    self.last_merge = self.merge.clone();
                    self.redo.clear();
                    self.bump();
                    self.prune_selection();
                }
                Ok(v)
            }
            Err(e) => {
                self.restore(snap);
                Err(e)
            }
        }
    }

    fn bump(&mut self) {
        self.counter = self.counter.wrapping_add(1);
        self.version = self.counter;
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let bytes = estimate_bytes(&self.doc, &self.file.cos);
        Snapshot {
            label: label.to_string(),
            doc: self.doc.clone(),
            cos: self.file.cos.clone(),
            vp_changed: self.vp_changed.clone(),
            version: self.version,
            bytes,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        let path = self.doc.path.clone();
        self.doc = s.doc;
        self.doc.path = path;
        self.file.cos = s.cos;
        self.vp_changed = s.vp_changed;
        self.version = s.version;
    }

    fn push_undo(&mut self, s: Snapshot) {
        self.undo_bytes = self.undo_bytes.saturating_add(s.bytes);
        self.undo.push_back(s);
        while self.undo.len() > self.limits.max_steps.max(1)
            || (self.undo_bytes > self.limits.max_bytes && self.undo.len() > 1)
        {
            match self.undo.pop_front() {
                Some(old) => self.undo_bytes = self.undo_bytes.saturating_sub(old.bytes),
                None => break,
            }
        }
    }

    /// Undo the last step; returns its label.
    pub fn undo(&mut self) -> Result<String> {
        let s = self.undo.pop_back().ok_or(EngineError::NothingToUndo)?;
        self.last_merge = None;
        self.undo_bytes = self.undo_bytes.saturating_sub(s.bytes);
        let label = s.label.clone();
        let current = self.snapshot(&label);
        self.restore(s);
        self.redo.push(current);
        self.prune_selection();
        Ok(label)
    }

    /// Redo the last undone step; returns its label.
    pub fn redo(&mut self) -> Result<String> {
        let s = self.redo.pop().ok_or(EngineError::NothingToRedo)?;
        self.last_merge = None;
        let label = s.label.clone();
        let current = self.snapshot(&label);
        self.restore(s);
        self.push_undo(current);
        self.prune_selection();
        Ok(label)
    }

    // ---- saving ------------------------------------------------------------------------------

    /// Save to the document's own file. `full` rewrites the whole file instead of appending
    /// an incremental update.
    pub fn save(&mut self, full: bool) -> Result<()> {
        let path = self.file.path.clone();
        self.save_as(path, full)
    }

    /// Save to `path` (atomic: a temporary file, then a rename). The session then belongs to
    /// `path`.
    pub fn save_as(&mut self, path: impl AsRef<Path>, full: bool) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let before = self.snapshot("save");
        let mode = if full { SaveMode::Full } else { SaveMode::Incremental };
        scales::write_viewports(&mut self.file.cos, &mut self.doc, &self.vp_changed);
        if let Err(e) = markupcraft_revu::save(&mut self.file, &mut self.doc, &path, mode) {
            self.restore(before);
            return Err(e.into());
        }
        self.vp_changed.clear();
        self.saved_version = self.version;
        Ok(())
    }

    /// Write unsaved markups, scales and z-order into the object graph (before a page
    /// operation, or to export a copy).
    pub(crate) fn flush(cos: &mut CosDoc, doc: &mut Document, vp_changed: &BTreeSet<usize>) {
        scales::write_viewports(cos, doc, vp_changed);
        markupcraft_revu::write::apply(cos, doc);
    }

    /// Replace the model by reloading it from the object graph (after a page operation).
    pub(crate) fn reload(&mut self) {
        let path = self.doc.path.clone();
        let mut doc = markupcraft_revu::read::load(&self.file.cos, &path);
        finish_load(&self.file.cos, &mut doc);
        self.doc = doc;
        self.vp_changed.clear();
    }

    pub(crate) fn new_id(&self) -> String {
        let used: HashSet<&str> = self.doc.markups.iter().map(|m| m.id.as_str()).collect();
        loop {
            let id = markupcraft_revu::new_markup_id();
            if !used.contains(id.as_str()) {
                return id;
            }
        }
    }
}

/// After loading: page labels, and a session id for markups the file left without `/NM`.
fn finish_load(cos: &CosDoc, doc: &mut Document) {
    if let Some(l) = labels::read(cos, doc.pages.len()) {
        for (p, spec) in doc.pages.iter_mut().zip(l) {
            p.label = spec.map(|s| s.text()).unwrap_or_default();
        }
    }
    let mut used: HashSet<String> = doc.markups.iter().map(|m| m.id.clone()).collect();
    for m in &mut doc.markups {
        if m.id.is_empty() {
            let id = loop {
                let id = markupcraft_revu::new_markup_id();
                if used.insert(id.clone()) {
                    break id;
                }
            };
            m.id = id;
        }
    }
}

/// A rough size of a snapshot, for the undo memory cap.
fn estimate_bytes(doc: &Document, cos: &CosDoc) -> usize {
    let markups: usize = doc
        .markups
        .iter()
        .map(|m| {
            let pts = m.pts.len() + m.holes.iter().map(Vec::len).sum::<usize>();
            512 + pts * 16 + m.contents.len() + m.subject.len() + m.label.len() + m.extra.len() * 64
        })
        .sum();
    1024 + markups + doc.pages.len() * 256 + cos.modified_objects().len() * 64
}

/// Write `bytes` to `path` through a temporary file and a rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let io = |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    };
    if markupcraft_revu::fsio::hosted() {
        return markupcraft_revu::fsio::write_atomic(path, bytes).map_err(io);
    }
    if path.is_dir() {
        return Err(invalid(format!("{} is a folder, not a file", path.display())));
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".markupcraft-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(e)
    })
}

#[cfg(test)]
mod tests;
