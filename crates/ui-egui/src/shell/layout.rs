//! The panel layout survives a restart (and travels with a profile): the dock state with each
//! tab written as its panel id. Panels that no longer exist are dropped on load; a layout
//! without the document area is rejected (the default layout is used instead).

use egui_dock::{DockState, Node};

use crate::dock::Tab;

const DOCUMENT: &str = "document";

/// The layout as JSON.
pub fn save(dock: &DockState<Tab>) -> Option<serde_json::Value> {
    let mut named: DockState<String> = dock.map_tabs(|t| match t {
        Tab::Document => DOCUMENT.to_string(),
        Tab::Panel(id) => format!("panel.{id}"),
    });
    // Screen rects are recomputed every frame; unset ones are infinite, which JSON cannot hold.
    for s in named.iter_surfaces_mut() {
        if let Some(tree) = s.node_tree_mut() {
            for n in tree.iter_mut() {
                match n {
                    Node::Leaf(l) => {
                        l.rect = egui::Rect::ZERO;
                        l.viewport = egui::Rect::ZERO;
                    }
                    Node::Vertical(sp) | Node::Horizontal(sp) => sp.rect = egui::Rect::ZERO,
                    Node::Empty => {}
                }
            }
        }
    }
    serde_json::to_value(&named).ok()
}

/// A saved layout back as a dock (`None` when it is unreadable or has no document area).
pub fn load(v: &serde_json::Value) -> Option<DockState<Tab>> {
    let named: DockState<String> = serde_json::from_value(v.clone()).ok()?;
    if !well_formed(&named) {
        return None;
    }
    let dock = named.filter_map_tabs(|name| {
        if name == DOCUMENT {
            return Some(Tab::Document);
        }
        name.strip_prefix("panel.")
            .and_then(crate::panels::find)
            .map(|p| Tab::Panel(p.id))
    });
    let docs = dock.iter_all_tabs().filter(|(_, t)| **t == Tab::Document).count();
    (docs == 1).then_some(dock)
}

/// The file is untrusted: one surface (the main window), every split with two real children
/// and a sane fraction, every leaf with tabs and an active tab inside them, at most 64 nodes.
fn well_formed(d: &DockState<String>) -> bool {
    if d.surfaces_count() != 1 {
        return false;
    }
    let Some(tree) = d.iter_surfaces().next().and_then(|s| s.node_tree()) else {
        return false;
    };
    let nodes: Vec<&Node<String>> = tree.iter().collect();
    if nodes.is_empty() || nodes.len() > 64 {
        return false;
    }
    let child = |i: usize| nodes.get(i).copied();
    nodes.iter().enumerate().all(|(i, n)| match n {
        Node::Vertical(s) | Node::Horizontal(s) => {
            s.fraction.is_finite()
                && (0.02..=0.98).contains(&s.fraction)
                && [2 * i + 1, 2 * i + 2]
                    .iter()
                    .all(|c| child(*c).is_some_and(|n| !matches!(n, Node::Empty)))
        }
        Node::Leaf(l) => {
            !l.tabs.is_empty()
                && l.active.0 < l.tabs.len()
                && l.tabs.len() <= 64
                && [2 * i + 1, 2 * i + 2]
                    .iter()
                    .all(|c| child(*c).is_none_or(|n| matches!(n, Node::Empty)))
        }
        Node::Empty => [2 * i + 1, 2 * i + 2]
            .iter()
            .all(|c| child(*c).is_none_or(|n| matches!(n, Node::Empty))),
    }) && !matches!(nodes.first(), Some(Node::Empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_round_trips_and_rejects_junk() {
        let mut dock = crate::dock::default_layout();
        crate::dock::toggle_panel(&mut dock, "bookmarks");
        let v = save(&dock).unwrap();
        let back = load(&v).unwrap_or_else(|| panic!("{v:#}"));
        assert_eq!(crate::dock::open_panels(&back), crate::dock::open_panels(&dock));
        assert!(load(&serde_json::json!({"nope": 1})).is_none());
        assert!(load(&serde_json::json!(5)).is_none());
    }
}
