//! Panels around the workspace: the panel access bars (icon strips at the window's edges that
//! open and close panels), the panel tab's right-click menu (show another panel, hide this one,
//! attach it to the left, right or bottom, float it), the bottom panel across the window or
//! between the side panels, and auto-hide document tabs.

use egui::{Rect, Stroke};
use egui_dock::{DockState, NodeIndex, SurfaceIndex};

use super::extra::DockOp;
use crate::AppState;
use crate::dock::Tab;
use crate::panels::{PANELS, Slot};
use crate::theme::Tokens;

/// The access bars (when the preference is on): left-slot panels on the left edge, right- and
/// bottom-slot panels on the right. A click opens a closed panel or closes an open one.
pub fn bars(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.ui.extra.panel_bars || !app.shell.chrome_visible() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    for (side, slots) in [("left", &[Slot::Left][..]), ("right", &[Slot::Right, Slot::Bottom][..])] {
        let panel = if side == "left" {
            egui::Panel::left("panel-bar-left")
        } else {
            egui::Panel::right("panel-bar-right")
        };
        panel
            .exact_size(30.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(2, 6))
                    .stroke(Stroke::new(1.0, t.divider)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for p in PANELS.iter().filter(|p| slots.contains(&p.slot)) {
                        let open = app.open_panels.contains(&p.id);
                        let tip = match p.keys {
                            Some(k) => format!("{} ({})", p.title, k.label()),
                            None => p.title.to_string(),
                        };
                        if crate::icons::button(ui, p.icon, 26.0, open, &tip).clicked() {
                            app.queue(&format!("panel.{}", p.id));
                        }
                    }
                });
            });
    }
}

/// The right-click menu of a dock tab.
pub fn tab_menu(app: &mut AppState, ui: &mut egui::Ui, tab: &Tab) {
    let Tab::Panel(id) = *tab else {
        ui.label(egui::RichText::new("Documents").weak());
        return;
    };
    let ops = &mut app.shell.extra.dock_ops;
    ui.menu_button("Show Tab", |ui| {
        for p in PANELS.iter().filter(|p| p.id != id) {
            let open = app.open_panels.contains(&p.id);
            if ui.selectable_label(open, p.title).clicked() {
                ops.push(DockOp::Show(p.id));
                ui.close();
            }
        }
    });
    if ui.button("Hide").clicked() {
        ops.push(DockOp::Hide(id));
        ui.close();
    }
    ui.separator();
    for (slot, label) in [
        (Slot::Left, "Attach Left"),
        (Slot::Right, "Attach Right"),
        (Slot::Bottom, "Attach Bottom"),
    ] {
        if ui.button(label).clicked() {
            ops.push(DockOp::Attach(id, slot));
            ui.close();
        }
    }
    if ui
        .button("Split Below")
        .on_hover_text("Show this panel under the rest of its group")
        .clicked()
    {
        ops.push(DockOp::SplitBelow(id));
        ui.close();
    }
    if ui.button("Float").clicked() {
        ops.push(DockOp::Float(id));
        ui.close();
    }
}

/// Put panel `id` beside the document area on `slot`'s side (next to a panel already there).
pub fn attach(dock: &mut DockState<Tab>, id: &'static str, slot: Slot) {
    if let Some(path) = dock.find_tab(&Tab::Panel(id)) {
        dock.remove_tab(path);
    }
    // Join an open panel whose default side is `slot` (and is docked in the main surface).
    let sibling = PANELS
        .iter()
        .filter(|p| p.slot == slot && p.id != id)
        .filter_map(|p| dock.find_tab(&Tab::Panel(p.id)))
        .find(|p| p.node_path().surface == SurfaceIndex::main());
    if let Some(path) = sibling
        && let Ok(leaf) = dock.leaf_mut(path.node_path())
    {
        leaf.append_tab(Tab::Panel(id));
        if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
            let _ = dock.set_active_tab(p);
        }
        return;
    }
    let node = dock
        .find_tab(&Tab::Document)
        .map(|p| p.node_path())
        .filter(|n| n.surface == SurfaceIndex::main())
        .map_or(NodeIndex::root(), |n| n.node);
    let tree = dock.main_surface_mut();
    let tabs = vec![Tab::Panel(id)];
    let _ = match slot {
        Slot::Left => tree.split_left(node, 0.2, tabs),
        Slot::Right => tree.split_right(node, 0.78, tabs),
        Slot::Bottom => tree.split_below(node, 0.72, tabs),
    };
}

/// Take panel `id` out of its tab group and show it below the group (both visible at once).
pub fn split_below(dock: &mut DockState<Tab>, id: &'static str) {
    let Some(path) = dock.find_tab(&Tab::Panel(id)) else {
        return;
    };
    let node = path.node_path();
    let alone = dock.leaf(node).map_or(true, |l| l.tabs().len() < 2);
    if alone || node.surface != SurfaceIndex::main() {
        return;
    }
    dock.remove_tab(path);
    let _ = dock
        .main_surface_mut()
        .split_below(node.node, 0.5, vec![Tab::Panel(id)]);
}

/// The layout with the open panels in their default places: the bottom panel across the
/// window (`full`) or between the side panels.
pub fn arranged(open: &[&'static str], full: bool) -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Document]);
    let tabs = |slot: Slot| -> Vec<Tab> {
        PANELS
            .iter()
            .filter(|p| p.slot == slot && open.contains(&p.id))
            .map(|p| Tab::Panel(p.id))
            .collect()
    };
    let (left, right, bottom) = (tabs(Slot::Left), tabs(Slot::Right), tabs(Slot::Bottom));
    let tree = dock.main_surface_mut();
    let mut center = NodeIndex::root();
    if full && !bottom.is_empty() {
        [center, _] = tree.split_below(center, 0.7, bottom.clone());
    }
    if !left.is_empty() {
        [center, _] = tree.split_left(center, 0.17, left);
    }
    if !right.is_empty() {
        [center, _] = tree.split_right(center, 0.76, right);
    }
    if !full && !bottom.is_empty() {
        let _ = tree.split_below(center, 0.7, bottom);
    }
    dock
}

/// Apply the queued layout changes.
pub fn apply_ops(app: &mut AppState, dock: &mut DockState<Tab>) {
    for op in std::mem::take(&mut app.shell.extra.dock_ops) {
        match op {
            DockOp::Hide(id) => {
                if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
                    dock.remove_tab(p);
                }
            }
            DockOp::Show(id) => {
                if dock.find_tab(&Tab::Panel(id)).is_none() {
                    crate::dock::toggle_panel(dock, id);
                }
                crate::dock::focus_panel(dock, id);
            }
            DockOp::Attach(id, slot) => attach(dock, id, slot),
            DockOp::SplitBelow(id) => split_below(dock, id),
            DockOp::Float(id) => {
                if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
                    let at = Rect::from_min_size(egui::pos2(200.0, 150.0), egui::vec2(320.0, 420.0));
                    let _ = dock.detach_tab(p, at);
                }
            }
            DockOp::Arrange => {
                let open = crate::dock::open_panels(dock);
                *dock = arranged(&open, app.shell.ui.extra.bottom_full_width);
            }
        }
    }
}

/// Auto-hide tabs: the document tabs show only while the pointer is near the top of the
/// document area (or the preference is off).
pub fn tabs_visible(app: &mut AppState, ui: &egui::Ui) -> bool {
    let area = ui.available_rect_before_wrap();
    app.shell.extra.doc_area = Some(area);
    if !app.shell.ui.extra.auto_hide_tabs {
        return true;
    }
    let near = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
        p.x >= area.left() && p.x <= area.right() && p.y >= area.top() - 2.0 && p.y <= area.top() + 30.0
    });
    near || app.shell.extra.dragging_tab
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_panel_across_or_between_and_attach() {
        let open: Vec<&'static str> = PANELS.iter().map(|p| p.id).collect();
        for full in [true, false] {
            let d = arranged(&open, full);
            assert_eq!(crate::dock::open_panels(&d).len(), PANELS.len());
        }
        let mut d = arranged(&open, true);
        attach(&mut d, "thumbnails", Slot::Right);
        let path = d.find_tab(&Tab::Panel("thumbnails")).unwrap();
        let props = d.find_tab(&Tab::Panel("properties")).unwrap();
        assert_eq!(path.node_path(), props.node_path(), "joins the right-hand group");
        split_below(&mut d, "thumbnails");
        let path = d.find_tab(&Tab::Panel("thumbnails")).unwrap();
        let props = d.find_tab(&Tab::Panel("properties")).unwrap();
        assert_ne!(path.node_path(), props.node_path(), "its own group under the others");
        assert_eq!(crate::dock::open_panels(&d).len(), PANELS.len());
    }
}
