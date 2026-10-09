//! Spaces in the file: MarkupCraft's own page key (Revu's storage is not public).
//!
//! ```text
//! Page /PCSpaces [ << /NM (id) /Name (Office 101) /Vertices [x y x y ...] /C [r g b] /CA 0.25 >> ... ]
//! ```

use markupcraft_model::Document;
use markupcraft_model::spaces::Space;
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object};

use crate::pdf::{self, color_arr, n, points_arr, real, s};

/// Most spaces read from one page.
pub const MAX_SPACES: usize = 10_000;
/// Most vertices of one space.
pub const MAX_VERTICES: usize = 100_000;

fn read_one(cos: &CosDoc, o: &Object) -> Option<Space> {
    let d = cos.dict(o)?;
    let mut pts = pdf::points(d.get(b"Vertices").map(|v| cos.resolve(v)).as_deref());
    pts.truncate(MAX_VERTICES);
    if pts.len() < 3 || pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let mut sp = Space {
        id: pdf::text(d.get(b"NM")),
        name: pdf::text(d.get(b"Name")),
        pts,
        ..Default::default()
    };
    if let Some(c) = pdf::color(d.get(b"C")) {
        sp.color = c;
    }
    sp.opacity = pdf::num_or(d.get(b"CA"), sp.opacity).clamp(0.0, 1.0);
    Some(sp)
}

/// The spaces stored on one page.
pub fn read_page(cos: &CosDoc, page: &Dict) -> Vec<Space> {
    let arr = page.get(b"PCSpaces").map(|a| cos.resolve(a));
    let Some(arr) = arr.as_ref().and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    arr.iter().take(MAX_SPACES).filter_map(|o| read_one(cos, o)).collect()
}

/// Fill every page's spaces (called while loading).
pub fn read(cos: &CosDoc, doc: &mut Document) {
    for (info, page) in doc.pages.iter_mut().zip(pdf::pages(cos)) {
        info.spaces = read_page(cos, &page.dict);
    }
}

/// One space as stored.
pub fn object(sp: &Space) -> Object {
    Object::Dict(pdf::dict(&[
        ("Type", n("PCSpace")),
        ("NM", s(&sp.id)),
        ("Name", s(&sp.name)),
        ("Vertices", points_arr(&sp.pts)),
        ("C", color_arr(&sp.color)),
        ("CA", real(sp.opacity)),
    ]))
}

/// Store `spaces` on page `page` (none: the key is removed).
pub fn write_page(cos: &mut CosDoc, page: ObjRef, spaces: &[Space]) {
    let arr: Vec<Object> = spaces.iter().map(object).collect();
    let _ = cos.update_dict(page, |d| {
        if arr.is_empty() {
            d.remove(b"PCSpaces");
        } else {
            d.set(b"PCSpaces".to_vec(), Object::Array(arr));
        }
    });
}
