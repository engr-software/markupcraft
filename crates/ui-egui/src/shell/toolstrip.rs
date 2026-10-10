//! The tool strip: a narrow vertical strip on the window's right edge with the markup tools in
//! groups (text, highlight and pen, stamp / image / snapshot, lines, shapes) and the
//! measurement tools at the bottom, a separator between groups and a grip at the top
//! (`docs/UI_LAYOUT.md`). Window > Tool Strip shows or hides it.

use egui::{Sense, Stroke, vec2};

use crate::theme::Tokens;
use crate::{AppState, commands, icons};

/// The markup groups, top down. Ids are tool or command ids (`tool.<id>`, `markup.image`).
pub const GROUPS: &[&[&str]] = &[
    &["tool.text", "tool.typewriter", "tool.callout", "tool.note"],
    &["tool.highlight", "tool.pen"],
    &["tool.stamp", "markup.image", "tool.snapshot"],
    &["tool.line", "tool.arrow", "tool.arc", "tool.polyline", "tool.dimension"],
    &["tool.rectangle", "tool.ellipse", "tool.polygon", "tool.cloud"],
];

/// The measurement group at the bottom of the strip.
pub const MEASURE: &[&str] = &[
    "tool.length",
    "tool.polylength",
    "tool.area",
    "tool.perimeter",
    "tool.count",
    "tool.angle",
    "tool.volume",
    "tool.calibrate",
];

const BUTTON: f32 = 26.0;
const WIDTH: f32 = 34.0;

/// The strip (after the bottom bar, so it runs between the top bars and the bottom bar).
pub fn strip(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() || !app.shell.ui.show_tool_strip {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("tool_strip")
        .exact_size(WIDTH)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(2, 4))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("tool-strip")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        grip(ui, &t);
                        for (k, group) in GROUPS.iter().enumerate() {
                            if k > 0 {
                                separator(ui, &t);
                            }
                            for id in *group {
                                button(app, ui, id);
                            }
                        }
                        // The measurement tools sit at the bottom when there is room.
                        let need = MEASURE.len() as f32 * (BUTTON + 2.0) + 8.0;
                        let room = ui.available_height() - need;
                        if room > 7.0 {
                            ui.add_space(room);
                        } else {
                            separator(ui, &t);
                        }
                        for id in MEASURE {
                            button(app, ui, id);
                        }
                    });
                });
        });
}

/// The grip at the top of the strip (three dots).
fn grip(ui: &mut egui::Ui, t: &Tokens) {
    let (rect, r) = ui.allocate_exact_size(vec2(BUTTON, 8.0), Sense::hover());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Tool strip"));
    for k in 0..3 {
        let x = rect.center().x + (k as f32 - 1.0) * 4.0;
        ui.painter()
            .circle_filled(egui::pos2(x, rect.center().y), 1.2, t.text_faint);
    }
}

fn separator(ui: &mut egui::Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(BUTTON, 5.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range().shrink(3.0), rect.center().y, Stroke::new(1.0, t.border));
}

/// An icon button for a tool or command: selected while that tool is active.
fn button(app: &mut AppState, ui: &mut egui::Ui, id: &str) {
    let Some((label, icon, keys)) = commands::describe(id) else {
        return;
    };
    let tip = match keys {
        Some(k) => format!("{label} ({})", k.label()),
        None => label,
    };
    let selected = app.checked(id).unwrap_or(false);
    let enabled = app.enabled(id) && commands::find(id).is_none_or(|c| c.built);
    let r = ui
        .add_enabled_ui(enabled, |ui| icons::button(ui, icon, BUTTON, selected, &tip))
        .inner;
    if r.clicked() {
        app.queue(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_strip_button_is_a_command_with_an_icon() {
        for id in GROUPS.iter().flat_map(|g| g.iter()).chain(MEASURE) {
            let d = commands::describe(id).unwrap_or_else(|| panic!("{id} is not a command"));
            assert!(icons::exists(d.1), "{id} has no icon ({})", d.1);
        }
    }
}
