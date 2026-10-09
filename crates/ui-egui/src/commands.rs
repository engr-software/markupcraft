//! The command table: one row per menu / toolbar command (id, label, menu, shortcut, icon).
//! Menus, toolbars, tooltips, the shortcut dispatcher and Help > Keyboard Shortcuts all read
//! it. Tools (`tools::TOOLS`) and panels (`panels::PANELS`) add their own rows with ids
//! `tool.<id>` and `panel.<id>`.
//!
//! Default keys follow Revu 21's public shortcut list (`docs/revu_features/05_shortcuts.md`).
//! To add a command: one row in [`COMMANDS`], one arm in [`crate::AppState::run`].

use egui::{Key, KeyboardShortcut, Modifiers};

/// A key with modifiers. `ctrl` is the platform command key (Ctrl, or Cmd on macOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Keys {
    pub const fn new(ctrl: bool, shift: bool, alt: bool, key: Key) -> Self {
        Self { ctrl, shift, alt, key }
    }

    pub fn shortcut(&self) -> KeyboardShortcut {
        KeyboardShortcut::new(
            Modifiers {
                alt: self.alt,
                ctrl: false,
                shift: self.shift,
                mac_cmd: false,
                command: self.ctrl,
            },
            self.key,
        )
    }

    /// `Ctrl+Shift+S` style text.
    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str(if cfg!(target_os = "macos") { "Cmd+" } else { "Ctrl+" });
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str(if cfg!(target_os = "macos") { "Option+" } else { "Alt+" });
        }
        s.push_str(match self.key {
            Key::Plus => "Plus",
            Key::Minus => "Minus",
            Key::ArrowLeft => "Left",
            Key::ArrowRight => "Right",
            Key::Delete => "Del",
            Key::Escape => "Esc",
            k => k.name(),
        });
        s
    }

    /// How many modifiers: more specific shortcuts are matched first.
    pub fn specificity(&self) -> usize {
        usize::from(self.ctrl) + usize::from(self.shift) + usize::from(self.alt)
    }
}

pub const fn key(k: Key) -> Option<Keys> {
    Some(Keys::new(false, false, false, k))
}
pub const fn ctrl(k: Key) -> Option<Keys> {
    Some(Keys::new(true, false, false, k))
}
pub const fn ctrl_shift(k: Key) -> Option<Keys> {
    Some(Keys::new(true, true, false, k))
}
pub const fn shift(k: Key) -> Option<Keys> {
    Some(Keys::new(false, true, false, k))
}
pub const fn alt(k: Key) -> Option<Keys> {
    Some(Keys::new(false, false, true, k))
}
pub const fn shift_alt(k: Key) -> Option<Keys> {
    Some(Keys::new(false, true, true, k))
}

/// One command.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    /// Top-level menu it appears in ("" = toolbar / shortcut only).
    pub menu: &'static str,
    /// Menu section: a separator goes between groups.
    pub group: u8,
    pub keys: Option<Keys>,
    /// A second key for the same command (e.g. `=` for Zoom In).
    pub alias: Option<Keys>,
    /// Lucide icon name ("" = none).
    pub icon: &'static str,
    /// `false` = listed for orientation but not built yet (shown disabled).
    pub built: bool,
}

const fn cmd(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    group: u8,
    keys: Option<Keys>,
    icon: &'static str,
) -> Command {
    Command {
        id,
        label,
        menu,
        group,
        keys,
        alias: None,
        icon,
        built: true,
    }
}

const fn later(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    group: u8,
    keys: Option<Keys>,
    icon: &'static str,
) -> Command {
    Command {
        built: false,
        ..cmd(id, label, menu, group, keys, icon)
    }
}

const fn with_alias(c: Command, alias: Option<Keys>) -> Command {
    Command { alias, ..c }
}

/// The menu bar, in order.
pub const MENUS: &[&str] = &[
    "File", "Edit", "View", "Markup", "Measure", "Tools", "Document", "Batch", "Window", "Help",
];

#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    // File
    cmd("file.open", "Open...", "File", 1, ctrl(Key::O), "folder-open"),
    cmd("file.close", "Close", "File", 1, ctrl(Key::F4), "file-x"),
    cmd("file.save", "Save", "File", 2, ctrl(Key::S), "save"),
    cmd("file.save_as", "Save As...", "File", 2, ctrl_shift(Key::S), "file-down"),
    later("file.print", "Print...", "File", 3, ctrl(Key::P), "printer"),
    cmd("file.exit", "Exit", "File", 4, None, ""),
    // Edit
    cmd("edit.undo", "Undo", "Edit", 1, ctrl(Key::Z), "undo-2"),
    cmd("edit.redo", "Redo", "Edit", 1, ctrl(Key::Y), "redo-2"),
    later("edit.cut", "Cut", "Edit", 2, ctrl(Key::X), "scissors"),
    later("edit.copy", "Copy", "Edit", 2, ctrl(Key::C), "copy"),
    later("edit.paste", "Paste", "Edit", 2, ctrl(Key::V), "clipboard-paste"),
    cmd("edit.delete", "Delete", "Edit", 3, key(Key::Delete), "trash-2"),
    cmd("edit.select_all", "Select All", "Edit", 3, ctrl(Key::A), ""),
    cmd("edit.deselect", "Deselect All", "Edit", 3, key(Key::Escape), ""),
    // View
    with_alias(cmd("view.zoom_in", "Zoom In", "View", 1, key(Key::Plus), "zoom-in"), key(Key::Equals)),
    cmd("view.zoom_out", "Zoom Out", "View", 1, key(Key::Minus), "zoom-out"),
    cmd("view.actual_size", "Actual Size", "View", 2, ctrl(Key::Num8), ""),
    cmd("view.fit_page", "Fit Page", "View", 2, ctrl(Key::Num9), "maximize"),
    cmd("view.fit_width", "Fit Width", "View", 2, ctrl(Key::Num0), "arrow-left-right"),
    cmd("view.single_page", "Single Page", "View", 3, ctrl(Key::Num4), "file-text"),
    cmd("view.continuous", "Continuous", "View", 3, ctrl(Key::Num5), "rows-3"),
    cmd("view.first_page", "First Page", "View", 4, key(Key::Home), "chevrons-left"),
    with_alias(cmd("view.prev_page", "Previous Page", "View", 4, ctrl(Key::ArrowLeft), "chevron-left"), key(Key::PageUp)),
    with_alias(cmd("view.next_page", "Next Page", "View", 4, ctrl(Key::ArrowRight), "chevron-right"), key(Key::PageDown)),
    cmd("view.last_page", "Last Page", "View", 4, key(Key::End), "chevrons-right"),
    cmd("view.hide_markups", "Hide Markups", "View", 5, None, "eye-off"),
    // Markup (drawing tools come from the tool registry)
    cmd("markup.lock", "Lock", "Markup", 9, ctrl_shift(Key::L), "lock"),
    // Measure
    later("measure.calibrate", "Calibrate...", "Measure", 9, None, "ruler"),
    // Tools (Select / Pan come from the tool registry)
    // Document
    cmd("document.properties", "Document Properties", "Document", 1, ctrl(Key::D), "info"),
    later("document.insert_pages", "Insert Pages...", "Document", 2, ctrl_shift(Key::I), ""),
    later("document.delete_pages", "Delete Pages...", "Document", 2, ctrl_shift(Key::D), ""),
    later("document.flatten", "Flatten...", "Document", 3, ctrl_shift(Key::M), ""),
    // Batch
    later("batch.summary", "Summary...", "Batch", 1, None, "list-checks"),
    later("batch.flatten", "Flatten...", "Batch", 1, None, ""),
    // Window (panels come from the panel registry)
    cmd("window.reset_layout", "Reset Panel Layout", "Window", 9, None, ""),
    cmd("window.next_document", "Next Document", "Window", 10, ctrl(Key::Tab), ""),
    cmd("window.prev_document", "Previous Document", "Window", 10, ctrl_shift(Key::Tab), ""),
    // Help
    cmd("help.shortcuts", "Keyboard Shortcuts", "Help", 1, key(Key::F1), "circle-help"),
    cmd("help.about", "About MarkupCraft", "Help", 2, None, "info"),
    // Status bar / toolbar only
    cmd("snap.grid", "Grid", "", 0, shift(Key::F9), "grid-3x3"),
    cmd("snap.content", "Snap to Content", "", 0, ctrl_shift(Key::F8), "scan"),
    cmd("snap.markup", "Snap to Markup", "", 0, ctrl_shift(Key::F7), "square-dashed-mouse-pointer"),
];

/// The main toolbar ("|" = separator).
pub const MAIN_TOOLBAR: &[&str] = &[
    "file.open",
    "file.save",
    "|",
    "edit.undo",
    "edit.redo",
    "|",
    "tool.select",
    "tool.pan",
    "|",
    "view.zoom_out",
    "view.zoom_in",
    "view.fit_page",
    "view.fit_width",
    "|",
    "view.single_page",
    "view.continuous",
    "|",
    "edit.delete",
];

pub fn find(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Every key binding (commands, tools, panels), most specific first, for the dispatcher.
pub fn bindings() -> Vec<(Keys, String)> {
    let mut out: Vec<(Keys, String)> = Vec::new();
    for c in COMMANDS.iter().filter(|c| c.built) {
        for k in [c.keys, c.alias].into_iter().flatten() {
            out.push((k, c.id.to_string()));
        }
    }
    for t in crate::tools::TOOLS {
        if let Some(k) = t.keys {
            out.push((k, format!("tool.{}", t.id)));
        }
    }
    for p in crate::panels::PANELS {
        if let Some(k) = p.keys {
            out.push((k, format!("panel.{}", p.id)));
        }
    }
    out.sort_by_key(|(k, _)| std::cmp::Reverse(k.specificity()));
    out
}

/// Label, icon and key text for any command id (including `tool.` and `panel.` ids).
pub fn describe(id: &str) -> Option<(String, &'static str, Option<Keys>)> {
    if let Some(t) = id.strip_prefix("tool.").and_then(crate::tools::find) {
        return Some((t.label.to_string(), t.icon, t.keys));
    }
    if let Some(p) = id.strip_prefix("panel.").and_then(crate::panels::find) {
        return Some((p.title.to_string(), p.icon, p.keys));
    }
    find(id).map(|c| (c.label.to_string(), c.icon, c.keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_keys_are_unique_and_icons_exist() {
        let mut ids = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(ids.insert(c.id), "duplicate id {}", c.id);
            assert!(c.icon.is_empty() || crate::icons::exists(c.icon), "icon {}", c.icon);
            assert!(c.menu.is_empty() || MENUS.contains(&c.menu), "menu {}", c.menu);
        }
        let mut keys = std::collections::HashSet::new();
        for (k, id) in bindings() {
            assert!(
                keys.insert((k.ctrl, k.shift, k.alt, k.key)),
                "{} bound twice ({id})",
                k.label()
            );
        }
        for id in MAIN_TOOLBAR.iter().filter(|i| **i != "|") {
            assert!(describe(id).is_some(), "toolbar id {id}");
        }
    }

    #[test]
    fn specific_shortcuts_come_first() {
        let b = bindings();
        let save_as = b.iter().position(|(_, id)| id == "file.save_as");
        let save = b.iter().position(|(_, id)| id == "file.save");
        assert!(save_as < save);
        assert_eq!(
            find("file.save_as").and_then(|c| c.keys).map(|k| k.label()).as_deref(),
            Some(if cfg!(target_os = "macos") {
                "Cmd+Shift+S"
            } else {
                "Ctrl+Shift+S"
            })
        );
    }
}
