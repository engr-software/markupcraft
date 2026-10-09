//! Layers (Alt+Y): the document's markup layers (PDF optional content groups) with view, print
//! and lock states and markup counts; new, rename, delete, isolate, show all, assign the
//! selected markups, select a layer's markups, flatten a layer.

use egui::RichText;
use markupcraft_engine::flatten::FlattenFilter;
use markupcraft_engine::layers::LayerState;

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "layers",
    title: "Layers",
    icon: "layers",
    slot: Slot::Left,
    keys: alt(egui::Key::Y),
    ui,
};

/// The panel's selection and typing fields (kept in `AppState::features`).
#[derive(Default)]
pub struct Fields {
    pub selected: Option<String>,
    pub new_name: String,
    pub rename: String,
}

enum Act {
    State(String, LayerState),
    Create(String),
    Rename(String, String),
    Delete(String),
    Isolate(String),
    ShowAll,
    Assign(String),
    Select(String),
    Flatten(String),
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let layers = d.session.layers();
    let selected_markups = d.selection().len();
    let mut f = std::mem::take(&mut app.features.layers);
    let mut act = None;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.new_name)
                .hint_text("new layer")
                .desired_width(120.0),
        );
        if ui.button("New").clicked() && !f.new_name.trim().is_empty() {
            act = Some(Act::Create(f.new_name.trim().to_string()));
            f.new_name.clear();
        }
        if ui.button("Show All").clicked() {
            act = Some(Act::ShowAll);
        }
    });
    ui.separator();
    if layers.is_empty() {
        super::empty(
            ui,
            "No layers yet. New makes one; Assign puts the selected markups on it.",
        );
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("layers-grid")
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                ui.label(RichText::new("View").small());
                ui.label(RichText::new("Print").small());
                ui.label(RichText::new("Lock").small());
                ui.label(RichText::new("Layer").small());
                ui.label(RichText::new("Markups").small());
                ui.end_row();
                for l in &layers {
                    let (mut v, mut p, mut k) = (l.visible, l.print, l.locked);
                    if ui.checkbox(&mut v, "").on_hover_text("Show").changed() {
                        act = Some(Act::State(
                            l.name.clone(),
                            LayerState {
                                visible: Some(v),
                                ..Default::default()
                            },
                        ));
                    }
                    if ui.checkbox(&mut p, "").on_hover_text("Print").changed() {
                        act = Some(Act::State(
                            l.name.clone(),
                            LayerState {
                                print: Some(p),
                                ..Default::default()
                            },
                        ));
                    }
                    if ui.checkbox(&mut k, "").on_hover_text("Lock").changed() {
                        act = Some(Act::State(
                            l.name.clone(),
                            LayerState {
                                locked: Some(k),
                                ..Default::default()
                            },
                        ));
                    }
                    let sel = f.selected.as_deref() == Some(l.name.as_str());
                    if ui.selectable_label(sel, &l.name).clicked() {
                        f.selected = Some(l.name.clone());
                        f.rename = l.name.clone();
                    }
                    ui.label(format!("{}", l.markups));
                    ui.end_row();
                }
            });
        if let Some(name) = f.selected.clone().filter(|n| layers.iter().any(|l| &l.name == n)) {
            ui.separator();
            ui.label(RichText::new(&name).strong());
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(selected_markups > 0, |ui| {
                    if ui.button(format!("Assign selected ({selected_markups})")).clicked() {
                        act = Some(Act::Assign(name.clone()));
                    }
                });
                if ui.button("Select markups").clicked() {
                    act = Some(Act::Select(name.clone()));
                }
                if ui.button("Isolate").clicked() {
                    act = Some(Act::Isolate(name.clone()));
                }
                if ui
                    .button("Flatten")
                    .on_hover_text("Flatten this layer's markups")
                    .clicked()
                {
                    act = Some(Act::Flatten(name.clone()));
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Delete the layer; its markups stay")
                    .clicked()
                {
                    act = Some(Act::Delete(name.clone()));
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.rename).desired_width(120.0));
                if ui.button("Rename").clicked() && !f.rename.trim().is_empty() && f.rename.trim() != name {
                    act = Some(Act::Rename(name.clone(), f.rename.trim().to_string()));
                }
            });
        }
    });
    if let Some(Act::Rename(_, n) | Act::Create(n)) = &act {
        f.selected = Some(n.clone());
    }
    app.features.layers = f;
    let Some(act) = act else { return };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let s = &mut d.session;
    app.status = match act {
        Act::State(n, st) => actions::report(s.set_layer_state(&n, st), |_| format!("Layer {n} changed")),
        Act::Create(n) => actions::report(s.create_layer(&n), |_| format!("Layer {n} created")),
        Act::Rename(a, b) => actions::report(s.rename_layer(&a, &b), |_| format!("Renamed {a} to {b}")),
        Act::Delete(n) => actions::report(s.delete_layer(&n, false), |_| format!("Deleted layer {n}")),
        Act::Isolate(n) => actions::report(s.isolate_layer(&n), |_| format!("Showing only {n}")),
        Act::ShowAll => actions::report(s.show_all_layers(), |_| "Showing every layer".into()),
        Act::Assign(n) => {
            let ids = s.selection().to_vec();
            actions::report(s.assign_layer(&ids, &n), |k| {
                format!("{} on {n}", actions::plural(k, "markup"))
            })
        }
        Act::Select(n) => {
            let ids = s.layer_markups(std::slice::from_ref(&n), false);
            let k = ids.len();
            actions::select(s, ids);
            format!("Selected {}", actions::plural(k, "markup"))
        }
        Act::Flatten(n) => {
            let r = s.flatten_markups(&FlattenFilter {
                layers: vec![n.clone()],
                ..Default::default()
            });
            d.rerender(threads);
            actions::report(r, |k| format!("Flattened {} of {n}", actions::plural(k, "markup")))
        }
    };
}
