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
    /// Correct a skewed scan before reading it (up to 10 degrees either way).
    pub deskew: bool,
    /// Find the page's reading direction (pages scanned on their side or upside down, and
    /// vertical text): the turn that reads the most text wins.
    pub detect_orientation: bool,
    /// Leave pages without images (vector drawings) alone.
    pub skip_vector_pages: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            dpi: 300.0,
            skip_text_pages: true,
            deskew: false,
            detect_orientation: false,
            skip_vector_pages: false,
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

/// The words recognised on pages, ready for [`Session::apply_ocr`].
pub type OcrWords = Vec<(usize, Vec<PlacedWord>)>;

/// An RGB image turned clockwise by `q` quarter turns.
fn turn_rgb(rgb: &[u8], w: usize, h: usize, q: u32) -> (Vec<u8>, usize, usize) {
    let q = q % 4;
    if q == 0 {
        return (rgb.to_vec(), w, h);
    }
    let (nw, nh) = if q % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![255u8; nw * nh * 3];
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = match q {
                1 => (y, h - 1 - x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (w - 1 - y, x),
            };
            let (si, di) = ((sy * w + sx) * 3, (y * nw + x) * 3);
            if let (Some(s), Some(d)) = (rgb.get(si..si + 3), out.get_mut(di..di + 3)) {
                d.copy_from_slice(s);
            }
        }
    }
    (out, nw, nh)
}

/// A point of an image turned by `q` quarter turns, back in the unturned image.
fn unturn(x: f32, y: f32, w: usize, h: usize, q: u32) -> (f32, f32) {
    let (w, h) = (w as f32, h as f32);
    match q % 4 {
        1 => (w - y, x),
        2 => (w - x, h - y),
        3 => (y, h - x),
        _ => (x, y),
    }
}

/// An RGB image rotated by `deg` degrees about its centre (white outside).
fn rotate_rgb(rgb: &[u8], w: usize, h: usize, deg: f64) -> Vec<u8> {
    let (s, c) = deg.to_radians().sin_cos();
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let mut out = vec![255u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            let (sx, sy) = (c * dx + s * dy + cx, -s * dx + c * dy + cy);
            if sx < 0.0 || sy < 0.0 || sx >= w as f64 || sy >= h as f64 {
                continue;
            }
            let si = (sy as usize * w + sx as usize) * 3;
            let di = (y * w + x) * 3;
            if let (Some(src), Some(d)) = (rgb.get(si..si + 3), out.get_mut(di..di + 3)) {
                d.copy_from_slice(src);
            }
        }
    }
    out
}

/// The skew of a scan in degrees: the angle whose row profile of dark pixels is sharpest.
pub fn estimate_skew(rgb: &[u8], w: usize, h: usize) -> f64 {
    // Work on a small sample of dark pixels.
    let step = (w.max(h) / 800).max(1);
    let mut dark: Vec<(f64, f64)> = Vec::new();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let i = (y * w + x) * 3;
            if let Some(p) = rgb.get(i..i + 3)
                && (p[0] as u32 + p[1] as u32 + p[2] as u32) < 300
            {
                dark.push((x as f64, y as f64));
            }
        }
        if dark.len() > 200_000 {
            break;
        }
    }
    if dark.len() < 50 {
        return 0.0;
    }
    let score = |deg: f64| -> f64 {
        let (s, c) = deg.to_radians().sin_cos();
        let rows = h.max(1) + w.max(1);
        let mut hist = vec![0f64; rows * 2 / step.max(1) + 2];
        for (x, y) in &dark {
            let r = (y * c - x * s) / step as f64 + w as f64 / step as f64;
            if let Some(b) = hist.get_mut(r.max(0.0) as usize) {
                *b += 1.0;
            }
        }
        hist.iter().map(|v| v * v).sum()
    };
    let mut best = (0.0, score(0.0));
    let mut a = -10.0;
    while a <= 10.0 {
        let v = score(a);
        if v > best.1 {
            best = (a, v);
        }
        a += 0.5;
    }
    best.0
}

/// Recognise the words on `pages` of a PDF (`bytes`), off any session: what OCR reads, page by
/// page, for [`Session::apply_ocr`]. Runs on a worker thread as well as inline.
pub fn ocr_recognize(
    bytes: Arc<Vec<u8>>,
    pages: &[usize],
    opts: &OcrOptions,
    rec: &dyn Recognizer,
) -> Result<(Vec<OcrPage>, OcrWords)> {
    if !(opts.dpi.is_finite() && (72.0..=600.0).contains(&opts.dpi)) {
        return Err(invalid("dpi must be from 72 to 600"));
    }
    let doc = Renderable::new(bytes.clone(), true)?;
    let cos = if opts.skip_vector_pages {
        markupcraft_revu::cos::Document::open(bytes).ok()
    } else {
        None
    };
    let mut found: OcrWords = Vec::new();
    let mut report = Vec::new();
    for &page in pages {
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
        if let Some(c) = &cos
            && pdfcraft_edit::page_images(c, page)
                .map(|v| v.is_empty())
                .unwrap_or(false)
        {
            report.push(skip("a vector page (no images)"));
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
        let (w, h) = (img.w, img.h);
        // Orientation: the quarter turn that reads the most characters.
        let turns: &[u32] = if opts.detect_orientation { &[0, 1, 2, 3] } else { &[0] };
        let mut best: Option<(u32, Vec<Line>, usize, usize, f64)> = None;
        for &q in turns {
            let (t, tw, th) = turn_rgb(&rgb, w, h, q);
            let skew = if opts.deskew { estimate_skew(&t, tw, th) } else { 0.0 };
            let t = if skew.abs() > 0.05 {
                rotate_rgb(&t, tw, th, skew)
            } else {
                t
            };
            let lines = match rec.recognize(&t, tw as u32, th as u32) {
                Ok(l) => l,
                Err(e) => {
                    if q == 0 {
                        report.push(skip(&e));
                    }
                    continue;
                }
            };
            let chars: usize = lines
                .iter()
                .flat_map(|l| &l.words)
                .map(|w| w.text.chars().filter(|c| c.is_alphanumeric()).count())
                .sum();
            if best.as_ref().is_none_or(|b| {
                chars
                    > b.1
                        .iter()
                        .flat_map(|l| &l.words)
                        .map(|w| w.text.chars().filter(|c| c.is_alphanumeric()).count())
                        .sum()
            }) {
                best = Some((q, lines, tw, th, skew));
            }
        }
        let Some((q, lines, tw, th, skew)) = best else { continue };
        let s = img.scale.max(1e-6);
        let (sn, cs) = skew.to_radians().sin_cos();
        let (cx, cy) = (tw as f64 / 2.0, th as f64 / 2.0);
        // A box of the (turned, deskewed) image back to user space.
        let to_user = |x: f32, y: f32| {
            // Undo the deskew: the image was rotated by `skew` about its centre.
            let (dx, dy) = (x as f64 - cx, y as f64 - cy);
            let (ux, uy) = (cs * dx + sn * dy + cx, -sn * dx + cs * dy + cy);
            let (ox, oy) = unturn(ux as f32, uy as f32, w, h, q);
            let [u, v] = img.geom.view_to_user(ox / s, oy / s);
            [u as f64, v as f64]
        };
        let words: Vec<PlacedWord> = lines
            .iter()
            .flat_map(|l| &l.words)
            .filter(|w| w.rect.iter().all(|v| v.is_finite()))
            .map(|w| PlacedWord::place(w, to_user))
            .collect();
        let text = lines.iter().map(Line::text).collect::<Vec<_>>().join("\\n");
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
    Ok((report, found))
}

impl Session {
    /// OCR rewrites page content, which would break signatures, so (as in Revu) it does not run
    /// on signed documents.
    fn ocr_allowed(&self) -> Result<()> {
        if self.standards().signatures > 0 {
            return Err(invalid(
                "OCR cannot run on a signed document (it would invalidate the signatures)",
            ));
        }
        Ok(())
    }

    /// The pages an OCR run reads (all when `opts.pages` is empty), checked.
    pub fn ocr_pages(&self, opts: &OcrOptions) -> Result<Vec<usize>> {
        self.ocr_allowed()?;
        let pages: Vec<usize> = if opts.pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            opts.pages.clone()
        };
        for p in &pages {
            self.page(*p)?;
        }
        Ok(pages)
    }

    /// Add the recognised words as invisible text layers (one undoable step).
    pub fn apply_ocr(&mut self, found: &OcrWords) -> Result<()> {
        if found.is_empty() {
            return Ok(());
        }
        self.ocr_allowed()?;
        self.edit("OCR", |s| {
            for (page, words) in found {
                pdfcraft_edit::stamp(&mut s.file.cos, *page, "OCR", pdfcraft_ocr::text_layer(words))
                    .map_err(|e| invalid(e.to_string()))?;
            }
            Ok(((), true))
        })
    }

    /// Recognise text on pages and add it as an invisible text layer (one undoable step).
    pub fn ocr(&mut self, opts: &OcrOptions, rec: &dyn Recognizer) -> Result<Vec<OcrPage>> {
        if !(opts.dpi.is_finite() && (72.0..=600.0).contains(&opts.dpi)) {
            return Err(invalid("dpi must be from 72 to 600"));
        }
        let pages = self.ocr_pages(opts)?;
        let (report, found) = ocr_recognize(self.current_bytes()?, &pages, opts, rec)?;
        self.apply_ocr(&found)?;
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

    /// The real ocrs models (`cargo xtask models`) on rendered text: upright, on its side and
    /// skewed. Skipped when the models are not installed.
    #[test]
    fn real_ocr_reads_rendered_text_turned_and_skewed() {
        let Some(models) = find_models() else {
            eprintln!("OCR models not installed: skipped");
            return;
        };
        let ocr = Ocr::load(&models).unwrap();
        let line = |t: &str| text(0.0, 0.0, 28.0, t);
        let upright = format!("q 1 0 0 1 60 600 cm {}Q\n", line("MECHANICAL ROOM 104"));
        let side = format!("q 0 1 -1 0 300 150 cm {}Q\n", line("ELECTRICAL CLOSET"));
        let skewed = format!("q 0.9976 0.0698 -0.0698 0.9976 60 300 cm {}Q\n", line("STORAGE AREA"));
        let bytes = pdf(&[
            SyntheticPage::new(612.0, 792.0, upright),
            SyntheticPage::new(612.0, 792.0, side),
            SyntheticPage::new(612.0, 792.0, skewed),
        ]);
        let mut s = Session::from_bytes(bytes, "scan.pdf").unwrap();
        let opts = OcrOptions {
            dpi: 200.0,
            skip_text_pages: false,
            detect_orientation: true,
            deskew: true,
            ..Default::default()
        };
        let r = s.ocr(&opts, &ocr).unwrap();
        let read = |i: usize| r[i].text.to_uppercase().replace(' ', "");
        assert!(read(0).contains("MECHANICAL"), "{:?}", r[0].text);
        assert!(read(1).contains("CLOSET"), "turned page: {:?}", r[1].text);
        assert!(read(2).contains("STORAGE"), "skewed page: {:?}", r[2].text);
        assert_eq!(s.undo_label(), Some("OCR"));
        // The turned page's words land where the text is (x about 300 - 28 .. 300).
        let side_hits = s.search_text("CLOSET", &Default::default()).unwrap();
        let ocr_hit = side_hits.hits.iter().rfind(|h| h.page == 1).unwrap();
        let r0 = ocr_hit.rects[0];
        assert!(r0.x1 > 250.0 && r0.x0 < 320.0, "{r0:?}");
        // Skip vector pages: none of these pages has an image.
        let vec_only = OcrOptions {
            skip_vector_pages: true,
            skip_text_pages: false,
            ..Default::default()
        };
        let r = s.ocr(&vec_only, &ocr).unwrap();
        assert!(r.iter().all(|p| p.skipped.is_some()));
        assert!(estimate_skew(&[255; 300], 10, 10).abs() < 1e-9);
    }

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
