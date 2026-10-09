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
    let places = d.session.places();
    let current = d.view.current;
    let (mut select, mut follow, mut delete, mut add) = (None, None, None, false);
    let (mut edit, mut urls, mut place_add, mut place_go, mut place_del) = (None, false, false, None, None);
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
            if ui
                .button("From URLs")
                .on_hover_text("Create Hyperlinks from URLs written on the pages")
                .clicked()
            {
                urls = true;
            }
        });
        ui.separator();
        if all.is_empty() {
            super::empty(ui, "This document has no links.");
        }
        egui::ScrollArea::vertical()
            .id_salt("links-list")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
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
                            if ui.small_button("Edit Action").clicked() {
                                edit = Some(link.clone());
                            }
                            if ui.small_button("Delete").clicked() {
                                delete = Some(link.id.clone());
                            }
                        });
                    }
                }
            });
        ui.separator();
        ui.label(RichText::new("Places").strong());
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut l.new_place)
                    .desired_width(110.0)
                    .hint_text("new Place"),
            );
            if ui
                .small_button("Add Here")
                .on_hover_text("A Place at the current page")
                .clicked()
            {
                place_add = true;
            }
            ui.add(
                egui::TextEdit::singleline(&mut l.place_filter)
                    .desired_width(70.0)
                    .hint_text("filter"),
            );
        });
        let filter = l.place_filter.to_lowercase();
        let mut last = None;
        for p in places
            .iter()
            .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(&filter))
        {
            if last != Some(p.page) {
                ui.label(
                    RichText::new(p.page.map_or("(no page)".to_string(), |x| format!("Page {}", x + 1)))
                        .small()
                        .weak(),
                );
                last = Some(p.page);
            }
            ui.horizontal(|ui| {
                if ui.selectable_label(false, &p.name).clicked() {
                    place_go = Some(p.name.clone());
                }
                if ui.small_button("x").clicked() {
                    place_del = Some(p.name.clone());
                }
            });
        }
        if !l.message.is_empty() {
            ui.label(RichText::new(&l.message).small());
        }
    }
    if let Some(link) = edit {
        links::edit_link_action(app, &link);
    }
    if urls {
        links::from_urls(app);
    }
    if place_add {
        let name = app.features.links.new_place.trim().to_string();
        if let Some(d) = app.doc_mut() {
            let r = d.session.set_place(&name, current, None, None, None);
            app.features.links.message = actions::report(r, |_| format!("Place {name} added"));
        }
    }
    if let Some(n) = place_go {
        let ctx = ui.ctx().clone();
        links::follow_target(app, &markupcraft_engine::links::LinkTarget::Place(n), &ctx);
    }
    if let Some(n) = place_del
        && let Some(d) = app.doc_mut()
    {
        let r = d.session.delete_places(&[n]);
        app.features.links.message = actions::report(r, |_| "Place deleted".into());
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
