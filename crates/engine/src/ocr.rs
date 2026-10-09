//! OCR: recognise the words on scanned pages and add them as an invisible, searchable text
//! layer over the page image (the image is left untouched).
//!
//! Recognition is PdfCraft's `pdfcraft-ocr` (the ocrs engine). Its two model files are not part
//! of the repository: `cargo xtask models` fetches them into `assets/models/`, or point
//! `MARKUPCRAFT_OCR_MODELS` (or `PDFCRAFT_MODELS`) at a folder holding them. The recogniser is
//! a trait so tests and other engines can supply their own.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pdfcraft_ocr::{Line, Models, Ocr, PlacedWord};

use crate::raster::Renderable;
use crate::{Result, Session, invalid};

/// Longest side of a page image given to the recogniser.
const MAX_SIDE: f32 = 7_000.0;

/// Something that reads words in an image.
pub trait Recognizer: Send + Sync {
    /// The lines of words in an RGB image (`width * height * 3` bytes), boxes in pixels.
    fn recognize(&self, rgb: &[u8], width: u32, height: u32) -> std::result::Result<Vec<Line>, String>;
}

impl Recognizer for Ocr {
    fn recognize(&self, rgb: &[u8], width: u32, height: u32) -> std::result::Result<Vec<Line>, String> {
        Ocr::recognize(self, rgb, width, height).map_err(|e| e.to_string())
    }
}

/// Where the model files are looked for: `$MARKUPCRAFT_OCR_MODELS`, `$PDFCRAFT_MODELS`,
/// `models/` beside the executable, then the source tree's `assets/models/`.
pub fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["MARKUPCRAFT_OCR_MODELS", "PDFCRAFT_MODELS"] {
        if let Some(d) = std::env::var_os(var) {
            dirs.push(PathBuf::from(d));
        }
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        dirs.push(exe.join("models"));
        dirs.push(exe.join("../Resources/models"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models"));
    dirs
}

/// The installed models, if any.
pub fn find_models() -> Option<Models> {
    model_dirs().iter().find_map(|d| Models::in_dir(d))
}

static RECOGNIZER: Mutex<Option<Arc<dyn Recognizer>>> = Mutex::new(None);

/// Use `r` for every later OCR in this process (tests, other engines).
pub fn install_recognizer(r: Arc<dyn Recognizer>) {
    let mut slot = RECOGNIZER.lock().unwrap_or_else(|e| e.into_inner());
    *slot = Some(r);
}

/// The recogniser: the installed one, else the ocrs models (loaded once).
pub fn recognizer() -> Result<Arc<dyn Recognizer>> {
    let mut slot = RECOGNIZER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = slot.as_ref() {
        return Ok(r.clone());
    }
    let models = find_models().ok_or_else(|| {
        invalid(
            "the OCR models are not installed: run `cargo xtask models`, or set MARKUPCRAFT_OCR_MODELS to a folder with text-detection.rten and text-recognition.rten",
        )
    })?;
    let ocr: Arc<dyn Recognizer> = Arc::new(Ocr::load(&models).map_err(|e| invalid(e.to_string()))?);
    *slot = Some(ocr.clone());
    Ok(ocr)
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrOptions {
    /// Pages (0-based); empty: every page.
    pub pages: Vec<usize>,
    /// Resolution the page is read at.
    pub dpi: f64,
    /// Leave pages that already have text alone.
    pub skip_text_pages: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            dpi: 300.0,
            skip_text_pages: true,
        }
    }
}

/// What OCR did on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrPage {
    pub page: usize,
    pub words: usize,
    pub text: String,
    /// Why the page was left alone.
    pub skipped: Option<String>,
}

impl Session {
    /// Recognise text on pages and add it as an invisible text layer (one undoable step).
    pub fn ocr(&mut self, opts: &OcrOptions, rec: &dyn Recognizer) -> Result<Vec<OcrPage>> {
        if !(opts.dpi.is_finite() && (72.0..=600.0).contains(&opts.dpi)) {
            return Err(invalid("dpi must be from 72 to 600"));
        }
        let pages: Vec<usize> = if opts.pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            opts.pages.clone()
        };
        for p in &pages {
            self.page(*p)?;
        }
        let doc: Renderable = self.renderable(true)?;
        let mut found: Vec<(usize, Vec<PlacedWord>)> = Vec::new();
        let mut report = Vec::new();
        for &page in &pages {
            let skip = |why: &str| OcrPage {
                page,
                words: 0,
                text: String::new(),
                skipped: Some(why.into()),
            };
            if opts.skip_text_pages && doc.text(page).is_some_and(|t| !t.plain_text().trim().is_empty()) {
                report.push(skip("the page already has text"));
                continue;
            }
            let img = doc.render_rgba(page, (opts.dpi / 72.0) as f32)?;
            let img = if img.w.max(img.h) as f32 > MAX_SIDE {
                doc.render_rgba(page, img.scale * MAX_SIDE / img.w.max(img.h) as f32)?
            } else {
                img
            };
            // Premultiplied RGBA over white, as RGB.
            let rgb: Vec<u8> = img
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| {
                    let a = 255 - p[3] as u16;
                    [
                        (p[0] as u16 + a).min(255) as u8,
                        (p[1] as u16 + a).min(255) as u8,
                        (p[2] as u16 + a).min(255) as u8,
                    ]
                })
                .collect();
            let lines = match rec.recognize(&rgb, img.w as u32, img.h as u32) {
                Ok(l) => l,
                Err(e) => {
                    report.push(skip(&e));
                    continue;
                }
            };
            let s = img.scale.max(1e-6);
            let to_user = |x: f32, y: f32| {
                let [u, v] = img.geom.view_to_user(x / s, y / s);
                [u as f64, v as f64]
            };
            let words: Vec<PlacedWord> = lines
                .iter()
                .flat_map(|l| &l.words)
                .filter(|w| w.rect.iter().all(|v| v.is_finite()))
                .map(|w| PlacedWord::place(w, to_user))
                .collect();
            let text = lines.iter().map(Line::text).collect::<Vec<_>>().join("\n");
            report.push(OcrPage {
                page,
                words: words.len(),
                text,
                skipped: None,
            });
            if !words.is_empty() {
                found.push((page, words));
            }
        }
        if !found.is_empty() {
            self.edit("OCR", |s| {
                for (page, words) in &found {
                    pdfcraft_edit::stamp(&mut s.file.cos, *page, "OCR", pdfcraft_ocr::text_layer(words))
                        .map_err(|e| invalid(e.to_string()))?;
                }
                Ok(((), true))
            })?;
        }
        Ok(report)
    }
}

/// A recogniser for tests: it "reads" `text` as one word covering all the ink in the image.
#[derive(Debug, Clone)]
pub struct InkWord {
    pub text: String,
}

impl Recognizer for InkWord {
    fn recognize(&self, rgb: &[u8], width: u32, height: u32) -> std::result::Result<Vec<Line>, String> {
        let (w, h) = (width as usize, height as usize);
        if rgb.len() < w * h * 3 {
            return Err("the image is smaller than its size".into());
        }
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for (i, p) in rgb.as_chunks::<3>().0.iter().enumerate() {
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) < 384 {
                let (x, y) = (i % w.max(1), i / w.max(1));
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if x0 >= x1 {
            return Ok(Vec::new());
        }
        Ok(vec![Line {
            words: vec![pdfcraft_ocr::Word {
                text: self.text.clone(),
                rect: [x0 as f32, y0 as f32, x1 as f32, y1 as f32],
            }],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect, text};
    use markupcraft_render::text::{TextExtractor, TextSource};

    #[test]
    fn ocr_adds_a_searchable_layer_over_the_ink() {
        // Page 1 is a "scan" (only shapes); page 2 already has text.
        let bytes = pdf(&[
            SyntheticPage::new(612.0, 792.0, rect(100.0, 500.0, 200.0, 40.0)),
            SyntheticPage::new(612.0, 792.0, text(50.0, 50.0, 12.0, "typed")),
        ]);
        let mut s = Session::from_bytes(bytes, "scan.pdf").unwrap();
        let rec = InkWord { text: "KITCHEN".into() };
        let r = s
            .ocr(
                &OcrOptions {
                    dpi: 150.0,
                    ..Default::default()
                },
                &rec,
            )
            .unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].words, r[0].text.as_str()), (1, "KITCHEN"));
        assert!(r[1].skipped.is_some());
        assert_eq!(s.undo_label(), Some("OCR"));

        let mut t = TextExtractor::new(s.current_bytes().unwrap());
        assert_eq!(t.find("kitchen"), vec![(0, 1)]);
        // The word sits over the ink.
        let page = t.page_text(0).unwrap();
        let hit = page.find("KITCHEN");
        let rects = page.line_rects(hit[0].clone());
        let [x0, y0, x1, y1] = rects[0];
        assert!((x0 - 100.0).abs() < 4.0 && (x1 - 300.0).abs() < 4.0, "{:?}", rects);
        // view space: the page is 792 tall, the box spans user y 500..540.
        assert!(y0 > 792.0 - 545.0 && y1 < 792.0 - 495.0, "{:?}", rects);

        s.undo().unwrap();
        let mut t = TextExtractor::new(s.current_bytes().unwrap());
        assert!(t.find("kitchen").is_empty());
        assert!(
            s.ocr(
                &OcrOptions {
                    dpi: 5.0,
                    ..Default::default()
                },
                &rec
            )
            .is_err()
        );
        assert!(
            s.ocr(
                &OcrOptions {
                    pages: vec![7],
                    ..Default::default()
                },
                &rec
            )
            .is_err()
        );
    }
}
