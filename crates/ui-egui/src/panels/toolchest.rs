//! Tool Chest (Alt+X): the drawing tools, grouped like the menus. Saved tool sets with styles
//! (Revu's "My Tools") arrive with the tool chest wave; for now each entry picks a tool.

use egui::{RichText, Sense, vec2};

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::theme::Tokens;
use crate::tools::{TOOLS, ToolKind};

pub static PANEL: PanelDef = PanelDef {
    id: "toolchest",
    title: "Tool Chest",
    icon: "wrench",
    slot: Slot::Right,
    keys: alt(egui::Key::X),
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for group in ["Markup", "Measure"] {
            ui.add_space(6.0);
            ui.label(RichText::new(group).strong().color(t.text_muted));
            let tools: Vec<_> = TOOLS
                .iter()
                .filter(|tl| tl.menu == group && matches!(tl.kind, ToolKind::Drag(_)))
                .collect();
            if tools.is_empty() {
                ui.label(
                    RichText::new("Measurement tools arrive in the takeoff wave.")
                        .color(t.text_faint)
                        .size(11.0),
                );
                continue;
            }
            for tool in tools {
                let active = app.tool == tool.id;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                let p = ui.painter();
                if active {
                    p.rect_filled(rect, t.radius, t.accent_soft);
                } else if resp.hovered() {
                    p.rect_filled(rect, t.radius, t.hover);
                }
                let icon_rect = egui::Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(24.0, 24.0));
                crate::icons::paint(
                    ui,
                    icon_rect,
                    tool.icon,
                    16.0,
                    if active { t.accent_text } else { t.icon },
                );
                let key = tool.keys.map(|k| k.label()).unwrap_or_default();
                p.text(
                    rect.left_center() + vec2(34.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    tool.label,
                    egui::FontId::proportional(13.0),
                    t.text,
                );
                p.text(
                    rect.right_center() - vec2(6.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    key,
                    egui::FontId::proportional(11.0),
                    t.text_faint,
                );
                if resp.clicked() {
                    app.tool = tool.id;
                }
            }
        }
    });
}
