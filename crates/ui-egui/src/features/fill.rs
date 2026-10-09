//! Dynamic Fill (J): click inside a region bounded by page linework to make an Area (with
//! cutouts), a Polygon, a Perimeter or a Space; a floating bar holds the settings while the
//! tool is on, with Hatch for the selected markups. Also Measure > Legend.

use egui::RichText;
use markupcraft_engine::fill::{FillOptions, FillOutput};
use markupcraft_engine::hatch::{Hatch, HatchStyle};
use markupcraft_engine::legend::{LegendColumn, LegendOptions};
use markupcraft_geom::Point;
use markupcraft_model::Kind;

use super::Pick;
use crate::{AppState, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Output {
    #[default]
    Area,
    Polygon,
    Perimeter,
    Space,
}

pub struct FillState {
    pub output: Output,
    pub gap: f64,
    pub cutouts: bool,
    pub space_name: String,
    pub hatch: Hatch,
    pub message: String,
    pub legend_open: bool,
    pub legend: LegendOptions,
}

impl Default for FillState {
    fn default() -> Self {
        let o = FillOptions::default();
        Self {
            output: Output::Area,
            gap: o.gap.max(2.0),
            cutouts: o.cutouts,
            space_name: "Room".into(),
            hatch: Hatch::default(),
            message: String::new(),
            legend_open: false,
            legend: LegendOptions::default(),
        }
    }
}

/// Measure > Dynamic Fill.
pub fn start(app: &mut AppState) {
    super::start_pick(
        app,
        Pick::Fill,
        "Dynamic Fill: click inside a closed region; Esc when done",
    );
}

/// A click inside a region (Dynamic Fill, or a space by fill).
pub fn picked(app: &mut AppState, what: Pick, page: usize, pts: &[Point]) {
    let Some(seed) = pts.first().copied() else { return };
    let f = &app.features.fill;
    let opts = FillOptions {
        gap: f.gap.clamp(0.0, 72.0),
        cutouts: f.cutouts,
        boundaries: Vec::new(),
    };
    let space = |name: &str, n: usize| {
        if name.trim().is_empty() {
            format!("Space {n}")
        } else {
            name.trim().to_string()
        }
    };
    let n = app.doc().map_or(0, |d| d.session.spaces(Some(page)).len()) + 1;
    let output = match (what, f.output) {
        (Pick::SpaceFill, _) | (_, Output::Space) => FillOutput::Space(space(&f.space_name, n)),
        (_, Output::Area) => FillOutput::Area,
        (_, Output::Polygon) => FillOutput::Polygon,
        (_, Output::Perimeter) => FillOutput::Perimeter,
    };
    let kind = match output {
        FillOutput::Area => Some(Kind::Area),
        FillOutput::Polygon => Some(Kind::Polygon),
        FillOutput::Perimeter => Some(Kind::Perimeter),
        FillOutput::Space(_) => None,
    };
    let look = kind.and_then(|k| {
        app.toolchest
            .defaults
            .get(crate::tools::tool_for_kind(k).map_or("", |t| t.id))
            .cloned()
            .or_else(|| crate::tools::new_markup(k, page, &[seed, seed, seed]))
    });
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.dynamic_fill_create(page, seed, &opts, &output, look);
    let msg = actions::report(r, |(_, region)| {
        format!(
            "Filled a region of {} sq pt with {}",
            region.area.round(),
            actions::plural(region.holes.len(), "cutout")
        )
    });
    app.status = msg.clone();
    app.features.fill.message = msg;
}

/// The Dynamic Fill bar (while the tool is on) and the Legend dialog.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    let filling = matches!(app.features.pick, Some((_, Pick::Fill)));
    if filling {
        let selected = app.doc().map_or(0, |d| d.selection().len());
        let mut hatch = None;
        let mut done = false;
        egui::Window::new("Dynamic Fill")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 80.0])
            .show(ctx, |ui| {
                let f = &mut app.features.fill;
                ui.horizontal(|ui| {
                    ui.label("Make");
                    ui.selectable_value(&mut f.output, Output::Area, "Area");
                    ui.selectable_value(&mut f.output, Output::Polygon, "Polygon");
                    ui.selectable_value(&mut f.output, Output::Perimeter, "Perimeter");
                    ui.selectable_value(&mut f.output, Output::Space, "Space");
                });
                if f.output == Output::Space {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut f.space_name);
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Close gaps up to");
                    ui.add(egui::DragValue::new(&mut f.gap).range(0.0..=72.0).suffix(" pt"));
                });
                ui.checkbox(&mut f.cutouts, "Islands become cutouts");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Hatch");
                    egui::ComboBox::from_id_salt("fill-hatch")
                        .selected_text(f.hatch.style.name())
                        .show_ui(ui, |ui| {
                            for s in HatchStyle::ALL {
                                ui.selectable_value(&mut f.hatch.style, s, s.name());
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut f.hatch.spacing)
                            .range(1.0..=72.0)
                            .suffix(" pt"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(selected > 0, |ui| {
                        if ui.button(format!("Hatch selected ({selected})")).clicked() {
                            hatch = Some(true);
                        }
                        if ui.button("Remove hatch").clicked() {
                            hatch = Some(false);
                        }
                    });
                });
                if !f.message.is_empty() {
                    ui.label(RichText::new(&f.message).small());
                }
                if ui.button("Done").clicked() {
                    done = true;
                }
            });
        if let Some(on) = hatch {
            let h = app.features.fill.hatch;
            if let Some(d) = app.doc_mut() {
                let ids = d.selection().to_vec();
                let r = d.session.set_hatch(&ids, on.then_some(h));
                app.features.fill.message =
                    actions::report(r, |n| format!("Hatch on {}", actions::plural(n, "markup")));
            }
        }
        if done {
            super::end_pick(app);
        }
    }
    legend_window(app, ctx);
}

fn legend_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.fill.legend_open {
        return;
    }
    let (mut open, mut place, mut update) = (true, false, false);
    super::window("Legend").open(&mut open).show(ctx, |ui| {
        let l = &mut app.features.fill.legend;
        ui.horizontal(|ui| {
            ui.label("Title");
            ui.text_edit_singleline(&mut l.title);
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut l.document, false, "This page");
            ui.selectable_value(&mut l.document, true, "All pages");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Columns");
            for c in [
                LegendColumn::Symbol,
                LegendColumn::Subject,
                LegendColumn::Type,
                LegendColumn::Count,
                LegendColumn::Total,
            ] {
                let mut on = l.columns.contains(&c);
                if ui.checkbox(&mut on, c.name()).changed() {
                    if on {
                        l.columns.push(c);
                    } else {
                        l.columns.retain(|x| *x != c);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Font size");
            ui.add(egui::DragValue::new(&mut l.font_size).range(4.0..=48.0));
        });
        ui.checkbox(&mut l.measurements_only, "Measurements only");
        ui.horizontal(|ui| {
            if ui.button("Place...").clicked() {
                place = true;
            }
            if ui.button("Update Legends").clicked() {
                update = true;
            }
        });
    });
    if !open {
        app.features.fill.legend_open = false;
    }
    if place {
        super::start_pick(app, Pick::Legend, "Click where the legend's top-left corner goes");
    }
    if update && let Some(d) = app.doc_mut() {
        let r = d.session.update_legends();
        app.status = actions::report(r, |n| format!("Updated {}", actions::plural(n, "legend")));
    }
}

/// Where the legend goes was clicked.
pub fn legend_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(at) = pts.first().copied() else { return };
    let o = app.features.fill.legend.clone();
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.add_legend(page, at, &o);
    app.status = actions::report(r, |_| "Legend placed".into());
}
