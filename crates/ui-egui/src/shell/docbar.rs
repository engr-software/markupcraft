//! The document bar under the menu bar: a document icon with a dropdown (the open documents,
//! Open and Open Recent), then "Name:" with the active document's title, "Pages:" with its page
//! count and a button for Document Properties (`docs/UI_LAYOUT.md`). Window > Document Bar
//! shows or hides it.

use egui::{RichText, Stroke};

use crate::theme::Tokens;
use crate::{AppState, icons};

/// The bar (after the menu bar, before any toolbar).
pub fn bar(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() || !app.shell.ui.show_document_bar {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("document_bar")
        .exact_size(28.0)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(6, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let r = icons::button(ui, "file-text", 24.0, false, "Documents");
                egui::Popup::menu(&r).show(|ui| {
                    ui.set_min_width(240.0);
                    open_documents(app, ui);
                    ui.separator();
                    if ui.button("Open...").clicked() {
                        app.queue("file.open");
                        ui.close();
                    }
                    crate::shell::recent::menu(app, ui);
                });
                let (name, pages) = app.doc().map_or((String::new(), String::new()), |d| {
                    let star = if d.session.is_dirty() { " *" } else { "" };
                    (format!("{}{star}", d.name), d.session.page_count().to_string())
                });
                ui.add_space(4.0);
                // One label per field ("Name: plan.pdf"), so the name is not a second widget
                // with the same text as the document's tab.
                let field = |key: &str, value: &str| {
                    let mut job = egui::text::LayoutJob::default();
                    let font = egui::FontId::proportional(12.0);
                    let fmt = |color| egui::TextFormat::simple(font.clone(), color);
                    job.append(&format!("{key}: "), 0.0, fmt(t.text_muted));
                    job.append(value, 0.0, fmt(t.text));
                    job
                };
                ui.add(egui::Label::new(field("Name", &name)).truncate());
                ui.add_space(10.0);
                ui.label(field("Pages", &pages));
                ui.add_space(6.0);
                let id = "document.properties";
                let tip = crate::commands::describe(id).map_or_else(String::new, |(l, _, k)| match k {
                    Some(k) => format!("{l} ({})", k.label()),
                    None => l,
                });
                let enabled = app.enabled(id);
                let r = ui
                    .add_enabled_ui(enabled, |ui| icons::button(ui, "info", 22.0, false, &tip))
                    .inner;
                if r.clicked() {
                    app.queue(id);
                }
            });
        });
}

/// The open documents as menu items (the active one checked); a click makes it active.
pub fn open_documents(app: &mut AppState, ui: &mut egui::Ui) {
    if app.docs.is_empty() {
        ui.label(RichText::new("No open documents").weak());
        return;
    }
    let mut pick = None;
    for (i, d) in app.docs.iter().enumerate() {
        let star = if d.session.is_dirty() { " *" } else { "" };
        let tip = d
            .path
            .as_ref()
            .map_or_else(|| d.name.clone(), |p| p.display().to_string());
        if ui
            .add(egui::Button::new(format!("{}{star}", d.name)).selected(i == app.active))
            .on_hover_text(tip)
            .clicked()
        {
            pick = Some(i);
        }
    }
    if let Some(i) = pick {
        app.active = i;
        ui.close();
    }
}
