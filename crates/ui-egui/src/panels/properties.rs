//! Properties (Alt+P): the selected markups' fields, grouped like Revu's panel (General,
//! Appearance, Text, Measurement, Layout, Options). A change applies to every selected markup
//! MarkupCraft can write; a slider drag or a typed word is one undo step (the engine merges a
//! gesture's edits). With several markups selected, a field whose values differ shows "Mixed"
//! until it is set. With nothing selected: the page and its scale.

use egui::{Grid, RichText};
use markupcraft_engine::MarkupPatch;
use markupcraft_geom::shapes::LINE_ENDINGS;
use markupcraft_measure::units::{
    display_unit_choices, precision_choices, scale_display_unit, scale_precision, scale_presets, set_display_unit,
    set_precision,
};
use markupcraft_model::hatch::{Hatch, HatchStyle};
use markupcraft_model::{Color, CountSymbol, Kind, Markup, flags, review_statuses};

use super::{PanelDef, Slot};
use crate::actions::{self, box_of, editable};
use crate::commands::alt;
use crate::theme::Tokens;
use crate::{AppState, DocTab};

pub static PANEL: PanelDef = PanelDef {
    id: "properties",
    title: "Properties",
    icon: "sliders-horizontal",
    slot: Slot::Right,
    keys: alt(egui::Key::P),
    ui,
};

/// Line styles: name and dash array (points).
pub const LINE_STYLES: &[(&str, &[f64])] = &[
    ("Solid", &[]),
    ("Dashed", &[6.0, 3.0]),
    ("Dotted", &[1.0, 2.0]),
    ("Dash Dot", &[6.0, 3.0, 1.0, 3.0]),
    ("Long Dash", &[12.0, 4.0]),
];

pub const FONTS: &[&str] = &["Helvetica", "Times", "Courier"];

pub const NOTE_ICONS: &[&str] = &["Comment", "Note", "Key", "Help", "NewParagraph", "Paragraph", "Insert"];

/// What a field shows when the selected markups disagree.
pub const MIXED: &str = "Mixed";

/// The name of a dash array.
pub fn line_style_name(dash: &[f64]) -> &'static str {
    LINE_STYLES
        .iter()
        .find(|(_, d)| *d == dash)
        .map_or(if dash.is_empty() { "Solid" } else { "Custom" }, |(n, _)| n)
}

/// Edits gathered while the panel draws, applied after: (merge key, patch).
type Edits = Vec<(&'static str, MarkupPatch)>;

/// Changes that are not a property patch (geometry, hatch, layer), applied after drawing.
#[derive(Debug, Clone, PartialEq)]
enum Change {
    Hatch(Option<Hatch>),
    Layer(String),
    Move(f64, f64),
    Resize(markupcraft_geom::Rect),
    Rotate(f64),
    Symbol(Vec<Vec<markupcraft_geom::Point>>),
}

/// The selected markups the panel describes.
struct Sel<'a> {
    first: &'a Markup,
    all: &'a [Markup],
}

impl Sel<'_> {
    /// The selected markups disagree on this value.
    fn mixed<T: PartialEq>(&self, f: impl Fn(&Markup) -> T) -> bool {
        let v = f(self.first);
        self.all.iter().any(|m| f(m) != v)
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut actions_out: Vec<&'static str> = Vec::new();
    let user_styles = app.toolchest.extras.line_styles.clone();
    let custom_statuses = app.toolchest.extras.statuses.clone();
    let mut new_status: Option<String> = None;
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let ids = doc.selection().to_vec();
        let all: Vec<Markup> = ids
            .iter()
            .filter_map(|id| doc.session.doc().find(id))
            .cloned()
            .collect();
        let Some(m) = all.first().cloned() else {
            page_info(ui, doc, &t);
            return;
        };
        let sel = Sel { first: &m, all: &all };
        let n = ids.len();
        let title = if n == 1 {
            m.kind.name().to_string()
        } else if !sel.mixed(|m| m.kind) {
            format!("{n} {} markups", m.kind.name())
        } else {
            format!("{n} markups")
        };
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().size(14.0));
        let targets: Vec<String> = ids
            .iter()
            .filter(|id| doc.session.doc().find(id).is_some_and(|m| editable(m) && !m.locked()))
            .cloned()
            .collect();
        let writable = !targets.is_empty();
        if !writable {
            let why = if all.iter().any(Markup::locked) {
                "Locked: unlock it to change its properties."
            } else {
                "Shown from the file. Editing this kind comes with its writer."
            };
            ui.label(RichText::new(why).color(t.text_faint).size(11.0));
        }
        ui.add_space(4.0);
        let mut edits: Edits = Vec::new();
        let mut changes: Vec<Change> = Vec::new();
        let mut reply_acts: Vec<super::properties_more::ReplyAct> = Vec::new();
        let layers: Vec<String> = doc.session.layers().into_iter().map(|l| l.name).collect();
        let columns = doc.session.doc().columns.clone();
        ui.add_enabled_ui(writable, |ui| {
            general(ui, &sel, &layers, &mut edits, &mut changes);
            super::properties_more::custom_status(ui, &sel_status(&sel), &custom_statuses, &mut edits, &mut new_status);
            appearance(ui, &sel, &mut edits, &mut changes, &user_styles, &mut actions_out);
            if m.kind.is_text() || m.kind == Kind::Stamp || m.kind.is_measurement() {
                text(ui, &sel, &mut edits);
            }
            if m.kind.is_measurement() {
                measurement(ui, &sel, &mut edits, &mut changes);
            }
            if m.kind == Kind::Note {
                note(ui, &sel, &mut edits);
            }
            super::properties_more::sections(ui, &m, &all, &columns, &mut edits);
            if n == 1 {
                super::properties_more::replies(ui, &m, &mut reply_acts);
            }
            layout(ui, &sel, &mut changes);
        });
        egui::CollapsingHeader::new("Options")
            .default_open(true)
            .show(ui, |ui| {
                let mut locked = m.locked();
                if ui.checkbox(&mut locked, "Locked").changed() {
                    let r = doc.session.set_locked(&ids, locked);
                    app_status(doc, r.map(|_| ()));
                }
                ui.add_enabled_ui(writable, |ui| {
                    for (label, bit, key) in [
                        ("Print", flags::PRINT, "print"),
                        ("Hidden", flags::HIDDEN, "hidden"),
                        ("No View", flags::NO_VIEW, "no-view"),
                    ] {
                        let mut on = m.flags & bit != 0;
                        let text = if sel.mixed(|x| x.flags & bit != 0) {
                            format!("{label} ({MIXED})")
                        } else {
                            label.to_string()
                        };
                        if ui.checkbox(&mut on, text).changed() {
                            let mut p = MarkupPatch::default();
                            match key {
                                "print" => p.print = Some(on),
                                "hidden" => p.hidden = Some(on),
                                _ => p.no_view = Some(on),
                            }
                            edits.push((key, p));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(n == 1, egui::Button::new("Set as Default")).clicked() {
                        actions_out.push("markup.set_default");
                    }
                    if ui.add_enabled(n == 1, egui::Button::new("Add to Tool Chest")).clicked() {
                        actions_out.push("markup.add_to_toolchest");
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(writable && n == 1, egui::Button::new("Format Painter"))
                        .clicked()
                    {
                        actions_out.push("markup.format_painter");
                    }
                });
            });
        apply(doc, &targets, edits);
        apply_changes(doc, &targets, changes);
        super::properties_more::apply_replies(doc, &m.id, reply_acts);
    });
    for a in actions_out {
        app.queue(a);
    }
    if let Some(st) = new_status {
        app.toolchest.add_status(&st);
    }
}

fn app_status(_doc: &mut DocTab, r: markupcraft_engine::Result<()>) {
    if let Err(e) = r {
        log::info!("properties: {e}");
    }
}

/// Apply the gathered edits; a gesture under one key is one undo step.
fn apply(doc: &mut DocTab, targets: &[String], edits: Edits) {
    if targets.is_empty() {
        return;
    }
    for (key, patch) in edits {
        doc.session.set_merge_key(Some(key));
        let r = doc.session.set_properties(targets, &patch);
        doc.session.set_merge_key(None);
        if let Err(e) = r {
            log::info!("properties: {e}");
        }
    }
}

fn apply_changes(doc: &mut DocTab, targets: &[String], changes: Vec<Change>) {
    if targets.is_empty() {
        return;
    }
    for c in changes {
        let r = match c {
            Change::Hatch(h) => {
                doc.session.set_merge_key(Some("hatch"));
                let hatchable: Vec<String> = targets
                    .iter()
                    .filter(|id| {
                        doc.session
                            .doc()
                            .find(id)
                            .is_some_and(|m| markupcraft_revu::hatch::hatchable(m.kind))
                    })
                    .cloned()
                    .collect();
                doc.session.set_hatch(&hatchable, h).map(|_| ())
            }
            Change::Layer(name) => doc.session.assign_layer(targets, &name).map(|_| ()),
            Change::Move(dx, dy) => {
                doc.session.set_merge_key(Some("layout-move"));
                doc.session.move_markups(targets, dx, dy).map(|_| ())
            }
            Change::Resize(r) => {
                doc.session.set_merge_key(Some("layout-size"));
                match targets.first() {
                    Some(id) => doc.session.resize_markup(id, r),
                    None => Ok(()),
                }
            }
            Change::Rotate(deg) => doc.session.rotate_markups(targets, deg, None).map(|_| ()),
            Change::Symbol(paths) => {
                let counts: Vec<String> = targets
                    .iter()
                    .filter(|id| doc.session.doc().find(id).is_some_and(|m| m.kind == Kind::Count))
                    .cloned()
                    .collect();
                let p = MarkupPatch {
                    symbol_paths: Some(paths),
                    ..Default::default()
                };
                doc.session.set_properties(&counts, &p).map(|_| ())
            }
        };
        doc.session.set_merge_key(None);
        if let Err(e) = r {
            log::info!("properties: {e}");
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
        Grid::new(id)
            .num_columns(2)
            .spacing([10.0, 5.0])
            .min_col_width(78.0)
            .show(ui, body);
    });
}

/// A row label; mixed values add a faint "Mixed" note under it.
fn label(ui: &mut egui::Ui, text: &str, mixed: bool) {
    if mixed {
        ui.vertical(|ui| {
            ui.label(text);
            ui.label(RichText::new(MIXED).weak().italics().size(10.0));
        });
    } else {
        ui.label(text);
    }
}

#[allow(clippy::too_many_arguments)]
fn text_row(
    ui: &mut egui::Ui,
    label_text: &str,
    sel: &Sel<'_>,
    get: fn(&Markup) -> &str,
    key: &'static str,
    edits: &mut Edits,
    set: fn(String) -> MarkupPatch,
) {
    let mixed = sel.mixed(|m| get(m).to_string());
    label(ui, label_text, mixed);
    let mut v = if mixed {
        String::new()
    } else {
        get(sel.first).to_string()
    };
    let mut te = egui::TextEdit::singleline(&mut v).desired_width(f32::INFINITY);
    if mixed {
        te = te.hint_text(MIXED);
    }
    if ui.add(te).changed() {
        edits.push((key, set(v)));
    }
    ui.end_row();
}

fn color_button(ui: &mut egui::Ui, c: &Color) -> Option<Color> {
    let mut rgb = [c.r as f32, c.g as f32, c.b as f32];
    ui.color_edit_button_rgb(&mut rgb)
        .changed()
        .then(|| Color::rgb(f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2])))
}

/// The status shown for the selection ("" = none, `MIXED` when they differ).
fn sel_status(sel: &Sel<'_>) -> String {
    if sel.mixed(|m| m.status.clone()) {
        MIXED.to_string()
    } else {
        sel.first.status.clone()
    }
}

fn general(ui: &mut egui::Ui, sel: &Sel<'_>, layers: &[String], edits: &mut Edits, changes: &mut Vec<Change>) {
    let m = sel.first;
    section(ui, "General", "props-general", |ui| {
        text_row(
            ui,
            "Subject",
            sel,
            |m| &m.subject,
            "subject",
            edits,
            |v| MarkupPatch {
                subject: Some(v),
                ..Default::default()
            },
        );
        text_row(
            ui,
            "Label",
            sel,
            |m| &m.label,
            "label",
            edits,
            |v| MarkupPatch {
                label: Some(v),
                ..Default::default()
            },
        );
        text_row(
            ui,
            "Author",
            sel,
            |m| &m.author,
            "author",
            edits,
            |v| MarkupPatch {
                author: Some(v),
                ..Default::default()
            },
        );
        if !m.kind.is_measurement() && !m.kind.is_text() {
            text_row(
                ui,
                "Comments",
                sel,
                |m| &m.contents,
                "contents",
                edits,
                |v| MarkupPatch {
                    contents: Some(v),
                    ..Default::default()
                },
            );
        }
        // Layer: one of the document's layers, a new name, or none.
        let mixed = sel.mixed(|m| m.layer.clone());
        label(ui, "Layer", mixed);
        let shown = if mixed {
            MIXED.to_string()
        } else if m.layer.is_empty() {
            "None".to_string()
        } else {
            m.layer.clone()
        };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("prop-layer")
                .selected_text(shown)
                .width(110.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(m.layer.is_empty() && !mixed, "None").clicked() {
                        changes.push(Change::Layer(String::new()));
                    }
                    for l in layers {
                        if ui.selectable_label(!mixed && m.layer == *l, l).clicked() {
                            changes.push(Change::Layer(l.clone()));
                        }
                    }
                });
            let id = ui.id().with("new-layer");
            let mut name: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .hint_text("New layer")
                    .desired_width(80.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty() {
                changes.push(Change::Layer(name.trim().to_string()));
                name.clear();
            }
            ui.data_mut(|d| d.insert_temp(id, name));
        });
        ui.end_row();
        let mixed = sel.mixed(|m| m.status.clone());
        label(ui, "Status", mixed);
        let current = if mixed {
            MIXED
        } else if m.status.is_empty() {
            "None"
        } else {
            m.status.as_str()
        };
        egui::ComboBox::from_id_salt("prop-status")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for st in review_statuses() {
                    if ui.selectable_label(current == *st, *st).clicked() {
                        edits.push((
                            "status",
                            MarkupPatch {
                                status: Some(st.to_string()),
                                ..Default::default()
                            },
                        ));
                    }
                }
            });
        ui.end_row();
        let mixed = sel.mixed(|m| m.checked);
        label(ui, "Checkmark", mixed);
        let mut c = m.checked;
        if ui.checkbox(&mut c, "").changed() {
            edits.push((
                "checked",
                MarkupPatch {
                    checked: Some(c),
                    ..Default::default()
                },
            ));
        }
        ui.end_row();
        label(ui, "Date", sel.mixed(|m| m.modified.clone()));
        let date = markupcraft_model::table::revu_date(&m.modified);
        ui.label(match (date.is_empty(), m.in_file()) {
            (false, _) => date,
            (true, true) => "Not recorded".to_string(),
            (true, false) => "Not saved yet".to_string(),
        });
        ui.end_row();
        if !m.created.is_empty() {
            ui.label("Created");
            ui.label(markupcraft_model::table::revu_date(&m.created));
            ui.end_row();
        }
        ui.label("Replies");
        let last = m
            .state_replies
            .last()
            .map(|r| r.state.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        ui.label(if last.is_empty() {
            format!("{}", m.replies.len())
        } else {
            format!("{} ({last})", m.replies.len())
        });
        ui.end_row();
    });
}

fn appearance(
    ui: &mut egui::Ui,
    sel: &Sel<'_>,
    edits: &mut Edits,
    changes: &mut Vec<Change>,
    user_styles: &[crate::chest_more::LineStyle],
    actions_out: &mut Vec<&'static str>,
) {
    let m = sel.first;
    section(ui, "Appearance", "props-appearance", |ui| {
        label(ui, "Color", sel.mixed(|m| m.color));
        if let Some(c) = color_button(ui, &m.color) {
            edits.push((
                "color",
                MarkupPatch {
                    color: Some(c),
                    ..Default::default()
                },
            ));
        }
        ui.end_row();

        let fillable = markupcraft_revu::kinds::kind_for(m.kind).closed
            || m.fill.is_some()
            || matches!(m.kind, Kind::Text | Kind::Callout | Kind::Stamp);
        if fillable {
            label(ui, "Fill", sel.mixed(|m| m.fill));
            ui.horizontal(|ui| {
                let mut on = m.fill.is_some();
                if ui.checkbox(&mut on, "").changed() {
                    edits.push((
                        "fill-on",
                        MarkupPatch {
                            fill: Some(on.then_some(m.fill.unwrap_or(m.color))),
                            ..Default::default()
                        },
                    ));
                }
                if let Some(f) = &m.fill
                    && let Some(c) = color_button(ui, f)
                {
                    edits.push((
                        "fill",
                        MarkupPatch {
                            fill: Some(Some(c)),
                            ..Default::default()
                        },
                    ));
                }
            });
            ui.end_row();
            label(
                ui,
                "Fill opacity",
                sel.mixed(|m| (m.fill_opacity * 100.0).round() as i64),
            );
            let mut fo = (m.fill_opacity * 100.0).round();
            if ui.add(egui::Slider::new(&mut fo, 0.0..=100.0).suffix(" %")).changed() {
                edits.push((
                    "fill-opacity",
                    MarkupPatch {
                        fill_opacity: Some(fo / 100.0),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }

        label(ui, "Opacity", sel.mixed(|m| (m.opacity * 100.0).round() as i64));
        let mut op = (m.opacity * 100.0).round();
        if ui.add(egui::Slider::new(&mut op, 0.0..=100.0).suffix(" %")).changed() {
            edits.push((
                "opacity",
                MarkupPatch {
                    opacity: Some(op / 100.0),
                    ..Default::default()
                },
            ));
        }
        ui.end_row();

        let mixed = sel.mixed(|m| m.multiply);
        label(ui, "Blend mode", mixed);
        let current = if mixed {
            MIXED
        } else if m.multiply {
            "Multiply"
        } else {
            "Normal"
        };
        egui::ComboBox::from_id_salt("prop-blend")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (name, on) in [("Normal", false), ("Multiply", true)] {
                    if ui.selectable_label(current == name, name).clicked() {
                        edits.push((
                            "blend",
                            MarkupPatch {
                                multiply: Some(on),
                                ..Default::default()
                            },
                        ));
                    }
                }
            });
        ui.end_row();

        if !matches!(m.kind, Kind::TextHighlight | Kind::Note | Kind::Snapshot) {
            label(ui, "Line width", sel.mixed(|m| m.line_width.to_bits()));
            let mut w = m.line_width;
            if ui
                .add(egui::DragValue::new(&mut w).range(0.0..=72.0).speed(0.1).suffix(" pt"))
                .changed()
            {
                edits.push((
                    "width",
                    MarkupPatch {
                        line_width: Some(w),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();

            let mixed = sel.mixed(|m| m.dash.iter().map(|d| d.to_bits()).collect::<Vec<_>>());
            label(ui, "Line style", mixed);
            let user = user_styles.iter().find(|s| s.dash == m.dash).map(|s| s.name.as_str());
            let current = if mixed {
                MIXED
            } else if let (Some(u), "Custom") = (user, line_style_name(&m.dash)) {
                u
            } else {
                line_style_name(&m.dash)
            };
            egui::ComboBox::from_id_salt("prop-dash")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let dashes = LINE_STYLES
                        .iter()
                        .map(|(n, d)| (*n, d.to_vec()))
                        .chain(user_styles.iter().map(|s| (s.name.as_str(), s.dash.clone())));
                    for (name, dash) in dashes {
                        if ui.selectable_label(current == name, name).clicked() {
                            edits.push((
                                "dash",
                                MarkupPatch {
                                    dash: Some(dash),
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                    ui.separator();
                    if ui.button("Manage Line Styles...").clicked() {
                        actions_out.push("markup.line_styles");
                    }
                });
            ui.end_row();
        }

        if matches!(
            m.kind,
            Kind::Line | Kind::Arrow | Kind::Polyline | Kind::Length | Kind::Polylength | Kind::Callout
        ) {
            if m.kind != Kind::Callout {
                ending_row(
                    ui,
                    "Start",
                    sel,
                    |m| &m.line_start,
                    "start",
                    edits,
                    |v| MarkupPatch {
                        line_start: Some(v),
                        ..Default::default()
                    },
                );
            }
            ending_row(
                ui,
                "End",
                sel,
                |m| &m.line_end,
                "end",
                edits,
                |v| MarkupPatch {
                    line_end: Some(v),
                    ..Default::default()
                },
            );
        }
        if matches!(
            m.kind,
            Kind::Cloud | Kind::Polygon | Kind::Area | Kind::Perimeter | Kind::Rectangle
        ) {
            label(ui, "Cloud", sel.mixed(|m| m.cloud.to_bits()));
            let mut c = m.cloud;
            if ui.add(egui::Slider::new(&mut c, 0.0..=2.0).step_by(0.1)).changed() {
                edits.push((
                    "cloud",
                    MarkupPatch {
                        cloud: Some(c),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
        if markupcraft_revu::hatch::hatchable(m.kind) {
            hatch_rows(ui, sel, changes);
        }
    });
}

fn hatch_rows(ui: &mut egui::Ui, sel: &Sel<'_>, changes: &mut Vec<Change>) {
    let m = sel.first;
    let mixed = sel.mixed(|m| m.hatch.map(|h| h.style));
    label(ui, "Hatch", mixed);
    let current = if mixed {
        MIXED.to_string()
    } else {
        m.hatch.map_or("None".to_string(), |h| h.style.name().to_string())
    };
    egui::ComboBox::from_id_salt("prop-hatch")
        .selected_text(current)
        .show_ui(ui, |ui| {
            if ui.selectable_label(m.hatch.is_none(), "None").clicked() {
                changes.push(Change::Hatch(None));
            }
            for s in HatchStyle::ALL {
                if ui
                    .selectable_label(m.hatch.is_some_and(|h| h.style == s), s.name())
                    .clicked()
                {
                    let mut h = m.hatch.unwrap_or_default();
                    h.style = s;
                    changes.push(Change::Hatch(Some(h)));
                }
            }
        });
    ui.end_row();
    if let Some(h) = m.hatch {
        ui.label("Hatch spacing");
        let mut sp = h.spacing;
        if ui
            .add(
                egui::DragValue::new(&mut sp)
                    .range(0.5..=200.0)
                    .speed(0.2)
                    .suffix(" pt"),
            )
            .changed()
        {
            changes.push(Change::Hatch(Some(Hatch { spacing: sp, ..h })));
        }
        ui.end_row();
        ui.label("Hatch color");
        ui.horizontal(|ui| {
            let mut own = h.color.is_some();
            if ui
                .checkbox(&mut own, "")
                .on_hover_text("Off: the line colour")
                .changed()
            {
                changes.push(Change::Hatch(Some(Hatch {
                    color: own.then_some(m.color),
                    ..h
                })));
            }
            if let Some(c) = &h.color
                && let Some(c) = color_button(ui, c)
            {
                changes.push(Change::Hatch(Some(Hatch { color: Some(c), ..h })));
            }
        });
        ui.end_row();
    }
}

fn ending_row(
    ui: &mut egui::Ui,
    label_text: &str,
    sel: &Sel<'_>,
    get: fn(&Markup) -> &str,
    key: &'static str,
    edits: &mut Edits,
    set: fn(String) -> MarkupPatch,
) {
    let mixed = sel.mixed(|m| get(m).to_string());
    label(ui, label_text, mixed);
    let value = get(sel.first);
    let shown = if mixed {
        MIXED
    } else if value.is_empty() {
        "None"
    } else {
        value
    };
    egui::ComboBox::from_id_salt(("prop-ending", key))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for e in LINE_ENDINGS {
                if ui.selectable_label(value == *e, *e).clicked() {
                    edits.push((key, set(e.to_string())));
                }
            }
        });
    ui.end_row();
}

fn text(ui: &mut egui::Ui, sel: &Sel<'_>, edits: &mut Edits) {
    let m = sel.first;
    let title = if m.kind.is_measurement() { "Caption" } else { "Text" };
    section(ui, title, "props-text", |ui| {
        let mixed = sel.mixed(|m| m.text.font.clone());
        label(ui, "Font", mixed);
        egui::ComboBox::from_id_salt("prop-font")
            .selected_text(if mixed { MIXED } else { m.text.font.as_str() })
            .show_ui(ui, |ui| {
                for f in FONTS {
                    if ui.selectable_label(m.text.font == *f, *f).clicked() {
                        edits.push((
                            "font",
                            MarkupPatch {
                                font: Some(f.to_string()),
                                ..Default::default()
                            },
                        ));
                    }
                }
            });
        ui.end_row();
        label(ui, "Size", sel.mixed(|m| m.text.size.to_bits()));
        let mut s = m.text.size;
        if ui
            .add(
                egui::DragValue::new(&mut s)
                    .range(1.0..=400.0)
                    .speed(0.25)
                    .suffix(" pt"),
            )
            .changed()
        {
            edits.push((
                "font-size",
                MarkupPatch {
                    font_size: Some(s),
                    ..Default::default()
                },
            ));
        }
        ui.end_row();
        label(ui, "Text color", sel.mixed(|m| m.text.color));
        if let Some(c) = color_button(ui, &m.text.color) {
            edits.push((
                "text-color",
                MarkupPatch {
                    text_color: Some(c),
                    ..Default::default()
                },
            ));
        }
        ui.end_row();
        label(
            ui,
            "Style",
            sel.mixed(|m| (m.text.bold, m.text.italic, m.text.underline)),
        );
        ui.horizontal(|ui| {
            let flags = [
                ("bold", "B", m.text.bold),
                ("italic", "I", m.text.italic),
                ("underline", "U", m.text.underline),
            ];
            for (key, letter, on) in flags {
                let rt = match key {
                    "bold" => RichText::new(letter).strong(),
                    "italic" => RichText::new(letter).italics(),
                    _ => RichText::new(letter).underline(),
                };
                if ui
                    .add(egui::Button::new(rt).selected(on).min_size(egui::vec2(24.0, 20.0)))
                    .clicked()
                {
                    let mut p = MarkupPatch::default();
                    match key {
                        "bold" => p.bold = Some(!on),
                        "italic" => p.italic = Some(!on),
                        _ => p.underline = Some(!on),
                    }
                    edits.push((key, p));
                }
            }
            if m.kind.is_measurement() {
                // caption-only styles: strike-through, superscript, subscript
                let strike = egui::Button::new(RichText::new("S").strikethrough())
                    .selected(m.text.strike)
                    .min_size(egui::vec2(24.0, 20.0));
                if ui.add(strike).on_hover_text("Strikethrough").clicked() {
                    edits.push((
                        "strike",
                        MarkupPatch {
                            strike: Some(!m.text.strike),
                            ..Default::default()
                        },
                    ));
                }
                for (v, t, tip) in [(1i8, "x\u{b2}", "Superscript"), (-1, "x\u{2082}", "Subscript")] {
                    let on = m.text.script == v;
                    let b = egui::Button::new(t).selected(on).min_size(egui::vec2(24.0, 20.0));
                    if ui.add(b).on_hover_text(tip).clicked() {
                        edits.push((
                            "script",
                            MarkupPatch {
                                script: Some(if on { 0 } else { v }),
                                ..Default::default()
                            },
                        ));
                    }
                }
            }
        });
        ui.end_row();
        if matches!(m.kind, Kind::Perimeter | Kind::Area | Kind::Volume | Kind::Polylength) {
            label(ui, "Placement", sel.mixed(|m| m.caption_last_segment));
            let mut last = m.caption_last_segment;
            if ui.checkbox(&mut last, "Along the last segment").changed() {
                edits.push((
                    "caption-last-segment",
                    MarkupPatch {
                        caption_last_segment: Some(last),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
        if m.kind.is_text() {
            label(ui, "Alignment", sel.mixed(|m| m.text.align));
            ui.horizontal(|ui| {
                for (i, icon, tip) in [
                    (0, "align-left", "Left"),
                    (1, "align-center", "Center"),
                    (2, "align-right", "Right"),
                ] {
                    if crate::icons::button(ui, icon, 22.0, m.text.align == i, tip).clicked() {
                        edits.push((
                            "align",
                            MarkupPatch {
                                align: Some(i),
                                ..Default::default()
                            },
                        ));
                    }
                }
            });
            ui.end_row();
        }
    });
}

fn measurement(ui: &mut egui::Ui, sel: &Sel<'_>, edits: &mut Edits, changes: &mut Vec<Change>) {
    let m = sel.first;
    section(ui, "Measurement", "props-measure", |ui| {
        ui.label("Value");
        let q = m.quantity_text();
        ui.label(RichText::new(if q.is_empty() { "No scale".into() } else { q }).strong());
        ui.end_row();
        if let Some(extra) = derived_values(m) {
            for (k, v) in extra {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            }
        }
        if m.kind != Kind::Count {
            let mixed = sel.mixed(|m| m.hide_caption);
            label(ui, "Caption", mixed);
            let mut show = !m.hide_caption;
            if ui.checkbox(&mut show, "Show Caption").changed() {
                edits.push((
                    "show-caption",
                    MarkupPatch {
                        show_caption: Some(show),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
            let mixed = sel.mixed(|m| m.scale.as_ref().map(|s| s.ratio.clone()));
            label(ui, "Scale", mixed);
            let current = if mixed {
                MIXED.to_string()
            } else {
                m.scale.as_ref().map_or("Not set", |s| s.ratio.as_str()).to_string()
            };
            egui::ComboBox::from_id_salt("prop-scale")
                .selected_text(&current)
                .width(150.0)
                .show_ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for p in scale_presets() {
                            if ui.selectable_label(false, &p.name).clicked() {
                                edits.push((
                                    "scale",
                                    MarkupPatch {
                                        scale: Some(p.scale.clone()),
                                        ..Default::default()
                                    },
                                ));
                            }
                        }
                    });
                });
            ui.end_row();
            if let Some(sc) = m.scale.as_ref().filter(|s| s.valid()) {
                ui.label("Units");
                let du = scale_display_unit(sc);
                egui::ComboBox::from_id_salt("prop-units")
                    .selected_text(du.map(|d| d.name()).unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for d in display_unit_choices() {
                            if ui.selectable_label(Some(d) == du, d.name()).clicked() {
                                let mut s = sc.clone();
                                if set_display_unit(&mut s, d) {
                                    edits.push((
                                        "units",
                                        MarkupPatch {
                                            scale: Some(s),
                                            ..Default::default()
                                        },
                                    ));
                                }
                            }
                        }
                    });
                ui.end_row();
                ui.label("Precision");
                let pr = scale_precision(sc);
                egui::ComboBox::from_id_salt("prop-precision")
                    .selected_text(pr.name())
                    .show_ui(ui, |ui| {
                        let fractions = du.is_some_and(|d| d.feet_inches || d.unit.label() == "in");
                        for p in precision_choices(fractions) {
                            if ui.selectable_label(p == pr, p.name()).clicked() {
                                let mut s = sc.clone();
                                set_precision(&mut s, p);
                                edits.push((
                                    "precision",
                                    MarkupPatch {
                                        scale: Some(s),
                                        ..Default::default()
                                    },
                                ));
                            }
                        }
                    });
                ui.end_row();
            }
        }
        if matches!(m.kind, Kind::Polylength | Kind::Perimeter | Kind::Area | Kind::Volume) {
            label(ui, "Segments", sel.mixed(|m| m.segment_values));
            let mut on = m.segment_values;
            if ui.checkbox(&mut on, "Show Segment Values").changed() {
                edits.push((
                    "segments",
                    MarkupPatch {
                        segment_values: Some(on),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
        if m.kind == Kind::Polylength {
            label(ui, "Rise/Drop", sel.mixed(|m| m.rise_drop.to_bits()));
            let mut v = m.rise_drop;
            if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                edits.push((
                    "rise",
                    MarkupPatch {
                        rise_drop: Some(v),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
        if matches!(m.kind, Kind::Area | Kind::Volume | Kind::Perimeter | Kind::Polylength) {
            label(ui, "Depth", sel.mixed(|m| m.depth.to_bits()));
            let mut v = m.depth;
            if ui
                .add(egui::DragValue::new(&mut v).speed(0.1).range(0.0..=1.0e6))
                .changed()
            {
                edits.push((
                    "depth",
                    MarkupPatch {
                        depth: Some(v),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
            if !m.holes.is_empty() {
                ui.label("Cutouts");
                ui.label(format!("{}", m.holes.len()));
                ui.end_row();
            }
        }
        if m.kind == Kind::Count {
            let mixed = sel.mixed(|m| m.count_symbol);
            label(ui, "Symbol", mixed);
            egui::ComboBox::from_id_salt("prop-symbol")
                .selected_text(if mixed {
                    MIXED.to_string()
                } else {
                    format!("{:?}", m.count_symbol)
                })
                .show_ui(ui, |ui| {
                    for s in [
                        CountSymbol::Circle,
                        CountSymbol::Square,
                        CountSymbol::Check,
                        CountSymbol::Cross,
                    ] {
                        if ui.selectable_label(m.count_symbol == s, format!("{s:?}")).clicked() {
                            edits.push((
                                "symbol",
                                MarkupPatch {
                                    count_symbol: Some(s),
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                });
            ui.end_row();
            // A custom symbol from another selected markup.
            if let Some(other) = sel.all.iter().find(|o| o.kind != Kind::Count) {
                ui.label("");
                if ui
                    .button(format!("Use the {} as the symbol", other.kind.name()))
                    .on_hover_text("The selected markup's outline becomes this count's symbol")
                    .clicked()
                {
                    changes.push(Change::Symbol(markupcraft_model::measure_extras::symbol_from_markup(
                        other,
                    )));
                }
                ui.end_row();
            }
            label(ui, "Symbol size", sel.mixed(|m| m.symbol_scale.to_bits()));
            let mut v = m.symbol_scale;
            if ui
                .add(egui::Slider::new(&mut v, 0.25..=8.0).logarithmic(true))
                .changed()
            {
                edits.push((
                    "symbol-scale",
                    MarkupPatch {
                        symbol_scale: Some(v),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
        if m.caption_offset.is_some() {
            ui.label("Caption position");
            if ui.button("Reset position").clicked() {
                edits.push((
                    "caption",
                    MarkupPatch {
                        caption_offset: Some(None),
                        ..Default::default()
                    },
                ));
            }
            ui.end_row();
        }
    });
}

/// Values derived from a measurement (wall area, circle area and circumference).
fn derived_values(m: &Markup) -> Option<Vec<(&'static str, String)>> {
    use markupcraft_model::measure_extras::{circle_area, circle_circumference, wall_area};
    let s = m.scale.as_ref().filter(|s| s.valid())?;
    let mut out = Vec::new();
    if let Some(w) = wall_area(m) {
        out.push(("Wall area", markupcraft_model::format_value(w, &s.area)));
    }
    if let Some(a) = circle_area(m) {
        out.push(("Area", markupcraft_model::format_value(a, &s.area)));
    }
    if let Some(c) = circle_circumference(m) {
        out.push(("Circumference", markupcraft_model::format_value(c, &s.dist)));
    }
    (!out.is_empty()).then_some(out)
}

fn note(ui: &mut egui::Ui, sel: &Sel<'_>, edits: &mut Edits) {
    let m = sel.first;
    section(ui, "Note", "props-note", |ui| {
        label(ui, "Icon", sel.mixed(|m| m.icon.clone()));
        egui::ComboBox::from_id_salt("prop-icon")
            .selected_text(if m.icon.is_empty() { "Comment" } else { m.icon.as_str() })
            .show_ui(ui, |ui| {
                for i in NOTE_ICONS {
                    if ui.selectable_label(m.icon == *i, *i).clicked() {
                        edits.push((
                            "icon",
                            MarkupPatch {
                                icon: Some(i.to_string()),
                                ..Default::default()
                            },
                        ));
                    }
                }
            });
        ui.end_row();
        text_row(
            ui,
            "Comment",
            sel,
            |m| &m.contents,
            "contents",
            edits,
            |v| MarkupPatch {
                contents: Some(v),
                ..Default::default()
            },
        );
    });
}

/// Layout: position and size in inches from the page's lower-left corner, and rotation.
fn layout(ui: &mut egui::Ui, sel: &Sel<'_>, changes: &mut Vec<Change>) {
    let m = sel.first;
    egui::CollapsingHeader::new("Layout")
        .default_open(false)
        .show(ui, |ui| {
            let single = sel.all.len() == 1;
            let b = if actions::uses_rect(m.kind) {
                box_of(m)
            } else {
                actions::markup_bbox(m)
            };
            let geometry = markupcraft_engine::props::geometry_editable(m);
            Grid::new("props-layout").num_columns(2).show(ui, |ui| {
                let mut v = [b.x0 / 72.0, b.y0 / 72.0, b.width() / 72.0, b.height() / 72.0];
                let names = ["X", "Y", "Width", "Height"];
                let mut changed = [false; 4];
                for (i, k) in names.iter().enumerate() {
                    ui.label(*k);
                    let enabled = single && geometry && (i < 2 || b.width() > 0.0 && b.height() > 0.0);
                    if let Some(x) = v.get_mut(i) {
                        let r = ui.add_enabled(
                            enabled,
                            egui::DragValue::new(x).speed(0.01).max_decimals(3).suffix(" in"),
                        );
                        if let Some(c) = changed.get_mut(i) {
                            *c = r.changed();
                        }
                    }
                    ui.end_row();
                }
                let [x, y, w, h] = v;
                if changed[0] || changed[1] {
                    changes.push(Change::Move(x * 72.0 - b.x0, y * 72.0 - b.y0));
                }
                if (changed[2] || changed[3]) && w > 0.0 && h > 0.0 {
                    changes.push(Change::Resize(markupcraft_geom::Rect::new(
                        b.x0,
                        b.y0,
                        b.x0 + w * 72.0,
                        b.y0 + h * 72.0,
                    )));
                }
                ui.label("Rotation");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(geometry, |ui| {
                        if ui
                            .button("\u{27f2} 90\u{b0}")
                            .on_hover_text("Rotate counterclockwise")
                            .clicked()
                        {
                            changes.push(Change::Rotate(90.0));
                        }
                        if ui
                            .button("\u{27f3} 90\u{b0}")
                            .on_hover_text("Rotate clockwise")
                            .clicked()
                        {
                            changes.push(Change::Rotate(-90.0));
                        }
                        let id = ui.id().with("rotate-by");
                        let mut deg: f64 = ui.data_mut(|d| d.get_temp::<f64>(id).unwrap_or(15.0));
                        ui.add(egui::DragValue::new(&mut deg).range(-360.0..=360.0).suffix("\u{b0}"));
                        ui.data_mut(|d| d.insert_temp(id, deg));
                        if ui.button("Rotate").clicked() && deg != 0.0 {
                            changes.push(Change::Rotate(deg));
                        }
                    });
                });
                ui.end_row();
                ui.label("Page");
                ui.label(format!("{}", m.page + 1));
                ui.end_row();
            });
        });
}

fn page_info(ui: &mut egui::Ui, doc: &DocTab, t: &Tokens) {
    ui.add_space(4.0);
    ui.label(RichText::new("No markup selected").strong());
    ui.label(
        RichText::new("Click a markup to see and change its properties.")
            .color(t.text_faint)
            .size(11.0),
    );
    ui.add_space(10.0);
    let page = doc.view.current;
    let Some(p) = doc.session.doc().pages.get(page) else {
        return;
    };
    Grid::new("page-props")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("Document");
            ui.label(&doc.name);
            ui.end_row();
            ui.label("Page");
            let label = markupcraft_model::table::page_label(doc.session.doc(), page);
            ui.label(format!("{} of {} ({label})", page + 1, doc.session.page_count()));
            ui.end_row();
            ui.label("Size");
            ui.label(format!(
                "{:.2} x {:.2} in",
                p.crop.width() / 72.0,
                p.crop.height() / 72.0
            ));
            ui.end_row();
            ui.label("Scale");
            ui.label(
                p.scale
                    .as_ref()
                    .or(p.viewports.first().map(|v| &v.scale))
                    .map_or("Not set", |s| s.ratio.as_str()),
            );
            ui.end_row();
            ui.label("Markups");
            ui.label(format!("{}", doc.session.doc().markups_on(page).count()));
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_styles_are_valid_dashes() {
        for (name, d) in LINE_STYLES {
            let p = MarkupPatch {
                dash: Some(d.to_vec()),
                ..Default::default()
            };
            assert!(p.validate().is_ok(), "{name}");
            assert_eq!(line_style_name(d), *name);
        }
        assert_eq!(line_style_name(&[7.0, 7.0]), "Custom");
    }

    #[test]
    fn mixed_compares_every_selected_markup() {
        let a = Markup::new(Kind::Line, 0, Vec::new());
        let mut b = a.clone();
        b.subject = "Other".into();
        let all = [a.clone(), b];
        let s = Sel { first: &a, all: &all };
        assert!(s.mixed(|m| m.subject.clone()));
        assert!(!s.mixed(|m| m.kind));
    }
}
