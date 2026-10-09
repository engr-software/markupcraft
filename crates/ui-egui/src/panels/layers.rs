//! Layers (Alt+Y): the document's markup layers (PDF optional content groups) with view, print,
//! export and lock states and markup counts, as a tree (drag a layer onto another to nest it);
//! new, rename, delete, isolate, show all, assign the selected markups, select a layer's
//! markups, flatten a layer; saved configurations; the layers on this page only or A-Z;
//! previewing the print or export layers; importing a PDF page as a layer and exporting one.

use egui::{RichText, Sense};
use markupcraft_engine::flatten::FlattenFilter;
use markupcraft_engine::layers::LayerState;
use markupcraft_engine::layers_more::LayerPreview;

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
    pub page_only: bool,
    pub alphabetical: bool,
    pub config_name: String,
    /// A print or export preview is showing.
    pub previewing: bool,
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
    Nest(String, Option<String>),
    Export(String, bool),
    SaveConfig(String),
    ApplyConfig(String),
    DeleteConfig(String),
    Preview(Option<LayerPreview>),
    Import,
    ExportLayer(String),
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let page_only_pref = app.shell.prefs.more.layers.current_page_only;
    let mut layers = d.session.layers();
    let tree = d.session.layer_tree();
    let configs = d.session.layer_configs();
    let on_page = d.session.layers_on_page(d.view.current).unwrap_or_default();
    let exports: std::collections::HashMap<String, bool> = layers
        .iter()
        .map(|l| (l.name.clone(), d.session.layer_export(&l.name).unwrap_or(true)))
        .collect();
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
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut f.page_only, "This page only");
        ui.checkbox(&mut f.alphabetical, "A-Z");
        if f.previewing {
            if ui.button("End Preview").clicked() {
                act = Some(Act::Preview(None));
            }
        } else {
            if ui
                .button("Print Layers")
                .on_hover_text("Show only the layers that print")
                .clicked()
            {
                act = Some(Act::Preview(Some(LayerPreview::Print)));
            }
            if ui
                .button("Export Layers")
                .on_hover_text("Show only the layers that export")
                .clicked()
            {
                act = Some(Act::Preview(Some(LayerPreview::Export)));
            }
        }
        if ui
            .button("Import...")
            .on_hover_text("A page of another PDF as a layer")
            .clicked()
        {
            act = Some(Act::Import);
        }
    });
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("layer-configs")
            .selected_text("Configurations")
            .show_ui(ui, |ui| {
                for c in &configs {
                    if ui.selectable_label(false, c).clicked() {
                        act = Some(Act::ApplyConfig(c.clone()));
                    }
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut f.config_name)
                .hint_text("configuration")
                .desired_width(100.0),
        );
        if ui.small_button("Save").clicked() && !f.config_name.trim().is_empty() {
            act = Some(Act::SaveConfig(f.config_name.trim().to_string()));
        }
        if ui.small_button("Delete").clicked() && configs.contains(&f.config_name.trim().to_string()) {
            act = Some(Act::DeleteConfig(f.config_name.trim().to_string()));
        }
    });
    // Tree order (depth per layer), or A-Z; this page's layers only when asked.
    let depth_of = |n: &str| tree.iter().find(|t| t.name == n).map_or(0, |t| t.depth);
    if f.alphabetical {
        layers.sort_by_key(|l| l.name.to_lowercase());
    } else {
        layers.sort_by_key(|l| tree.iter().position(|t| t.name == l.name).unwrap_or(usize::MAX));
    }
    if f.page_only || page_only_pref {
        layers.retain(|l| on_page.contains(&l.name));
    }
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
            .num_columns(6)
            .show(ui, |ui| {
                ui.label(RichText::new("View").small());
                ui.label(RichText::new("Print").small());
                ui.label(RichText::new("Export").small());
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
                    let mut ex = exports.get(&l.name).copied().unwrap_or(true);
                    if ui.checkbox(&mut ex, "").on_hover_text("Export").changed() {
                        act = Some(Act::Export(l.name.clone(), ex));
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
                    let depth = if f.alphabetical { 0 } else { depth_of(&l.name) };
                    let label = format!("{}{}", "    ".repeat(depth.min(8)), l.name);
                    let r = ui.add(egui::Button::selectable(sel, label).sense(Sense::click_and_drag()));
                    r.dnd_set_drag_payload(l.name.clone());
                    if let Some(dragged) = r.dnd_release_payload::<String>()
                        && *dragged != l.name
                    {
                        act = Some(Act::Nest((*dragged).clone(), Some(l.name.clone())));
                    }
                    if r.clicked() {
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
                if ui.button("Top Level").on_hover_text("Out of its parent").clicked() {
                    act = Some(Act::Nest(name.clone(), None));
                }
                if ui
                    .button("Export...")
                    .on_hover_text("This layer alone as a PDF")
                    .clicked()
                {
                    act = Some(Act::ExportLayer(name.clone()));
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
    if let Some(Act::Preview(p)) = &act {
        f.previewing = p.is_some();
    }
    app.features.layers = f;
    let Some(act) = act else { return };
    match &act {
        Act::Import => {
            app.dialogs.open(
                crate::dialogs::Purpose::Feature(crate::features::Ask::LayerImport),
                crate::dialogs::PDF,
                false,
            );
            return;
        }
        Act::ExportLayer(n) => {
            app.dialogs.save(
                crate::dialogs::Purpose::Feature(crate::features::Ask::LayerExport(n.clone())),
                crate::dialogs::PDF,
                &format!("{n}.pdf"),
            );
            return;
        }
        _ => {}
    }
    // Preferences > Layers: hiding or showing a layer does the same to its child layers.
    let children: Vec<String> = match &act {
        Act::State(n, st) if st.visible.is_some() => crate::features::more6::prefs::layer_children(app, n),
        _ => Vec::new(),
    };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let s = &mut d.session;
    if let Act::State(_, st) = &act {
        for c in &children {
            let _ = s.set_layer_state(c, *st);
        }
    }
    let rerender = !matches!(act, Act::Create(_) | Act::Select(_) | Act::Assign(_));
    let status = match act {
        Act::Nest(n, p) => actions::report(s.nest_layer(&n, p.as_deref(), None), |_| match &p {
            Some(p) => format!("{n} is now under {p}"),
            None => format!("{n} is now at the top level"),
        }),
        Act::Export(n, on) => actions::report(s.set_layer_export(&n, on), |_| format!("Layer {n} export changed")),
        Act::SaveConfig(n) => actions::report(s.save_layer_config(&n), |_| format!("Configuration {n} saved")),
        Act::ApplyConfig(n) => actions::report(s.apply_layer_config(&n), |_| format!("Configuration {n}")),
        Act::DeleteConfig(n) => actions::report(s.delete_layer_config(&n), |_| format!("Configuration {n} deleted")),
        Act::Preview(Some(p)) => actions::report(s.preview_layers(p), |_| {
            "Previewing; End Preview restores the view".into()
        }),
        Act::Preview(None) => actions::report(s.end_layer_preview(), |_| "Preview ended".into()),
        Act::Import | Act::ExportLayer(_) => String::new(),
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
    // Page content on layers shows or hides with them.
    if rerender {
        d.rerender(threads);
    }
    app.status = status;
}
