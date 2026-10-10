//! Panels around the workspace (`docs/UI_LAYOUT.md`): the panel bar (a vertical strip of panel
//! icons on the window's left edge), the left panel area (one panel at a time, chosen from the
//! bar; clicking the active icon collapses it), the bottom panel under the canvas (the Markups
//! List), the panel header (title with a dropdown, float and close buttons), the dock tab's
//! right-click menu for panels docked elsewhere (show another panel, hide this one, attach it to
//! the left, right or bottom, float it), the bottom panel across the window or beside the left
//! panel, and auto-hide document tabs.
//!
//! A panel is open in exactly one place: the left panel area, the bottom panel, or the dock
//! (where the user put it with Attach Right / Float, or a saved layout has it).

use egui::{Rect, RichText, Stroke};
use egui_dock::{DockState, NodeIndex, SurfaceIndex};

use super::extra::DockOp;
use crate::AppState;
use crate::dock::Tab;
use crate::panels::{PANELS, Slot};
use crate::theme::Tokens;

/// The panel bar's order, top down (any panel not listed follows).
pub const BAR_ORDER: &[&str] = &[
    "thumbnails",
    "bookmarks",
    "layers",
    "toolchest",
    "spaces",
    "properties",
    "measurements",
    "markups",
    "signatures",
    "links",
    "sets",
    "search",
    "file_access",
    "compare",
];

/// The panels in bar order.
pub fn bar_panels() -> Vec<&'static crate::panels::PanelDef> {
    let mut out: Vec<_> = BAR_ORDER.iter().filter_map(|id| crate::panels::find(id)).collect();
    out.extend(PANELS.iter().copied().filter(|p| !BAR_ORDER.contains(&p.id)));
    out
}

/// Whether panel `id` opens under the canvas (else in the left panel area).
pub fn opens_below(id: &str) -> bool {
    crate::panels::find(id).is_some_and(|p| p.slot == Slot::Bottom)
}

/// The panel shown in the left panel area (none while the panels are hidden).
pub fn left(app: &AppState) -> Option<&'static str> {
    if app.shell.panels_hidden {
        return None;
    }
    app.shell
        .ui
        .left_panel
        .as_deref()
        .and_then(crate::panels::find)
        .map(|p| p.id)
}

/// The panel shown under the canvas (none while the panels are hidden).
pub fn bottom(app: &AppState) -> Option<&'static str> {
    if app.shell.panels_hidden {
        return None;
    }
    app.shell
        .ui
        .bottom_panel
        .as_deref()
        .and_then(crate::panels::find)
        .map(|p| p.id)
}

/// Every open panel: the side areas' and the dock's.
pub fn open(app: &AppState, dock: &DockState<Tab>) -> Vec<&'static str> {
    let mut out = crate::dock::open_panels(dock);
    for p in [left(app), bottom(app)].into_iter().flatten() {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The side area panel `id` is in (or opens in: under the canvas for the Markups List and
/// Compare, else on the left).
fn side_slot<'a>(app: &'a mut AppState, id: &str) -> &'a mut Option<String> {
    let below = match (&app.shell.ui.left_panel, &app.shell.ui.bottom_panel) {
        (_, Some(b)) if b == id => true,
        (Some(l), _) if l == id => false,
        _ => opens_below(id),
    };
    if below {
        &mut app.shell.ui.bottom_panel
    } else {
        &mut app.shell.ui.left_panel
    }
}

/// Clear `id` from the side areas (it moved into the dock, or was hidden).
fn clear_side(app: &mut AppState, id: &str) {
    for s in [&mut app.shell.ui.left_panel, &mut app.shell.ui.bottom_panel] {
        if s.as_deref() == Some(id) {
            *s = None;
        }
    }
    app.shell.layout_dirty = true;
}

/// A panel command (`panel.<id>`, the panel bar, Window > Panels): close the panel where it is
/// open; otherwise open it in its side area, replacing the panel shown there.
pub fn toggle(app: &mut AppState, dock: &mut DockState<Tab>, id: &'static str) {
    if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
        dock.remove_tab(p);
        return;
    }
    let showing = !app.shell.panels_hidden && side_slot(app, id).as_deref() == Some(id);
    app.shell.panels_hidden = false;
    *side_slot(app, id) = if showing { None } else { Some(id.to_string()) };
    app.shell.layout_dirty = true;
}

/// Show panel `id` (open it if closed; bring it to the front where it is docked).
pub fn show(app: &mut AppState, dock: &mut DockState<Tab>, id: &'static str) {
    if dock.find_tab(&Tab::Panel(id)).is_some() {
        crate::dock::focus_panel(dock, id);
        return;
    }
    app.shell.panels_hidden = false;
    if side_slot(app, id).as_deref() != Some(id) {
        *side_slot(app, id) = Some(id.to_string());
        app.shell.layout_dirty = true;
    }
}

/// A panel docked by the user is not also shown in a side area.
pub fn dedupe(app: &mut AppState, dock: &DockState<Tab>) {
    for id in crate::dock::open_panels(dock) {
        if app.shell.ui.left_panel.as_deref() == Some(id) || app.shell.ui.bottom_panel.as_deref() == Some(id) {
            clear_side(app, id);
        }
    }
}

/// The panel bar (when the preference is on, as by default): every panel's icon on the left
/// edge. A click opens the panel in the left panel area (the Markups List under the canvas), or
/// closes it when it is the one showing.
pub fn bars(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.ui.extra.panel_bars || !app.shell.chrome_visible() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("panel-bar-left")
        .exact_size(34.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(2, 6))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("panel-bar")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for p in bar_panels() {
                            let open = app.open_panels.contains(&p.id);
                            let tip = match p.keys {
                                Some(k) => format!("{} ({})", crate::i18n::tr(p.title), k.label()),
                                None => crate::i18n::tr(p.title).to_string(),
                            };
                            if crate::icons::button(ui, p.icon, 28.0, open, &tip).clicked() {
                                app.queue(&format!("panel.{}", p.id));
                            }
                        }
                    });
                });
        });
}

/// The left panel area: the chosen panel under its header.
pub fn left_area(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() {
        return;
    }
    let Some(id) = left(app) else { return };
    let Some(p) = crate::panels::find(id) else { return };
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("left-panel-area")
        .resizable(true)
        .default_size(300.0)
        .min_size(180.0)
        .max_size(640.0)
        .frame(egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| side_panel(app, ui, p, &t));
}

/// The bottom panel under the canvas (across the left panel area and the canvas when the
/// bottom panel spans the window, else under the canvas only).
pub fn bottom_area(app: &mut AppState, ui: &mut egui::Ui, full: bool) {
    if !app.shell.chrome_visible() || app.shell.ui.extra.bottom_full_width != full {
        return;
    }
    let Some(id) = bottom(app) else { return };
    let Some(p) = crate::panels::find(id) else { return };
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("bottom-panel-area")
        .resizable(true)
        .default_size(260.0)
        .min_size(120.0)
        .frame(egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| side_panel(app, ui, p, &t));
}

/// A side panel: its header, then its contents. The contents draw in a child clipped to the
/// area, as in a dock tab, so a wide row (a selection's properties) never widens the panel
/// and shifts the canvas under the pointer; only the user resizes it.
fn side_panel(app: &mut AppState, ui: &mut egui::Ui, p: &'static crate::panels::PanelDef, t: &Tokens) {
    let area = ui.available_rect_before_wrap();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(area.intersect(ui.clip_rect()));
    egui::Frame::NONE
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(&mut child, |ui| {
            ui.set_min_width(ui.available_width());
            header(app, ui, p);
        });
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(&mut child, |ui| {
            ui.set_min_size(ui.available_size());
            (p.ui)(app, ui);
        });
    ui.advance_cursor_after_rect(area);
}

/// The panel header: the title with a dropdown (switch to another panel, attach the panel
/// elsewhere, float it), then Float and Close buttons.
fn header(app: &mut AppState, ui: &mut egui::Ui, p: &'static crate::panels::PanelDef) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let (at, _) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::hover());
        crate::icons::paint(ui, at, p.icon, 14.0, Tokens::get(ui.ctx()).icon);
        let title = crate::i18n::tr(p.title).to_string();
        let r = ui.add(egui::Button::new(RichText::new(title).strong()).frame(false));
        let more = crate::icons::button(ui, "chevron-down", 18.0, false, "Panels");
        let r = r.union(more);
        egui::Popup::menu(&r).show(|ui| {
            ui.set_min_width(200.0);
            for q in bar_panels()
                .into_iter()
                .filter(|q| opens_below(q.id) == opens_below(p.id))
            {
                if ui.selectable_label(q.id == p.id, crate::i18n::tr(q.title)).clicked() {
                    if q.id != p.id {
                        app.shell.extra.dock_ops.push(DockOp::Show(q.id));
                    }
                    ui.close();
                }
            }
            ui.separator();
            let mut attach = vec![(Slot::Right, "Attach Right")];
            if opens_below(p.id) {
                attach.insert(0, (Slot::Left, "Attach Left"));
            } else {
                attach.push((Slot::Bottom, "Attach Bottom"));
            }
            for (slot, label) in attach {
                if ui.button(label).clicked() {
                    app.shell.extra.dock_ops.push(DockOp::Attach(p.id, slot));
                    ui.close();
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::button(ui, "x", 20.0, false, "Close Panel").clicked() {
                app.shell.extra.dock_ops.push(DockOp::Hide(p.id));
            }
            if crate::icons::button(ui, "maximize", 20.0, false, "Float Panel").clicked() {
                app.shell.extra.dock_ops.push(DockOp::Float(p.id));
            }
        });
    });
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
                clear_side(app, id);
            }
            DockOp::Show(id) => show(app, dock, id),
            // Left and bottom are the side areas; Right docks the panel beside the document.
            DockOp::Attach(id, Slot::Left) | DockOp::Attach(id, Slot::Bottom) => {
                if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
                    dock.remove_tab(p);
                }
                clear_side(app, id);
                app.shell.panels_hidden = false;
                match op {
                    DockOp::Attach(_, Slot::Left) => app.shell.ui.left_panel = Some(id.to_string()),
                    _ => app.shell.ui.bottom_panel = Some(id.to_string()),
                }
            }
            DockOp::Attach(id, slot) => {
                clear_side(app, id);
                attach(dock, id, slot);
            }
            DockOp::SplitBelow(id) => split_below(dock, id),
            DockOp::Float(id) => {
                if dock.find_tab(&Tab::Panel(id)).is_none() {
                    clear_side(app, id);
                    attach(dock, id, Slot::Right);
                }
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
