//! Properties (Alt+P): the selected markups' fields, grouped like Revu's panel (General,
//! Appearance, Text, Measurement, Layout, Options). A change applies to every selected markup
//! MarkupCraft can write; a slider drag or a typed word is one undo step (the engine merges a
//! gesture's edits). With nothing selected: the page and its scale.

use egui::{Grid, RichText};
use markupcraft_engine::MarkupPatch;
use markupcraft_geom::shapes::LINE_ENDINGS;
use markupcraft_measure::units::{
    display_unit_choices, precision_choices, scale_display_unit, scale_precision, scale_presets, set_display_unit,
    set_precision,
};
use markupcraft_model::{Color, CountSymbol, Kind, Markup, review_statuses};

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

/// The name of a dash array.
pub fn line_style_name(dash: &[f64]) -> &'static str {
    LINE_STYLES
        .iter()
        .find(|(_, d)| *d == dash)
        .map_or(if dash.is_empty() { "Solid" } else { "Custom" }, |(n, _)| n)
}

/// Edits gathered while the panel draws, applied after: (merge key, patch).
type Edits = Vec<(&'static str, MarkupPatch)>;

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut actions_out: Vec<&'static str> = Vec::new();
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let ids = doc.selection().to_vec();
        let first = ids.iter().find_map(|id| doc.session.doc().find(id)).cloned();
        let Some(m) = first else {
            page_info(ui, doc, &t);
            return;
        };
        let n = ids.len();
        let title = if n == 1 {
            m.kind.name().to_string()
        } else {
            let kinds: std::collections::BTreeSet<&str> = ids
                .iter()
                .filter_map(|id| doc.session.doc().find(id))
                .map(|m| m.kind.name())
                .collect();
            if kinds.len() == 1 {
                format!("{n} {} markups", m.kind.name())
            } else {
                format!("{n} markups")
            }
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
            let why = if ids
                .iter()
                .filter_map(|id| doc.session.doc().find(id))
                .any(|m| m.locked())
            {
                "Locked: unlock it to change its properties."
            } else {
                "Shown from the file. Editing this kind comes with its writer."
            };
            ui.label(RichText::new(why).color(t.text_faint).size(11.0));
        }
        ui.add_space(4.0);
        let mut edits: Edits = Vec::new();
        ui.add_enabled_ui(writable, |ui| {
            general(ui, &m, &mut edits);
            appearance(ui, &m, &mut edits);
            if m.kind.is_text() || m.kind == Kind::Stamp || m.kind.is_measurement() {
                text(ui, &m, &mut edits);
            }
            if m.kind.is_measurement() {
                measurement(ui, &m, &mut edits);
            }
            if m.kind == Kind::Note {
                note(ui, &m, &mut edits);
            }
        });
        layout(ui, &m);
        egui::CollapsingHeader::new("Options")
            .default_open(true)
            .show(ui, |ui| {
                let mut locked = m.locked();
                if ui.checkbox(&mut locked, "Locked").changed() {
                    let r = doc.session.set_locked(&ids, locked);
                    app_status(doc, r.map(|_| ()));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(n == 1, egui::Button::new("Set as Default")).clicked() {
                        actions_out.push("markup.set_default");
                    }
                    if ui.add_enabled(n == 1, egui::Button::new("Add to Tool Chest")).clicked() {
                        actions_out.push("markup.add_to_toolchest");
                    }
                });
            });
        apply(doc, &targets, edits);
    });
    for a in actions_out {
        app.queue(a);
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

fn section(ui: &mut egui::Ui, title: &str, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
        Grid::new(id)
            .num_columns(2)
            .spacing([10.0, 5.0])
            .min_col_width(78.0)
            .show(ui, body);
    });
}

fn text_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    key: &'static str,
    edits: &mut Edits,
    set: fn(String) -> MarkupPatch,
) {
    ui.label(label);
    let mut v = value.to_string();
    if ui
        .add(egui::TextEdit::singleline(&mut v).desired_width(f32::INFINITY))
        .changed()
    {
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

fn general(ui: &mut egui::Ui, m: &Markup, edits: &mut Edits) {
    section(ui, "General", "props-general", |ui| {
        text_row(ui, "Subject", &m.subject, "subject", edits, |v| MarkupPatch {
            subject: Some(v),
            ..Default::default()
        });
        text_row(ui, "Label", &m.label, "label", edits, |v| MarkupPatch {
            label: Some(v),
            ..Default::default()
        });
        text_row(ui, "Author", &m.author, "author", edits, |v| MarkupPatch {
            author: Some(v),
            ..Default::default()
        });
        if !m.kind.is_measurement() && !m.kind.is_text() {
            text_row(ui, "Comments", &m.contents, "contents", edits, |v| MarkupPatch {
                contents: Some(v),
                ..Default::default()
            });
        }
        text_row(ui, "Layer", &m.layer, "layer", edits, |v| MarkupPatch {
            layer: Some(v),
            ..Default::default()
        });
        ui.label("Status");
        let current = if m.status.is_empty() { "None" } else { m.status.as_str() };
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
        ui.label("Checkmark");
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
        ui.label("Date");
        ui.label(markupcraft_model::table::revu_date(&m.modified));
        ui.end_row();
    });
}

fn appearance(ui: &mut egui::Ui, m: &Markup, edits: &mut Edits) {
    section(ui, "Appearance", "props-appearance", |ui| {
        ui.label("Color");
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
            ui.label("Fill");
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
            ui.label("Fill opacity");
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

        ui.label("Opacity");
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

        if !matches!(m.kind, Kind::TextHighlight | Kind::Note | Kind::Snapshot) {
            ui.label("Line width");
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

            ui.label("Line style");
            let current = line_style_name(&m.dash);
            egui::ComboBox::from_id_salt("prop-dash")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (name, dash) in LINE_STYLES {
                        if ui.selectable_label(current == *name, *name).clicked() {
                            edits.push((
                                "dash",
                                MarkupPatch {
                                    dash: Some(dash.to_vec()),
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                });
            ui.end_row();
        }

        if matches!(
            m.kind,
            Kind::Line | Kind::Arrow | Kind::Polyline | Kind::Length | Kind::Polylength | Kind::Callout
        ) {
            if m.kind != Kind::Callout {
                ending_row(ui, "Start", &m.line_start, "start", edits, |v| MarkupPatch {
                    line_start: Some(v),
                    ..Default::default()
                });
            }
            ending_row(ui, "End", &m.line_end, "end", edits, |v| MarkupPatch {
                line_end: Some(v),
                ..Default::default()
            });
        }
        if matches!(
            m.kind,
            Kind::Cloud | Kind::Polygon | Kind::Area | Kind::Perimeter | Kind::Rectangle
        ) {
            ui.label("Cloud");
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
    });
}

fn ending_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    key: &'static str,
    edits: &mut Edits,
    set: fn(String) -> MarkupPatch,
) {
    ui.label(label);
    egui::ComboBox::from_id_salt(("prop-ending", key))
        .selected_text(if value.is_empty() { "None" } else { value })
        .show_ui(ui, |ui| {
            for e in LINE_ENDINGS {
                if ui.selectable_label(value == *e, *e).clicked() {
                    edits.push((key, set(e.to_string())));
                }
            }
        });
    ui.end_row();
}

fn text(ui: &mut egui::Ui, m: &Markup, edits: &mut Edits) {
    section(ui, "Text", "props-text", |ui| {
        ui.label("Font");
        egui::ComboBox::from_id_salt("prop-font")
            .selected_text(&m.text.font)
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
        ui.label("Size");
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
        ui.label("Text color");
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
        ui.label("Style");
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
        });
        ui.end_row();
        if m.kind.is_text() {
            ui.label("Alignment");
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

fn measurement(ui: &mut egui::Ui, m: &Markup, edits: &mut Edits) {
    section(ui, "Measurement", "props-measure", |ui| {
        ui.label("Value");
        let q = m.quantity_text();
        ui.label(RichText::new(if q.is_empty() { "No scale".into() } else { q }).strong());
        ui.end_row();
        if m.kind != Kind::Count {
            ui.label("Scale");
            let current = m.scale.as_ref().map_or("Not set", |s| s.ratio.as_str()).to_string();
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
            ui.label("Segments");
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
            ui.label("Rise/Drop");
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
        if matches!(m.kind, Kind::Area | Kind::Volume) {
            ui.label("Depth");
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
            ui.label("Symbol");
            egui::ComboBox::from_id_salt("prop-symbol")
                .selected_text(format!("{:?}", m.count_symbol))
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
            ui.label("Symbol size");
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
            ui.label("Caption");
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

fn note(ui: &mut egui::Ui, m: &Markup, edits: &mut Edits) {
    section(ui, "Note", "props-note", |ui| {
        ui.label("Icon");
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
        text_row(ui, "Comment", &m.contents, "contents", edits, |v| MarkupPatch {
            contents: Some(v),
            ..Default::default()
        });
    });
}

fn layout(ui: &mut egui::Ui, m: &Markup) {
    egui::CollapsingHeader::new("Layout")
        .default_open(false)
        .show(ui, |ui| {
            let b = if actions::uses_rect(m.kind) {
                box_of(m)
            } else {
                actions::markup_bbox(m)
            };
            Grid::new("props-layout").num_columns(2).show(ui, |ui| {
                for (k, v) in [
                    ("X", b.x0 / 72.0),
                    ("Y", b.y0 / 72.0),
                    ("Width", b.width() / 72.0),
                    ("Height", b.height() / 72.0),
                ] {
                    ui.label(k);
                    ui.label(format!("{v:.3} in"));
                    ui.end_row();
                }
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
}
