//! Redaction: Mark for Redaction (drag boxes on the page), the marks shown on the pages, and
//! Apply Redactions, which removes the text, images and paths under them for good.

use egui::{Color32, RichText};
use markupcraft_geom::Point;

use super::Mark;
use crate::{AppState, DocTab, actions};

#[derive(Default)]
pub struct RedactState {
    /// The Apply Redactions confirmation is open.
    pub confirm: bool,
    pub pages: String,
    pub message: String,
    /// How new marks look (Redaction Properties).
    pub style: markupcraft_engine::redact::MarkStyle,
    pub properties_open: bool,
    /// Also remove the document properties, metadata, attachments and scripts.
    pub scrub: bool,
}

impl RedactState {
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        for m in d.session.redact_marks() {
            for r in &m.rects {
                out.push(Mark::rect(
                    m.page,
                    *r,
                    Color32::from_rgba_premultiplied(60, 0, 0, 60),
                    Color32::from_rgb(200, 0, 0),
                ));
            }
        }
    }
}

/// A box was dragged in Mark for Redaction.
pub fn rect_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    let style = app.features.redact.style.clone();
    let Some(d) = app.doc_mut() else { return };
    let res = d.session.redact_mark(page, &[r], &style);
    app.status = actions::report(res, |_| {
        "Marked for redaction (Esc when done; Document > Apply Redactions)".into()
    });
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.redact.confirm {
        return;
    }
    let marks = app.doc().map_or(0, |d| d.session.redact_marks().len());
    let (mut open, mut go) = (true, false);
    super::window("Apply Redactions").open(&mut open).show(ctx, |ui| {
        let s = &mut app.features.redact;
        ui.label(format!("{} marked for redaction.", actions::plural(marks, "area")));
        ui.label(
            RichText::new("Applying removes the text, images and drawing under the marks permanently.")
                .color(Color32::from_rgb(200, 60, 40)),
        );
        super::pages_field(ui, &mut s.pages);
        ui.checkbox(
            &mut s.scrub,
            "Also remove document properties, metadata, attachments and scripts",
        );
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(marks > 0, |ui| {
                if ui.button("Apply").clicked() {
                    go = true;
                }
            });
            if ui.button("Cancel").clicked() {
                s.confirm = false;
            }
        });
    });
    if !open {
        app.features.redact.confirm = false;
    }
    if go {
        apply(app);
    }
}

pub fn apply(app: &mut AppState) {
    let threads = app.threads;
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let text = app.features.redact.pages.trim().to_string();
    let pages = if text.is_empty() {
        None
    } else {
        match super::parse_pages(&text, d.session.page_count()) {
            Some(p) => Some(p),
            None => {
                app.features.redact.message = "Pages: a range like 1-3, 5".into();
                return;
            }
        }
    };
    let r = d.session.redact_apply_with(pages.as_deref(), app.features.redact.scrub);
    let msg = actions::report(r, |rep| {
        let mut s = format!(
            "Redacted {} on {}: {} glyphs, {} images, {} paths removed",
            actions::plural(rep.marks, "area"),
            actions::plural(rep.pages, "page"),
            rep.glyphs,
            rep.images,
            rep.paths
        );
        if !rep.residue.is_empty() {
            s.push_str(&format!(" (check: {})", rep.residue.join("; ")));
        }
        s
    });
    d.rerender(threads);
    app.status = msg.clone();
    app.features.redact.message = msg;
    app.features.redact.confirm = false;
}
