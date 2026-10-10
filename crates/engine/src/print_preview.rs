//! The Print dialog's live preview: the print job laid out exactly as it prints (the
//! print-ready PDF), one sheet rendered as an image, with the sheet size and its margins.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::printing::PrintJob;
use crate::raster::Renderable;
use crate::{Result, Session, invalid};

/// One sheet of a print job, as it will print.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintPreview {
    /// Sheets the job prints.
    pub sheets: usize,
    /// The sheet shown (0-based).
    pub sheet: usize,
    /// The sheet's size in points.
    pub paper: (f64, f64),
    /// The margin on every side, points.
    pub margin: f64,
    /// The rendered sheet: premultiplied RGBA, `width * height * 4` bytes.
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl PrintPreview {
    /// The sheet as a PNG file.
    pub fn write_png(&self, out: &Path) -> Result<()> {
        let w = u32::try_from(self.width).map_err(|_| invalid("the preview is too large"))?;
        let h = u32::try_from(self.height).map_err(|_| invalid("the preview is too large"))?;
        // Premultiplied over white, as RGB.
        let rgb: Vec<u8> = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = 255u16.saturating_sub(u16::from(p[3]));
                [p[0], p[1], p[2]].map(|c| (u16::from(c) + a).min(255) as u8)
            })
            .collect();
        let img = image::RgbImage::from_raw(w, h, rgb).ok_or_else(|| invalid("the preview image is incomplete"))?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| invalid(e.to_string()))?;
        crate::write_atomic(out, bytes.get_ref())
    }
}

impl Session {
    /// Lay the job out and render sheet `sheet` (0-based) with its longer side at most
    /// `max_px` pixels (64 to 4000).
    pub fn print_preview(&self, job: &PrintJob, sheet: usize, max_px: u32) -> Result<PrintPreview> {
        static N: AtomicU32 = AtomicU32::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "markupcraft-print-preview-{}-{}.pdf",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let r = self.print_job_to_pdf(&tmp, job);
        let bytes = std::fs::read(&tmp);
        let _ = std::fs::remove_file(&tmp);
        let sheets = r?;
        let bytes = bytes.map_err(|e| invalid(format!("the preview could not be read back: {e}")))?;
        let doc = Renderable::new(Arc::new(bytes), false)?;
        let sheet = sheet.min(doc.page_count().saturating_sub(1));
        let g = doc.geom(sheet)?.clone();
        let side = f64::from(g.width.max(g.height)).max(1.0);
        let max_px = f64::from(max_px.clamp(64, 4_000));
        let img = doc.render_rgba(sheet, (max_px / side) as f32)?;
        Ok(PrintPreview {
            sheets,
            sheet,
            paper: (f64::from(g.width), f64::from(g.height)),
            margin: job.margin,
            width: img.w,
            height: img.h,
            rgba: img.rgba,
        })
    }
}
