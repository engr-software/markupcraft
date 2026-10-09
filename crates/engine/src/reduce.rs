//! Reduce File Size: PdfCraft's optimizer (`pdfcraft-optimize`) resamples and recompresses
//! images above a resolution (colour and gray images each with their own settings), discards
//! page thumbnails and alternate images, compresses unencoded streams and removes links that
//! point nowhere; our own clean-ups drop metadata, private application data and structure
//! tags and crop pages to their crop box. The next save then rewrites the file in full
//! (dropping unused objects and earlier revisions). Markups are untouched.
//!
//! The work can run on another thread: [`Session::graph_copy`] hands out the object graph,
//! [`reduce_graph`] reduces it, and [`Session::apply_graph`] takes it back as one undo step.

use markupcraft_revu::cos::{Document as CosDoc, Object, write_full};
use pdfcraft_optimize::{Compression, ImageSettings, Settings};

use crate::docutil::{err, page_objs, save_options};
use crate::{Result, Session, invalid};

/// Reduce File Size choices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReduceSettings {
    /// Resample colour images drawn above `above_ppi` down to `target_ppi`.
    pub downsample: bool,
    pub target_ppi: f64,
    pub above_ppi: f64,
    /// JPEG quality 1 to 100; `None` keeps images lossless (Flate).
    pub jpeg_quality: Option<u8>,
    pub discard_thumbnails: bool,
    /// Gray images: their own target and threshold ppi and quality (`None` = as colour).
    pub gray: Option<(f64, f64, Option<u8>)>,
    pub discard_alternate_images: bool,
    pub discard_tags: bool,
    pub discard_print_settings: bool,
    pub compress_streams: bool,
    pub remove_invalid_links: bool,
    pub remove_unreferenced_dests: bool,
    /// Drop the document's XMP metadata and Info (title, author ...).
    pub discard_metadata: bool,
    /// Drop other applications' private data (`/PieceInfo`).
    pub discard_private: bool,
    /// Make each page's media box its crop box.
    pub crop_to_crop_box: bool,
}

impl Default for ReduceSettings {
    fn default() -> Self {
        let r = ImageSettings::REDUCE;
        let d = Settings::default();
        Self {
            downsample: r.downsample,
            target_ppi: r.target_ppi,
            above_ppi: r.above_ppi,
            jpeg_quality: Some(60),
            discard_thumbnails: true,
            gray: None,
            discard_alternate_images: d.discard_alternate_images,
            discard_tags: d.discard_tags,
            discard_print_settings: d.discard_print_settings,
            compress_streams: d.flate_unencoded,
            remove_invalid_links: d.remove_invalid_links,
            remove_unreferenced_dests: d.remove_unreferenced_dests,
            discard_metadata: false,
            discard_private: false,
            crop_to_crop_box: false,
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

fn check(s: &ReduceSettings) -> Result<()> {
    let ppi_ok = |t: f64, a: f64| t.is_finite() && a.is_finite() && (9.0..=2400.0).contains(&t) && a >= t;
    if s.downsample && !ppi_ok(s.target_ppi, s.above_ppi) {
        return Err(invalid(
            "target ppi must be 9 to 2400 and the threshold at least the target",
        ));
    }
    if let Some((t, a, _)) = s.gray
        && s.downsample
        && !ppi_ok(t, a)
    {
        return Err(invalid(
            "gray images: target ppi 9 to 2400, threshold at least the target",
        ));
    }
    let q_ok = |q: Option<u8>| q.is_none_or(|q| (1..=100).contains(&q));
    if !q_ok(s.jpeg_quality) || !s.gray.is_none_or(|g| q_ok(g.2)) {
        return Err(invalid("JPEG quality must be 1 to 100"));
    }
    Ok(())
}

/// Reduce an object graph (the heavy part; any thread). The graph is changed in place.
pub fn reduce_graph(cos: &mut CosDoc, s: &ReduceSettings) -> Result<ReduceReport> {
    check(s)?;
    let color = ImageSettings {
        downsample: s.downsample,
        target_ppi: s.target_ppi,
        above_ppi: s.above_ppi,
        compression: s.jpeg_quality.map_or(Compression::Flate, Compression::Jpeg),
    };
    let gray = match s.gray {
        Some((t, a, q)) => ImageSettings {
            downsample: s.downsample,
            target_ppi: t,
            above_ppi: a,
            compression: q.map_or(Compression::Flate, Compression::Jpeg),
        },
        None => color,
    };
    let settings = Settings {
        color,
        gray,
        discard_thumbnails: s.discard_thumbnails,
        discard_alternate_images: s.discard_alternate_images,
        discard_tags: s.discard_tags,
        discard_print_settings: s.discard_print_settings,
        flate_unencoded: s.compress_streams,
        remove_invalid_links: s.remove_invalid_links,
        remove_unreferenced_dests: s.remove_unreferenced_dests,
    };
    let bytes_before = write_full(cos, &save_options())?.len();
    let r = pdfcraft_optimize::optimize(cos, &settings).map_err(err)?;
    if s.discard_metadata || s.discard_private {
        if let Some(root) = cos.root() {
            cos.update_dict(root, |d| {
                if s.discard_metadata {
                    d.remove(b"Metadata");
                }
                if s.discard_private {
                    d.remove(b"PieceInfo");
                }
            })?;
        }
        if s.discard_metadata {
            cos.trailer_mut().remove(b"Info");
        }
    }
    if s.discard_private || s.crop_to_crop_box {
        for p in page_objs(cos)? {
            let crop = cos.dict(&Object::Ref(p)).and_then(|d| d.get(b"CropBox").cloned());
            cos.update_dict(p, |d| {
                if s.discard_private {
                    d.remove(b"PieceInfo");
                }
                if s.crop_to_crop_box
                    && let Some(c) = crop
                {
                    d.set(b"MediaBox".to_vec(), c);
                }
            })?;
        }
    }
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
}

impl Session {
    /// Reduce the document's size; undoable until saved.
    pub fn reduce_file_size(&mut self, s: &ReduceSettings) -> Result<ReduceReport> {
        check(s)?;
        let s = *s;
        self.graph_edit("Reduce File Size", |cos, _| reduce_graph(cos, &s))
    }

    /// The object graph with the unsaved markups and scales written in (for work on another
    /// thread; give it back with [`Self::apply_graph`]).
    pub fn graph_copy(&self) -> CosDoc {
        let mut cos = self.file.cos.clone();
        let mut doc = self.doc.clone();
        Session::flush(&mut cos, &mut doc, &self.vp_changed);
        cos
    }

    /// Replace the object graph with `cos` (made from [`Self::graph_copy`]) as one undoable
    /// step called `label`; the markups are reloaded from it.
    pub fn apply_graph(&mut self, label: &str, cos: CosDoc) -> Result<()> {
        self.edit(label, |s| {
            s.vp_changed.clear();
            s.file.cos = cos;
            s.reload();
            Ok(((), true))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn reduce_on_another_thread_and_the_clean_ups() {
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(1.0, 1.0, 5.0, 5.0))]),
            "r.pdf",
        )
        .unwrap();
        s.set_doc_properties(&[("Title".into(), Some("Secret plan".into()))])
            .unwrap();
        let mut cos = s.graph_copy();
        let settings = ReduceSettings {
            discard_metadata: true,
            discard_private: true,
            crop_to_crop_box: true,
            gray: Some((100.0, 150.0, Some(50))),
            ..Default::default()
        };
        let worker = std::thread::spawn(move || {
            let r = reduce_graph(&mut cos, &settings);
            (cos, r)
        });
        let (cos, r) = worker.join().unwrap();
        assert!(r.unwrap().bytes_after > 0);
        s.apply_graph("Reduce File Size", cos).unwrap();
        assert_eq!(s.undo_label(), Some("Reduce File Size"));
        assert!(
            s.doc_properties()
                .standard
                .iter()
                .all(|(k, v)| k != "Title" || v.is_empty()),
            "metadata gone"
        );
        let bad = ReduceSettings {
            gray: Some((500.0, 100.0, None)),
            ..Default::default()
        };
        assert!(s.reduce_file_size(&bad).is_err());
    }
}
