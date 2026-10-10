//! The docking layout (egui_dock, MIT): the document area in the middle, panels around it in
//! Revu's default arrangement (Thumbnails and Bookmarks left, Properties and Tool Chest right,
//! Markups List at the bottom). Panels can be dragged, stacked, split, undocked and closed;
//! Window menu entries reopen them.

use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabViewer};

use crate::AppState;
use crate::panels::{PANELS, Slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Document,
    Panel(&'static str),
}

fn slot_tabs(slot: Slot) -> Vec<Tab> {
    PANELS
        .iter()
        .filter(|p| p.slot == slot)
        .map(|p| Tab::Panel(p.id))
        .collect()
}

/// Revu's default arrangement.
pub fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Document]);
    let tree = dock.main_surface_mut();
    let mut center = NodeIndex::root();
    let bottom = slot_tabs(Slot::Bottom);
    if !bottom.is_empty() {
        [center, _] = tree.split_below(center, 0.7, bottom);
    }
    // egui_dock's fraction is the share of the first (left / upper) child.
    let left = slot_tabs(Slot::Left);
    if !left.is_empty() {
        [center, _] = tree.split_left(center, 0.17, left);
    }
    let right = slot_tabs(Slot::Right);
    if !right.is_empty() {
        let _ = tree.split_right(center, 0.76, right);
    }
    dock
}

/// Which panels are open.
pub fn open_panels(dock: &DockState<Tab>) -> Vec<&'static str> {
    dock.iter_all_tabs()
        .filter_map(|(_, t)| match t {
            Tab::Panel(id) => Some(*id),
            Tab::Document => None,
        })
        .collect()
}

/// Close the panel if open; otherwise open it next to a panel of the same slot (or split the
/// document area on that side).
pub fn toggle_panel(dock: &mut DockState<Tab>, id: &'static str) {
    if let Some(path) = dock.find_tab(&Tab::Panel(id)) {
        dock.remove_tab(path);
        return;
    }
    let Some(def) = crate::panels::find(id) else { return };
    let sibling = PANELS
        .iter()
        .filter(|p| p.slot == def.slot && p.id != id)
        .find_map(|p| dock.find_tab(&Tab::Panel(p.id)));
    if let Some(path) = sibling
        && let Ok(leaf) = dock.leaf_mut(path.node_path())
    {
        leaf.append_tab(Tab::Panel(id));
        if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
            let _ = dock.set_active_tab(p);
        }
        return;
    }
    let Some(doc) = dock.find_tab(&Tab::Document) else {
        dock.push_to_first_leaf(Tab::Panel(id));
        return;
    };
    let node = doc.node_path();
    if node.surface != SurfaceIndex::main() {
        dock.push_to_first_leaf(Tab::Panel(id));
        return;
    }
    let tree = dock.main_surface_mut();
    let tabs = vec![Tab::Panel(id)];
    let _ = match def.slot {
        Slot::Left => tree.split_left(node.node, 0.2, tabs),
        Slot::Right => tree.split_right(node.node, 0.78, tabs),
        Slot::Bottom => tree.split_below(node.node, 0.72, tabs),
    };
}

/// Bring an open panel to the front of its tab group.
pub fn focus_panel(dock: &mut DockState<Tab>, id: &'static str) {
    if let Some(p) = dock.find_tab(&Tab::Panel(id)) {
        let _ = dock.set_active_tab(p);
    }
}

/// Draws each dock tab.
pub struct Viewer<'a> {
    pub app: &'a mut AppState,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Document => "Documents".into(),
            Tab::Panel(id) => crate::i18n::tr(crate::panels::find(id).map_or(*id, |p| p.title))
                .to_string()
                .into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Document => crate::chrome::document_area(self.app, ui),
            Tab::Panel(id) => {
                if let Some(p) = crate::panels::find(id) {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(6, 4))
                        .show(ui, |ui| (p.ui)(self.app, ui));
                }
            }
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: egui_dock::NodePath) {
        crate::shell::panelbars::tab_menu(self.app, ui, tab);
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        !matches!(tab, Tab::Document)
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        !matches!(tab, Tab::Document)
    }

    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }

    fn clear_background(&self, _tab: &Tab) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_every_panel_and_toggles() {
        let mut dock = default_layout();
        let open = open_panels(&dock);
        assert_eq!(open.len(), PANELS.len());
        toggle_panel(&mut dock, "bookmarks");
        assert!(!open_panels(&dock).contains(&"bookmarks"));
        toggle_panel(&mut dock, "bookmarks");
        assert!(open_panels(&dock).contains(&"bookmarks"));
        // Close a whole slot, then reopen: the panel comes back beside the document.
        toggle_panel(&mut dock, "markups");
        toggle_panel(&mut dock, "markups");
        assert!(open_panels(&dock).contains(&"markups"));
        assert!(dock.find_tab(&Tab::Document).is_some());
    }
}
