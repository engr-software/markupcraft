//! Markup exchange as XFDF (ISO 19444-1) and FDF, through PdfCraft's data exchange
//! (`pdfcraft-xfdf`): export every markup (unsaved ones included) with geometry, colours,
//! authors, dates, replies and pop-ups; import merges, a markup whose `/NM` already exists on
//! its page is replaced. Imported markups load into the session like any others.

use std::path::Path;

use crate::{EngineError, Result, Session, invalid};

/// Largest XFDF/FDF file imported.
const MAX_IMPORT: u64 = 256 << 20;

/// What an import did.
#[derive(Debug, Clone, PartialEq)]
pub struct XfdfReport {
    pub imported: usize,
    pub markups_before: usize,
    pub markups_after: usize,
}

impl Session {
    /// The markups as XFDF text (`fdf: true` writes FDF instead).
    pub fn export_markups_xfdf(&self, fdf: bool) -> Vec<u8> {
        let cos = self.current_copy();
        let file = self
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if fdf {
            pdfcraft_xfdf::export_fdf(&cos, true, false, &file)
        } else {
            pdfcraft_xfdf::export_xfdf(&cos, true, false, &file).into_bytes()
        }
    }

    /// Write the markups to `out` as XFDF (or FDF when it ends in `.fdf`). Returns the byte count.
    pub fn export_xfdf_file(&self, out: &Path) -> Result<usize> {
        let fdf = out
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("fdf"));
        let bytes = self.export_markups_xfdf(fdf);
        crate::write_atomic(out, &bytes)?;
        Ok(bytes.len())
    }

    /// Import markups from XFDF or FDF bytes (detected from the content). Undoable.
    pub fn import_xfdf(&mut self, bytes: &[u8]) -> Result<XfdfReport> {
        let before = self.doc.markups.len();
        let imported = self.graph_edit("Import Markups", |cos, _| {
            let r = pdfcraft_xfdf::import(cos, bytes).map_err(|e| invalid(format!("import: {e}")))?;
            if r.comments == 0 {
                return Err(invalid("the file holds no markups for this document"));
            }
            Ok(r.comments)
        })?;
        Ok(XfdfReport {
            imported,
            markups_before: before,
            markups_after: self.doc.markups.len(),
        })
    }

    /// [`Self::import_xfdf`] from a file.
    pub fn import_xfdf_file(&mut self, path: &Path) -> Result<XfdfReport> {
        let io = |e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        };
        let len = std::fs::metadata(path).map_err(io)?.len();
        if len > MAX_IMPORT {
            return Err(invalid(format!("{} is too large to import", path.display())));
        }
        let bytes = std::fs::read(path).map_err(io)?;
        self.import_xfdf(&bytes)
    }
}
