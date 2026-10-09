//! More of the Form Fields window: Highlight Fields (every field tinted on the page), Lay Out
//! XFA Form, and a field's Properties (name, tooltip, required, read-only, multiline, most
//! characters, default value) and Actions (by trigger: a web link, a page, a named menu
//! command, reset, show / hide, JavaScript, or none).

use egui::{Color32, RichText};
use markupcraft_engine::finish::forms_more::{FieldAction, FieldEdit, Trigger};

use super::Mark;
use crate::{AppState, DocTab, actions};

#[derive(Default)]
pub struct FormsMore {
    pub highlight: bool,
    /// The field whose properties are shown, and the edit buffers.
    pub field: Option<String>,
    pub name: String,
    pub tooltip: String,
    pub required: bool,
    pub read_only: bool,
    pub multiline: bool,
    pub max_len: String,
    pub default_value: String,
    /// Actions: the trigger (index into `Trigger::ALL`), the action kind and its value.
    pub trigger: usize,
    pub action: usize,
    pub action_value: String,
    pub actions_text: Vec<String>,
}

const ACTIONS: [&str; 7] = [
    "None",
    "Open a web link",
    "Go to a page",
    "Run a menu command",
    "Reset the form",
    "Show / hide fields",
    "Run a JavaScript",
];

impl FormsMore {
    /// Every field's box, tinted, while Highlight Fields is on.
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        if !self.highlight {
            return;
        }
        for f in d.session.form_fields() {
            if let (Some(p), Some(r)) = (f.page, f.rect) {
                out.push(Mark::rect(
                    p,
                    r,
                    Color32::from_rgba_unmultiplied(120, 150, 255, 70),
                    Color32::from_rgb(60, 90, 220),
                ));
            }
        }
    }
}

/// What the rows ask for.
pub enum Act {
    Xfa,
    Select(String),
    Props,
    SetAction,
}

/// The extra rows at the end of the Form Fields window.
pub fn rows(ui: &mut egui::Ui, m: &mut FormsMore, names: &[String], xfa: bool) -> Option<Act> {
    let mut act = None;
    ui.separator();
    ui.horizontal(|ui| {
        ui.checkbox(&mut m.highlight, "Highlight fields");
        if xfa
            && ui
                .button("Lay Out XFA Form")
                .on_hover_text("Turn the dynamic XFA form into pages and fields that can be filled")
                .clicked()
        {
            act = Some(Act::Xfa);
        }
    });
    ui.label(RichText::new("Field properties").strong());
    egui::ComboBox::from_id_salt("form-props-field")
        .selected_text(m.field.clone().unwrap_or_else(|| "Choose a field".into()))
        .show_ui(ui, |ui| {
            for n in names {
                if ui.selectable_label(m.field.as_deref() == Some(n.as_str()), n).clicked() {
                    act = Some(Act::Select(n.clone()));
                }
            }
        });
    if m.field.is_none() {
        return act;
    }
    egui::Grid::new("form-props").num_columns(2).show(ui, |ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut m.name);
        ui.end_row();
        ui.label("Tooltip");
        ui.text_edit_singleline(&mut m.tooltip);
        ui.end_row();
        ui.label("Most characters");
        ui.add(egui::TextEdit::singleline(&mut m.max_len).hint_text("no limit"));
        ui.end_row();
        ui.label("Default value");
        ui.text_edit_singleline(&mut m.default_value);
        ui.end_row();
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut m.required, "Required");
        ui.checkbox(&mut m.read_only, "Read-only");
        ui.checkbox(&mut m.multiline, "Multiline");
    });
    if ui.button("Apply Properties").clicked() {
        act = Some(Act::Props);
    }
    ui.label(RichText::new("Actions").strong());
    for a in &m.actions_text {
        ui.label(RichText::new(a).small());
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("form-trigger")
            .selected_text(Trigger::ALL.get(m.trigger).map_or("", |t| t.label()))
            .show_ui(ui, |ui| {
                for (i, t) in Trigger::ALL.iter().enumerate() {
                    ui.selectable_value(&mut m.trigger, i, t.label());
                }
            });
        egui::ComboBox::from_id_salt("form-action")
            .selected_text(ACTIONS.get(m.action).copied().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (i, a) in ACTIONS.iter().enumerate() {
                    ui.selectable_value(&mut m.action, i, *a);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut m.action_value).hint_text("link, page number, command or field names"));
        if ui.button("Set Action").clicked() {
            act = Some(Act::SetAction);
        }
    });
    act
}

fn load(app: &mut AppState, name: &str) {
    let Some(d) = app.doc() else { return };
    let f = d.session.form_fields().into_iter().find(|f| f.name == name);
    let acts = d.session.form_actions(name).unwrap_or_default();
    let m = &mut app.features.forms.more;
    m.field = Some(name.to_string());
    m.name = name.to_string();
    if let Some(f) = f {
        m.required = f.required;
        m.read_only = f.read_only;
    }
    m.actions_text = acts
        .iter()
        .map(|(t, a)| format!("{}: {}", t.label(), a.describe()))
        .collect();
}

/// Run what the rows asked for.
pub fn apply(app: &mut AppState, act: Act) {
    let threads = app.threads;
    match act {
        Act::Select(n) => load(app, &n),
        Act::Xfa => {
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.xfa_render();
            d.sync_pages(threads);
            d.rerender(threads);
            app.features.forms.message = actions::report(r, |(p, f, w)| {
                let mut s = format!(
                    "Laid out {} with {}",
                    actions::plural(p, "page"),
                    actions::plural(f, "field")
                );
                if !w.is_empty() {
                    s.push_str(&format!(" ({})", w.join("; ")));
                }
                s
            });
            super::forms::open(app);
        }
        Act::Props => {
            let m = &app.features.forms.more;
            let Some(old) = m.field.clone() else { return };
            let max = m.max_len.trim();
            let e = FieldEdit {
                name: (m.name.trim() != old && !m.name.trim().is_empty()).then(|| m.name.trim().to_string()),
                tooltip: Some(m.tooltip.clone()),
                required: Some(m.required),
                read_only: Some(m.read_only),
                multiline: Some(m.multiline),
                max_len: Some(if max.is_empty() { None } else { max.parse().ok() }),
                default_value: Some((!m.default_value.is_empty()).then(|| m.default_value.clone())),
                ..Default::default()
            };
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.form_set_props(&old, &e);
            d.rerender(threads);
            match r {
                Ok(n) => {
                    app.features.forms.message = format!("Properties of {n} set");
                    super::forms::open(app);
                    load(app, &n);
                }
                Err(e) => app.features.forms.message = e.to_string(),
            }
        }
        Act::SetAction => {
            let m = &app.features.forms.more;
            let Some(name) = m.field.clone() else { return };
            let Some(trigger) = Trigger::ALL.get(m.trigger).copied() else {
                return;
            };
            let v = m.action_value.trim().to_string();
            let list = |s: &str| -> Vec<String> {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            };
            let action = match m.action {
                1 => Some(FieldAction::Uri(v)),
                2 => Some(FieldAction::GoTo(v.parse::<usize>().unwrap_or(1).saturating_sub(1))),
                3 => Some(FieldAction::Named(v)),
                4 => Some(FieldAction::Reset(list(&v))),
                5 => Some(FieldAction::ShowHide {
                    fields: list(&v),
                    hide: true,
                }),
                6 => Some(FieldAction::JavaScript(v)),
                _ => None,
            };
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.form_set_action(&name, trigger, action);
            app.features.forms.message = actions::report(r, |n| format!("{name}: {}", actions::plural(n, "action")));
            load(app, &name);
        }
    }
}
