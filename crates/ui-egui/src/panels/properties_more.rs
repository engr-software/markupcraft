//! More of the Properties panel: a Dimension's offset and extension lines; a measurement's
//! caption contents (fields and custom columns, Show All Measurements), caption leader,
//! slope, centroid, and its own area and volume units. Gathered as patches like the rest of the
//! panel (`properties.rs` applies them).

use egui::Grid;
use markupcraft_engine::MarkupPatch;
use markupcraft_measure::units::{format_unit, real_units, set_area_unit, set_volume_unit};
use markupcraft_model::caption::CAPTION_FIELDS;
use markupcraft_model::measure_extras::closed_shape;
use markupcraft_model::{CustomColumn, Kind, Markup};

/// Edits gathered while the panel draws: (merge key, patch).
pub type Edits = Vec<(&'static str, MarkupPatch)>;

/// Slope types as Properties lists them: (name, `/SlopeType`).
pub const SLOPES: &[(&str, i64)] = &[("No slope", 0), ("Pitch (in 12)", 1), ("Degrees", 2), ("Grade %", 3)];

fn mixed<T: PartialEq>(all: &[Markup], f: impl Fn(&Markup) -> T) -> bool {
    let mut it = all.iter().map(f);
    match it.next() {
        Some(v) => it.any(|w| w != v),
        None => false,
    }
}

fn label(ui: &mut egui::Ui, text: &str, is_mixed: bool) {
    if is_mixed {
        ui.label(format!("{text} ({})", super::properties::MIXED));
    } else {
        ui.label(text);
    }
}

/// Custom review statuses: the user's own statuses as buttons, and a field for a new one
/// (Enter sets it on the selection and keeps it for the status menus).
pub fn custom_status(
    ui: &mut egui::Ui,
    current: &str,
    custom: &[String],
    edits: &mut Edits,
    new_status: &mut Option<String>,
) {
    let set = |s: &str| MarkupPatch {
        status: Some(s.to_string()),
        ..Default::default()
    };
    egui::CollapsingHeader::new("Custom Status")
        .id_salt("props-custom-status")
        .default_open(!custom.is_empty())
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for st in custom {
                    if ui.selectable_label(current == st, st).clicked() {
                        edits.push(("status", set(st)));
                    }
                }
            });
            let id = ui.id().with("new-status");
            let mut text: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("New status (Enter)")
                    .desired_width(160.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
                let st = text.trim().to_string();
                edits.push(("status", set(&st)));
                *new_status = Some(st);
                text.clear();
            }
            ui.data_mut(|d| d.insert_temp(id, text));
        });
}

/// A change to the selected markup's replies (applied after the panel draws).
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyAct {
    Add(String),
    Edit(usize, String),
    Delete(usize),
}

/// Replies: the markup's comments thread; add, change or delete a reply.
pub fn replies(ui: &mut egui::Ui, m: &Markup, out: &mut Vec<ReplyAct>) {
    egui::CollapsingHeader::new(format!("Replies ({})", m.replies.len()))
        .id_salt("props-replies")
        .default_open(!m.replies.is_empty())
        .show(ui, |ui| {
            for (i, r) in m.replies.iter().enumerate().take(500) {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  {}",
                            r.author,
                            markupcraft_model::table::revu_date(&r.date)
                        ))
                        .size(10.5)
                        .weak(),
                    );
                    if crate::icons::button(ui, "trash-2", 18.0, false, "Delete reply").clicked() {
                        out.push(ReplyAct::Delete(i));
                    }
                });
                let mut text = r.text.clone();
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .id_salt(("reply", i)),
                );
                if resp.changed() {
                    out.push(ReplyAct::Edit(i, text));
                }
            }
            let id = ui.id().with("new-reply");
            let mut text: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
            let r = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Reply (Enter)")
                    .desired_width(f32::INFINITY),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
                out.push(ReplyAct::Add(text.trim().to_string()));
                text.clear();
            }
            ui.data_mut(|d| d.insert_temp(id, text));
        });
}

/// Apply the reply changes to markup `id` (an edit while typing is one undo step).
pub fn apply_replies(doc: &mut crate::DocTab, id: &str, acts: Vec<ReplyAct>) {
    for a in acts {
        let r = match a {
            ReplyAct::Add(t) => doc.session.add_reply(id, &t).map(|_| ()),
            ReplyAct::Edit(i, t) => {
                doc.session.set_merge_key(Some("reply-edit"));
                let r = doc.session.edit_reply(id, i, &t);
                doc.session.set_merge_key(None);
                r
            }
            ReplyAct::Delete(i) => doc.session.delete_reply(id, i).map(|_| ()),
        };
        if let Err(e) = r {
            log::info!("replies: {e}");
        }
    }
}

/// The extra sections for the selection (`m` is the first selected markup).
pub fn sections(ui: &mut egui::Ui, m: &Markup, all: &[Markup], columns: &[CustomColumn], edits: &mut Edits) {
    if m.kind == Kind::Dimension {
        dimension(ui, m, all, edits);
    }
    if m.kind.is_measurement() && m.kind != Kind::Count {
        caption(ui, m, all, columns, edits);
        takeoff(ui, m, all, edits);
    }
    if m.kind == Kind::Count {
        count_dims(ui, m, all, edits);
    }
    if m.kind == Kind::Attachment {
        attachment(ui, m, all, edits);
    }
    if m.kind.is_text() {
        text_layout(ui, m, all, edits);
    }
    if m.kind == Kind::Note {
        let mut open = m.popup_open;
        if ui
            .checkbox(&mut open, "Pop-up open")
            .on_hover_text("Show the note's comment in its pop-up on the page")
            .changed()
        {
            edits.push((
                "popup-open",
                MarkupPatch {
                    popup_open: Some(open),
                    ..Default::default()
                },
            ));
        }
    }
}

/// A text box's margin and line spacing.
fn text_layout(ui: &mut egui::Ui, m: &Markup, all: &[Markup], edits: &mut Edits) {
    egui::CollapsingHeader::new("Text Layout")
        .id_salt("props-text-layout")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-text-layout-grid").num_columns(2).show(ui, |ui| {
                label(ui, "Margin", mixed(all, |x| x.text.margin.to_bits()));
                let mut v = m.text.margin;
                if ui
                    .add(
                        egui::DragValue::new(&mut v)
                            .range(-4.0..=200.0)
                            .speed(0.25)
                            .suffix(" pt"),
                    )
                    .on_hover_text("Extra space between the frame and the text")
                    .changed()
                {
                    edits.push((
                        "text-margin",
                        MarkupPatch {
                            text_margin: Some(v),
                            ..Default::default()
                        },
                    ));
                }
                ui.end_row();
                label(ui, "Line spacing", mixed(all, |x| x.text.line_spacing.to_bits()));
                let mut sp = m.text.line_spacing;
                if ui
                    .add(egui::DragValue::new(&mut sp).range(0.5..=5.0).speed(0.05).suffix(" x"))
                    .changed()
                {
                    edits.push((
                        "line-spacing",
                        MarkupPatch {
                            line_spacing: Some(sp),
                            ..Default::default()
                        },
                    ));
                }
                ui.end_row();
            });
        });
}

/// A File Attachment: its icon and the attached file's name.
fn attachment(ui: &mut egui::Ui, m: &Markup, all: &[Markup], edits: &mut Edits) {
    egui::CollapsingHeader::new("File Attachment")
        .id_salt("props-attachment")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-attachment-grid").num_columns(2).show(ui, |ui| {
                label(ui, "Icon", mixed(all, |x| x.icon.clone()));
                let current = if m.icon.is_empty() { "PushPin" } else { m.icon.as_str() };
                egui::ComboBox::from_id_salt("prop-attachment-icon")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for i in markupcraft_revu::kinds::more::ATTACHMENT_ICONS {
                            if ui.selectable_label(current == *i, *i).clicked() {
                                edits.push((
                                    "attachment-icon",
                                    MarkupPatch {
                                        icon: Some(i.to_string()),
                                        ..Default::default()
                                    },
                                ));
                            }
                        }
                    });
                ui.end_row();
                ui.label("File");
                ui.label(if m.attachment_name.is_empty() {
                    "-"
                } else {
                    m.attachment_name.as_str()
                });
                ui.end_row();
            });
        });
}

/// A count's item dimensions (uniform items: diffusers, doors), in the scale's length unit.
fn count_dims(ui: &mut egui::Ui, m: &Markup, all: &[Markup], edits: &mut Edits) {
    egui::CollapsingHeader::new("Item Size")
        .id_salt("props-count-dims")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-count-dims-grid").num_columns(2).show(ui, |ui| {
                for (name, key, v) in [
                    ("Width", "count-width", m.item_width),
                    ("Height", "count-height", m.item_height),
                    ("Depth", "count-depth", m.depth),
                ] {
                    label(
                        ui,
                        name,
                        mixed(all, |x| match key {
                            "count-width" => x.item_width.to_bits(),
                            "count-height" => x.item_height.to_bits(),
                            _ => x.depth.to_bits(),
                        }),
                    );
                    let mut x = v;
                    if ui
                        .add(egui::DragValue::new(&mut x).range(0.0..=1.0e6).speed(0.05))
                        .changed()
                    {
                        let mut p = MarkupPatch::default();
                        match key {
                            "count-width" => p.item_width = Some(x),
                            "count-height" => p.item_height = Some(x),
                            _ => p.depth = Some(x),
                        }
                        edits.push((key, p));
                    }
                    ui.end_row();
                }
            });
        });
}

fn dimension(ui: &mut egui::Ui, m: &Markup, all: &[Markup], edits: &mut Edits) {
    egui::CollapsingHeader::new("Dimension")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-dimension").num_columns(2).show(ui, |ui| {
                label(ui, "Offset", mixed(all, |m| m.leader.to_bits()));
                let mut v = m.leader;
                if ui
                    .add(
                        egui::DragValue::new(&mut v)
                            .range(-2000.0..=2000.0)
                            .speed(0.5)
                            .suffix(" pt"),
                    )
                    .on_hover_text("How far the dimension line sits from the measured points")
                    .changed()
                {
                    edits.push((
                        "dim-offset",
                        MarkupPatch {
                            leader: Some(v),
                            ..Default::default()
                        },
                    ));
                }
                ui.end_row();
                label(ui, "Extension", mixed(all, |m| m.leader_ext.to_bits()));
                let mut e = m.leader_ext;
                if ui
                    .add(egui::DragValue::new(&mut e).range(0.0..=500.0).speed(0.5).suffix(" pt"))
                    .on_hover_text("How far the extension lines run past the dimension line")
                    .changed()
                {
                    edits.push((
                        "dim-ext",
                        MarkupPatch {
                            leader_ext: Some(e),
                            ..Default::default()
                        },
                    ));
                }
                ui.end_row();
            });
        });
}

fn caption(ui: &mut egui::Ui, m: &Markup, all: &[Markup], columns: &[CustomColumn], edits: &mut Edits) {
    egui::CollapsingHeader::new("Caption Contents")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-caption").num_columns(2).show(ui, |ui| {
                let set = |t: String| MarkupPatch {
                    caption_template: Some(t),
                    ..Default::default()
                };
                label(ui, "Contents", mixed(all, |m| m.caption_template.clone()));
                let mut t = m.caption_template.clone();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut t)
                        .hint_text("{value}")
                        .desired_width(f32::INFINITY)
                        .id_salt("prop-caption-template"),
                );
                if r.changed() {
                    edits.push(("caption-template", set(t.clone())));
                }
                ui.end_row();
                ui.label("");
                egui::ComboBox::from_id_salt("prop-caption-field")
                    .selected_text("Insert field...")
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        let mut add = |name: &str, shown: &str, ui: &mut egui::Ui| {
                            if ui.selectable_label(false, shown).clicked() {
                                let mut next = m.caption_template.clone();
                                if next.is_empty() {
                                    next.push_str("{value}");
                                }
                                if !next.ends_with(' ') {
                                    next.push(' ');
                                }
                                next.push_str(&format!("{{{name}}}"));
                                edits.push(("caption-field", set(next)));
                            }
                        };
                        for (name, shown) in CAPTION_FIELDS {
                            add(name, shown, ui);
                        }
                        for c in columns {
                            add(&format!("c:{}", c.id), &format!("{} (column)", c.name), ui);
                        }
                    });
                ui.end_row();
                label(ui, "Show All", mixed(all, |m| m.caption_template == "{all}"));
                let mut on = m.caption_template == "{all}";
                if ui
                    .checkbox(&mut on, "Show All Measurements")
                    .on_hover_text("Perimeter, area, wall area and volume in one caption")
                    .changed()
                {
                    edits.push(("caption-all", set(if on { "{all}".into() } else { String::new() })));
                }
                ui.end_row();
                label(ui, "Leader", mixed(all, |m| m.caption_leader));
                let mut lead = m.caption_leader;
                if ui
                    .checkbox(&mut lead, "Show Caption Leader Line")
                    .on_hover_text("Join a moved caption to its markup")
                    .changed()
                {
                    edits.push((
                        "caption-leader",
                        MarkupPatch {
                            caption_leader: Some(lead),
                            ..Default::default()
                        },
                    ));
                }
                ui.end_row();
            });
        });
}

fn takeoff(ui: &mut egui::Ui, m: &Markup, all: &[Markup], edits: &mut Edits) {
    let sloped = matches!(m.kind, Kind::Length | Kind::Polylength | Kind::Perimeter | Kind::Area);
    let units = matches!(m.kind, Kind::Area | Kind::Volume) && m.scale.as_ref().is_some_and(|s| s.valid());
    if !(sloped || closed_shape(m.kind) || units) {
        return;
    }
    egui::CollapsingHeader::new("Takeoff")
        .default_open(true)
        .show(ui, |ui| {
            Grid::new("props-takeoff").num_columns(2).show(ui, |ui| {
                if sloped {
                    label(ui, "Slope", mixed(all, |m| (m.slope_type, m.slope.to_bits())));
                    ui.horizontal(|ui| {
                        let current = SLOPES
                            .iter()
                            .find(|(_, c)| *c == m.slope_type)
                            .map_or("No slope", |(n, _)| n);
                        egui::ComboBox::from_id_salt("prop-slope")
                            .selected_text(current)
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                for (name, code) in SLOPES {
                                    if ui.selectable_label(m.slope_type == *code, *name).clicked() {
                                        let v = match (*code, m.slope) {
                                            (0, _) => 0.0,
                                            (_, v) if v != 0.0 => v,
                                            (1, _) => 4.0,
                                            (2, _) => 15.0,
                                            _ => 10.0,
                                        };
                                        edits.push((
                                            "slope-type",
                                            MarkupPatch {
                                                slope: Some((*code, v)),
                                                ..Default::default()
                                            },
                                        ));
                                    }
                                }
                            });
                        if m.slope_type != 0 {
                            let mut v = m.slope;
                            let max = if m.slope_type == 2 { 89.0 } else { 1000.0 };
                            if ui
                                .add(egui::DragValue::new(&mut v).range(-max..=max).speed(0.1))
                                .changed()
                            {
                                edits.push((
                                    "slope",
                                    MarkupPatch {
                                        slope: Some((m.slope_type, v)),
                                        ..Default::default()
                                    },
                                ));
                            }
                        }
                    });
                    ui.end_row();
                }
                if closed_shape(m.kind) {
                    label(ui, "Centroid", mixed(all, |m| m.show_centroid));
                    let mut on = m.show_centroid;
                    if ui.checkbox(&mut on, "Show Centroid").changed() {
                        edits.push((
                            "centroid",
                            MarkupPatch {
                                show_centroid: Some(on),
                                ..Default::default()
                            },
                        ));
                    }
                    ui.end_row();
                }
                if units && let Some(sc) = m.scale.as_ref() {
                    unit_row(
                        ui,
                        "Area unit",
                        "prop-area-unit",
                        format_unit(&sc.area),
                        |u| {
                            let mut s = sc.clone();
                            set_area_unit(&mut s, u).then_some(s)
                        },
                        |u| u.area_label(),
                        edits,
                    );
                    if m.kind == Kind::Volume {
                        unit_row(
                            ui,
                            "Volume unit",
                            "prop-volume-unit",
                            format_unit(&sc.volume),
                            |u| {
                                let mut s = sc.clone();
                                set_volume_unit(&mut s, u).then_some(s)
                            },
                            |u| u.volume_label(),
                            edits,
                        );
                    }
                }
            });
        });
}

fn unit_row(
    ui: &mut egui::Ui,
    text: &str,
    id: &str,
    current: Option<markupcraft_measure::units::LengthUnit>,
    make: impl Fn(markupcraft_measure::units::LengthUnit) -> Option<markupcraft_model::Scale>,
    name: fn(markupcraft_measure::units::LengthUnit) -> &'static str,
    edits: &mut Edits,
) {
    ui.label(text);
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.map_or("", name))
        .show_ui(ui, |ui| {
            for u in real_units() {
                if ui.selectable_label(current == Some(u), name(u)).clicked()
                    && let Some(s) = make(u)
                {
                    edits.push((
                        "units-own",
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
