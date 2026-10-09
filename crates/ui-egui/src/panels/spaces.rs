//! Spaces (Alt+S): the named areas of the pages, what each contains (markups by subject),
//! new spaces drawn or filled, rename, delete, highlight, export and import.

use egui::RichText;
use markupcraft_engine::spaces::SpacePatch;

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::dialogs::Purpose;
use crate::features::{self, Ask, spaces};
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "spaces",
    title: "Spaces",
    icon: "square-dashed",
    slot: Slot::Left,
    keys: alt(egui::Key::S),
    ui,
};

enum Act {
    Draw,
    Fill,
    Select(String, usize),
    Rename(String, String),
    Delete(String),
    SelectMarkups(String),
    Snapshot(String),
    Export,
    Import,
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let current = d.view.current;
    let s = &app.features.spaces;
    let list = d.session.spaces(if s.all_pages { None } else { Some(current) });
    let tallies = d.session.space_tallies(if s.all_pages { None } else { Some(current) });
    let mut act = None;
    {
        let s = &mut app.features.spaces;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut s.new_name)
                    .hint_text("new space name")
                    .desired_width(110.0),
            );
            if ui.button("Draw").on_hover_text("Click the corners").clicked() {
                act = Some(Act::Draw);
            }
            if ui.button("Fill").on_hover_text("Click inside a room").clicked() {
                act = Some(Act::Fill);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut s.highlight, "Highlight");
            ui.checkbox(&mut s.all_pages, "All pages");
            if ui.small_button("Export...").clicked() {
                act = Some(Act::Export);
            }
            if ui.small_button("Import...").clicked() {
                act = Some(Act::Import);
            }
        });
        ui.separator();
        if list.is_empty() {
            super::empty(ui, "No spaces on this page. Draw one, or Fill a room.");
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (page, sp) in &list {
                let sel = s.selected.as_deref() == Some(sp.id.as_str());
                let tally = tallies.iter().find(|t| t.page == *page && t.space == sp.name);
                let n = tally.map_or(0, |t| t.markups);
                let head = if s.all_pages {
                    format!("{}  (p.{}, {n})", sp.name, page + 1)
                } else {
                    format!("{}  ({n})", sp.name)
                };
                let r = ui.selectable_label(sel, RichText::new(head).strong());
                if r.clicked() {
                    act = Some(Act::Select(sp.id.clone(), *page));
                }
                if sel {
                    ui.indent(("space", &sp.id), |ui| {
                        if let Some(t) = tally {
                            for (subject, c) in &t.counts {
                                ui.label(RichText::new(format!("{subject}: {c}")).small());
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut s.rename).desired_width(110.0));
                            if ui.small_button("Rename").clicked() && !s.rename.trim().is_empty() {
                                act = Some(Act::Rename(sp.id.clone(), s.rename.trim().to_string()));
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("Select markups").clicked() {
                                act = Some(Act::SelectMarkups(sp.id.clone()));
                            }
                            if ui.small_button("Delete").clicked() {
                                act = Some(Act::Delete(sp.id.clone()));
                            }
                            if ui
                                .small_button("Snapshot")
                                .on_hover_text("Copy the space's region as a snapshot (Ctrl+V pastes it)")
                                .clicked()
                            {
                                act = Some(Act::Snapshot(sp.id.clone()));
                            }
                        });
                    });
                }
            }
        });
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
    }
    let Some(act) = act else { return };
    match act {
        Act::Draw => spaces::draw(app),
        Act::Fill => spaces::fill(app),
        Act::Select(id, page) => {
            let name = list.iter().find(|(_, s)| s.id == id).map(|(_, s)| s.name.clone());
            app.features.spaces.selected = Some(id);
            app.features.spaces.rename = name.unwrap_or_default();
            if let Some(d) = app.doc_mut() {
                let n = d.session.page_count();
                if d.view.current != page {
                    d.view.go_to_page(page, n);
                }
            }
        }
        Act::Rename(id, name) => {
            if let Some(d) = app.doc_mut() {
                let patch = SpacePatch {
                    name: Some(name.clone()),
                    ..Default::default()
                };
                let r = d.session.edit_space(&id, &patch);
                app.features.spaces.message = actions::report(r, |_| format!("Renamed to {name}"));
            }
        }
        Act::Delete(id) => {
            if let Some(d) = app.doc_mut() {
                let r = d.session.delete_spaces(&[id]);
                app.features.spaces.message =
                    actions::report(r, |n| format!("Deleted {}", actions::plural(n, "space")));
            }
            app.features.spaces.selected = None;
        }
        Act::SelectMarkups(id) => {
            if let Some(d) = app.doc_mut() {
                match d.session.markups_in_space(&id) {
                    Ok(ids) => {
                        let n = ids.len();
                        actions::select(&mut d.session, ids);
                        app.status = format!("Selected {}", actions::plural(n, "markup"));
                    }
                    Err(e) => app.status = e.to_string(),
                }
            }
        }
        Act::Snapshot(id) => {
            if let Some(d) = app.doc_mut() {
                let r = d.session.snapshot_to_clipboard(0, None, Some(&id));
                app.status = actions::report(r, |_| "Space copied as a snapshot: Ctrl+V pastes it".into());
            }
        }
        Act::Export => app
            .dialogs
            .save(Purpose::Feature(Ask::SpacesExport), features::JSON, "spaces.json"),
        Act::Import => app
            .dialogs
            .open(Purpose::Feature(Ask::SpacesImport), features::JSON, false),
    }
}
