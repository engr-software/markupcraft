//! Markups List data stored beside the geometry: custom columns (`/PCColumns`), per-markup
//! column values (`/PCColumnData`), review status and checkmark (`/IRT` state replies),
//! replies, groups (`/RT /Group`) and page labels.
//!
//! Port target: the C++ `core/MarkupExtras.cpp` and `core/MarkupGroups.cpp`. The Markups List
//! wave fills these in; the hooks are already called by `read` and `write`.

use markupcraft_model::{Document, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef};

use crate::pdf;

/// Per-annotation extras, called once per annotation while loading.
pub fn read_markup(_cos: &CosDoc, a: &Dict, m: &mut Markup) {
    if let Some(r) = a.reference(b"IRT") {
        m.irt = Some((r.num, r.generation));
    }
    m.group = String::new();
    let _ = pdf::text(a.get(b"RT"));
}

/// Document-level extras, called after every page was loaded.
pub fn read_document(_cos: &CosDoc, _doc: &mut Document) {}

/// Per-annotation extras, called after the annotation's own keys were written.
pub fn write_markup(_cos: &mut CosDoc, _a: &mut Dict, _m: &mut Markup, _page: ObjRef) {}

/// Document-level extras, called before the markups are written.
pub fn write_document(_cos: &mut CosDoc, _doc: &mut Document) {}
