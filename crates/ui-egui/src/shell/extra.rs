//! More of Revu's application shell: the MarkupCraft (Revu) menu, Publish As, Revert As, Email,
//! Copy Page to Snapshot, Select All Text, Deskew, page labels from a page region, the
//! workspace's dark mode, line weights and reply indicators, panel access bars, auto-hide tabs,
//! the panel tab menu, detached document windows, the Properties toolbar, Alt menu
//! accelerators, the keyboard context menu (Shift+F10), a printable shortcut reference, and
//! the preferences behind them ([`ExtraPrefs`], kept in `UiPrefs::extra`).
//!
//! The rest of the shell reaches this module through one-line hooks: [`COMMANDS`] joins the
//! command table; [`enabled`], [`checked`] and [`run`] are the first arms of the shell's; and
//! [`begin_frame`], [`windows`] and [`dialog_answer`] run from the shell's own.

use std::path::PathBuf;

use egui::{Key, RichText};
use markupcraft_engine::docfile::PublishMode;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::commands::{Command, Keys, alt, ctrl, ctrl_alt, ctrl_shift, shift};
use crate::dialogs::Purpose;

/// Interface preferences of these features (in `UiPrefs::extra`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraPrefs {
    /// View > Dark Mode: pages drawn light on dark.
    pub dark_workspace: bool,
    /// View > Disable Line Weights: all page linework at the thinnest visible width.
    pub thin_lines: bool,
    /// View > Always Show Reply Indicators.
    pub reply_indicators: bool,
    /// Window > Auto-Hide Tabs: document tabs show when the pointer reaches the top.
    pub auto_hide_tabs: bool,
    /// Window > Panel Access Bars: icon strips beside the workspace.
    pub panel_bars: bool,
    /// The bottom panel spans the window (else it sits between the side panels).
    pub bottom_full_width: bool,
    /// Window > Properties Toolbar.
    pub properties_toolbar: bool,
    /// General > Navigation: Alt + a menu's first letter opens it.
    pub alt_menus: bool,
    /// General > Options: `last`, `markup` (all toolbars) or `view` (markup tools hidden).
    pub startup_mode: String,
    /// General > Options: a PDF to open on start ("" = none).
    pub startup_file: String,
    /// General > Options: start in full screen.
    pub startup_full_screen: bool,
    /// General > Document: offer a read-only copy when a file is in use or read-only.
    pub locked_prompt: bool,
    /// General > Document: Rotate Pages acts on every page by default.
    pub rotate_all_pages: bool,
    /// Interface > File Access: thumbnails beside recent files.
    pub recents_preview: bool,
    /// Grid & Snap: the colour of the snap indicator and the crosshair (`None` = magenta).
    pub snap_color: Option<[u8; 3]>,
    /// Grid & Snap > Snap to: which points are snap targets.
    pub snap_endpoints: bool,
    pub snap_midpoints: bool,
    pub snap_intersections: bool,
    pub snap_nearest: bool,
    pub snap_centers: bool,
    /// General > Navigation: scrollbars on the document, and the vertical one on the left.
    pub scrollbars: bool,
    pub scrollbars_left: bool,
    /// General > Navigation: how new split views synchronize (`off`, `document`, `page`).
    pub sync_default: String,
    /// Tools > Sketch: rotation typed relative to the last segment (else absolute).
    pub sketch_relative: bool,
    /// Tools > Sketch: an ellipse is typed as a radius from its centre (else width x height).
    pub sketch_radius: bool,
    /// General > Document: bookmarks follow their pages when pages move.
    pub reorder_bookmarks: bool,
    /// General > Document: Ctrl+click on text that is a web address opens it.
    pub detect_urls: bool,
    /// General > Spelling.
    pub spell: crate::spell_prefs::SpellPrefs,
    /// Advanced > PDF/A: PDF/A documents refuse page edits.
    pub pdfa_locked: bool,
}

impl Default for ExtraPrefs {
    fn default() -> Self {
        Self {
            dark_workspace: false,
            thin_lines: false,
            reply_indicators: false,
            auto_hide_tabs: false,
            panel_bars: false,
            bottom_full_width: true,
            properties_toolbar: false,
            alt_menus: false,
            startup_mode: "last".into(),
            startup_file: String::new(),
            startup_full_screen: false,
            locked_prompt: true,
            rotate_all_pages: false,
            recents_preview: true,
            snap_color: None,
            snap_endpoints: true,
            snap_midpoints: true,
            snap_intersections: true,
            snap_nearest: true,
            snap_centers: true,
            scrollbars: false,
            scrollbars_left: false,
            sync_default: "off".into(),
            sketch_relative: false,
            sketch_radius: false,
            reorder_bookmarks: true,
            detect_urls: true,
            spell: Default::default(),
            pdfa_locked: true,
        }
    }
}

impl ExtraPrefs {
    pub fn sanitize(&mut self) {
        if !["last", "markup", "view"].contains(&self.startup_mode.as_str()) {
            self.startup_mode = "last".into();
        }
        if !["off", "document", "page"].contains(&self.sync_default.as_str()) {
            self.sync_default = "off".into();
        }
        if self.startup_file.len() > 4096 {
            self.startup_file.clear();
        }
    }

    /// Bits of the snap targets that are off (see `snapping::set_target_mask`).
    pub fn snap_mask(&self) -> u8 {
        let mut m = 0;
        for (on, bit) in [
            (self.snap_endpoints, crate::snapping::MASK_ENDPOINT),
            (self.snap_midpoints, crate::snapping::MASK_MIDPOINT),
            (self.snap_intersections, crate::snapping::MASK_INTERSECTION),
            (self.snap_nearest, crate::snapping::MASK_NEAREST),
            (self.snap_centers, crate::snapping::MASK_CENTER),
        ] {
            if !on {
                m |= bit;
            }
        }
        m
    }
}

/// A change to the panel layout the state cannot make itself (applied by the shell's dock
/// pass).
#[derive(Debug, Clone, PartialEq)]
pub enum DockOp {
    Hide(&'static str),
    Show(&'static str),
    Attach(&'static str, crate::panels::Slot),
    Float(&'static str),
    /// Show the panel below the others of its group, so both show at once.
    SplitBelow(&'static str),
    /// Rebuild the layout with the open panels (bottom panel across the window or not).
    Arrange,
}

/// The Revert As dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct RevertDialog {
    pub count: usize,
    pub pick: usize,
}

/// The Deskew dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DeskewDialog {
    pub degrees: f64,
    pub all_pages: bool,
}

/// Page labels from a page region (AutoMark).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RegionLabels {
    pub regions: Vec<markupcraft_geom::Rect>,
    pub before: String,
    pub between: String,
    pub after: String,
    /// Labels found (page, label), for the preview.
    pub preview: Vec<(usize, String)>,
}

/// The Set Scale dialog from Thumbnails.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleDialog {
    pub pages: Vec<usize>,
    pub paper: f64,
    pub paper_unit: markupcraft_measure::units::LengthUnit,
    pub real: f64,
    pub real_unit: markupcraft_measure::units::LengthUnit,
}

/// State of these features (lives in `Shell::extra`).
#[derive(Default)]
pub struct ExtraState {
    /// Panels collapsed by clicking the window's edge (`edges`).
    pub edges: super::edges::EdgeState,
    /// Menu button ids by menu name (Alt accelerators open them).
    pub menu_ids: Vec<(&'static str, egui::Id)>,
    /// Shift+F10: the keyboard context menu is open at this screen point.
    pub key_menu: Option<egui::Pos2>,
    pub revert: Option<RevertDialog>,
    pub deskew: Option<DeskewDialog>,
    pub region: Option<RegionLabels>,
    pub scale: Option<ScaleDialog>,
    /// A page command waiting for "page edits clear the signatures" to be confirmed.
    pub sign_warning: Option<String>,
    /// Documents (uid) whose signature warning was accepted.
    pub sign_ok: Vec<u64>,
    pub files: super::files::FileState,
    pub workspace: super::workspace::WorkspaceState,
    pub dock_ops: Vec<DockOp>,
    pub detached: Vec<super::detach::Detached>,
    /// The document area last frame (auto-hide tabs, drops on the split pane).
    pub doc_area: Option<egui::Rect>,
    /// The second split pane last frame (a tab dropped there shows in it).
    pub pane_rect: Option<egui::Rect>,
    /// The Thumbnails panel last frame (files dropped there are inserted).
    pub thumbs_rect: Option<egui::Rect>,
    /// A thumbnail's label being renamed in place: (page, text).
    pub label_edit: Option<(usize, String)>,
    /// Open other programs (mail, file manager): off in tests and screenshots.
    pub launch: bool,
    /// A document tab is being dragged (tabs stay shown; the split pane takes a drop).
    pub dragging_tab: bool,
    /// The Sketch angle preference last applied.
    pub sketch_applied: Option<bool>,
}

const fn c(
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

/// Commands of this module (Revu's default keys). Groups 30+ keep them apart from the core
/// rows of the same menu.
#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    // File
    c("file.publish_compressed", "Publish As Compressed...", "File", 30, ctrl_shift(Key::P), ""),
    c("file.publish_flattened", "Publish As Flattened...", "File", 30, ctrl_alt(Key::F), ""),
    c("file.publish_uncompressed", "Publish As Uncompressed...", "File", 30, None, ""),
    c("file.revert_as", "Revert As...", "File", 30, None, ""),
    c("file.email", "Email...", "File", 31, ctrl(Key::E), ""),
    // Edit
    c("edit.copy_page_snapshot", "Copy Page to Snapshot", "Edit", 30, ctrl_alt(Key::C), ""),
    c("edit.select_all_text", "Select All Text", "Edit", 30, ctrl_shift(Key::A), ""),
    // View
    c("view.dark_workspace", "Dark Mode", "View", 30, None, ""),
    c("view.line_weights", "Disable Line Weights", "View", 30, None, ""),
    c("view.reply_indicators", "Always Show Reply Indicators", "View", 30, None, ""),
    // Document
    c("document.deskew", "Deskew...", "Document", 30, ctrl_alt(Key::D), ""),
    c("document.region_labels", "Page Labels from Region...", "Document", 30, None, ""),
    // Window
    c("window.forms", "Forms", "Window", 30, alt(Key::Q), ""),
    c("window.context_menu", "Show Context Menu", "Window", 30, shift(Key::F10), ""),
    c("window.properties_toolbar", "Properties Toolbar", "Window", 31, None, ""),
    c("window.panel_bars", "Panel Access Bars", "Window", 31, None, ""),
    c("window.auto_hide_tabs", "Auto-Hide Tabs", "Window", 31, None, ""),
    c("window.bottom_full_width", "Bottom Panel Across the Window", "Window", 31, None, ""),
    c("window.detach", "Detach Tab to New Window", "Window", 32, None, ""),
    c("window.reattach", "Reattach All Windows", "Window", 32, None, ""),
    // Help
    c("help.shortcut_reference", "Shortcut Reference (PDF)...", "Help", 30, None, "printer"),
];

/// The top-level menu of application commands (Revu's "Revu" menu).
pub const APP_MENU: &str = "MarkupCraft";

fn ours(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
}

/// Page commands guarded by signatures, certification and PDF/A.
pub const PAGE_EDITS: &[&str] = &[
    "document.rotate_cw",
    "document.rotate_ccw",
    "document.delete_page",
    "document.insert_blank",
    "document.insert_pages",
    "document.rotate_pages",
    "document.crop_pages",
    "document.page_setup",
    "document.replace_pages",
    "document.insert_blank_pages",
    "document.delete_pages",
    "document.deskew",
    "pages.insert",
];

pub fn enabled(app: &AppState, id: &str) -> Option<bool> {
    if !ours(id) {
        return None;
    }
    let doc = app.has_doc();
    Some(match id {
        "view.dark_workspace"
        | "view.line_weights"
        | "view.reply_indicators"
        | "window.context_menu"
        | "window.properties_toolbar"
        | "window.panel_bars"
        | "window.auto_hide_tabs"
        | "window.bottom_full_width"
        | "help.shortcut_reference" => true,
        "window.reattach" => !app.shell.extra.detached.is_empty(),
        "file.revert_as" => app.doc().is_some_and(|d| d.path.is_some()),
        "window.forms" => doc && crate::commands::find("tools.forms").is_some(),
        _ => doc,
    })
}

pub fn checked(app: &AppState, id: &str) -> Option<bool> {
    let x = &app.shell.ui.extra;
    Some(match id {
        "view.dark_workspace" => x.dark_workspace,
        "view.line_weights" => x.thin_lines,
        "view.reply_indicators" => x.reply_indicators,
        "window.properties_toolbar" => x.properties_toolbar,
        "window.panel_bars" => x.panel_bars,
        "window.auto_hide_tabs" => x.auto_hide_tabs,
        "window.bottom_full_width" => x.bottom_full_width,
        _ => return None,
    })
}

fn toggle(app: &mut AppState, f: impl FnOnce(&mut ExtraPrefs) -> &mut bool) {
    let v = f(&mut app.shell.ui.extra);
    *v = !*v;
    app.shell.save_ui();
}

/// Run a command of this module; `false` when `id` is not one.
pub fn run(app: &mut AppState, id: &str, ctx: &egui::Context) -> bool {
    if !ours(id) {
        return false;
    }
    if page_edit_blocked(app, id) {
        return true;
    }
    match id {
        "file.publish_compressed" => ask_save(app, "x-publish-compressed", "published"),
        "file.publish_flattened" => ask_save(app, "x-publish-flattened", "flattened"),
        "file.publish_uncompressed" => ask_save(app, "x-publish-uncompressed", "uncompressed"),
        "file.revert_as" => {
            if let Some(d) = app.doc() {
                let count = d.session.revision_count();
                app.shell.extra.revert = Some(RevertDialog {
                    count,
                    pick: count.saturating_sub(1),
                });
            }
        }
        "file.email" => super::files::email(app),
        "edit.copy_page_snapshot" => copy_page_snapshot(app),
        "edit.select_all_text" => select_all_text(app, ctx),
        "view.dark_workspace" => {
            toggle(app, |x| &mut x.dark_workspace);
            for d in &mut app.docs {
                d.view.invalidate();
            }
        }
        "view.line_weights" => toggle(app, |x| &mut x.thin_lines),
        "view.reply_indicators" => toggle(app, |x| &mut x.reply_indicators),
        "document.deskew" => {
            app.shell.extra.deskew = Some(DeskewDialog {
                degrees: 0.0,
                all_pages: false,
            })
        }
        "document.region_labels" => {
            app.shell.extra.region = Some(RegionLabels {
                between: " - ".into(),
                ..Default::default()
            })
        }
        "window.forms" => app.queue("tools.forms"),
        "window.context_menu" => open_key_menu(app),
        "window.properties_toolbar" => toggle(app, |x| &mut x.properties_toolbar),
        "window.panel_bars" => toggle(app, |x| &mut x.panel_bars),
        "window.auto_hide_tabs" => toggle(app, |x| &mut x.auto_hide_tabs),
        "window.bottom_full_width" => {
            toggle(app, |x| &mut x.bottom_full_width);
            app.shell.extra.dock_ops.push(DockOp::Arrange);
        }
        "window.detach" => super::detach::detach(app),
        "window.reattach" => app.shell.extra.detached.clear(),
        "help.shortcut_reference" => app.dialogs.save(
            Purpose::Shell {
                tag: "x-shortcut-reference".into(),
            },
            crate::dialogs::PDF,
            "MarkupCraft shortcuts.pdf",
        ),
        _ => {}
    }
    true
}

fn ask_save(app: &mut AppState, tag: &str, what: &str) {
    let Some(name) = app.doc().map(|d| d.name.trim_end_matches(".pdf").to_string()) else {
        return;
    };
    app.dialogs.save(
        Purpose::Shell { tag: tag.into() },
        crate::dialogs::PDF,
        &format!("{name} ({what}).pdf"),
    );
}

/// A file dialog answered for a `x-` tag.
pub fn dialog_answer(app: &mut AppState, tag: &str, paths: &[PathBuf]) {
    let Some(first) = paths.first() else { return };
    let publish = match tag {
        "x-publish-compressed" => Some(PublishMode::Compressed),
        "x-publish-flattened" => Some(PublishMode::Flattened),
        "x-publish-uncompressed" => Some(PublishMode::Uncompressed),
        _ => None,
    };
    if let Some(mode) = publish {
        if let Some(d) = app.doc() {
            let r = d.session.publish_as(first, mode);
            app.status = crate::actions::report(r, |n| format!("Published {} ({} bytes)", first.display(), n));
        }
        return;
    }
    if let Some(i) = tag.strip_prefix("x-revert-").and_then(|n| n.parse::<usize>().ok()) {
        if let Some(d) = app.doc() {
            let r = d.session.revert_as(i, first);
            app.status = crate::actions::report(r, |_| format!("Revision {} saved as {}", i + 1, first.display()));
        }
        return;
    }
    match tag {
        "x-shortcut-reference" => {
            let r = super::shortcutref::write(app, first);
            app.status = match r {
                Ok(n) => format!("Wrote {n} shortcuts to {}", first.display()),
                Err(e) => format!("Could not write the shortcut reference: {e}"),
            };
        }
        "x-startup-file" => {
            app.shell.ui.extra.startup_file = first.display().to_string();
            app.shell.save_ui();
        }
        _ => {}
    }
}

/// Edit > Copy Page to Snapshot: the current page as a snapshot on the clipboard (Ctrl+V
/// pastes it as a snapshot markup).
pub fn copy_page_snapshot(app: &mut AppState) {
    let Some(d) = app.doc_mut() else { return };
    let page = d.view.current;
    let Some(crop) = d.session.doc().pages.get(page).map(|p| p.crop.normalized()) else {
        return;
    };
    let pts = [
        markupcraft_geom::Point::new(crop.x0, crop.y0),
        markupcraft_geom::Point::new(crop.x1, crop.y1),
    ];
    match crate::tools::new_markup(markupcraft_model::Kind::Snapshot, page, &pts) {
        Some(m) => {
            d.session.set_clipboard(vec![m]);
            app.status = format!("Page {} copied as a snapshot: Ctrl+V pastes it", page + 1);
        }
        None => app.status = "This page cannot be copied as a snapshot".into(),
    }
}

/// Edit > Select All Text: the current page's text, on the clipboard.
fn select_all_text(app: &mut AppState, ctx: &egui::Context) {
    let Some(d) = app.doc_mut() else { return };
    let page = d.view.current;
    let text = d.page_text(page).map(|t| t.plain_text()).unwrap_or_default();
    if text.trim().is_empty() {
        app.status = format!("Page {} has no text", page + 1);
        return;
    }
    let n = text.split_whitespace().count();
    ctx.copy_text(text);
    app.shell.extra.workspace.last_text_copy = Some(page);
    app.status = format!("Selected and copied the text of page {} ({n} words)", page + 1);
}

/// Shift+F10: the right-click menu of the selection (or of the page) from the keyboard.
fn open_key_menu(app: &mut AppState) {
    let Some(d) = app.doc_mut() else { return };
    let page = d.view.current;
    let sel = d.selection().first().cloned();
    let at = sel
        .as_ref()
        .and_then(|id| d.session.doc().find(id))
        .map(|m| (m.page, crate::actions::center_of(m)));
    let (page, at) = at.unwrap_or_else(|| {
        let c = d
            .session
            .doc()
            .pages
            .get(page)
            .map(|p| p.crop.normalized())
            .unwrap_or_default();
        (
            page,
            markupcraft_geom::Point::new((c.x0 + c.x1) / 2.0, (c.y0 + c.y1) / 2.0),
        )
    });
    d.view.context = Some(crate::interact::ContextTarget {
        page,
        at,
        markup: sel,
        vertex: None,
        segment: None,
        hole: None,
    });
    let screen = d
        .render
        .as_ref()
        .and_then(|r| d.view.user_to_screen(page, at, r.pages()))
        .unwrap_or_else(|| d.view.viewport().center());
    app.shell.extra.key_menu = Some(screen);
}

fn key_menu(app: &mut AppState, ctx: &egui::Context) {
    let Some(at) = app.shell.extra.key_menu else { return };
    let mut out = crate::canvas::CanvasOut::default();
    let mut close = false;
    let Some(d) = app.docs.get_mut(app.active) else {
        app.shell.extra.key_menu = None;
        return;
    };
    let r = egui::Area::new(egui::Id::new("key-context-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                crate::context_menu::show(ui, d, &mut out);
                if ui.should_close() {
                    close = true;
                }
            });
        });
    let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
    if close || esc || r.response.clicked_elsewhere() || !out.commands.is_empty() || !out.actions.is_empty() {
        app.shell.extra.key_menu = None;
    }
    app.apply_canvas_out(out);
}

/// Why page edits are refused for the active document now (certified or PDF/A), if they are.
pub fn page_edits_refused(app: &AppState) -> Option<String> {
    let mut st = app.doc()?.session.standards();
    if !app.shell.ui.extra.pdfa_locked {
        st.pdfa = None;
    }
    if st.certified {
        Some("This document is certified: page edits are blocked".into())
    } else {
        st.pdfa
            .map(|p| format!("This document is PDF/A-{p}: page edits are blocked"))
    }
}

/// Page edits on a certified or PDF/A document are refused; on a signed one the user is asked
/// first (the signatures stop being valid). True when `id` must not run now.
pub fn page_edit_blocked(app: &mut AppState, id: &str) -> bool {
    if !PAGE_EDITS.contains(&id) {
        return false;
    }
    let Some(d) = app.doc() else { return false };
    let mut st = d.session.standards();
    if !app.shell.ui.extra.pdfa_locked {
        st.pdfa = None;
    }
    if st.blocks_page_edits() {
        app.status = if st.certified {
            "This document is certified: page edits are blocked".into()
        } else {
            format!(
                "This document is PDF/A-{}: page edits are blocked",
                st.pdfa.unwrap_or_default()
            )
        };
        return true;
    }
    if st.signatures > 0 && !app.shell.extra.sign_ok.contains(&d.uid) {
        app.shell.extra.sign_warning = Some(id.to_string());
        return true;
    }
    false
}

/// Start of a frame: preferences that act on every frame, the Alt accelerators.
pub fn begin_frame(app: &mut AppState, ctx: &egui::Context) {
    // Esc closes the keyboard context menu before it reaches the Esc shortcut.
    if app.shell.extra.key_menu.is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        app.shell.extra.key_menu = None;
    }
    app.snaps.mask = app.shell.ui.extra.snap_mask();
    app.snaps.color = app.shell.ui.extra.snap_color;
    super::workspace::begin_frame(app, ctx);
    // Tools > Sketch: the angle mode follows the preference when it changes.
    let rel = app.shell.ui.extra.sketch_relative;
    if app.shell.extra.sketch_applied != Some(rel) {
        app.shell.extra.sketch_applied = Some(rel);
        app.edit.sketch.relative = rel;
    }
    app.edit.sketch.radius = app.shell.ui.extra.sketch_radius;
    crate::spell_prefs::set(&app.shell.ui.extra.spell);
    super::files::begin_frame(app, ctx);
    if app.shell.ui.extra.alt_menus && !ctx.egui_wants_keyboard_input() {
        alt_menus(app, ctx);
    }
}

/// Alt + a menu's first letter opens that menu (when the preference is on; the letter wins over
/// a panel shortcut on the same key).
fn alt_menus(app: &mut AppState, ctx: &egui::Context) {
    let ids = app.shell.extra.menu_ids.clone();
    for (menu, id) in ids {
        let Some(letter) = menu.chars().next() else { continue };
        let Some(key) = Key::from_name(&letter.to_string()) else {
            continue;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, key)) {
            egui::Popup::open_id(ctx, id.with("popup"));
            break;
        }
    }
}

/// The menu bar drew `menu`'s button (for the Alt accelerators).
pub fn menu_drawn(app: &mut AppState, menu: &'static str, r: &egui::Response) {
    let ids = &mut app.shell.extra.menu_ids;
    match ids.iter_mut().find(|(m, _)| *m == menu) {
        Some(e) => e.1 = r.id,
        None => ids.push((menu, r.id)),
    }
}

/// The MarkupCraft menu (Revu's application menu); `false` for other menus.
pub fn app_menu(app: &mut AppState, ui: &mut egui::Ui, menu: &str) -> bool {
    if menu != APP_MENU {
        return false;
    }
    for id in [
        "help.about",
        "|",
        "window.preferences",
        "tools.customize_keys",
        "help.shortcuts",
    ] {
        if id == "|" {
            ui.separator();
            continue;
        }
        let Some((label, _, _)) = crate::commands::describe(id) else {
            continue;
        };
        let mut b = egui::Button::new(label);
        if let Some(k) = app.keys.keys_for(id) {
            b = b.shortcut_text(k.label());
        }
        if ui.add_enabled(app.enabled(id), b).clicked() {
            app.queue(id);
            ui.close();
        }
    }
    let profiles = app.shell.store.as_ref().map(|s| (s.profiles(), s.active()));
    ui.menu_button("Profiles", |ui| match profiles {
        Some((list, active)) => {
            for p in list {
                if ui.selectable_label(p == active, &p).clicked() && p != active {
                    crate::prefs_ui::profile(app, "switch", &p);
                    ui.close();
                }
            }
        }
        None => {
            ui.label(RichText::new("Settings are not saved in this session.").weak());
        }
    });
    ui.separator();
    if ui.button("Exit").clicked() {
        app.queue("file.exit");
        ui.close();
    }
    true
}

/// The windows of this module.
pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    key_menu(app, ctx);
    revert_window(app, ctx);
    deskew_window(app, ctx);
    region_window(app, ctx);
    scale_window(app, ctx);
    sign_warning(app, ctx);
    super::files::windows(app, ctx);
    super::detach::show(app, ctx);
}

fn revert_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut dlg) = app.shell.extra.revert.clone() else {
        return;
    };
    let mut open = true;
    let mut go = false;
    egui::Window::new("Revert As")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!(
                "The file keeps {} (each incremental save adds one).",
                crate::actions::plural(dlg.count, "revision")
            ));
            for i in 0..dlg.count.min(200) {
                let label = if i + 1 == dlg.count {
                    format!("Revision {} (current)", i + 1)
                } else {
                    format!("Revision {}", i + 1)
                };
                ui.radio_value(&mut dlg.pick, i, label);
            }
            go = ui.button("Save Revision As...").clicked();
        });
    if go {
        let name = app
            .doc()
            .map(|d| d.name.trim_end_matches(".pdf").to_string())
            .unwrap_or_default();
        app.dialogs.save(
            Purpose::Shell {
                tag: format!("x-revert-{}", dlg.pick),
            },
            crate::dialogs::PDF,
            &format!("{name} (revision {}).pdf", dlg.pick + 1),
        );
        open = false;
    }
    app.shell.extra.revert = open.then_some(dlg);
}

fn deskew_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut dlg) = app.shell.extra.deskew.clone() else {
        return;
    };
    let mut open = true;
    let (mut apply, mut line) = (false, false);
    egui::Window::new("Deskew")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Turn the page content to straighten a skewed scan.");
            ui.horizontal(|ui| {
                ui.label("Angle");
                ui.add(
                    egui::DragValue::new(&mut dlg.degrees)
                        .range(-45.0..=45.0)
                        .speed(0.05)
                        .suffix(" deg"),
                );
                line = ui
                    .button("Get Line")
                    .on_hover_text("Click two points that should be level")
                    .clicked();
            });
            ui.checkbox(&mut dlg.all_pages, "All pages (else the current page)");
            apply = ui.button("Deskew").clicked();
        });
    if line {
        app.shell.extra.deskew = Some(dlg);
        app.set_tool(super::deskew::TOOL.id);
        app.status = "Deskew: click two points that should be level".into();
        return;
    }
    if apply {
        let threads = app.threads;
        if let Some(d) = app.doc_mut() {
            let pages: Vec<usize> = if dlg.all_pages {
                (0..d.session.page_count()).collect()
            } else {
                vec![d.view.current]
            };
            let r = d.session.deskew_pages(&pages, dlg.degrees);
            d.sync_pages(threads);
            d.rerender(threads);
            app.status = crate::actions::report(r, |_| format!("Deskewed {:.2} degrees", dlg.degrees));
        }
        open = false;
    }
    app.shell.extra.deskew = open.then_some(dlg);
}

/// The canvas reported: the Deskew line and the label region come back here instead of going
/// to Calibrate or the viewport dialog.
pub fn canvas_out(app: &mut AppState, out: &mut crate::canvas::CanvasOut) {
    if app.tool == super::deskew::TOOL.id
        && let Some((_, a, b)) = out.calibrate.take()
    {
        let deg = markupcraft_engine::docfile::deskew_angle(a, b);
        let all = app.shell.extra.deskew.as_ref().is_some_and(|d| d.all_pages);
        app.shell.extra.deskew = Some(DeskewDialog {
            degrees: deg,
            all_pages: all,
        });
        out.done = true;
    }
    if app.tool == super::deskew::REGION_TOOL.id {
        let mut keep = Vec::new();
        for a in std::mem::take(&mut out.actions) {
            match a {
                crate::canvas::CanvasAction::NewViewport(_, r) => {
                    let st = app.shell.extra.region.get_or_insert_with(|| RegionLabels {
                        between: " - ".into(),
                        ..Default::default()
                    });
                    st.regions.push(r);
                    out.done = true;
                }
                other => keep.push(other),
            }
        }
        out.actions = keep;
        preview_region_labels(app);
    }
}

fn preview_region_labels(app: &mut AppState) {
    let Some(st) = app.shell.extra.region.clone() else {
        return;
    };
    let Some(d) = app.doc() else { return };
    let r = d.session.region_labels(&st.regions, &st.before, &st.between, &st.after);
    if let Some(s) = &mut app.shell.extra.region {
        match r {
            Ok(v) => s.preview = v,
            Err(e) => app.status = e.to_string(),
        }
    }
}

fn region_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut st) = app.shell.extra.region.clone() else {
        return;
    };
    let mut open = true;
    let (mut pick, mut apply, mut clear) = (false, false, false);
    let before = st.clone();
    egui::Window::new("Page Labels from Region")
        .open(&mut open)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.label("Drag boxes over the sheet number and title in the title block: the text found there on every page becomes its label.");
            ui.horizontal(|ui| {
                pick = ui.button("Add Region").clicked();
                clear = ui.add_enabled(!st.regions.is_empty(), egui::Button::new("Clear Regions")).clicked();
                ui.label(format!("{} region(s)", st.regions.len()));
            });
            egui::Grid::new("region-text").num_columns(2).show(ui, |ui| {
                ui.label("Text before");
                ui.text_edit_singleline(&mut st.before);
                ui.end_row();
                ui.label("Between regions");
                ui.text_edit_singleline(&mut st.between);
                ui.end_row();
                ui.label("Text after");
                ui.text_edit_singleline(&mut st.after);
                ui.end_row();
            });
            ui.separator();
            ui.label(RichText::new("Preview").strong());
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for (p, l) in st.preview.iter().take(500) {
                    ui.label(format!("Page {}: {l}", p + 1));
                }
            });
            apply = ui
                .add_enabled(!st.preview.is_empty(), egui::Button::new("Apply Labels"))
                .clicked();
        });
    if clear {
        st.regions.clear();
        st.preview.clear();
    }
    let changed = st.before != before.before || st.between != before.between || st.after != before.after;
    app.shell.extra.region = open.then_some(st.clone());
    if changed || clear {
        preview_region_labels(app);
    }
    if pick {
        app.set_tool(super::deskew::REGION_TOOL.id);
        app.status = "Drag a box over the text to use as the label".into();
    }
    if apply {
        let threads = app.threads;
        if let Some(d) = app.doc_mut() {
            let r = d.session.set_page_labels(&st.preview);
            d.rerender(threads);
            app.status = crate::actions::report(r, |_| {
                format!("Labelled {}", crate::actions::plural(st.preview.len(), "page"))
            });
        }
        app.shell.extra.region = None;
    }
}

fn unit_combo(ui: &mut egui::Ui, id: &str, u: &mut markupcraft_measure::units::LengthUnit) {
    use markupcraft_measure::units::LengthUnit as L;
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{u:?}"))
        .width(90.0)
        .show_ui(ui, |ui| {
            for v in [
                L::Inch,
                L::Foot,
                L::Yard,
                L::Millimeter,
                L::Centimeter,
                L::Meter,
                L::Point,
            ] {
                ui.selectable_value(u, v, format!("{v:?}"));
            }
        });
}

/// Thumbnails > Set Scale: `paper` on the sheet measures `real`.
pub fn open_scale(app: &mut AppState, pages: Vec<usize>) {
    use markupcraft_measure::units::LengthUnit as L;
    app.shell.extra.scale = Some(ScaleDialog {
        pages,
        paper: 1.0,
        paper_unit: L::Inch,
        real: 8.0,
        real_unit: L::Foot,
    });
}

/// Apply the Set Scale dialog.
pub fn apply_scale(app: &mut AppState, dlg: &ScaleDialog) -> String {
    let points = dlg.paper * dlg.paper_unit.meters() / markupcraft_measure::units::LengthUnit::Point.meters();
    let sc = match markupcraft_engine::calibrated_scale(points, dlg.real, dlg.real_unit) {
        Ok(s) => s,
        Err(e) => return e.to_string(),
    };
    let Some(d) = app.doc_mut() else { return String::new() };
    let r = d.session.set_page_scale(&dlg.pages, &sc, true);
    crate::actions::report(r, |_| {
        format!(
            "Scale {} on {}",
            sc.ratio,
            crate::actions::plural(dlg.pages.len(), "page")
        )
    })
}

fn scale_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut dlg) = app.shell.extra.scale.clone() else {
        return;
    };
    let mut open = true;
    let mut ok = false;
    egui::Window::new("Set Scale")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!(
                "Pages: {}",
                dlg.pages
                    .iter()
                    .map(|p| (p + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut dlg.paper).range(0.001..=10_000.0).speed(0.05));
                unit_combo(ui, "scale-paper-unit", &mut dlg.paper_unit);
                ui.label("=");
                ui.add(
                    egui::DragValue::new(&mut dlg.real)
                        .range(0.001..=1_000_000.0)
                        .speed(0.5),
                );
                unit_combo(ui, "scale-real-unit", &mut dlg.real_unit);
            });
            ok = ui.button("OK").clicked();
        });
    if ok {
        app.status = apply_scale(app, &dlg);
        open = false;
    }
    app.shell.extra.scale = open.then_some(dlg);
}

fn sign_warning(app: &mut AppState, ctx: &egui::Context) {
    let Some(id) = app.shell.extra.sign_warning.clone() else {
        return;
    };
    let mut answer = None;
    egui::Window::new("Signed Document")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("This document is signed. Changing its pages makes the signatures invalid.");
            ui.horizontal(|ui| {
                if ui.button("Continue").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
    match answer {
        Some(true) => {
            if let Some(u) = app.doc().map(|d| d.uid) {
                app.shell.extra.sign_ok.push(u);
            }
            app.shell.extra.sign_warning = None;
            app.queue(&id);
        }
        Some(false) => app.shell.extra.sign_warning = None,
        None => {}
    }
}

/// The page the navigation bar's page box names: a page number, or a page label (exact,
/// then ignoring case).
pub fn page_from_entry(app: &AppState, text: &str) -> Option<usize> {
    let t = text.trim();
    let d = app.doc()?;
    let count = d.session.page_count();
    if let Ok(n) = t.parse::<usize>()
        && (1..=count).contains(&n)
    {
        return Some(n - 1);
    }
    if t.is_empty() {
        return None;
    }
    let pages = &d.session.doc().pages;
    pages
        .iter()
        .position(|p| p.label == t)
        .or_else(|| pages.iter().position(|p| p.label.eq_ignore_ascii_case(t)))
}

/// Startup preferences: a file to open, full screen, the startup mode.
pub fn startup(app: &mut AppState) {
    let x = app.shell.ui.extra.clone();
    if !x.startup_file.trim().is_empty() {
        let p = PathBuf::from(x.startup_file.trim());
        if p.is_file() {
            app.open_path(&p);
        }
    }
    if x.startup_full_screen {
        app.queue("window.full_screen");
    }
    match x.startup_mode.as_str() {
        "view" => {
            app.shell.ui.toolbars.show_markup = false;
            app.shell.ui.toolbars.show_measure = false;
        }
        "markup" => {
            app.shell.ui.toolbars.show_markup = true;
            app.shell.ui.toolbars.show_measure = true;
        }
        _ => {}
    }
    app.shell.extra.launch = true;
}
