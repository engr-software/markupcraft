//! Collapse panels by clicking the edge: a thin strip on each side of the window, beside the
//! panels; a click slides that side's panels shut, another click opens them again.

use egui::{Sense, Stroke};

use crate::AppState;
use crate::panels::{PANELS, Slot};
use crate::theme::Tokens;

/// The panels each edge collapsed (reopened by the next click).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeState {
    pub left: Vec<&'static str>,
    pub right: Vec<&'static str>,
}

/// Click the edge of `slot`'s side: collapse its open panels, or reopen the ones it collapsed.
pub fn toggle(app: &mut AppState, slot: Slot) {
    let collapsed = match slot {
        Slot::Left => std::mem::take(&mut app.shell.extra.edges.left),
        _ => std::mem::take(&mut app.shell.extra.edges.right),
    };
    if !collapsed.is_empty() {
        for id in collapsed {
            if !app.open_panels.contains(&id) {
                app.queue(&format!("panel.{id}"));
            }
        }
        app.status = "Panels opened".into();
        return;
    }
    let slots: &[Slot] = if slot == Slot::Left {
        &[Slot::Left]
    } else {
        &[Slot::Right, Slot::Bottom]
    };
    // The left edge: the left panel area's panel and left-docked panels; the right edge: the
    // other docked panels.
    let side = super::panelbars::left(app);
    let bottom = super::panelbars::bottom(app);
    let open: Vec<&'static str> = PANELS
        .iter()
        .filter(|p| app.open_panels.contains(&p.id) && Some(p.id) != bottom)
        .filter(|p| {
            if Some(p.id) == side {
                slot == Slot::Left
            } else {
                slots.contains(&p.slot)
            }
        })
        .map(|p| p.id)
        .collect();
    if open.is_empty() {
        return;
    }
    for id in &open {
        app.queue(&format!("panel.{id}"));
    }
    match slot {
        Slot::Left => app.shell.extra.edges.left = open,
        _ => app.shell.extra.edges.right = open,
    }
    app.status = "Panels collapsed: click the edge again to open them".into();
}

/// The edge strips: thin click targets over the window's left and right edges, beside the
/// panels (drawn over them, so the layout does not move).
pub fn strips(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() {
        return;
    }
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let screen = ctx.content_rect();
    // between the toolbars and the status bar
    let (top, bottom) = (screen.top() + 70.0, screen.bottom() - 60.0);
    if bottom - top < 40.0 {
        return;
    }
    for slot in [Slot::Left, Slot::Right] {
        let collapsed = match slot {
            Slot::Left => !app.shell.extra.edges.left.is_empty(),
            _ => !app.shell.extra.edges.right.is_empty(),
        };
        // Inside the panel bar and the tool strip (which stay at the window's edges).
        let bar = if app.shell.ui.extra.panel_bars { 34.0 } else { 0.0 };
        let strip = if app.shell.ui.show_tool_strip { 34.0 } else { 0.0 };
        let x = if slot == Slot::Left {
            screen.left() + bar
        } else {
            screen.right() - 5.0 - strip
        };
        let rect = egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + 5.0, bottom));
        let label = match (slot, collapsed) {
            (Slot::Left, false) => "Collapse the left panels",
            (Slot::Left, true) => "Open the left panels",
            (_, false) => "Collapse the right panels",
            (_, true) => "Open the right panels",
        };
        let mut clicked = false;
        egui::Area::new(egui::Id::new(("panel-edge", slot == Slot::Left)))
            .order(egui::Order::Middle)
            .fixed_pos(rect.min)
            .show(&ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(rect.size(), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
                if resp.hovered() {
                    ui.painter().rect_filled(r, 1.0, t.divider);
                    let mid = r.center();
                    let dir = if (slot == Slot::Left) != collapsed { -1.0 } else { 1.0 };
                    let c = t.text;
                    ui.painter().line_segment(
                        [mid + egui::vec2(-dir * 2.0, -5.0), mid + egui::vec2(dir * 2.0, 0.0)],
                        Stroke::new(1.0, c),
                    );
                    ui.painter().line_segment(
                        [mid + egui::vec2(dir * 2.0, 0.0), mid + egui::vec2(-dir * 2.0, 5.0)],
                        Stroke::new(1.0, c),
                    );
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                clicked = resp.on_hover_text(label).clicked();
            });
        if clicked {
            toggle(app, slot);
        }
    }
}
