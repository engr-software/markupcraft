//! New PDFs of blank pages (a new document, and synthetic files for tests).

use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, SaveOptions, Stream, write_full};
use markupcraft_revu::pdf::{n, real};

use crate::{Result, invalid};

/// Most pages a new blank document may have.
pub const MAX_PAGES: usize = 10_000;
/// Largest page side in points (ISO 32000 limits user units to 14,400).
pub const MAX_SIDE: f64 = 14_400.0;

/// Check a page size from untrusted input.
pub fn check_size(w: f64, h: f64) -> Result<()> {
    if !(w.is_finite() && h.is_finite() && (1.0..=MAX_SIDE).contains(&w) && (1.0..=MAX_SIDE).contains(&h)) {
        return Err(invalid(format!(
            "page size {w} x {h} is out of range (1 to {MAX_SIDE} points per side)"
        )));
    }
    Ok(())
}

/// A blank page dictionary (no `/Parent` yet).
pub(crate) fn page_dict(cos: &mut CosDoc, w: f64, h: f64) -> Dict {
    let contents = cos.add(Object::Stream(Stream::from_raw(Dict::new(), Vec::new())));
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), n("Page"));
    d.set(
        b"MediaBox".to_vec(),
        Object::Array(vec![Object::Int(0), Object::Int(0), real(w), real(h)]),
    );
    d.set(b"Resources".to_vec(), Object::Dict(Dict::new()));
    d.set(b"Contents".to_vec(), Object::Ref(contents));
    d
}

/// The bytes of a PDF with one blank page per `(width, height)`.
pub fn pdf_bytes(sizes: &[(f64, f64)]) -> Result<Vec<u8>> {
    if sizes.is_empty() || sizes.len() > MAX_PAGES {
        return Err(invalid(format!("a new document needs 1 to {MAX_PAGES} pages")));
    }
    for (w, h) in sizes {
        check_size(*w, *h)?;
    }
    let mut cos = CosDoc::new_empty();
    let root = cos.root().ok_or_else(|| invalid("new document has no catalog"))?;
    let pages = cos
        .get(root)
        .as_dict()
        .and_then(|d| d.reference(b"Pages"))
        .ok_or_else(|| invalid("new document has no page tree"))?;
    let mut kids = Vec::new();
    for (w, h) in sizes {
        let mut d = page_dict(&mut cos, *w, *h);
        d.set(b"Parent".to_vec(), Object::Ref(pages));
        kids.push(Object::Ref(cos.add(Object::Dict(d))));
    }
    let count = kids.len() as i64;
    cos.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(kids));
        d.set(b"Count".to_vec(), Object::Int(count));
    })?;
    Ok(write_full(&cos, &SaveOptions::default())?)
}
