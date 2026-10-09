//! Links (Alt+N): every link of the document with where it goes; go to it, follow it, delete
//! it, or draw a new one with the Hyperlink tool.

use egui::RichText;

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::features::links;
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "links",
    title: "Links",
    icon: "arrow-up-right",
    slot: Slot::Left,
    keys: alt(egui::Key::N),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let all = d.session.links();
    let (mut select, mut follow, mut delete, mut add) = (None, None, None, false);
    {
        let l = &mut app.features.links;
        ui.horizontal(|ui| {
            if ui
                .button("New Link")
                .on_hover_text("Shift+H: drag the link area")
                .clicked()
            {
                add = true;
            }
            ui.checkbox(&mut l.highlight, "Highlight");
        });
        ui.separator();
        if all.is_empty() {
            super::empty(ui, "This document has no links.");
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for link in &all {
                let sel = l.selected.as_deref() == Some(link.id.as_str());
                let r = ui.selectable_label(sel, format!("p.{}  {}", link.page + 1, links::describe(&link.target)));
                if r.clicked() {
                    select = Some(link.clone());
                }
                if r.double_clicked() {
                    follow = Some(link.clone());
                }
                if sel {
                    ui.horizontal(|ui| {
                        if ui.small_button("Follow").clicked() {
                            follow = Some(link.clone());
                        }
                        if ui.small_button("Delete").clicked() {
                            delete = Some(link.id.clone());
                        }
                    });
                }
            }
        });
        if !l.message.is_empty() {
            ui.label(RichText::new(&l.message).small());
        }
    }
    if add {
        app.queue("markup.hyperlink");
    }
    if let Some(link) = select {
        app.features.links.selected = Some(link.id.clone());
        if let Some(d) = app.doc_mut() {
            let n = d.session.page_count();
            if d.view.current != link.page {
                d.view.go_to_page(link.page, n);
            }
        }
    }
    if let Some(link) = follow {
        let ctx = ui.ctx().clone();
        links::follow(app, &link, &ctx);
    }
    if let Some(id) = delete
        && let Some(d) = app.doc_mut()
    {
        let r = d.session.delete_links(&[id]);
        app.features.links.message = actions::report(r, |n| format!("Deleted {}", actions::plural(n, "link")));
        app.features.links.selected = None;
    }
}
