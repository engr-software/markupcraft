//! Measurements (Alt+U): the scale of the current page and its viewports, a preset or custom
//! scale applied to this page, all pages or a range, Calibrate, the display unit and precision,
//! and the totals of the selected measurements.

use egui::{Grid, RichText};
use markupcraft_measure::units::{
    LengthUnit, custom_scale, display_unit_choices, paper_units, precision_choices, real_units, scale_display_unit,
    scale_precision, scale_presets, set_display_unit, set_precision,
};
use markupcraft_model::{Scale, measure_extras};

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::theme::Tokens;
use crate::{AppState, actions, pages_from_text};

pub static PANEL: PanelDef = PanelDef {
    id: "measurements",
    title: "Measurements",
    icon: "ruler",
    slot: Slot::Right,
    keys: alt(egui::Key::U),
    ui,
};

/// The panel's entry fields.
pub struct MeasureState {
    pub paper: String,
    pub paper_unit: LengthUnit,
    pub real: String,
    pub real_unit: LengthUnit,
    /// "" = this page, "all", or a range
    pub pages: String,
    pub update_markups: bool,
    /// Separate Y Scale: the vertical ratio's paper and real lengths
    pub separate_y: bool,
    pub y_paper: String,
    pub y_real: String,
    /// the name typed for Add Preset
    pub preset_name: String,
    /// apply scales as temporary (not saved)
    pub temporary: bool,
}

impl Default for MeasureState {
    fn default() -> Self {
        Self {
            paper: "1".into(),
            paper_unit: LengthUnit::Inch,
            real: "8".into(),
            real_unit: LengthUnit::Foot,
            pages: String::new(),
            update_markups: true,
            separate_y: false,
            y_paper: "1".into(),
            y_real: "8".into(),
            preset_name: String::new(),
            temporary: false,
        }
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut calibrate = false;
    let mut status = None;
    let tool_now = app.tool;
    let mut pick_tool: Option<&'static str> = None;
    let user_presets = app.toolchest.extras.scale_presets.clone();
    let mut remove_preset: Option<String> = None;
    let mut add_preset: Option<(String, Scale)> = None;
    let Some(doc) = app.docs.get_mut(app.active) else {
        super::empty(ui, "No document open.");
        return;
    };
    let st = &mut app.measure;
    let edit = &mut app.edit;
    let page = doc.view.current;
    let count = doc.session.page_count();
    let Some(info) = doc.session.doc().pages.get(page).cloned() else {
        return;
    };
    let current: Option<Scale> = info.page_scale().cloned();
    let mut new_scale: Option<Scale> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Page {} scale", page + 1)).strong());
        ui.label(
            RichText::new(
                current
                    .as_ref()
                    .map_or("Not set: measurements need a scale", |s| s.ratio.as_str()),
            )
            .color(if current.is_some() { t.text } else { t.text_faint }),
        );
        ui.add_space(6.0);
        let key = (doc.uid, page);
        let protected = edit.more.protected.contains(&key);
        if current.is_some() {
            let mut p = protected;
            if ui
                .checkbox(&mut p, "Protect scale")
                .on_hover_text("Lock this page's scale against changes")
                .changed()
            {
                if p {
                    edit.more.protected.insert(key);
                } else {
                    edit.more.protected.remove(&key);
                }
            }
        }
        if ui
            .add_enabled(!protected, egui::Button::new("Calibrate..."))
            .on_hover_text("Click two points of a known distance")
            .clicked()
        {
            calibrate = true;
        }
        ui.add_space(4.0);
        if let Some(t) = crate::more::measure_modes(ui, tool_now) {
            pick_tool = Some(t);
        }
        ui.add_space(6.0);
        ui.add_enabled_ui(!protected, |ui| {
            Grid::new("measure-scale")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Preset");
                    egui::ComboBox::from_id_salt("measure-preset")
                        .selected_text("Choose...")
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                // the user's presets first; only they can be deleted
                                for p in &user_presets {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(false, &p.name).clicked() {
                                            new_scale = Some(p.scale.clone());
                                        }
                                        if crate::icons::button(ui, "trash-2", 18.0, false, "Delete preset").clicked() {
                                            remove_preset = Some(p.name.clone());
                                        }
                                    });
                                }
                                if !user_presets.is_empty() {
                                    ui.separator();
                                }
                                for p in scale_presets() {
                                    if ui.selectable_label(false, &p.name).clicked() {
                                        new_scale = Some(p.scale.clone());
                                    }
                                }
                            });
                        });
                    ui.end_row();
                    if let Some(sc) = &current {
                        ui.label("");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut st.preset_name)
                                    .hint_text(sc.ratio.as_str())
                                    .desired_width(110.0),
                            );
                            if ui
                                .button("+ Add Preset")
                                .on_hover_text("Save this page's scale as a preset")
                                .clicked()
                            {
                                let name = if st.preset_name.trim().is_empty() {
                                    sc.ratio.clone()
                                } else {
                                    st.preset_name.trim().to_string()
                                };
                                add_preset = Some((name, sc.clone()));
                                st.preset_name.clear();
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("Custom");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut st.paper).desired_width(36.0));
                        unit_combo(ui, "paper-unit", &mut st.paper_unit, &paper_units());
                        ui.label("=");
                        ui.add(egui::TextEdit::singleline(&mut st.real).desired_width(36.0));
                        unit_combo(ui, "real-unit", &mut st.real_unit, &real_units());
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut st.separate_y, "Separate Y Scale");
                    ui.end_row();
                    if st.separate_y {
                        ui.label("Y");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut st.y_paper).desired_width(36.0));
                            ui.label(st.paper_unit.label());
                            ui.label("=");
                            ui.add(egui::TextEdit::singleline(&mut st.y_real).desired_width(36.0));
                            ui.label(st.real_unit.label());
                        });
                        ui.end_row();
                    }
                    ui.label("");
                    if ui.button("Apply custom scale").clicked() {
                        match (st.paper.trim().parse::<f64>(), st.real.trim().parse::<f64>()) {
                            (Ok(p), Ok(r)) if p > 0.0 && r > 0.0 => {
                                let mut s = custom_scale(p, st.paper_unit, r, st.real_unit, None, None);
                                if st.separate_y {
                                    match (st.y_paper.trim().parse::<f64>(), st.y_real.trim().parse::<f64>()) {
                                        (Ok(yp), Ok(yr)) if yp > 0.0 && yr > 0.0 => {
                                            let ys = custom_scale(yp, st.paper_unit, yr, st.real_unit, None, None);
                                            s.y = ys.x.clone();
                                            s.ratio = format!("{} (Y {})", s.ratio, ys.ratio);
                                            new_scale = Some(s);
                                        }
                                        _ => status = Some("Separate Y Scale: type two positive numbers".to_string()),
                                    }
                                } else {
                                    new_scale = Some(s);
                                }
                            }
                            _ => status = Some("Custom scale: type two positive numbers".to_string()),
                        }
                    }
                    ui.end_row();
                    ui.label("Apply to");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut st.pages)
                                .hint_text("this page")
                                .desired_width(90.0),
                        );
                        if ui.small_button("All").clicked() {
                            st.pages = "all".into();
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut st.update_markups, "Update measurements");
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut st.temporary, "Temporary (not saved)")
                        .on_hover_text("Measure with this scale until the document closes; the file keeps its own");
                    ui.end_row();
                    if let Some(sc) = &current {
                        ui.label("Units");
                        let du = scale_display_unit(sc);
                        egui::ComboBox::from_id_salt("measure-units")
                            .selected_text(du.map(|d| d.name()).unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for d in display_unit_choices() {
                                    if ui.selectable_label(Some(d) == du, d.name()).clicked() {
                                        let mut s = sc.clone();
                                        if set_display_unit(&mut s, d) {
                                            new_scale = Some(s);
                                        }
                                    }
                                }
                            });
                        ui.end_row();
                        ui.label("Precision");
                        let pr = scale_precision(sc);
                        egui::ComboBox::from_id_salt("measure-precision")
                            .selected_text(pr.name())
                            .show_ui(ui, |ui| {
                                let fractions = du.is_some_and(|d| d.feet_inches || d.unit == LengthUnit::Inch);
                                for p in precision_choices(fractions) {
                                    if ui.selectable_label(p == pr, p.name()).clicked() {
                                        let mut s = sc.clone();
                                        set_precision(&mut s, p);
                                        new_scale = Some(s);
                                    }
                                }
                            });
                        ui.end_row();
                    }
                })
        });

        // Viewports.
        if let Some(s) =
            crate::viewports::panel_section(ui, doc, page, &mut edit.viewports_panel, &mut edit.highlight_viewports)
        {
            status = Some(s);
        }

        // Totals of the selection.
        let sel = doc.selection().to_vec();
        let idx: Vec<usize> = doc
            .session
            .doc()
            .markups
            .iter()
            .enumerate()
            .filter(|(_, m)| sel.contains(&m.id) && m.kind.is_measurement())
            .map(|(i, _)| i)
            .collect();
        if !idx.is_empty() {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Selected ({})", actions::plural(idx.len(), "measurement"))).strong());
            for tot in measure_extras::totals(doc.session.doc(), &idx) {
                ui.label(format!("{}  ({})", tot.text, actions::plural(tot.items, "item")));
            }
        }
    });
    if let Some(sc) = new_scale {
        status = Some(match pages_from_text(&st.pages, page, count) {
            Some(pages) if st.temporary => actions::report(
                doc.session.set_page_scale_temporary(&pages, &sc, st.update_markups),
                |n| {
                    format!(
                        "Temporary scale {} on {} ({} updated; not saved)",
                        sc.ratio,
                        actions::plural(pages.len(), "page"),
                        actions::plural(n, "measurement")
                    )
                },
            ),
            Some(pages) => actions::report(doc.session.set_page_scale(&pages, &sc, st.update_markups), |n| {
                format!(
                    "Scale {} on {} ({} updated)",
                    sc.ratio,
                    actions::plural(pages.len(), "page"),
                    actions::plural(n, "measurement")
                )
            }),
            None => "Apply to: leave empty for this page, type all, or a range like 1-3, 7".into(),
        });
    }
    if let Some(s) = status {
        app.status = s;
    }
    if calibrate {
        app.set_tool("calibrate");
    }
    if let Some(n) = remove_preset {
        app.toolchest.remove_scale_preset(&n);
        app.status = format!("Deleted the scale preset {n}");
    }
    if let Some((n, sc)) = add_preset {
        app.status = match app.toolchest.add_scale_preset(&n, &sc) {
            Ok(()) => format!("Saved the scale preset {n}"),
            Err(e) => e,
        };
    }
    if let Some(t) = pick_tool {
        app.edit.more.measure_tool = t;
        app.set_tool(t);
    }
}

fn unit_combo(ui: &mut egui::Ui, id: &str, unit: &mut LengthUnit, choices: &[LengthUnit]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(unit.label())
        .width(48.0)
        .show_ui(ui, |ui| {
            for u in choices {
                ui.selectable_value(unit, *u, u.name());
            }
        });
}
