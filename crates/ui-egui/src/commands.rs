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
pub const fn ctrl_alt(k: Key) -> Option<Keys> {
    Some(Keys::new(true, false, true, k))
}
pub const fn ctrl_shift_alt(k: Key) -> Option<Keys> {
    Some(Keys::new(true, true, true, k))
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

#[allow(dead_code)] // for rows listed before they are built
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
    "MarkupCraft",
    "File",
    "Edit",
    "View",
    "Markup",
    "Measure",
    "Tools",
    "Document",
    "Batch",
    "Window",
    "Help",
];

#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    // File
    cmd("file.new", "New Blank PDF", "File", 1, ctrl(Key::N), "file-plus"),
    cmd("file.open", "Open...", "File", 1, ctrl(Key::O), "folder-open"),
    cmd("file.close", "Close", "File", 1, ctrl(Key::F4), "file-x"),
    cmd("file.close_others", "Close Others", "File", 1, None, ""),
    cmd("file.close_all", "Close All", "File", 1, ctrl_shift(Key::W), ""),
    cmd("file.save", "Save", "File", 2, ctrl(Key::S), "save"),
    cmd("file.save_as", "Save As...", "File", 2, ctrl_shift(Key::S), "file-down"),
    cmd("file.save_all", "Save All", "File", 2, shift(Key::F2), "save-all"),
    cmd("file.refresh", "Refresh Document", "File", 2, shift(Key::F5), ""),
    cmd("file.clear_recent", "Clear Recent Files", "", 0, None, ""),
    cmd("file.print", "Print...", "File", 3, ctrl(Key::P), "printer"),
    cmd("file.exit", "Exit", "File", 4, None, ""),
    // Edit
    cmd("edit.undo", "Undo", "Edit", 1, ctrl(Key::Z), "undo-2"),
    cmd("edit.redo", "Redo", "Edit", 1, ctrl(Key::Y), "redo-2"),
    cmd("edit.cut", "Cut", "Edit", 2, ctrl(Key::X), "scissors"),
    cmd("edit.copy", "Copy", "Edit", 2, ctrl(Key::C), "copy"),
    cmd("edit.paste", "Paste", "Edit", 2, ctrl(Key::V), "clipboard-paste"),
    cmd("edit.paste_in_place", "Paste in Place", "Edit", 2, ctrl_shift(Key::V), ""),
    cmd("edit.duplicate", "Duplicate", "Edit", 2, None, ""),
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
    cmd("view.side_by_side", "Side by Side", "View", 3, ctrl(Key::Num6), "columns-3"),
    cmd("view.continuous_side", "Continuous Side by Side", "View", 3, ctrl(Key::Num7), ""),
    cmd("view.cover_page", "Show Cover Page Alone", "View", 3, None, ""),
    cmd("view.rotate_view_cw", "Rotate View Clockwise", "View", 3, ctrl_shift(Key::Plus), "rotate-cw"),
    cmd("view.rotate_view_ccw", "Rotate View Counterclockwise", "View", 3, ctrl_shift(Key::Minus), "rotate-ccw"),
    cmd("view.prev_view", "Previous View", "View", 4, alt(Key::ArrowLeft), ""),
    cmd("view.next_view", "Next View", "View", 4, alt(Key::ArrowRight), ""),
    cmd("view.toggle_zoom", "Toggle Zoom Tool", "View", 1, shift(Key::Z), ""),
    cmd("view.split_vertical", "Split Vertical", "View", 6, ctrl(Key::Num2), "columns-3"),
    cmd("view.split_horizontal", "Split Horizontal", "View", 6, ctrl(Key::H), "rows-3"),
    cmd("view.toggle_split", "Toggle Split", "View", 6, ctrl(Key::I), ""),
    cmd("view.switch", "Switch", "View", 6, ctrl(Key::Num1), ""),
    cmd("view.balance", "Balance", "View", 6, shift(Key::F12), ""),
    cmd("view.unsplit", "Unsplit", "View", 6, ctrl_shift(Key::Num2), ""),
    cmd("view.sync_off", "Synchronize: Off", "View", 7, None, ""),
    cmd("view.sync_document", "Synchronize: Document", "View", 7, None, ""),
    cmd("view.sync_page", "Synchronize: Page", "View", 7, None, ""),
    cmd("view.rulers", "Rulers", "View", 8, ctrl(Key::R), "ruler"),
    cmd("view.crosshair", "Full-Screen Crosshair", "View", 8, None, ""),
    cmd("view.dimmer", "Dimmer", "View", 8, ctrl(Key::F5), "eye"),
    cmd("view.refresh", "Refresh", "View", 8, key(Key::F5), ""),
    cmd("view.first_page", "First Page", "View", 4, key(Key::Home), "chevrons-left"),
    with_alias(cmd("view.prev_page", "Previous Page", "View", 4, ctrl(Key::ArrowLeft), "chevron-left"), key(Key::PageUp)),
    with_alias(cmd("view.next_page", "Next Page", "View", 4, ctrl(Key::ArrowRight), "chevron-right"), key(Key::PageDown)),
    cmd("view.last_page", "Last Page", "View", 4, key(Key::End), "chevrons-right"),
    cmd("view.hide_markups", "Hide Markups", "View", 5, None, "eye-off"),
    cmd("view.show_grid", "Show Grid", "View", 5, shift(Key::F9), "grid-3x3"),
    cmd("view.highlight_viewports", "Highlight Viewports", "View", 5, None, "frame"),
    // Markup (drawing tools come from the tool registry)
    cmd("markup.edit_text", "Edit Text", "Markup", 8, None, "type"),
    cmd("markup.autosize", "Autosize Text Box", "Markup", 8, alt(Key::Z), ""),
    cmd("markup.lock", "Lock", "Markup", 9, ctrl_shift(Key::L), "lock"),
    cmd("markup.unlock", "Unlock", "Markup", 9, None, "lock-open"),
    cmd("markup.group", "Group", "Markup", 10, ctrl(Key::G), ""),
    cmd("markup.ungroup", "Ungroup", "Markup", 10, ctrl_shift(Key::G), ""),
    cmd("arrange.bring_to_front", "Bring to Front", "Markup", 11, ctrl_shift(Key::CloseBracket), ""),
    cmd("arrange.bring_forward", "Bring Forward", "Markup", 11, ctrl(Key::CloseBracket), ""),
    cmd("arrange.send_backward", "Send Backward", "Markup", 11, ctrl(Key::OpenBracket), ""),
    cmd("arrange.send_to_back", "Send to Back", "Markup", 11, ctrl_shift(Key::OpenBracket), ""),
    cmd("arrange.align_left", "Align Left", "Markup", 13, ctrl_alt(Key::L), "align-start-vertical"),
    cmd("arrange.align_center", "Align Center", "Markup", 13, ctrl_alt(Key::E), "align-center-vertical"),
    cmd("arrange.align_right", "Align Right", "Markup", 13, ctrl_alt(Key::R), "align-end-vertical"),
    cmd("arrange.align_top", "Align Top", "Markup", 13, ctrl_alt(Key::T), "align-start-horizontal"),
    cmd("arrange.align_middle", "Align Middle", "Markup", 13, ctrl_alt(Key::M), "align-center-horizontal"),
    cmd("arrange.align_bottom", "Align Bottom", "Markup", 13, ctrl_alt(Key::B), "align-end-horizontal"),
    cmd("arrange.distribute_horizontal", "Distribute Horizontally", "Markup", 13, None, ""),
    cmd("arrange.distribute_vertical", "Distribute Vertically", "Markup", 13, None, ""),
    cmd("arrange.flip_horizontal", "Flip Horizontal", "Markup", 14, ctrl_alt(Key::H), "flip-horizontal-2"),
    cmd("arrange.flip_vertical", "Flip Vertical", "Markup", 14, ctrl_alt(Key::V), "flip-vertical-2"),
    cmd("markup.remove_from_group", "Remove From Group", "Markup", 10, ctrl_shift_alt(Key::G), ""),
    cmd("markup.apply_to_all_pages", "Apply to All Pages", "Markup", 14, None, "copy-plus"),
    cmd("markup.format_painter", "Format Painter", "Markup", 14, ctrl_shift(Key::C), "paintbrush"),
    cmd("edit.nudge_left", "Nudge Left", "", 0, key(Key::ArrowLeft), ""),
    cmd("edit.nudge_right", "Nudge Right", "", 0, key(Key::ArrowRight), ""),
    cmd("edit.nudge_up", "Nudge Up", "", 0, key(Key::ArrowUp), ""),
    cmd("edit.nudge_down", "Nudge Down", "", 0, key(Key::ArrowDown), ""),
    cmd("edit.nudge_left_far", "Nudge Left 10", "", 0, shift(Key::ArrowLeft), ""),
    cmd("edit.nudge_right_far", "Nudge Right 10", "", 0, shift(Key::ArrowRight), ""),
    cmd("edit.nudge_up_far", "Nudge Up 10", "", 0, shift(Key::ArrowUp), ""),
    cmd("edit.nudge_down_far", "Nudge Down 10", "", 0, shift(Key::ArrowDown), ""),
    cmd("markup.set_default", "Set as Default", "Markup", 12, None, ""),
    cmd("markup.add_to_toolchest", "Add to Tool Chest", "Markup", 12, None, "wrench"),
    // Measure (the measurement tools come from the tool registry)
    // Tools (Select / Pan come from the tool registry)
    cmd("tools.keep_tool", "Keep Tool Selected", "Tools", 9, None, "pin"),
    cmd("tools.customize_keys", "Customize Keyboard...", "Tools", 10, None, "keyboard"),
    // Document
    cmd("document.properties", "Document Properties", "Document", 1, ctrl(Key::D), "info"),
    cmd("document.rotate_cw", "Rotate Page Clockwise", "Document", 2, shift_alt(Key::Plus), "rotate-cw"),
    cmd("document.rotate_ccw", "Rotate Page Counterclockwise", "Document", 2, shift_alt(Key::Minus), "rotate-ccw"),
    cmd("document.insert_blank", "Insert Blank Page", "Document", 2, ctrl_shift(Key::N), "file-plus"),
    cmd("document.insert_pages", "Insert Pages...", "Document", 2, ctrl_shift(Key::I), ""),
    cmd("document.extract_page", "Extract Page...", "Document", 2, ctrl_shift(Key::X), "file-output"),
    cmd("document.delete_page", "Delete Page", "Document", 2, ctrl_shift(Key::D), "file-minus"),
    cmd("document.insert_blank_pages", "Insert Blank Pages...", "Document", 4, None, ""),
    cmd("pages.insert", "Insert Pages At...", "Document", 4, None, ""),
    cmd("document.extract_pages", "Extract Pages...", "Document", 4, None, ""),
    cmd("document.replace_pages", "Replace Pages...", "Document", 4, ctrl_shift(Key::Y), ""),
    cmd("document.delete_pages", "Delete Pages...", "Document", 4, None, ""),
    cmd("document.rotate_pages", "Rotate Pages...", "Document", 4, ctrl_shift(Key::R), ""),
    cmd("document.crop_pages", "Crop Pages...", "Document", 4, shift_alt(Key::O), ""),
    cmd("document.page_setup", "Page Setup...", "Document", 4, None, ""),
    cmd("document.flatten", "Flatten...", "Document", 3, ctrl_shift(Key::M), ""),
    // Batch
    cmd("batch.summary", "Summary...", "Batch", 1, None, "list-checks"),
    cmd("batch.flatten", "Flatten...", "Batch", 1, None, ""),
    // Window (panels come from the panel registry)
    cmd("window.hide_panels", "Hide Panels", "Window", 8, shift(Key::F4), "panel-left"),
    cmd("window.menu_bar", "Menu Bar", "Window", 8, key(Key::F9), ""),
    cmd("window.nav_bar", "Navigation Bar", "Window", 8, key(Key::F4), ""),
    cmd("window.status_bar", "Status Bar", "Window", 8, key(Key::F8), ""),
    cmd("window.full_screen", "Full Screen", "Window", 8, key(Key::F11), "maximize"),
    cmd("window.presentation", "Presentation", "Window", 8, ctrl(Key::Enter), ""),
    cmd("window.always_on_top", "Always on Top", "Window", 8, ctrl(Key::F12), "pin"),
    cmd("window.reset_layout", "Reset Panel Layout", "Window", 9, None, ""),
    cmd("window.preferences", "Preferences...", "Window", 11, ctrl(Key::K), "settings"),
    cmd("window.toolbar_main", "Main Toolbar", "", 0, None, ""),
    cmd("window.toolbar_markup", "Markup Tools", "", 0, None, ""),
    cmd("window.toolbar_measure", "Measure Tools", "", 0, None, ""),
    cmd("window.customize_toolbars", "Customize...", "", 0, None, "sliders-horizontal"),
    cmd("window.lock_toolbars", "Lock Toolbars", "", 0, None, "lock"),
    cmd("window.reuse_tools", "Reuse Markup Tools", "", 0, None, ""),
    cmd("window.next_document", "Next Document", "Window", 10, ctrl(Key::Tab), ""),
    cmd("window.prev_document", "Previous Document", "Window", 10, ctrl_shift(Key::Tab), ""),
    // Help
    cmd("help.shortcuts", "Keyboard Shortcuts", "Help", 1, key(Key::F1), "circle-help"),
    cmd("help.about", "About MarkupCraft", "Help", 2, None, "info"),
    // Status bar / toolbar only
    cmd("snap.grid", "Snap to Grid", "", 0, ctrl_shift(Key::F9), "grid-3x3"),
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

/// Every command row: the core table, the document features' and the shell's.
pub fn all() -> impl Iterator<Item = &'static Command> {
    COMMANDS
        .iter()
        .chain(crate::features::COMMANDS)
        .chain(crate::shell::extra::COMMANDS)
        .chain(crate::more::COMMANDS)
}

pub fn find(id: &str) -> Option<&'static Command> {
    all().find(|c| c.id == id)
}

/// Every key binding (commands, tools, panels), most specific first, for the dispatcher.
pub fn bindings() -> Vec<(Keys, String)> {
    let mut out: Vec<(Keys, String)> = Vec::new();
    for c in all().filter(|c| c.built) {
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

    /// Revu 21's default keys (docs/revu_features/05_shortcuts.md) for the commands MarkupCraft
    /// has, each bound to the matching command.
    #[test]
    fn revu_default_shortcuts_are_bound() {
        let (c, s, a) = (true, true, true);
        let k = |ctrl: bool, shift: bool, alt: bool, key: Key| Keys::new(ctrl, shift, alt, key);
        let n = false;
        #[rustfmt::skip]
        let expected: &[(Keys, &str)] = &[
            // Markup tools and commands
            (k(n, n, n, Key::A), "tool.arrow"), (k(n, n, n, Key::Q), "tool.callout"),
            (k(n, n, n, Key::C), "tool.cloud"), (k(n, n, n, Key::K), "tool.cloudplus"),
            (k(n, n, n, Key::E), "tool.ellipse"), (k(n, n, n, Key::H), "tool.highlight"),
            (k(n, n, n, Key::L), "tool.line"), (k(n, n, n, Key::N), "tool.note"),
            (k(n, n, n, Key::P), "tool.pen"), (k(n, s, n, Key::P), "tool.polygon"),
            (k(n, s, n, Key::N), "tool.polyline"), (k(n, n, n, Key::R), "tool.rectangle"),
            (k(n, n, n, Key::S), "tool.stamp"), (k(n, n, n, Key::T), "tool.text"),
            (k(n, n, n, Key::W), "tool.typewriter"), (k(n, n, n, Key::G), "tool.snapshot"),
            (k(n, n, a, Key::Z), "markup.autosize"), (k(c, n, n, Key::G), "markup.group"),
            (k(c, s, n, Key::G), "markup.ungroup"), (k(c, s, n, Key::L), "markup.lock"),
            (k(c, n, n, Key::CloseBracket), "arrange.bring_forward"),
            (k(c, s, n, Key::CloseBracket), "arrange.bring_to_front"),
            (k(c, n, n, Key::OpenBracket), "arrange.send_backward"),
            (k(c, s, n, Key::OpenBracket), "arrange.send_to_back"),
            // Measure
            (k(n, s, a, Key::A), "tool.area"), (k(n, s, a, Key::C), "tool.count"),
            (k(n, s, a, Key::L), "tool.length"), (k(n, s, a, Key::P), "tool.perimeter"),
            (k(n, s, a, Key::Q), "tool.polylength"),
            (k(n, s, a, Key::G), "tool.angle"), (k(n, s, a, Key::D), "tool.diameter"),
            (k(n, s, a, Key::U), "tool.radius"), (k(n, s, a, Key::V), "tool.volume"),
            // Arrange
            (k(c, n, a, Key::B), "arrange.align_bottom"), (k(c, n, a, Key::E), "arrange.align_center"),
            (k(c, n, a, Key::L), "arrange.align_left"), (k(c, n, a, Key::M), "arrange.align_middle"),
            (k(c, n, a, Key::R), "arrange.align_right"), (k(c, n, a, Key::T), "arrange.align_top"),
            (k(c, n, a, Key::H), "arrange.flip_horizontal"), (k(c, n, a, Key::V), "arrange.flip_vertical"),
            (k(c, s, a, Key::G), "markup.remove_from_group"), (k(n, s, n, Key::F2), "file.save_all"),
            // Edit
            (k(c, n, n, Key::C), "edit.copy"), (k(c, n, n, Key::X), "edit.cut"),
            (k(n, n, n, Key::Delete), "edit.delete"), (k(c, n, n, Key::V), "edit.paste"),
            (k(c, s, n, Key::V), "edit.paste_in_place"), (k(c, n, n, Key::Y), "edit.redo"),
            (k(c, n, n, Key::A), "edit.select_all"), (k(c, n, n, Key::Z), "edit.undo"),
            // View
            (k(c, n, n, Key::Num8), "view.actual_size"), (k(c, n, n, Key::Num5), "view.continuous"),
            (k(c, n, n, Key::Num9), "view.fit_page"), (k(c, n, n, Key::Num0), "view.fit_width"),
            (k(c, n, n, Key::ArrowRight), "view.next_page"), (k(c, n, n, Key::ArrowLeft), "view.prev_page"),
            (k(n, s, n, Key::F9), "view.show_grid"), (k(c, n, n, Key::Num4), "view.single_page"),
            (k(c, s, n, Key::F8), "snap.content"), (k(c, s, n, Key::F9), "snap.grid"),
            (k(c, s, n, Key::F7), "snap.markup"),
            (k(n, n, n, Key::Plus), "view.zoom_in"), (k(n, n, n, Key::Minus), "view.zoom_out"),
            // Document
            (k(c, s, n, Key::D), "document.delete_page"), (k(c, n, n, Key::D), "document.properties"),
            (k(c, s, n, Key::X), "document.extract_page"), (k(c, s, n, Key::N), "document.insert_blank"),
            (k(c, s, n, Key::I), "document.insert_pages"),
            (k(n, s, a, Key::Plus), "document.rotate_cw"), (k(n, s, a, Key::Minus), "document.rotate_ccw"),
            // File
            (k(c, n, n, Key::F4), "file.close"), (k(c, n, n, Key::O), "file.open"),
            (k(c, n, n, Key::S), "file.save"), (k(c, s, n, Key::S), "file.save_as"),
            // Selection
            (k(n, s, n, Key::V), "tool.pan"), (k(n, n, n, Key::V), "tool.select"),
            // Window
            (k(n, n, a, Key::B), "panel.bookmarks"), (k(n, n, a, Key::L), "panel.markups"),
            (k(n, n, a, Key::U), "panel.measurements"), (k(n, n, a, Key::P), "panel.properties"),
            (k(n, n, a, Key::T), "panel.thumbnails"), (k(n, n, a, Key::X), "panel.toolchest"),
            // Help and navigation
            (k(n, n, n, Key::F1), "help.shortcuts"),
            (k(n, n, n, Key::Home), "view.first_page"), (k(n, n, n, Key::End), "view.last_page"),
            (k(c, n, n, Key::Tab), "window.next_document"), (k(c, s, n, Key::Tab), "window.prev_document"),
        ];
        let b = bindings();
        for (keys, id) in expected {
            assert!(
                b.iter().any(|(bk, bid)| bk == keys && bid == id),
                "{} should run {id}",
                keys.label()
            );
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
