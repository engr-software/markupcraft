//! Forms: fill the document's form fields, add new fields (drag their box on the page), reset
//! and flatten them.

use egui::RichText;
use markupcraft_engine::forms::{FillValue, FormField, NewFieldKind};
use markupcraft_geom::Point;

use super::Pick;
use crate::{AppState, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewKind {
    #[default]
    Text,
    CheckBox,
    Combo,
    List,
    Button,
    Signature,
}

#[derive(Default)]
pub struct FormsState {
    pub open: bool,
    pub fields: Vec<FormField>,
    /// Edits not applied yet: field name -> value.
    pub edits: Vec<(String, FillValue)>,
    pub new_kind: NewKind,
    pub new_name: String,
    pub new_options: String,
    pub message: String,
}

pub fn open(app: &mut AppState) {
    refresh(app);
    app.features.forms.open = true;
}

fn refresh(app: &mut AppState) {
    let fields = app.doc().map(|d| d.session.form_fields()).unwrap_or_default();
    let f = &mut app.features.forms;
    f.fields = fields;
    f.edits.clear();
}

fn value_text(v: &[String]) -> String {
    v.join(", ")
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.forms.open {
        return;
    }
    let (mut open, mut apply, mut add, mut reset, mut flatten) = (true, false, false, false, false);
    super::window("Form Fields")
        .open(&mut open)
        .default_width(480.0)
        .show(ctx, |ui| {
            let f = &mut app.features.forms;
            if f.fields.is_empty() {
                ui.label(RichText::new("This document has no form fields.").weak());
            }
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                egui::Grid::new("form-fields")
                    .striped(true)
                    .num_columns(3)
                    .show(ui, |ui| {
                        for field in &f.fields {
                            ui.label(&field.name);
                            ui.label(RichText::new(field.kind).small());
                            let current = f.edits.iter().find(|(n, _)| *n == field.name).map(|(_, v)| v.clone());
                            let mut changed: Option<FillValue> = None;
                            ui.add_enabled_ui(!field.read_only, |ui| match field.kind {
                                "checkbox" | "radio" => {
                                    let mut on = match &current {
                                        Some(FillValue::Bool(b)) => *b,
                                        _ => field.value.iter().any(|v| !v.is_empty() && v != "Off"),
                                    };
                                    if ui.checkbox(&mut on, "").changed() {
                                        changed = Some(FillValue::Bool(on));
                                    }
                                }
                                "combo" | "list" => {
                                    let sel = match &current {
                                        Some(FillValue::Choice(v)) => value_text(v),
                                        _ => value_text(&field.value),
                                    };
                                    egui::ComboBox::from_id_salt(("form", &field.name))
                                        .selected_text(sel)
                                        .show_ui(ui, |ui| {
                                            for o in &field.options {
                                                if ui.selectable_label(false, o).clicked() {
                                                    changed = Some(FillValue::Choice(vec![o.clone()]));
                                                }
                                            }
                                        });
                                }
                                "signature" | "button" => {
                                    ui.label(RichText::new("-").weak());
                                }
                                _ => {
                                    let mut t = match &current {
                                        Some(FillValue::Text(t)) => t.clone(),
                                        _ => value_text(&field.value),
                                    };
                                    if ui.text_edit_singleline(&mut t).changed() {
                                        changed = Some(FillValue::Text(t));
                                    }
                                }
                            });
                            if let Some(v) = changed {
                                f.edits.retain(|(n, _)| *n != field.name);
                                f.edits.push((field.name.clone(), v));
                            }
                            ui.end_row();
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!f.edits.is_empty(), |ui| {
                    if ui.button("Apply Values").clicked() {
                        apply = true;
                    }
                });
                if ui.button("Reset All").clicked() {
                    reset = true;
                }
                if ui
                    .button("Flatten")
                    .on_hover_text("Draw the fields into the pages")
                    .clicked()
                {
                    flatten = true;
                }
            });
            ui.separator();
            ui.label(RichText::new("New field").strong());
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("form-new-kind")
                    .selected_text(format!("{:?}", f.new_kind))
                    .show_ui(ui, |ui| {
                        for k in [
                            NewKind::Text,
                            NewKind::CheckBox,
                            NewKind::Combo,
                            NewKind::List,
                            NewKind::Button,
                            NewKind::Signature,
                        ] {
                            ui.selectable_value(&mut f.new_kind, k, format!("{k:?}"));
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut f.new_name)
                        .hint_text("name")
                        .desired_width(120.0),
                );
                if matches!(f.new_kind, NewKind::Combo | NewKind::List) {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.new_options)
                            .hint_text("options, comma separated")
                            .desired_width(150.0),
                    );
                }
                if ui
                    .button("Draw...")
                    .on_hover_text("Drag the field's box on the page")
                    .clicked()
                {
                    add = true;
                }
            });
            if !f.message.is_empty() {
                ui.label(RichText::new(&f.message).small());
            }
        });
    if !open {
        app.features.forms.open = false;
    }
    let threads = app.threads;
    if apply {
        let edits = std::mem::take(&mut app.features.forms.edits);
        if let Some(d) = app.doc_mut() {
            let r = d.session.form_fill(&edits);
            d.rerender(threads);
            app.features.forms.message = actions::report(r, |n| format!("Filled {}", actions::plural(n, "field")));
        }
        refresh(app);
    }
    if reset || flatten {
        if let Some(d) = app.doc_mut() {
            let r = if reset {
                d.session.form_reset(None)
            } else {
                let all: Vec<usize> = (0..d.session.page_count()).collect();
                d.session.form_flatten(&all)
            };
            d.rerender(threads);
            app.features.forms.message = actions::report(r, |n| {
                format!(
                    "{} {}",
                    if reset { "Reset" } else { "Flattened" },
                    actions::plural(n, "field")
                )
            });
        }
        refresh(app);
    }
    if add {
        super::start_pick(app, Pick::FormField, "Drag the new field's box");
    }
}

/// The new field's box was dragged.
pub fn rect_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    let f = &app.features.forms;
    let options: Vec<String> = f
        .new_options
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let kind = match f.new_kind {
        NewKind::Text => NewFieldKind::Text { multiline: false },
        NewKind::CheckBox => NewFieldKind::CheckBox,
        NewKind::Combo => NewFieldKind::Combo {
            options,
            editable: false,
        },
        NewKind::List => NewFieldKind::List { options, multi: false },
        NewKind::Button => NewFieldKind::Button {
            caption: f.new_name.clone(),
        },
        NewKind::Signature => NewFieldKind::Signature,
    };
    let name = f.new_name.trim().to_string();
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let res = d
        .session
        .form_add_field(page, r, &kind, (!name.is_empty()).then_some(name.as_str()));
    d.rerender(threads);
    app.features.forms.message = actions::report(res, |n| format!("Added field {n}"));
    app.features.forms.open = true;
    refresh(app);
}
