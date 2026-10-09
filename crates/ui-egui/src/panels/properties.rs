//! Properties (Alt+P): the selected markups' fields. Changes apply to every selected markup
//! MarkupCraft can write; a slider drag or a typed word is one undo step.

use egui::{Grid, RichText};
use markupcraft_model::{Color, Markup, review_statuses};

use super::{PanelDef, Slot};
use crate::actions::{Session, editable};
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

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let DocTab {
            session,
            selection,
            view,
            ..
        } = doc;
        let first = selection.iter().find_map(|id| session.doc.find(id)).cloned();
        let Some(m) = first else {
            page_info(ui, session, view.current, &t);
            return;
        };
        let n = selection.len();
        let title = if n == 1 {
            m.kind.name().to_string()
        } else {
            format!("{n} markups")
        };
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().size(14.0));
        let writable = selection.iter().filter_map(|id| session.doc.find(id)).any(editable);
        if !writable {
            ui.label(
                RichText::new("Shown from the file. Editing this kind comes with its writer.")
                    .color(t.text_faint)
                    .size(11.0),
            );
        }
        ui.add_space(6.0);
        let ids = selection.clone();
        ui.add_enabled_ui(writable, |ui| fields(ui, session, &ids, &m));
    });
}

fn page_info(ui: &mut egui::Ui, session: &Session, page: usize, t: &Tokens) {
    ui.add_space(4.0);
    ui.label(RichText::new("No markup selected").strong());
    ui.label(
        RichText::new("Click a markup to see and change its properties.")
            .color(t.text_faint)
            .size(11.0),
    );
    ui.add_space(10.0);
    let Some(p) = session.doc.pages.get(page) else { return };
    Grid::new("page-props")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("Page");
            ui.label(format!("{}", page + 1));
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
            ui.label(format!("{}", session.doc.markups_on(page).count()));
            ui.end_row();
        });
}

fn color_row(ui: &mut egui::Ui, c: &Color) -> Option<Color> {
    let mut rgb = [c.r as f32, c.g as f32, c.b as f32];
    ui.color_edit_button_rgb(&mut rgb)
        .changed()
        .then(|| Color::rgb(f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2])))
}

fn fields(ui: &mut egui::Ui, s: &mut Session, ids: &[String], m: &Markup) {
    Grid::new("markup-props")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("Subject");
            let mut subject = m.subject.clone();
            if ui.text_edit_singleline(&mut subject).changed() {
                s.edit(ids, "subject", |x| x.subject = subject.clone());
            }
            ui.end_row();

            ui.label("Label");
            let mut label = m.label.clone();
            if ui.text_edit_singleline(&mut label).changed() {
                s.edit(ids, "label", |x| x.label = label.clone());
            }
            ui.end_row();

            ui.label("Author");
            ui.label(if m.author.is_empty() { "-" } else { m.author.as_str() });
            ui.end_row();

            ui.label("Color");
            if let Some(c) = color_row(ui, &m.color) {
                s.edit(ids, "color", |x| x.color = c);
            }
            ui.end_row();

            if markupcraft_revu::kinds::kind_for(m.kind).closed || m.fill.is_some() {
                ui.label("Fill");
                ui.horizontal(|ui| {
                    let mut on = m.fill.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        let c = m.color;
                        s.edit(ids, "fill-on", |x| x.fill = on.then_some(x.fill.unwrap_or(c)));
                    }
                    if let Some(f) = &m.fill
                        && let Some(c) = color_row(ui, f)
                    {
                        s.edit(ids, "fill", |x| x.fill = Some(c));
                    }
                });
                ui.end_row();

                ui.label("Fill opacity");
                let mut fo = (m.fill_opacity * 100.0).round();
                if ui.add(egui::Slider::new(&mut fo, 0.0..=100.0).suffix(" %")).changed() {
                    s.edit(ids, "fill-opacity", |x| x.fill_opacity = fo / 100.0);
                }
                ui.end_row();
            }

            ui.label("Opacity");
            let mut op = (m.opacity * 100.0).round();
            if ui.add(egui::Slider::new(&mut op, 0.0..=100.0).suffix(" %")).changed() {
                s.edit(ids, "opacity", |x| x.opacity = op / 100.0);
            }
            ui.end_row();

            ui.label("Line width");
            let mut w = m.line_width;
            if ui
                .add(egui::DragValue::new(&mut w).range(0.0..=72.0).speed(0.1).suffix(" pt"))
                .changed()
            {
                s.edit(ids, "width", |x| x.line_width = w);
            }
            ui.end_row();

            ui.label("Line style");
            let dashed = !m.dash.is_empty();
            egui::ComboBox::from_id_salt("line-style")
                .selected_text(if dashed { "Dashed" } else { "Solid" })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(!dashed, "Solid").clicked() {
                        s.edit(ids, "dash", |x| x.dash.clear());
                    }
                    if ui.selectable_label(dashed, "Dashed").clicked() {
                        s.edit(ids, "dash", |x| x.dash = vec![3.0, 3.0]);
                    }
                });
            ui.end_row();

            ui.label("Status");
            let current = if m.status.is_empty() { "None" } else { m.status.as_str() };
            egui::ComboBox::from_id_salt("status")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for st in review_statuses() {
                        if ui.selectable_label(current == *st, *st).clicked() {
                            let v = if *st == "None" { String::new() } else { st.to_string() };
                            s.edit(ids, "status", |x| x.status = v.clone());
                        }
                    }
                });
            ui.end_row();

            ui.label("Locked");
            let mut locked = m.locked();
            if ui.checkbox(&mut locked, "").changed() {
                s.edit(ids, "lock", |x| x.set_locked(locked));
            }
            ui.end_row();

            if !m.layer.is_empty() {
                ui.label("Layer");
                ui.label(&m.layer);
                ui.end_row();
            }
            if m.kind.is_measurement() {
                ui.label("Measurement");
                ui.label(RichText::new(m.quantity_text()).strong());
                ui.end_row();
                if let Some(sc) = &m.scale {
                    ui.label("Scale");
                    ui.label(&sc.ratio);
                    ui.end_row();
                }
            }
            ui.label("Page");
            ui.label(format!("{}", m.page + 1));
            ui.end_row();
        });
}
