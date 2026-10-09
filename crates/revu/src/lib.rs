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
pub mod kinds;
pub mod pdf;
pub mod read;
pub mod scale;
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

/// Load from bytes already in memory (`path` is recorded, not read).
pub fn open_bytes(data: Arc<Vec<u8>>, path: &Path) -> Result<(PdfFile, Document), RevuError> {
    let cos = CosDoc::open(data)?;
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
    let opts = SaveOptions {
        mod_date: Some(pdf_date_now()),
        ..Default::default()
    };
    let bytes = match mode {
        SaveMode::Incremental if !file.cos.full_save_required() => pdfcraft_cos::write_incremental(&file.cos, &opts)?,
        _ => pdfcraft_cos::write_full(&file.cos, &opts)?,
    };
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
