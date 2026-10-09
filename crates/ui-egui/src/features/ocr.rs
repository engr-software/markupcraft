//! OCR: make scanned pages searchable (an invisible text layer) with the bundled recognizer.
//! Recognition runs on a worker thread (the window keeps painting); the text layer is added to
//! the document as one undo step when it is done.

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use markupcraft_engine::ocr::{OcrOptions, OcrPage, OcrWords, ocr_recognize, recognizer};

use crate::{AppState, actions};

type Outcome = markupcraft_engine::Result<(Vec<OcrPage>, OcrWords)>;

pub struct OcrState {
    pub open: bool,
    pub pages: String,
    pub dpi: f64,
    pub skip_text_pages: bool,
    pub deskew: bool,
    pub detect_orientation: bool,
    pub skip_vector_pages: bool,
    /// Text documents read at a lower resolution than drawings.
    pub text_document: bool,
    pub message: String,
    /// A run in progress: the document it is for and the worker's answer.
    pub running: Option<(u64, Receiver<Outcome>)>,
}

impl Default for OcrState {
    fn default() -> Self {
        let o = OcrOptions::default();
        Self {
            open: false,
            pages: String::new(),
            dpi: o.dpi,
            skip_text_pages: o.skip_text_pages,
            deskew: o.deskew,
            detect_orientation: o.detect_orientation,
            skip_vector_pages: o.skip_vector_pages,
            text_document: false,
            message: String::new(),
            running: None,
        }
    }
}

impl OcrState {
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }
}

/// Take a finished run's words into its document.
fn poll(app: &mut AppState, ctx: &egui::Context) {
    let Some((uid, rx)) = &app.features.ocr.running else {
        return;
    };
    let uid = *uid;
    let outcome = match rx.try_recv() {
        Ok(o) => o,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(markupcraft_engine::EngineError::Invalid(
            "the OCR worker stopped".into(),
        )),
    };
    app.features.ocr.running = None;
    let threads = app.threads;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    let r = outcome.and_then(|(report, found)| d.session.apply_ocr(&found).map(|_| report));
    d.rerender(threads);
    let msg = actions::report(r, |done| {
        let words: usize = done.iter().map(|p| p.words).sum();
        let skipped = done.iter().filter(|p| p.skipped.is_some()).count();
        format!(
            "OCR: {} recognized on {}, {} skipped",
            actions::plural(words, "word"),
            actions::plural(done.len() - skipped, "page"),
            skipped
        )
    });
    app.status = msg.clone();
    app.features.ocr.message = msg;
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    poll(app, ctx);
    if !app.features.ocr.open {
        return;
    }
    let (mut open, mut go) = (true, false);
    super::window("OCR").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.ocr;
        ui.label("Recognize the text of scanned pages so they can be searched and selected.");
        super::pages_field(ui, &mut s.pages);
        ui.horizontal(|ui| {
            ui.label("Page type");
            if ui.selectable_label(!s.text_document, "Drawing").clicked() {
                s.text_document = false;
                s.dpi = 300.0;
            }
            if ui.selectable_label(s.text_document, "Text document").clicked() {
                s.text_document = true;
                s.dpi = 200.0;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Resolution");
            ui.add(egui::DragValue::new(&mut s.dpi).range(72.0..=600.0).suffix(" dpi"))
                .on_hover_text("Higher reads small text better; lower is faster");
        });
        ui.checkbox(&mut s.skip_text_pages, "Skip pages that already have text");
        ui.checkbox(&mut s.skip_vector_pages, "Skip vector pages (no scanned image)");
        ui.checkbox(&mut s.deskew, "Correct skew");
        ui.checkbox(&mut s.detect_orientation, "Detect orientation (and vertical text)");
        ui.label(
            RichText::new("Language: English and other Latin-script text")
                .small()
                .weak(),
        );
        if s.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading pages...");
            });
        }
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!s.busy(), |ui| {
                if ui.button("Run OCR").clicked() {
                    go = true;
                }
            });
            if ui.button("Close").clicked() {
                s.open = false;
            }
        });
    });
    if !open {
        app.features.ocr.open = false;
    }
    if go {
        run(app);
    }
}

/// Start OCR on the active document (on a worker thread).
pub fn run(app: &mut AppState) {
    if app.features.ocr.busy() {
        return;
    }
    let Some(d) = app.docs.get(app.active) else { return };
    let s = &app.features.ocr;
    let Some(pages) = super::parse_pages(&s.pages, d.session.page_count()) else {
        app.features.ocr.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
        return;
    };
    let opts = OcrOptions {
        pages,
        dpi: s.dpi,
        skip_text_pages: s.skip_text_pages,
        deskew: s.deskew,
        detect_orientation: s.detect_orientation,
        skip_vector_pages: s.skip_vector_pages,
    };
    let prepared = recognizer().and_then(|rec| {
        let pages = d.session.ocr_pages(&opts)?;
        let bytes = d.session.current_bytes()?;
        Ok((rec, pages, bytes))
    });
    let (rec, pages, bytes) = match prepared {
        Ok(p) => p,
        Err(e) => {
            app.features.ocr.message = e.to_string();
            return;
        }
    };
    let uid = d.uid;
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new()
        .name("markupcraft-ocr".into())
        .spawn(move || {
            let _ = tx.send(ocr_recognize(bytes, &pages, &opts, rec.as_ref()));
        });
    match started {
        Ok(_) => {
            app.features.ocr.running = Some((uid, rx));
            app.features.ocr.message = "Reading pages...".into();
        }
        Err(e) => app.features.ocr.message = format!("the OCR worker could not start: {e}"),
    }
}
