//! OCR: make scanned pages searchable (an invisible text layer) with the bundled recognizer.

use egui::RichText;
use markupcraft_engine::ocr::{OcrOptions, recognizer};

use crate::{AppState, actions};

pub struct OcrState {
    pub open: bool,
    pub pages: String,
    pub dpi: f64,
    pub skip_text_pages: bool,
    pub message: String,
}

impl Default for OcrState {
    fn default() -> Self {
        let o = OcrOptions::default();
        Self {
            open: false,
            pages: String::new(),
            dpi: o.dpi,
            skip_text_pages: o.skip_text_pages,
            message: String::new(),
        }
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.ocr.open {
        return;
    }
    let (mut open, mut go) = (true, false);
    super::window("OCR").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.ocr;
        ui.label("Recognize the text of scanned pages so they can be searched and selected.");
        super::pages_field(ui, &mut s.pages);
        ui.horizontal(|ui| {
            ui.label("Resolution");
            ui.add(egui::DragValue::new(&mut s.dpi).range(72.0..=600.0).suffix(" dpi"));
        });
        ui.checkbox(&mut s.skip_text_pages, "Skip pages that already have text");
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("Run OCR").clicked() {
                go = true;
            }
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

pub fn run(app: &mut AppState) {
    let threads = app.threads;
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let s = &app.features.ocr;
    let Some(pages) = super::parse_pages(&s.pages, d.session.page_count()) else {
        app.features.ocr.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
        return;
    };
    let opts = OcrOptions {
        pages,
        dpi: s.dpi,
        skip_text_pages: s.skip_text_pages,
    };
    let r = recognizer().and_then(|rec| d.session.ocr(&opts, rec.as_ref()));
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
    d.rerender(threads);
    app.status = msg.clone();
    app.features.ocr.message = msg;
}
