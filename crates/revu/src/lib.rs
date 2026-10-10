//! Read and write markups as Revu-compatible PDF annotations.
//!
//! Built on PdfCraft's object layer (`pdfcraft-cos`): lazy tolerant parsing, copy-on-write
//! edits, incremental or full saves. Clean-room: everything here comes from ISO 32000 and from
//! PDFs we own, never from Revu's code.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod ap;
pub mod extras;
pub mod hatch;
pub mod kinds;
pub mod layers;
pub mod pdf;
pub mod pdfa1;
pub mod read;
pub mod scale;
pub mod spaces;
pub mod write;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use markupcraft_model::Document;
pub use pdfcraft_cos as cos;
use pdfcraft_cos::{CosError, Document as CosDoc, SaveOptions};

#[derive(Debug, thiserror::Error)]
pub enum RevuError {
    #[error("could not read {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("not a readable PDF: {0}")]
    Pdf(#[from] CosError),
}

/// An open PDF: the object graph plus the bytes it came from.
pub struct PdfFile {
    pub cos: CosDoc,
    pub path: PathBuf,
}

impl PdfFile {
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.cos.bytes().clone()
    }
}

/// Open `path` and load every page and markup.
pub fn open(path: impl AsRef<Path>) -> Result<(PdfFile, Document), RevuError> {
    let path = path.as_ref();
    let data = std::fs::read(path).map_err(|e| RevuError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    open_bytes(Arc::new(data), path)
}

/// Object numbers from here up are refused: ISO 32000-1 (Annex C) caps a file at 8,388,607
/// objects, and an object numbered near `u32::MAX` leaves the object layer no number for the
/// next new object (it would wrap to 0 and the next save would write a broken file).
pub const MAX_OBJECT_NUMBER: u32 = 1 << 30;

/// [`CosDoc::open`] for untrusted bytes: a panic in the object layer is an error, and a file
/// whose object numbers leave no room for new objects is refused instead of being saved broken.
pub fn open_cos(data: Arc<Vec<u8>>) -> Result<CosDoc, CosError> {
    let refused = |detail: &str| CosError::Syntax {
        offset: 0,
        detail: detail.to_string(),
    };
    let cos = std::panic::catch_unwind(|| CosDoc::open(data))
        .map_err(|_| refused("the object layer could not read the file"))??;
    // The next new object is numbered after the highest object and after the trailer's /Size.
    let size = cos.trailer().int(b"Size").unwrap_or(0);
    if size >= i64::from(MAX_OBJECT_NUMBER) || cos.object_numbers().last().is_some_and(|&n| n >= MAX_OBJECT_NUMBER) {
        return Err(refused("an object number is out of range (the file is damaged)"));
    }
    Ok(cos)
}

/// Load from bytes already in memory (`path` is recorded, not read).
pub fn open_bytes(data: Arc<Vec<u8>>, path: &Path) -> Result<(PdfFile, Document), RevuError> {
    let cos = open_cos(data)?;
    let doc = read::load(&cos, &path.display().to_string());
    Ok((
        PdfFile {
            cos,
            path: path.to_path_buf(),
        },
        doc,
    ))
}

/// How [`save`] writes the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveMode {
    /// Append the changes; the original bytes stay untouched (fast, like Revu's save).
    #[default]
    Incremental,
    /// Rewrite the whole file, dropping unused objects.
    Full,
}

/// Write `doc` to `out`: the source file with new/changed markups written back. `out` may be
/// the open file itself; the write goes to a temporary file first, then replaces it.
pub fn save(file: &mut PdfFile, doc: &mut Document, out: impl AsRef<Path>, mode: SaveMode) -> Result<(), RevuError> {
    let out = out.as_ref();
    write::apply(&mut file.cos, doc);
    // A PDF/A-1 file is PDF 1.4: a classic cross-reference table and a 1.4 header.
    let pdfa1 = pdfa1::declares_part1(&file.cos);
    let opts = SaveOptions {
        mod_date: Some(pdf_date_now()),
        object_streams: !pdfa1,
        ..Default::default()
    };
    let incremental = mode == SaveMode::Incremental && !file.cos.full_save_required() && !pdfa1;
    let mut bytes = if incremental {
        pdfcraft_cos::write_incremental(&file.cos, &opts)?
    } else {
        pdfcraft_cos::write_full(&file.cos, &opts)?
    };
    if pdfa1 {
        pdfa1::header_14(&mut bytes);
    }
    let mut tmp = out.as_os_str().to_owned();
    tmp.push(".markupcraft-tmp");
    let tmp = PathBuf::from(tmp);
    let io = |e| RevuError::Io {
        path: out.display().to_string(),
        source: e,
    };
    std::fs::write(&tmp, &bytes).map_err(io)?;
    std::fs::rename(&tmp, out).map_err(io)?;
    let bytes = Arc::new(bytes);
    file.cos = match file.cos.reopen_after_save(bytes) {
        Ok(c) => c,
        // A file saved with an open password cannot be reread without it: keep the object
        // graph in memory, and rewrite the whole file on later saves.
        Err(CosError::NeedsPassword) => {
            let mut c = file.cos.clone();
            c.require_full_save();
            c
        }
        Err(e) => return Err(e.into()),
    };
    file.path = out.to_path_buf();
    if !incremental {
        renumber_markups(&file.cos, doc);
    }
    for m in &mut doc.markups {
        if m.dirty {
            m.stored_look = false;
        }
        m.dirty = false;
    }
    doc.deleted.clear();
    doc.path = out.display().to_string();
    Ok(())
}

/// A full save numbers every object afresh: point each markup at its annotation in the new
/// file (by page and `/Annots` slot, which a full save keeps, checked against `/NM`), so later
/// edits write to the right object.
fn renumber_markups(cos: &CosDoc, doc: &mut Document) {
    let fresh = read::load(cos, "");
    let mut by_slot: std::collections::HashMap<(usize, usize), &markupcraft_model::Markup> =
        std::collections::HashMap::new();
    let mut by_id: std::collections::HashMap<(usize, &str), &markupcraft_model::Markup> =
        std::collections::HashMap::new();
    for f in &fresh.markups {
        if let Some(i) = f.annot_index {
            by_slot.insert((f.page, i), f);
        }
        if !f.id.is_empty() {
            by_id.insert((f.page, f.id.as_str()), f);
        }
    }
    for m in &mut doc.markups {
        if !m.in_file() {
            continue;
        }
        let found = by_id
            .get(&(m.page, m.id.as_str()))
            .or_else(|| m.annot_index.and_then(|i| by_slot.get(&(m.page, i))));
        match found {
            Some(f) => {
                m.obj = f.obj;
                m.annot_index = f.annot_index;
            }
            // not in the new file: written as a new annotation next time
            None => m.obj = (0, 0),
        }
    }
}

/// 16 uppercase letters, the shape Revu uses for `/NM`.
pub fn new_markup_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut x = nanos
        ^ COUNTER
            .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed)
            .rotate_left(17);
    (0..16)
        .map(|_| {
            // xorshift64*
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
            char::from(b'A' + (v >> 59) as u8 % 26)
        })
        .collect()
}

/// PDF date string for now (UTC), e.g. `D:20261009153000Z`.
pub fn pdf_date_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    pdfcraft_cos::pdf_date(secs)
}
