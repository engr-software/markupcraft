//! Completions of features that were partly built: viewport calibration, count status
//! reports, raster Dynamic Fill, Quantity Link into existing workbooks, GIF and multi-page TIFF,
//! Office and DXF conversion, page setup and insert/extract options, print files, form field
//! properties and actions, flatten extras, ID management, redaction choices and more. Each
//! file is one area; they reach the rest of the engine through `Session` methods.

pub mod bookmark_sort;
pub mod create;
pub mod extract_links;
pub mod flatten_more;
pub mod forms_more;
pub mod idstore;
pub mod imaging;
pub mod office;
pub mod pages;
pub mod preview;
pub mod print_more;
pub mod rasterfill;
pub mod redact_kinds;
pub mod snapshot_cut;
pub mod status;
pub mod summary_more;
pub mod xlsx_edit;
pub mod zip;

/// Write `bytes` to `path` through a temporary file and a rename (for tools and the UI).
pub fn write_file(path: &std::path::Path, bytes: &[u8]) -> crate::Result<()> {
    crate::write_atomic(path, bytes)
}
