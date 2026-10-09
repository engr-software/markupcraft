//! Thumbnails (Alt+T): every page as a small picture; click to go there.

use egui::{Align2, Color32, FontId, Sense, Stroke, Vec2, vec2};

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::theme::Tokens;

pub static PANEL: PanelDef = PanelDef {
    id: "thumbnails",
    title: "Thumbnails",
    icon: "layout-grid",
    slot: Slot::Left,
    keys: alt(egui::Key::T),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    app.thumbs_wanted = true;
    let t = Tokens::get(ui.ctx());
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    let Some(render) = doc.render.as_ref() else { return };
    let count = render.page_count();
    let width = (ui.available_width() - 24.0).clamp(60.0, 260.0);
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        for (i, g) in render.pages().iter().enumerate() {
            let h = width * g.height / g.width.max(1.0);
            ui.vertical_centered(|ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(width, h.min(width * 2.0)), Sense::click());
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, Color32::WHITE);
                    match doc.view.thumbs.get(&i) {
                        Some(tex) => {
                            p.image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            p.text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "...",
                                FontId::proportional(12.0),
                                t.text_faint,
                            );
                        }
                    }
                    let current = doc.view.current == i;
                    let stroke = if current {
                        Stroke::new(2.5, t.accent)
                    } else {
                        Stroke::new(1.0, t.border)
                    };
                    p.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
                }
                if resp.clicked() {
                    go = Some(i);
                }
                resp.on_hover_text(format!("Page {} of {count}", i + 1));
                ui.label(egui::RichText::new(&g.label).size(11.0).color(t.text_muted));
            });
            ui.add_space(8.0);
        }
        ui.allocate_space(Vec2::ZERO);
    });
    if let Some(i) = go {
        doc.view.go_to_page(i, count);
    }
}
