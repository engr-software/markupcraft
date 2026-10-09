//! Reduce File Size: PdfCraft's optimizer (`pdfcraft-optimize`) resamples and recompresses
//! images above a resolution, discards page thumbnails and alternate images, compresses
//! unencoded streams and removes links that point nowhere; the next save then rewrites the
//! file in full (dropping unused objects and earlier revisions). Markups are untouched.

use markupcraft_revu::cos::write_full;
use pdfcraft_optimize::{Compression, ImageSettings, Settings};

use crate::docutil::{err, save_options};
use crate::{Result, Session, invalid};

/// Reduce File Size choices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReduceSettings {
    /// Resample images drawn above `above_ppi` down to `target_ppi`.
    pub downsample: bool,
    pub target_ppi: f64,
    pub above_ppi: f64,
    /// JPEG quality 1 to 100; `None` keeps images lossless (Flate).
    pub jpeg_quality: Option<u8>,
    pub discard_thumbnails: bool,
}

impl Default for ReduceSettings {
    fn default() -> Self {
        let r = ImageSettings::REDUCE;
        Self {
            downsample: r.downsample,
            target_ppi: r.target_ppi,
            above_ppi: r.above_ppi,
            jpeg_quality: Some(60),
            discard_thumbnails: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReduceReport {
    /// A full save of the document before and after, in bytes.
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub images: usize,
    pub images_resampled: usize,
    pub images_recompressed: usize,
    pub streams_compressed: usize,
    pub thumbnails: usize,
    pub invalid_links: usize,
}

impl Session {
    /// Reduce the document's size; undoable until saved.
    pub fn reduce_file_size(&mut self, s: &ReduceSettings) -> Result<ReduceReport> {
        if s.downsample
            && !(s.target_ppi.is_finite()
                && s.above_ppi.is_finite()
                && (9.0..=2400.0).contains(&s.target_ppi)
                && s.above_ppi >= s.target_ppi)
        {
            return Err(invalid(
                "target ppi must be 9 to 2400 and the threshold at least the target",
            ));
        }
        if s.jpeg_quality.is_some_and(|q| !(1..=100).contains(&q)) {
            return Err(invalid("JPEG quality must be 1 to 100"));
        }
        let images = ImageSettings {
            downsample: s.downsample,
            target_ppi: s.target_ppi,
            above_ppi: s.above_ppi,
            compression: s.jpeg_quality.map_or(Compression::Flate, Compression::Jpeg),
        };
        let settings = Settings {
            color: images,
            gray: images,
            discard_thumbnails: s.discard_thumbnails,
            ..Settings::default()
        };
        self.graph_edit("Reduce File Size", |cos, _| {
            let bytes_before = write_full(cos, &save_options())?.len();
            let r = pdfcraft_optimize::optimize(cos, &settings).map_err(err)?;
            cos.require_full_save();
            let bytes_after = write_full(cos, &save_options())?.len();
            Ok(ReduceReport {
                bytes_before,
                bytes_after,
                images: r.images,
                images_resampled: r.images_resampled,
                images_recompressed: r.images_recompressed,
                streams_compressed: r.streams_compressed,
                thumbnails: r.thumbnails,
                invalid_links: r.invalid_links,
            })
        })
    }
}
