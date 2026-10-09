//! The panel registry. A panel is a dockable tab: write `pub static PANEL: PanelDef` in
//! `panels/<name>.rs` and add one line to [`PANELS`]. Its slot decides where it opens in the
//! default layout; Window menu entries and shortcuts come from the same row.

pub mod bookmarks;
pub mod markups_list;
pub mod measurements;
pub mod properties;
pub mod thumbnails;
pub mod toolchest;

use crate::AppState;
use crate::commands::Keys;

/// Where a panel sits in the default layout (Revu's arrangement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Left,
    Right,
    Bottom,
}

pub struct PanelDef {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub slot: Slot,
    /// Window menu shortcut (Revu's defaults: Alt+letter).
    pub keys: Option<Keys>,
    pub ui: fn(&mut AppState, &mut egui::Ui),
}

/// Every panel, in default tab order within each slot. One line per panel.
pub static PANELS: &[&PanelDef] = &[
    &thumbnails::PANEL,
    &bookmarks::PANEL,
    &properties::PANEL,
    &toolchest::PANEL,
    &measurements::PANEL,
    &markups_list::PANEL,
];

pub fn find(id: &str) -> Option<&'static PanelDef> {
    PANELS.iter().copied().find(|p| p.id == id)
}

/// A panel's empty state: one muted line.
pub(crate) fn empty(ui: &mut egui::Ui, text: &str) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text).color(t.text_faint));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for p in PANELS {
            assert!(ids.insert(p.id), "duplicate panel {}", p.id);
            assert!(crate::icons::exists(p.icon), "icon {}", p.icon);
        }
        for slot in [Slot::Left, Slot::Right, Slot::Bottom] {
            assert!(PANELS.iter().any(|p| p.slot == slot), "{slot:?} has no panel");
        }
    }
}
