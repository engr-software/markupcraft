//! The Properties toolbar (Window > Properties Toolbar): a strip under the main toolbar with the
//! common properties of the selected markups (colour, fill, line width, opacity, font size), or,
//! with nothing selected and a drawing tool active, of that tool's next markup (its Set as
//! Default look). Changes apply at once and are one undo step per gesture.

use egui::{Color32, RichText, Stroke};
use markupcraft_engine::MarkupPatch;
use markupcraft_model::{Color, Markup};

use crate::AppState;
use crate::theme::Tokens;

fn to32(c: Color) -> Color32 {
    let f = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(f(c.r), f(c.g), f(c.b))
}

fn from32(c: Color32) -> Color {
    Color::rgb(
        f64::from(c.r()) / 255.0,
        f64::from(c.g()) / 255.0,
        f64::from(c.b()) / 255.0,
    )
}

/// What the strip edits: the first selected markup (for the shown values) and whether it is
/// the selection (else the active tool's default look).
fn subject(app: &AppState) -> Option<(Markup, bool)> {
    let d = app.doc()?;
    if let Some(m) = d.selection().first().and_then(|id| d.session.doc().find(id)) {
        return Some((m.clone(), true));
    }
    let tool = crate::tools::find(app.tool)?;
    let kind = tool.creates()?;
    let m = app.template().0.cloned().or_else(|| {
        let pts = [
            markupcraft_geom::Point::new(0.0, 0.0),
            markupcraft_geom::Point::new(10.0, 10.0),
            markupcraft_geom::Point::new(20.0, 0.0),
        ];
        crate::tools::new_markup(kind, 0, &pts)
    })?;
    Some((m, false))
}

/// Apply `patch` to the selection, or make it the active tool's default look.
pub fn apply(app: &mut AppState, patch: &MarkupPatch) {
    let Some((m, selected)) = subject(app) else { return };
    if selected {
        if let Some(d) = app.doc_mut() {
            let ids = d.selection().to_vec();
            d.session.set_merge_key(Some("properties-toolbar"));
            let r = d.session.set_properties(&ids, patch);
            d.session.set_merge_key(None);
            if let Err(e) = r {
                app.status = e.to_string();
            }
        }
        return;
    }
    let mut m = m;
    if let Some(c) = patch.color {
        m.color = c;
    }
    if let Some(f) = patch.fill {
        m.fill = f;
    }
    if let Some(w) = patch.line_width {
        m.line_width = w;
    }
    if let Some(o) = patch.opacity {
        m.opacity = o;
    }
    if let Some(s) = patch.font_size {
        m.text.size = s;
    }
    if app.toolchest.set_default(&m).is_none() {
        app.status = "This tool has no look to set".into();
    }
}

/// The strip (drawn under the main toolbar when the preference is on).
pub fn bar(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.ui.extra.properties_toolbar || !app.shell.chrome_visible() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let subj = subject(app);
    let mut patch = MarkupPatch::default();
    egui::Panel::top("properties-toolbar")
        .exact_size(30.0)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(8, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let Some((m, selected)) = &subj else {
                    ui.label(
                        RichText::new("Select a markup or a drawing tool to edit its properties")
                            .size(11.0)
                            .color(t.text_muted),
                    );
                    return;
                };
                let what = if *selected {
                    let n = app.doc().map_or(0, |d| d.selection().len());
                    format!("{} ({n})", m.subject)
                } else {
                    format!("{} tool", m.subject)
                };
                ui.label(RichText::new(what).size(11.0).strong());
                ui.separator();
                ui.label(RichText::new("Color").size(11.0));
                let mut c = to32(m.color);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    patch.color = Some(from32(c));
                }
                ui.label(RichText::new("Fill").size(11.0));
                let mut has_fill = m.fill.is_some();
                if ui.checkbox(&mut has_fill, "").changed() {
                    patch.fill = Some(has_fill.then_some(m.fill.unwrap_or(m.color)));
                }
                if let Some(f) = m.fill {
                    let mut c = to32(f);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        patch.fill = Some(Some(from32(c)));
                    }
                }
                ui.label(RichText::new("Line").size(11.0));
                let mut w = m.line_width;
                if ui
                    .add(egui::DragValue::new(&mut w).range(0.0..=72.0).speed(0.1).suffix(" pt"))
                    .changed()
                {
                    patch.line_width = Some(w);
                }
                ui.label(RichText::new("Opacity").size(11.0));
                let mut o = (m.opacity * 100.0).round();
                if ui
                    .add(egui::DragValue::new(&mut o).range(0.0..=100.0).suffix(" %"))
                    .changed()
                {
                    patch.opacity = Some(o / 100.0);
                }
                if m.kind.is_text() {
                    ui.label(RichText::new("Font").size(11.0));
                    let mut s = m.text.size;
                    if ui
                        .add(egui::DragValue::new(&mut s).range(1.0..=400.0).suffix(" pt"))
                        .changed()
                    {
                        patch.font_size = Some(s);
                    }
                }
            });
        });
    if patch != MarkupPatch::default() {
        apply(app, &patch);
    }
}
