//! Bookmarks (Alt+B): the PDF outline; click an entry to go to its page.

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;

pub static PANEL: PanelDef = PanelDef {
    id: "bookmarks",
    title: "Bookmarks",
    icon: "bookmark",
    slot: Slot::Left,
    keys: alt(egui::Key::B),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    let Some(render) = doc.render.as_ref() else { return };
    if render.outline().is_empty() {
        super::empty(ui, "This document has no bookmarks.");
        return;
    }
    let count = render.page_count();
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for e in render.outline() {
            ui.horizontal(|ui| {
                ui.add_space(4.0 + 14.0 * e.depth.min(12) as f32);
                let label = if e.title.trim().is_empty() {
                    "(untitled)"
                } else {
                    e.title.as_str()
                };
                let r = ui.selectable_label(e.page == Some(doc.view.current), label);
                if r.clicked()
                    && let Some(p) = e.page
                {
                    go = Some(p);
                }
                if let Some(p) = e.page {
                    r.on_hover_text(format!("Page {}", p + 1));
                }
            });
        }
    });
    if let Some(p) = go {
        doc.view.go_to_page(p, count);
    }
}
