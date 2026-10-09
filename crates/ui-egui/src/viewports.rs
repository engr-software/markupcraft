//! Viewports in the interface: the Add Viewport tool's dialog (name and scale, a preset or a
//! custom ratio), Highlight Viewports over the pages, and the Measurements panel's viewport
//! list (rename, rescale, delete, clear all, copy to other pages). Every change is an engine
//! call (`Session::add_viewport`, `rename_viewport`, `set_viewport_scale`, `clear_viewports`,
//! `copy_viewports`).

use egui::{Color32, FontId, Stroke, vec2};
use markupcraft_geom::Rect;
use markupcraft_measure::units::{LengthUnit, custom_scale, paper_units, real_units, scale_presets};
use markupcraft_model::{PageInfo, Scale};

use crate::painter::Xf;
use crate::{AppState, DocTab, actions, pages_from_text};

/// The Add Viewport dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct NewViewport {
    pub doc: u64,
    pub page: usize,
    pub rect: Rect,
    pub name: String,
    /// index into `scale_presets()`, or `None` for the custom ratio
    pub preset: Option<usize>,
    pub paper: String,
    pub paper_unit: LengthUnit,
    pub real: String,
    pub real_unit: LengthUnit,
}

/// Open the dialog for a box drawn with the Add Viewport tool.
pub fn open_dialog(app: &mut AppState, page: usize, rect: Rect) {
    let Some(uid) = app.doc().map(|d| d.uid) else { return };
    let n = app
        .doc()
        .and_then(|d| d.session.doc().pages.get(page))
        .map_or(0, |p| p.viewports.len());
    app.edit.viewport = Some(NewViewport {
        doc: uid,
        page,
        rect,
        name: format!("Viewport {}", n + 1),
        preset: scale_presets().iter().position(|p| p.name.starts_with("1/8")),
        paper: "1".into(),
        paper_unit: LengthUnit::Inch,
        real: "10".into(),
        real_unit: LengthUnit::Foot,
    });
}

impl NewViewport {
    /// The scale the dialog describes.
    pub fn scale(&self) -> Option<Scale> {
        match self.preset {
            Some(i) => scale_presets().get(i).map(|p| p.scale.clone()),
            None => match (self.paper.trim().parse::<f64>(), self.real.trim().parse::<f64>()) {
                (Ok(p), Ok(r)) if p > 0.0 && r > 0.0 && p.is_finite() && r.is_finite() => {
                    Some(custom_scale(p, self.paper_unit, r, self.real_unit, None, None))
                }
                _ => None,
            },
        }
    }
}

/// Add the viewport the dialog describes; returns the status text.
pub fn create(app: &mut AppState, v: &NewViewport) -> String {
    let Some(sc) = v.scale() else {
        return "Viewport: choose a preset or type two positive numbers".into();
    };
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == v.doc) else {
        return "The document was closed".into();
    };
    actions::report(d.session.add_viewport(v.page, v.rect, v.name.trim(), &sc), |_| {
        format!("Viewport {} at {}", v.name.trim(), sc.ratio)
    })
}

/// The preset / custom scale picker (for other viewport rows).
pub fn scale_picker_pub(ui: &mut egui::Ui, salt: &str, v: &mut NewViewport) {
    scale_picker(ui, salt, v);
}

fn scale_picker(ui: &mut egui::Ui, salt: &str, v: &mut NewViewport) {
    let presets = scale_presets();
    let text = v
        .preset
        .and_then(|i| presets.get(i))
        .map_or("Custom".to_string(), |p| p.name.clone());
    egui::ComboBox::from_id_salt(salt)
        .selected_text(text)
        .width(170.0)
        .show_ui(ui, |ui| {
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                if ui.selectable_label(v.preset.is_none(), "Custom").clicked() {
                    v.preset = None;
                }
                for (i, p) in presets.iter().enumerate() {
                    if ui.selectable_label(v.preset == Some(i), &p.name).clicked() {
                        v.preset = Some(i);
                    }
                }
            });
        });
    if v.preset.is_none() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut v.paper).desired_width(36.0));
            unit_combo(ui, (salt, "paper"), &mut v.paper_unit, &paper_units());
            ui.label("=");
            ui.add(egui::TextEdit::singleline(&mut v.real).desired_width(36.0));
            unit_combo(ui, (salt, "real"), &mut v.real_unit, &real_units());
        });
    }
}

fn unit_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    unit: &mut LengthUnit,
    choices: &[LengthUnit],
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(unit.label())
        .width(48.0)
        .show_ui(ui, |ui| {
            for u in choices {
                ui.selectable_value(unit, *u, u.name());
            }
        });
}

/// The Add Viewport window.
pub fn dialog(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut v) = app.edit.viewport.clone() else { return };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Add Viewport")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            egui::Grid::new("viewport-dialog")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(170.0));
                    ui.end_row();
                    ui.label("Scale");
                    ui.vertical(|ui| scale_picker(ui, "viewport-dialog-scale", &mut v));
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ok = ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        app.status = create(app, &v);
        app.edit.viewport = None;
    } else if cancel || !open {
        app.edit.viewport = None;
    } else {
        app.edit.viewport = Some(v);
    }
}

/// Highlight Viewports: each partial viewport's outline with its name and scale.
pub fn paint(p: &egui::Painter, xf: &Xf, info: &PageInfo) {
    let col = Color32::from_rgb(0, 140, 200);
    for v in info
        .viewports
        .iter()
        .filter(|v| v.bbox.normalized() != info.media.normalized())
    {
        let r = xf.rect_of(v.bbox);
        p.rect_filled(r, 0.0, col.gamma_multiply(0.06));
        p.rect_stroke(r, 0.0, Stroke::new(1.5, col), egui::StrokeKind::Middle);
        let name = if v.name.is_empty() { "Viewport" } else { v.name.as_str() };
        p.text(
            r.left_top() + vec2(4.0, 3.0),
            egui::Align2::LEFT_TOP,
            format!("{name}  {}", v.scale.ratio),
            FontId::proportional(11.0),
            col,
        );
    }
}

/// Panel fields for the viewport list.
#[derive(Debug, Clone, Default)]
pub struct PanelState {
    /// "all" or a range for Copy to Pages
    pub copy_to: String,
    /// the viewport whose scale is being changed, with the picker state
    pub rescale: Option<(usize, NewViewport)>,
    /// calibrate and separate Y (`viewports_more`)
    pub more: crate::viewports_more::ScaleMore,
}

/// The Measurements panel's Viewports section for `page`; returns a status text.
pub fn panel_section(
    ui: &mut egui::Ui,
    doc: &mut DocTab,
    page: usize,
    st: &mut PanelState,
    highlight: &mut bool,
) -> Option<String> {
    let info = doc.session.doc().pages.get(page)?.clone();
    let partial: Vec<(usize, markupcraft_model::Viewport)> = info
        .viewports
        .iter()
        .enumerate()
        .filter(|(_, v)| v.bbox.normalized() != info.media.normalized())
        .map(|(i, v)| (i, v.clone()))
        .collect();
    let mut status = None;
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Viewports").strong());
        ui.checkbox(highlight, "Highlight");
    });
    if partial.is_empty() {
        ui.label(
            egui::RichText::new("None on this page: Measure > Add Viewport draws one.")
                .weak()
                .size(11.0),
        );
        return None;
    }
    let mut delete = None;
    for (i, v) in &partial {
        ui.horizontal(|ui| {
            let mut name = v.name.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(90.0)
                    .hint_text("Viewport"),
            );
            if r.changed() {
                doc.session.set_merge_key(Some("viewport-name"));
                status = Some(actions::report(doc.session.rename_viewport(page, *i, &name), |_| {
                    "Viewport renamed".into()
                }));
                doc.session.set_merge_key(None);
            }
            if ui
                .link(&v.scale.ratio)
                .on_hover_text("Change this viewport's scale")
                .clicked()
            {
                let mut nv = NewViewport {
                    doc: 0,
                    page,
                    rect: v.bbox,
                    name: v.name.clone(),
                    preset: None,
                    paper: "1".into(),
                    paper_unit: LengthUnit::Inch,
                    real: "10".into(),
                    real_unit: LengthUnit::Foot,
                };
                nv.preset = scale_presets().iter().position(|p| p.scale.ratio == v.scale.ratio);
                st.rescale = Some((*i, nv));
            }
            if crate::icons::button(ui, "trash-2", 18.0, false, "Delete viewport").clicked() {
                delete = Some(*i);
            }
        });
    }
    if let Some((i, mut nv)) = st.rescale.take() {
        let mut keep = true;
        ui.group(|ui| {
            scale_picker(ui, "viewport-rescale", &mut nv);
            crate::viewports_more::rescale_rows(ui, &mut st.more, page, i, &nv);
            ui.horizontal(|ui| {
                if ui.button("Apply scale").clicked() {
                    keep = false;
                    status = Some(match nv.scale().map(|sc| crate::viewports_more::with_y(&st.more, sc)) {
                        Some(sc) => actions::report(doc.session.set_viewport_scale(page, i, &sc, true), |n| {
                            format!(
                                "Viewport scale {} ({} updated)",
                                sc.ratio,
                                actions::plural(n, "measurement")
                            )
                        }),
                        None => "Viewport: choose a preset or type two positive numbers".into(),
                    });
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
        if keep {
            st.rescale = Some((i, nv));
        }
    }
    if let Some(i) = delete {
        status = Some(actions::report(doc.session.delete_viewport(page, i), |_| {
            "Viewport deleted".into()
        }));
    }
    ui.horizontal(|ui| {
        if ui.button("Clear All").clicked() {
            status = Some(actions::report(doc.session.clear_viewports(page, true), |n| {
                format!("Removed {}", actions::plural(n, "viewport"))
            }));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Copy to");
        ui.add(
            egui::TextEdit::singleline(&mut st.copy_to)
                .hint_text("all")
                .desired_width(70.0),
        );
        if ui.button("Copy").clicked() {
            let count = doc.session.page_count();
            let text = if st.copy_to.trim().is_empty() {
                "all"
            } else {
                st.copy_to.as_str()
            };
            status = Some(match pages_from_text(text, page, count) {
                Some(pages) => actions::report(doc.session.copy_viewports(page, None, &pages), |n| {
                    format!("Copied {}", actions::plural(n, "viewport"))
                }),
                None => "Copy to: all, or a range like 2-5".into(),
            });
        }
    });
    status
}
