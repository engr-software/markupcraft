//! Print-ready PDF: the pages laid out on sheets of paper the way a print dialog does, written
//! as a new PDF (PdfCraft's imposition, `pdfcraft-print`): one page per sheet (fit, actual
//! size, shrink oversized, a percentage), several pages per sheet (N-up), or each page tiled
//! across several sheets with overlap and cut marks. Markups print with the page unless the
//! content choice leaves them out. The open document is not changed.

use std::path::Path;

use pdfcraft_print::{Content, Layout, Orientation, PageOrder, Settings, SizeMode};

use crate::docutil::err;
use crate::{Result, Session, invalid};

/// How pages go on sheets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrintLayout {
    Fit,
    ActualSize,
    /// Shrink pages larger than the paper; others print at actual size.
    Shrink,
    /// A percentage of actual size.
    Percent(f64),
    /// `cols` x `rows` pages per sheet, left to right then down.
    NUp {
        cols: usize,
        rows: usize,
        border: bool,
    },
    /// Each page enlarged to `percent` and tiled over sheets that overlap by `overlap` points.
    Tile {
        percent: f64,
        overlap: f64,
        cut_marks: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintSettings {
    /// Pages in print order (0-based); empty = every page.
    pub pages: Vec<usize>,
    /// Paper (width, height) in points.
    pub paper: (f64, f64),
    /// `None` = automatic.
    pub landscape: Option<bool>,
    pub layout: PrintLayout,
    /// Print markups (otherwise the page content only).
    pub markups: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            paper: (612.0, 792.0),
            landscape: None,
            layout: PrintLayout::Fit,
            markups: true,
        }
    }
}

/// Paper sizes by name (points, portrait), architectural and ISO sizes included.
pub fn paper_size(name: &str) -> Option<(f64, f64)> {
    let n = name.to_ascii_lowercase().replace([' ', '-', '_'], "");
    Some(match n.as_str() {
        "letter" | "usletter" | "ansia" => (612.0, 792.0),
        "legal" | "uslegal" => (612.0, 1008.0),
        "tabloid" | "ledger" | "ansib" | "11x17" => (792.0, 1224.0),
        "ansic" | "17x22" => (1224.0, 1584.0),
        "ansid" | "22x34" => (1584.0, 2448.0),
        "ansie" | "34x44" => (2448.0, 3168.0),
        "archa" | "9x12" => (648.0, 864.0),
        "archb" | "12x18" => (864.0, 1296.0),
        "archc" | "18x24" => (1296.0, 1728.0),
        "archd" | "24x36" => (1728.0, 2592.0),
        "arche" | "36x48" => (2592.0, 3456.0),
        "a4" => (595.28, 841.89),
        "a3" => (841.89, 1190.55),
        "a2" => (1190.55, 1683.78),
        "a1" => (1683.78, 2383.94),
        "a0" => (2383.94, 3370.39),
        "a5" => (419.53, 595.28),
        _ => return None,
    })
}

impl Session {
    /// Write a print-ready PDF of the document to `out`. Returns the number of sheets.
    pub fn print_to_pdf(&self, out: &Path, s: &PrintSettings) -> Result<usize> {
        if crate::docutil::same_file(out, self.path()) {
            return Err(invalid(
                "write the print file to a different file than the open document",
            ));
        }
        let pages: Vec<usize> = if s.pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            for p in &s.pages {
                self.page(*p)?;
            }
            s.pages.clone()
        };
        let (w, h) = s.paper;
        if !(w.is_finite() && h.is_finite() && (72.0..=14_400.0).contains(&w) && (72.0..=14_400.0).contains(&h)) {
            return Err(invalid("the paper must be 1 to 200 inches on each side"));
        }
        let layout = match s.layout {
            PrintLayout::Fit => Layout::Size(SizeMode::Fit),
            PrintLayout::ActualSize => Layout::Size(SizeMode::Actual),
            PrintLayout::Shrink => Layout::Size(SizeMode::Shrink),
            PrintLayout::Percent(p) => {
                if !(p.is_finite() && (1.0..=1000.0).contains(&p)) {
                    return Err(invalid("the percentage must be 1 to 1000"));
                }
                Layout::Size(SizeMode::Custom(p))
            }
            PrintLayout::NUp { cols, rows, border } => {
                if !(1..=16).contains(&cols) || !(1..=16).contains(&rows) {
                    return Err(invalid("pages per sheet: 1 to 16 columns and rows"));
                }
                Layout::Multiple {
                    cols,
                    rows,
                    order: PageOrder::Horizontal,
                    border,
                    auto_rotate: true,
                }
            }
            PrintLayout::Tile {
                percent,
                overlap,
                cut_marks,
            } => {
                if !(percent.is_finite() && (10.0..=1000.0).contains(&percent)) {
                    return Err(invalid("the tile scale must be 10 to 1000 percent"));
                }
                if !(overlap.is_finite() && (0.0..=144.0).contains(&overlap)) {
                    return Err(invalid("the overlap must be 0 to 144 points"));
                }
                Layout::Poster {
                    scale: percent,
                    overlap,
                    cut_marks,
                }
            }
        };
        let settings = Settings {
            pages,
            paper: (w.min(h), w.max(h)),
            orientation: match s.landscape {
                None => Orientation::Auto,
                Some(true) => Orientation::Landscape,
                Some(false) => Orientation::Portrait,
            },
            layout,
            content: if s.markups {
                Content::DocumentAndMarkups
            } else {
                Content::Document
            },
        };
        let src = self.current_copy();
        let bytes = pdfcraft_print::impose(&src, &settings).map_err(err)?;
        let sheets = markupcraft_revu::cos::Document::open(std::sync::Arc::new(bytes.clone()))
            .ok()
            .and_then(|d| crate::docutil::page_objs(&d).ok())
            .map_or(0, |p| p.len());
        crate::write_atomic(out, &bytes)?;
        Ok(sheets)
    }
}
