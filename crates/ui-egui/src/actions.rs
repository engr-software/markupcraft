//! The thin edit layer the UI calls. Every change to a document goes through a [`Session`]
//! method, so the engine crate (`markupcraft-engine`, with its own command table and undo) can
//! replace this module without touching panels or tools.
//!
//! Undo is whole-[`Document`] snapshots for now: simple and always correct, fine for the
//! thousands of markups a drawing set carries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Document, Kind, Markup};
use markupcraft_revu::{PdfFile, SaveMode};

/// How many undo steps are kept.
const UNDO_LIMIT: usize = 100;

/// One open document: the model, the PDF it came from, and undo history.
pub struct Session {
    pub doc: Document,
    pub file: Option<PdfFile>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    /// The key of the last coalescing checkpoint, while the gesture lasts.
    coalesce: Option<String>,
    /// Changed since open or the last save.
    pub modified: bool,
}

/// Whether MarkupCraft can write this markup back after an edit. Kinds without a writer entry
/// (and looks we cannot redraw) are shown from the file and stay read-only for now.
pub fn editable(m: &Markup) -> bool {
    !m.foreign_look && m.kind != Kind::Other && markupcraft_revu::kinds::kind_for(m.kind).kind == m.kind
}

/// Whether the canvas draws this markup from the model (and the renderer hides its `/AP`):
/// everything we can write, plus markups the file gives no appearance (the PDF would show
/// nothing for them).
pub fn drawn_by_us(m: &Markup) -> bool {
    editable(m) || (!m.stored_look && !m.foreign_look && m.kind != Kind::Other)
}

impl Session {
    pub fn new(mut doc: Document, file: Option<PdfFile>) -> Self {
        // Selection and the Markups List address markups by `/NM`; give the few without one an
        // id (it is written only if the markup is later edited and saved).
        for m in &mut doc.markups {
            if m.id.is_empty() {
                m.id = markupcraft_revu::new_markup_id();
            }
        }
        Self {
            doc,
            file,
            undo: Vec::new(),
            redo: Vec::new(),
            coalesce: None,
            modified: false,
        }
    }

    /// Open a PDF from bytes; `path` is where Save writes (None = Save As first).
    pub fn open_bytes(bytes: Arc<Vec<u8>>, path: Option<&Path>) -> Result<Self, String> {
        let p = path.map_or_else(|| PathBuf::from("untitled.pdf"), Path::to_path_buf);
        let (file, doc) = markupcraft_revu::open_bytes(bytes, &p).map_err(|e| e.to_string())?;
        Ok(Self::new(doc, Some(file)))
    }

    /// The bytes the document came from (after the last save).
    pub fn bytes(&self) -> Option<Arc<Vec<u8>>> {
        self.file.as_ref().map(PdfFile::bytes)
    }

    /// Objects of annotations the canvas draws itself (hidden in the renderer).
    pub fn drawn_objects(&self) -> Vec<(u32, u16)> {
        self.doc
            .markups
            .iter()
            .filter(|m| m.in_file() && drawn_by_us(m))
            .map(|m| m.obj)
            .collect()
    }

    // ---- undo ------------------------------------------------------------------------------

    /// Record the current state before a change.
    pub fn checkpoint(&mut self) {
        self.coalesce = None;
        self.push_undo();
    }

    /// Like [`Self::checkpoint`], but repeated calls with the same key (one slider drag, one
    /// typed word) make a single undo step until [`Self::seal`].
    pub fn checkpoint_coalesced(&mut self, key: &str) {
        if self.coalesce.as_deref() == Some(key) {
            self.modified = true;
            return;
        }
        self.push_undo();
        self.coalesce = Some(key.to_string());
    }

    /// End the current coalescing gesture.
    pub fn seal(&mut self) {
        self.coalesce = None;
    }

    fn push_undo(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.modified = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        self.coalesce = None;
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.doc, prev));
        self.modified = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        self.coalesce = None;
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.doc, next));
        self.modified = true;
        true
    }

    // ---- edits -----------------------------------------------------------------------------

    /// Add a new markup (one undo step). Returns its id.
    pub fn add(&mut self, mut m: Markup) -> String {
        self.checkpoint();
        if m.id.is_empty() {
            m.id = markupcraft_revu::new_markup_id();
        }
        let now = markupcraft_revu::pdf_date_now();
        if m.created.is_empty() {
            m.created = now.clone();
        }
        m.modified = now;
        m.dirty = true;
        let id = m.id.clone();
        self.doc.markups.push(m);
        id
    }

    /// Delete markups (one undo step); locked and read-only ones stay. Returns how many went.
    pub fn delete(&mut self, ids: &[String]) -> usize {
        let doomed = |m: &Markup| ids.contains(&m.id) && !m.locked() && (editable(m) || !m.in_file());
        if !self.doc.markups.iter().any(doomed) {
            return 0;
        }
        self.checkpoint();
        let before = self.doc.markups.len();
        let mut gone = Vec::new();
        self.doc.markups.retain(|m| {
            if doomed(m) {
                if m.in_file() {
                    gone.push(m.obj);
                }
                false
            } else {
                true
            }
        });
        self.doc.deleted.extend(gone);
        before - self.doc.markups.len()
    }

    /// Change markups through `f` (coalesced under `key`). Read-only markups are skipped.
    pub fn edit(&mut self, ids: &[String], key: &str, mut f: impl FnMut(&mut Markup)) {
        if !self.doc.markups.iter().any(|m| ids.contains(&m.id) && editable(m)) {
            return;
        }
        self.checkpoint_coalesced(key);
        for m in self
            .doc
            .markups
            .iter_mut()
            .filter(|m| ids.contains(&m.id) && editable(m))
        {
            f(m);
            touch(m);
        }
    }

    /// Move markups by `d` (PDF units). No checkpoint: call [`Self::checkpoint`] when the drag
    /// starts. Locked and read-only markups stay put.
    pub fn translate(&mut self, ids: &[String], d: Point) {
        if d.x == 0.0 && d.y == 0.0 {
            return;
        }
        for m in self.doc.markups.iter_mut().filter(|m| ids.contains(&m.id)) {
            if m.locked() || !editable(m) {
                continue;
            }
            translate_markup(m, d);
            touch(m);
        }
    }

    /// Replace one markup's geometry (a handle drag). No checkpoint.
    pub fn set_geometry(&mut self, id: &str, pts: Vec<Point>, rect: Rect) {
        if let Some(m) = self.doc.find_mut(id)
            && !m.locked()
            && editable(m)
        {
            m.pts = pts;
            m.rect = rect;
            touch(m);
        }
    }

    /// Write to `out` (the open file when `None`). Returns the path written.
    pub fn save(&mut self, out: Option<&Path>) -> Result<PathBuf, String> {
        let Some(file) = self.file.as_mut() else {
            return Err("this document has no PDF to save into".into());
        };
        let out = out.map_or_else(|| file.path.clone(), Path::to_path_buf);
        markupcraft_revu::save(file, &mut self.doc, &out, SaveMode::Incremental).map_err(|e| e.to_string())?;
        self.modified = false;
        self.coalesce = None;
        Ok(out)
    }
}

fn touch(m: &mut Markup) {
    m.dirty = true;
    m.stored_look = false;
}

fn translate_markup(m: &mut Markup, d: Point) {
    for p in &mut m.pts {
        *p = *p + d;
    }
    for h in &mut m.holes {
        for p in h {
            *p = *p + d;
        }
    }
    m.rect = Rect::new(m.rect.x0 + d.x, m.rect.y0 + d.y, m.rect.x1 + d.x, m.rect.y1 + d.y);
    if let Some(r) = &mut m.popup {
        *r = Rect::new(r.x0 + d.x, r.y0 + d.y, r.x1 + d.x, r.y1 + d.y);
    }
}

/// The user-space bounding box of a markup (its points, else its `/Rect`).
pub fn markup_bbox(m: &Markup) -> Rect {
    match bbox(&m.pts) {
        Some(b) if !uses_rect(m.kind) => b,
        _ => m.rect.normalized(),
    }
}

/// Kinds whose geometry is their rectangle rather than a vertex list.
pub fn uses_rect(k: Kind) -> bool {
    matches!(
        k,
        Kind::Rectangle
            | Kind::Ellipse
            | Kind::Text
            | Kind::Typewriter
            | Kind::Stamp
            | Kind::Snapshot
            | Kind::Note
            | Kind::Attachment
            | Kind::Hyperlink
            | Kind::Caret
            | Kind::Other
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut doc = Document::default();
        doc.pages.push(Default::default());
        let r = Rect::new(10.0, 10.0, 50.0, 30.0);
        let mut m = Markup::new(Kind::Rectangle, 0, r.corners().to_vec());
        m.rect = r;
        m.id = "A".into();
        doc.markups.push(m);
        let mut t = Markup::new(Kind::Text, 0, Vec::new());
        t.id = "T".into();
        t.obj = (12, 0);
        t.dirty = false;
        doc.markups.push(t);
        Session::new(doc, None)
    }

    #[test]
    fn move_undo_redo() {
        let mut s = session();
        s.checkpoint();
        s.translate(&["A".into()], Point::new(5.0, -2.0));
        assert_eq!(s.doc.markups[0].rect, Rect::new(15.0, 8.0, 55.0, 28.0));
        assert!(s.doc.markups[0].dirty);
        assert!(s.undo());
        assert_eq!(s.doc.markups[0].rect, Rect::new(10.0, 10.0, 50.0, 30.0));
        assert!(s.redo());
        assert_eq!(s.doc.markups[0].pts[0], Point::new(15.0, 8.0));
        assert!(!s.redo());
    }

    #[test]
    fn coalesced_edits_are_one_step() {
        let mut s = session();
        for w in 1..5 {
            s.edit(&["A".into()], "width", |m| m.line_width = w as f64);
        }
        s.seal();
        s.edit(&["A".into()], "width", |m| m.line_width = 9.0);
        assert!(s.undo());
        assert_eq!(s.doc.markups[0].line_width, 4.0);
        assert!(s.undo());
        assert_eq!(s.doc.markups[0].line_width, 1.0);
        assert!(!s.can_undo());
    }

    #[test]
    fn read_only_kinds_are_left_alone() {
        let mut s = session();
        // An annotation kind MarkupCraft has no writer for: not moved, not deleted.
        s.doc.markups[1].kind = Kind::Other;
        assert!(!editable(&s.doc.markups[1]));
        s.translate(&["T".into()], Point::new(1.0, 1.0));
        assert!(!s.doc.markups[1].dirty);
        assert_eq!(s.delete(&["T".into()]), 0);
        assert_eq!(s.delete(&["A".into(), "T".into()]), 1);
        assert_eq!(s.doc.markups.len(), 1);
        assert!(s.undo());
        assert_eq!(s.doc.markups.len(), 2);
    }

    #[test]
    fn deleting_a_file_markup_records_its_object() {
        let mut s = session();
        s.doc.markups[0].obj = (40, 0);
        s.delete(&["A".into()]);
        assert_eq!(s.doc.deleted, vec![(40, 0)]);
    }
}
