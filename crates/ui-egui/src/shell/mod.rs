//! The application shell (Revu's window furniture): which bars and panels show, full screen,
//! presentation, always on top, view history, view rotation and page layout modes, rulers and
//! the crosshair, the dimmer, split views with synchronisation, document tabs, recent files and
//! the last session, toolbar customisation, the panel layout that survives a restart, and the
//! Preferences the rest of the app reads.
//!
//! Commands with ids in [`COMMAND_PREFIXES`] land in [`run`]; `AppState::run`, `enabled` and
//! `checked` ask this module first, so the shell adds commands without touching their bodies.

pub mod deskew;
pub mod detach;
pub mod edges;
pub mod extra;
pub mod extra2;
pub mod files;
pub mod history;
pub mod layout;
pub mod overlay;
pub mod pages;
pub mod pages_more;
pub mod panelbars;
pub mod prefs_more;
pub mod proptoolbar;
pub mod recent;
pub mod shortcutref;
pub mod split;
pub mod tabs;
pub mod toolbars;
pub mod toolbars_more;
pub mod workspace;

use std::collections::HashMap;

use markupcraft_engine::prefs::{PrefStore, Preferences};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::canvas::{PageMode, ViewOpts};

/// Ruler units (right-click a ruler).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RulerUnit {
    #[default]
    In,
    Cm,
    Mm,
    Pt,
    Pica,
}

impl RulerUnit {
    pub const ALL: [RulerUnit; 5] = [
        RulerUnit::In,
        RulerUnit::Cm,
        RulerUnit::Mm,
        RulerUnit::Pt,
        RulerUnit::Pica,
    ];

    /// PDF points per unit.
    pub fn points(self) -> f32 {
        match self {
            RulerUnit::In => 72.0,
            RulerUnit::Cm => 72.0 / 2.54,
            RulerUnit::Mm => 72.0 / 25.4,
            RulerUnit::Pt => 1.0,
            RulerUnit::Pica => 12.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RulerUnit::In => "Inches",
            RulerUnit::Cm => "Centimeters",
            RulerUnit::Mm => "Millimeters",
            RulerUnit::Pt => "Points",
            RulerUnit::Pica => "Picas",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            RulerUnit::In => "in",
            RulerUnit::Cm => "cm",
            RulerUnit::Mm => "mm",
            RulerUnit::Pt => "pt",
            RulerUnit::Pica => "pc",
        }
    }
}

/// Interface preferences MarkupCraft keeps beside the engine's (per profile, in
/// `<config>/ui/<profile>.json`): navigation, document defaults, the look of the workspace,
/// toolbars and the panel layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    // General > Navigation
    pub wheel_zooms_single: bool,
    pub wheel_zooms_continuous: bool,
    pub reverse_wheel: bool,
    pub wheel_sensitivity: f32,
    pub lock_fit_width: bool,
    // General > Document
    /// `single`, `continuous`, `side`, `continuous-side`, `auto` (by page size)
    pub default_mode: String,
    /// `page` or `width`
    pub default_fit: String,
    pub max_zoom_pct: f32,
    pub remember_last_page: bool,
    // General > Options: startup
    pub reopen_last_session: bool,
    pub show_recents_on_start: bool,
    /// Tabs: longest name shown, and which end is cut (`end` or `start`).
    pub tab_max_chars: usize,
    pub tab_truncate_start: bool,
    // View
    pub rulers: bool,
    pub ruler_unit: RulerUnit,
    pub crosshair: bool,
    /// Dimmer amount, percent (Advanced > 2D Rendering).
    pub dimmer_pct: f32,
    // Tools > Markup
    pub reuse_tools: bool,
    // Interface > File Access
    pub recents_days: u32,
    /// File Access: show a preview of a recent file's first page on hover.
    pub recents_preview: bool,
    // Window > Presentation
    pub presentation_loop: bool,
    pub presentation_advance_secs: f32,
    // Window
    pub show_menu: bool,
    pub show_nav_bar: bool,
    pub show_status_bar: bool,
    pub toolbars: toolbars::ToolbarPrefs,
    /// The dock layout (panels), as saved by [`layout::save`].
    pub layout: Option<serde_json::Value>,
    /// Workspace, panels, startup, snapping and other preferences of `extra`.
    pub extra: extra::ExtraPrefs,
    /// Preferences of `extra2`.
    pub extra2: extra2::Prefs2,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            wheel_zooms_single: true,
            wheel_zooms_continuous: true,
            reverse_wheel: false,
            wheel_sensitivity: 1.0,
            lock_fit_width: false,
            default_mode: "continuous".into(),
            default_fit: "page".into(),
            max_zoom_pct: 6400.0,
            remember_last_page: true,
            reopen_last_session: true,
            show_recents_on_start: true,
            tab_max_chars: 32,
            tab_truncate_start: false,
            rulers: false,
            ruler_unit: RulerUnit::In,
            crosshair: false,
            dimmer_pct: 50.0,
            reuse_tools: false,
            recents_days: 90,
            recents_preview: true,
            presentation_loop: false,
            presentation_advance_secs: 0.0,
            show_menu: true,
            show_nav_bar: true,
            show_status_bar: true,
            toolbars: toolbars::ToolbarPrefs::default(),
            layout: None,
            extra: extra::ExtraPrefs::default(),
            extra2: extra2::Prefs2::default(),
        }
    }
}

impl UiPrefs {
    /// Clamp what a settings file may have put out of range.
    pub fn sanitize(&mut self) {
        let fin = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.wheel_sensitivity = fin(self.wheel_sensitivity, 0.1, 10.0, 1.0);
        self.max_zoom_pct = fin(self.max_zoom_pct, 100.0, 6400.0, 6400.0);
        self.dimmer_pct = fin(self.dimmer_pct, 5.0, 95.0, 50.0);
        self.presentation_advance_secs = fin(self.presentation_advance_secs, 0.0, 3600.0, 0.0);
        self.tab_max_chars = self.tab_max_chars.clamp(8, 200);
        self.recents_days = self.recents_days.min(3650);
        if !["single", "continuous", "side", "continuous-side", "auto"].contains(&self.default_mode.as_str()) {
            self.default_mode = "continuous".into();
        }
        if !["page", "width"].contains(&self.default_fit.as_str()) {
            self.default_fit = "page".into();
        }
        self.toolbars.sanitize();
        self.extra.sanitize();
        self.extra2.sanitize();
    }

    /// The page layout a newly opened document takes (`auto`: drawings one page at a time).
    pub fn mode_for(&self, first_page: Option<(f32, f32)>) -> PageMode {
        match self.default_mode.as_str() {
            "single" => PageMode::Single,
            "side" => PageMode::SideBySide,
            "continuous-side" => PageMode::ContinuousSideBySide,
            "auto" => match first_page {
                // Larger than tabloid: a drawing.
                Some((w, h)) if w.max(h) > 17.0 * 72.0 => PageMode::Single,
                _ => PageMode::Continuous,
            },
            _ => PageMode::Continuous,
        }
    }
}

/// A full-window mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Normal,
    /// F11: the chrome hides; the document fills the window.
    FullScreen,
    /// Ctrl+Enter: one page at a time, full screen, arrows or clicks advance, Esc leaves.
    Presentation,
}

/// The shell's state (lives in `AppState::shell`).
pub struct Shell {
    pub ui: UiPrefs,
    /// The engine's preferences (author, units, snapping, theme, recent count...).
    pub prefs: Preferences,
    /// Where preferences persist (`None` in tests and screenshots: in memory only).
    pub store: Option<PrefStore>,
    pub screen: Screen,
    /// What the view looked like before presentation (to restore).
    pub before_presentation: Option<(PageMode, crate::canvas::Fit)>,
    pub presentation_since: f64,
    pub always_on_top: bool,
    /// Shift+F4: the panels are hidden (the layout to restore is kept).
    pub panels_hidden: bool,
    pub hidden_layout: Option<serde_json::Value>,
    pub dimmer: bool,
    pub history: HashMap<u64, history::History>,
    pub split: Option<split::Split>,
    pub recent: recent::RecentStore,
    /// Where `recent` persists (`None` = in memory).
    pub recent_path: Option<std::path::PathBuf>,
    pub show_prefs: bool,
    pub prefs_page: &'static str,
    pub prefs_error: String,
    pub show_customize: bool,
    pub page_dialog: Option<pages::PageDialog>,
    pub thumbs: crate::panels::thumbnails::ThumbState,
    /// The dock layout changed this frame (save it).
    pub layout_dirty: bool,
    /// Layout to apply to the dock at the end of the frame.
    pub apply_layout: Option<serde_json::Value>,
    /// Docs whose renderer the split view borrowed (rebuilt when they show again).
    pub needs_render: Vec<u64>,
    /// Untitled documents made so far.
    pub untitled: u32,
    /// The tool before Shift+Z.
    pub zoom_toggle_from: Option<&'static str>,
    /// Last applied theme (to re-style only on change).
    pub applied_theme: Option<bool>,
    /// Dark workspace, files, detached windows, panel bars and the rest of `extra`.
    pub extra: extra::ExtraState,
    /// Help and the rest of `extra2`.
    pub extra2: extra2::State2,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            ui: UiPrefs::default(),
            prefs: Preferences::default(),
            store: None,
            screen: Screen::Normal,
            before_presentation: None,
            presentation_since: 0.0,
            always_on_top: false,
            panels_hidden: false,
            hidden_layout: None,
            dimmer: false,
            history: HashMap::new(),
            split: None,
            recent: recent::RecentStore::default(),
            recent_path: None,
            show_prefs: false,
            prefs_page: "General",
            prefs_error: String::new(),
            show_customize: false,
            page_dialog: None,
            thumbs: Default::default(),
            layout_dirty: false,
            apply_layout: None,
            needs_render: Vec::new(),
            untitled: 0,
            zoom_toggle_from: None,
            applied_theme: None,
            extra: extra::ExtraState::default(),
            extra2: extra2::State2::default(),
        }
    }
}

impl Shell {
    /// The view options every document's canvas follows.
    pub fn view_opts(&self) -> ViewOpts {
        ViewOpts {
            wheel_zooms_single: self.ui.wheel_zooms_single,
            wheel_zooms_continuous: self.ui.wheel_zooms_continuous,
            reverse_wheel: self.ui.reverse_wheel,
            wheel_sensitivity: self.ui.wheel_sensitivity,
            max_zoom: self.ui.max_zoom_pct / 100.0,
            lock_fit_width: self.ui.lock_fit_width,
            dim: if self.dimmer { self.ui.dimmer_pct / 100.0 } else { 0.0 },
            dark: self.ui.extra.dark_workspace,
            tilt_pans: self.ui.extra2.tilt_pans,
            background: (self.screen == Screen::Presentation).then(|| {
                let [r, g, b] = self.ui.extra2.presentation_background;
                egui::Color32::from_rgb(r, g, b)
            }),
        }
    }

    /// The menu bar shows (hidden by F9, full screen and presentation).
    pub fn menu_visible(&self) -> bool {
        self.ui.show_menu && self.screen == Screen::Normal
    }

    pub fn chrome_visible(&self) -> bool {
        self.screen == Screen::Normal
    }

    /// Save the interface preferences (when there is a store).
    pub fn save_ui(&mut self) {
        let Some(store) = &self.store else { return };
        let path = ui_prefs_path(store);
        let r = serde_json::to_vec_pretty(&self.ui)
            .map_err(|e| e.to_string())
            .and_then(|b| crate::chest::write_atomic(&path, &b).map_err(|e| e.to_string()));
        if let Err(e) = r {
            log::warn!("saving {}: {e}", path.display());
        }
    }

    /// Save the engine preferences (when there is a store).
    pub fn save_prefs(&mut self) {
        if let Some(store) = &self.store
            && let Err(e) = store.save(&self.prefs)
        {
            self.prefs_error = e.to_string();
        }
    }
}

/// `<config>/ui/<active profile>.json`
pub fn ui_prefs_path(store: &PrefStore) -> std::path::PathBuf {
    store.dir.join("ui").join(format!("{}.json", store.active()))
}

/// Read the interface preferences of the store's active profile (defaults when missing or bad).
pub fn load_ui(store: &PrefStore) -> UiPrefs {
    let path = ui_prefs_path(store);
    let mut p = std::fs::read(&path)
        .ok()
        .filter(|b| b.len() < (4 << 20))
        .and_then(|b| serde_json::from_slice::<UiPrefs>(&b).ok())
        .or_else(|| extra2::shipped_ui(&store.active()))
        .unwrap_or_default();
    p.sanitize();
    p
}

/// Command id prefixes the shell answers.
pub const COMMAND_PREFIXES: &[&str] = &["shell.", "split.", "pages."];

/// Shell commands that live under the ordinary menus (`view.`, `window.`, `file.` ids).
const OWN: &[&str] = &[
    "file.new",
    "file.close_all",
    "file.close_others",
    "file.save_all",
    "file.refresh",
    "file.clear_recent",
    "view.side_by_side",
    "view.continuous_side",
    "view.cover_page",
    "view.prev_view",
    "view.next_view",
    "view.rotate_view_cw",
    "view.rotate_view_ccw",
    "view.refresh",
    "view.rulers",
    "view.crosshair",
    "view.dimmer",
    "view.toggle_zoom",
    "view.split_vertical",
    "view.split_horizontal",
    "view.unsplit",
    "view.toggle_split",
    "view.switch",
    "view.balance",
    "view.sync_off",
    "view.sync_document",
    "view.sync_page",
    "window.full_screen",
    "window.presentation",
    "window.always_on_top",
    "window.hide_panels",
    "window.menu_bar",
    "window.nav_bar",
    "window.status_bar",
    "window.preferences",
    "window.toolbar_main",
    "window.toolbar_markup",
    "window.toolbar_measure",
    "window.customize_toolbars",
    "window.lock_toolbars",
    "window.reuse_tools",
    "document.rotate_pages",
    "document.crop_pages",
    "document.page_setup",
    "document.replace_pages",
    "document.insert_blank_pages",
    "document.extract_pages",
    "document.delete_pages",
];

fn ours(id: &str) -> bool {
    OWN.contains(&id) || COMMAND_PREFIXES.iter().any(|p| id.starts_with(p))
}

/// Whether a shell command can run now (`None` = not a shell command).
pub fn enabled(app: &AppState, id: &str) -> Option<bool> {
    if let Some(e) = extra2::enabled(app, id) {
        return Some(e);
    }
    if let Some(e) = extra::enabled(app, id) {
        return Some(e);
    }
    if !ours(id) {
        return None;
    }
    let doc = app.has_doc();
    Some(match id {
        "view.prev_view" => app
            .doc()
            .is_some_and(|d| app.shell.history.get(&d.uid).is_some_and(|h| !h.back.is_empty())),
        "view.next_view" => app
            .doc()
            .is_some_and(|d| app.shell.history.get(&d.uid).is_some_and(|h| !h.forward.is_empty())),
        "file.close_all" | "file.save_all" => doc,
        "file.close_others" => app.docs.len() > 1,
        "view.unsplit" | "view.toggle_split" | "view.switch" | "view.balance" | "view.sync_off"
        | "view.sync_document" | "view.sync_page" => app.shell.split.is_some(),
        _ if id.starts_with("window.") || id.starts_with("file.") || id == "view.rulers" || id == "view.crosshair" => {
            true
        }
        _ => doc,
    })
}

/// Checkmarks for shell toggles (`None` = not a shell toggle).
pub fn checked(app: &AppState, id: &str) -> Option<bool> {
    if let Some(c) = extra2::checked(app, id) {
        return Some(c);
    }
    if let Some(c) = extra::checked(app, id) {
        return Some(c);
    }
    let s = &app.shell;
    let mode = |m: PageMode| app.doc().is_some_and(|d| d.view.mode == m);
    let sync = |k: split::Sync| s.split.as_ref().is_some_and(|sp| sp.sync == k);
    Some(match id {
        "view.side_by_side" => mode(PageMode::SideBySide),
        "view.continuous_side" => mode(PageMode::ContinuousSideBySide),
        "view.cover_page" => app.doc().is_some_and(|d| d.view.cover),
        "view.rulers" => s.ui.rulers,
        "view.crosshair" => s.ui.crosshair,
        "view.dimmer" => s.dimmer,
        "view.split_vertical" => s.split.as_ref().is_some_and(|sp| sp.vertical),
        "view.split_horizontal" => s.split.as_ref().is_some_and(|sp| !sp.vertical),
        "view.sync_off" => sync(split::Sync::Off),
        "view.sync_document" => sync(split::Sync::Document),
        "view.sync_page" => sync(split::Sync::Page),
        "window.full_screen" => s.screen == Screen::FullScreen,
        "window.presentation" => s.screen == Screen::Presentation,
        "window.always_on_top" => s.always_on_top,
        "window.hide_panels" => s.panels_hidden,
        "window.menu_bar" => s.ui.show_menu,
        "window.nav_bar" => s.ui.show_nav_bar,
        "window.status_bar" => s.ui.show_status_bar,
        "window.toolbar_main" => s.ui.toolbars.show_main,
        "window.toolbar_markup" => s.ui.toolbars.show_markup,
        "window.toolbar_measure" => s.ui.toolbars.show_measure,
        "window.lock_toolbars" => s.ui.toolbars.locked,
        "window.reuse_tools" => app.tool_locked,
        _ => return None,
    })
}

/// Run a shell command; `false` when `id` is not one.
pub fn run(app: &mut AppState, id: &str, ctx: &egui::Context) -> bool {
    if extra2::run(app, id, ctx) || extra::run(app, id, ctx) {
        return true;
    }
    if !ours(id) {
        return false;
    }
    if extra::page_edit_blocked(app, id) {
        return true;
    }
    let now = ctx.input(|i| i.time);
    match id {
        "file.new" => recent::new_blank(app),
        "file.close_all" => tabs::close_all(app, None),
        "file.close_others" => {
            let keep = app.doc().map(|d| d.uid);
            tabs::close_all(app, keep);
        }
        "file.save_all" => tabs::save_all(app),
        "file.refresh" => recent::reload(app),
        "file.clear_recent" => {
            app.shell.recent.files.clear();
            recent::persist(app);
        }
        "view.side_by_side" | "view.continuous_side" => {
            let m = if id == "view.side_by_side" {
                PageMode::SideBySide
            } else {
                PageMode::ContinuousSideBySide
            };
            if let Some(d) = app.doc_mut() {
                d.view.set_mode(m);
            }
        }
        "view.cover_page" => {
            if let Some(d) = app.doc_mut() {
                d.view.cover = !d.view.cover;
                d.view.set_mode(d.view.mode);
            }
        }
        "view.prev_view" | "view.next_view" => history::step(app, id == "view.prev_view"),
        "view.rotate_view_cw" | "view.rotate_view_ccw" => {
            if let Some(d) = app.doc_mut() {
                let step = if id == "view.rotate_view_cw" { 90 } else { 270 };
                d.view.rotation = (d.view.rotation + step) % 360;
                d.view.set_fit(d.view.fit);
                d.view.invalidate();
            }
        }
        "view.refresh" => {
            if let Some(d) = app.doc_mut() {
                d.view.invalidate();
            }
            app.status = "Refreshed".into();
        }
        "view.rulers" => {
            app.shell.ui.rulers = !app.shell.ui.rulers;
            app.shell.save_ui();
        }
        "view.crosshair" => {
            app.shell.ui.crosshair = !app.shell.ui.crosshair;
            app.shell.save_ui();
        }
        "view.dimmer" => app.shell.dimmer = !app.shell.dimmer,
        "view.toggle_zoom" => {
            if app.tool == "zoom" {
                let back = app.shell.zoom_toggle_from.take().unwrap_or("select");
                app.set_tool(back);
            } else {
                app.shell.zoom_toggle_from = Some(app.tool);
                app.set_tool("zoom");
            }
        }
        "view.split_vertical" | "view.split_horizontal" => split::split(app, id == "view.split_vertical"),
        "view.unsplit" => split::unsplit(app),
        "view.toggle_split" => {
            if let Some(s) = &mut app.shell.split {
                s.vertical = !s.vertical;
            }
        }
        "view.switch" => split::switch(app),
        "view.balance" => {
            if let Some(s) = &mut app.shell.split {
                s.frac = 0.5;
            }
        }
        "view.sync_off" | "view.sync_document" | "view.sync_page" => {
            if let Some(s) = &mut app.shell.split {
                s.sync = match id {
                    "view.sync_document" => split::Sync::Document,
                    "view.sync_page" => split::Sync::Page,
                    _ => split::Sync::Off,
                };
                s.last = None;
            }
        }
        "window.full_screen" => {
            let full = app.shell.screen != Screen::FullScreen;
            leave_presentation(app);
            app.shell.screen = if full { Screen::FullScreen } else { Screen::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(full));
        }
        "window.presentation" => {
            if app.shell.screen == Screen::Presentation {
                leave_presentation(app);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            } else {
                enter_presentation(app, now);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            }
        }
        "window.always_on_top" => {
            app.shell.always_on_top = !app.shell.always_on_top;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if app.shell.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }
        "window.hide_panels" => app.shell.panels_hidden = !app.shell.panels_hidden,
        "window.menu_bar" => toggle_ui(app, |u| &mut u.show_menu),
        "window.nav_bar" => toggle_ui(app, |u| &mut u.show_nav_bar),
        "window.status_bar" => toggle_ui(app, |u| &mut u.show_status_bar),
        "window.toolbar_main" => toggle_ui(app, |u| &mut u.toolbars.show_main),
        "window.toolbar_markup" => toggle_ui(app, |u| &mut u.toolbars.show_markup),
        "window.toolbar_measure" => toggle_ui(app, |u| &mut u.toolbars.show_measure),
        "window.lock_toolbars" => toggle_ui(app, |u| &mut u.toolbars.locked),
        "window.customize_toolbars" => app.shell.show_customize = true,
        "window.preferences" => app.shell.show_prefs = true,
        "window.reuse_tools" => {
            app.tool_locked = !app.tool_locked;
            app.shell.ui.reuse_tools = app.tool_locked;
            app.shell.save_ui();
        }
        _ if id.starts_with("document.") || id.starts_with("pages.") => pages::open(app, id),
        _ => log::debug!("shell command {id} is not wired"),
    }
    true
}

fn toggle_ui(app: &mut AppState, f: impl FnOnce(&mut UiPrefs) -> &mut bool) {
    let v = f(&mut app.shell.ui);
    *v = !*v;
    app.shell.save_ui();
}

fn enter_presentation(app: &mut AppState, now: f64) {
    if let Some(d) = app.doc_mut() {
        let before = (d.view.mode, d.view.fit);
        d.view.set_mode(PageMode::Single);
        d.view.set_fit(crate::canvas::Fit::Page);
        app.shell.before_presentation = Some(before);
    }
    app.shell.screen = Screen::Presentation;
    app.shell.presentation_since = now;
}

/// Back from presentation to how the view was.
pub fn leave_presentation(app: &mut AppState) {
    if app.shell.screen != Screen::Presentation {
        return;
    }
    app.shell.screen = Screen::Normal;
    if let Some((mode, fit)) = app.shell.before_presentation.take()
        && let Some(d) = app.doc_mut()
    {
        d.view.set_mode(mode);
        d.view.set_fit(fit);
    }
}

/// Esc leaves full screen and presentation (before it reaches the canvas).
pub fn escape(app: &mut AppState, ctx: &egui::Context) -> bool {
    match app.shell.screen {
        Screen::Normal => false,
        Screen::FullScreen => {
            app.shell.screen = Screen::Normal;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            true
        }
        Screen::Presentation => {
            leave_presentation(app);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            true
        }
    }
}

/// Start of a frame: apply preferences to the views, record view history, presentation keys.
pub fn begin_frame(app: &mut AppState, ctx: &egui::Context) {
    let opts = app.shell.view_opts();
    for d in &mut app.docs {
        d.view.opts = opts;
    }
    if let Some(s) = &mut app.shell.split {
        s.pane.view.opts = opts;
        for p in &mut s.more {
            p.view.opts = opts;
        }
    }
    let dark = match app.shell.prefs.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => ctx.system_theme() == Some(egui::Theme::Dark) && app.shell.store.is_some(),
    };
    if app.shell.applied_theme != Some(dark) {
        crate::theme::apply_mode(ctx, dark);
        app.shell.applied_theme = Some(dark);
    }
    if app.shell.screen != Screen::Normal
        && !ctx.egui_wants_keyboard_input()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        escape(app, ctx);
    }
    if app.shell.screen == Screen::Presentation {
        presentation_keys(app, ctx);
    }
    history::record(app);
    extra::begin_frame(app, ctx);
    extra2::begin_frame(app, ctx);
}

fn presentation_keys(app: &mut AppState, ctx: &egui::Context) {
    let (fwd, back, now) = ctx.input_mut(|i| {
        let fwd = [
            egui::Key::ArrowRight,
            egui::Key::ArrowDown,
            egui::Key::PageDown,
            egui::Key::Space,
        ]
        .iter()
        .any(|k| i.consume_key(egui::Modifiers::NONE, *k));
        let back = [egui::Key::ArrowLeft, egui::Key::ArrowUp, egui::Key::PageUp]
            .iter()
            .any(|k| i.consume_key(egui::Modifiers::NONE, *k));
        (fwd, back, i.time)
    });
    let advance = app.shell.ui.presentation_advance_secs;
    let timed = advance > 0.0 && now - app.shell.presentation_since >= f64::from(advance);
    if timed {
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(advance.max(0.1)));
    }
    let looped = app.shell.ui.presentation_loop;
    if let Some(d) = app.doc_mut() {
        let n = d.session.page_count();
        if n == 0 {
            return;
        }
        let c = d.view.current;
        let to = if fwd || timed {
            if c + 1 < n {
                Some(c + 1)
            } else if looped {
                Some(0)
            } else {
                None
            }
        } else if back {
            Some(c.saturating_sub(1))
        } else {
            None
        };
        if let Some(p) = to {
            d.view.go_to_page(p, n);
        }
    }
    if fwd || back || timed {
        app.shell.presentation_since = now;
    }
}

/// A file dialog answered with `Purpose::Shell { tag }`.
pub fn dialog_answer(app: &mut AppState, tag: &str, paths: &[std::path::PathBuf]) {
    let Some(first) = paths.first() else { return };
    if tag.starts_with("x-") {
        extra::dialog_answer(app, tag, paths);
    } else if tag.starts_with("prefs-") {
        crate::prefs_ui::answer(app, tag, first);
    } else if !pages_more::answer(app, tag, paths) {
        pages::answer(app, tag, first);
    }
}

/// The shell's windows: Preferences, Customize Toolbars, page dialogs.
pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    crate::prefs_ui::window(app, ctx);
    toolbars::customize_window(app, ctx);
    pages::window(app, ctx);
    extra::windows(app, ctx);
    extra2::windows(app, ctx);
}

/// The desktop app: preferences, interface settings, recent files and the last session from
/// the user's config folder.
pub fn load_user(app: &mut AppState) {
    let Ok(store) = PrefStore::user() else { return };
    app.shell.recent_path = Some(store.dir.join("recent.json"));
    app.shell.recent = recent::RecentStore::load(&store.dir.join("recent.json"));
    app.shell.store = Some(store);
    files::enable_recovery(app, store_dir_recovery(app));
    crate::prefs_ui::load_from_store(app);
    recent::reopen_last_session(app);
    extra::startup(app);
    extra2::startup(app);
}

fn store_dir_recovery(app: &AppState) -> std::path::PathBuf {
    app.shell
        .store
        .as_ref()
        .map_or_else(std::env::temp_dir, |s| s.dir.clone())
        .join("recovery")
}

/// Dock work the state cannot do itself: hide / show all panels, apply a loaded layout, and
/// save the layout when it changes.
pub fn dock_frame(app: &mut AppState, dock: &mut egui_dock::DockState<crate::dock::Tab>) {
    panelbars::apply_ops(app, dock);
    if let Some(l) = app.shell.apply_layout.take() {
        *dock = layout::load(&l).unwrap_or_else(crate::dock::default_layout);
        app.shell.panels_hidden = false;
        app.shell.hidden_layout = None;
    }
    let hidden_now = app.shell.hidden_layout.is_some();
    if app.shell.panels_hidden != hidden_now {
        if app.shell.panels_hidden {
            app.shell.hidden_layout = layout::save(dock);
            *dock = egui_dock::DockState::new(vec![crate::dock::Tab::Document]);
        } else {
            let back = app.shell.hidden_layout.take();
            *dock = back
                .as_ref()
                .and_then(layout::load)
                .unwrap_or_else(crate::dock::default_layout);
        }
    }
    if !app.shell.panels_hidden && app.shell.store.is_some() {
        let now = layout::save(dock);
        if now != app.shell.ui.layout {
            // Save once the pointer is up (not on every frame of a drag).
            app.shell.layout_dirty = true;
            app.shell.ui.layout = now;
        }
    }
}

/// End of a frame: persist what changed.
pub fn end_frame(app: &mut AppState, ctx: &egui::Context) {
    if app.shell.layout_dirty && !ctx.input(|i| i.pointer.any_down()) {
        app.shell.layout_dirty = false;
        app.shell.save_ui();
    }
    recent::track_session(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_rule_by_page_size() {
        let mut u = UiPrefs::default();
        assert_eq!(u.mode_for(Some((612.0, 792.0))), PageMode::Continuous);
        u.default_mode = "auto".into();
        // A 24 x 36 in drawing opens one page at a time; a letter page scrolls.
        assert_eq!(u.mode_for(Some((2592.0, 1728.0))), PageMode::Single);
        assert_eq!(u.mode_for(Some((612.0, 792.0))), PageMode::Continuous);
        u.default_mode = "side".into();
        assert_eq!(u.mode_for(None), PageMode::SideBySide);
        // Out-of-range values from a settings file are pulled back.
        u.default_mode = "sideways".into();
        u.max_zoom_pct = f32::NAN;
        u.tab_max_chars = 0;
        u.sanitize();
        assert_eq!(u.default_mode, "continuous");
        assert_eq!(u.max_zoom_pct, 6400.0);
        assert_eq!(u.tab_max_chars, 8);
    }
}
