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
        }
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut calibrate = false;
    let mut status = None;
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
    let current: Option<Scale> = info.scale.clone().filter(Scale::valid);
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
        if ui
            .button("Calibrate...")
            .on_hover_text("Click two points of a known distance")
            .clicked()
        {
            calibrate = true;
        }
        ui.add_space(6.0);
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
                            for p in scale_presets() {
                                if ui.selectable_label(false, &p.name).clicked() {
                                    new_scale = Some(p.scale.clone());
                                }
                            }
                        });
                    });
                ui.end_row();
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
                if ui.button("Apply custom scale").clicked() {
                    match (st.paper.trim().parse::<f64>(), st.real.trim().parse::<f64>()) {
                        (Ok(p), Ok(r)) if p > 0.0 && r > 0.0 => {
                            new_scale = Some(custom_scale(p, st.paper_unit, r, st.real_unit, None, None));
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
