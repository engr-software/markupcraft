//! The Revu-style interface of MarkupCraft (egui + eframe on wgpu).
//!
//! Layout: menu bar and toolbar on top, a docking area (egui_dock) with the document canvas in
//! the middle and the panels around it, the status bar at the bottom. Three tables drive it:
//! [`commands::COMMANDS`] (menus, toolbar, shortcuts), [`tools::TOOLS`] (what the mouse does)
//! and [`panels::PANELS`] (dockable panels). Every document change goes through
//! `markupcraft_engine::Session`, the same session the automation tools drive, so undo/redo,
//! the command table and page operations are shared with the CLI and MCP server.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod actions;
#[cfg(target_arch = "wasm32")]
pub mod browser;
#[cfg(all(feature = "camera", not(target_arch = "wasm32")))]
pub mod camera;
pub mod canvas;
pub mod chest;
pub mod chest_more;
pub mod chest_sets;
pub mod chest_shared;
pub mod chrome;
pub mod commands;
pub mod context_menu;
pub mod context_text;
pub mod dialogs;
pub mod dock;
pub mod editing;
pub mod features;
pub mod gestures;
pub mod i18n;
pub mod icon_data;
pub mod icons;
pub mod interact;
pub mod keyprefs;
pub mod markup_prefs;
pub mod markups_more;
pub mod modkeys;
pub mod more;
pub mod painter;
pub mod panels;
pub mod prefs_ui;
pub mod richedit;
pub mod shapes_more;
pub mod shell;
pub mod sketch;
pub mod snapping;
pub mod snapping_more;
pub mod spell_prefs;
pub mod tablet_prefs;
pub mod theme;
pub mod tools;
pub mod viewports;
pub mod viewports_more;
pub mod windows;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_dock::{DockArea, DockState};
use markupcraft_engine::Session;
use markupcraft_model::Markup;
use markupcraft_render::text::{PageText, TextExtractor, TextSource};
use markupcraft_render::{RenderDoc, RenderOptions};

use crate::canvas::{CanvasAction, CanvasCx, CanvasOut, DocView, Fit, PageMode};
use crate::chest::ToolChest;
use crate::dialogs::{Dialogs, Purpose};
use crate::dock::Tab;
use crate::snapping::SnapCache;

/// One open document.
pub struct DocTab {
    /// Stable for the life of the tab (prompts and dialogs refer to it).
    pub uid: u64,
    /// File name shown on the tab.
    pub name: String,
    /// Where Save writes; `None` = Save As first.
    pub path: Option<PathBuf>,
    pub session: Session,
    pub render: Option<RenderDoc>,
    /// The bytes the renderer, snapping and text layer read.
    pub bytes: Arc<Vec<u8>>,
    pub view: DocView,
    pub snaps: SnapCache,
    text: Option<TextExtractor>,
    /// Page objects when the renderer was built (a page operation changes them).
    page_refs: Vec<(u32, u16, i32, [i64; 4])>,
}

impl DocTab {
    /// The page's text layer (for text markups), read on first use.
    pub fn page_text(&mut self, page: usize) -> Option<Arc<PageText>> {
        let bytes = self.bytes.clone();
        self.text
            .get_or_insert_with(|| TextExtractor::new(bytes))
            .page_text(page)
    }

    pub fn selection(&self) -> &[String] {
        self.session.selection()
    }

    /// What identifies the page structure: each page object with its rotation and size.
    fn current_refs(&self) -> Vec<(u32, u16, i32, [i64; 4])> {
        let pages = &self.session.doc().pages;
        markupcraft_render::snap::page_refs(&self.session.pdf().cos)
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                let (rot, m) = pages.get(i).map_or((0, Default::default()), |p| (p.rotate, p.media));
                let q = |v: f64| (v * 100.0).round() as i64;
                (r.num, r.generation, rot, [q(m.x0), q(m.y0), q(m.x1), q(m.y1)])
            })
            .collect()
    }

    /// Rebuild the renderer from the session's current bytes (after a save or a page
    /// operation).
    fn rerender(&mut self, threads: usize) {
        match self.session.render_bytes() {
            Ok(bytes) => {
                let opts = RenderOptions {
                    hide: actions::drawn_objects(self.session.doc()),
                    threads,
                    hide_all_markups: false,
                };
                self.render = RenderDoc::open(bytes.clone(), &opts).ok();
                self.bytes = bytes;
            }
            Err(e) => log::warn!("render {}: {e}", self.name),
        }
        self.view.invalidate();
        self.snaps.invalidate();
        self.text = None;
        self.page_refs = self.current_refs();
        let count = self.session.page_count();
        if self.view.current >= count {
            self.view.go_to_page(count.saturating_sub(1), count);
        }
    }

    /// Re-render when a page operation (or its undo) changed the pages.
    fn sync_pages(&mut self, threads: usize) {
        if self.current_refs() != self.page_refs {
            self.rerender(threads);
        }
    }
}

/// Status bar toggles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Snaps {
    pub grid: bool,
    pub content: bool,
    pub markup: bool,
    /// Snap targets turned off (`snapping::MASK_*` bits; Preferences > Grid & Snap > Snap to).
    pub mask: u8,
    /// The snap indicator's colour (`None` = the default).
    pub color: Option<[u8; 3]>,
}

/// The Calibrate dialog.
#[derive(Debug, Clone)]
pub struct Calibrate {
    pub doc: u64,
    pub page: usize,
    pub a: markupcraft_geom::Point,
    pub b: markupcraft_geom::Point,
    pub length: String,
    pub unit: markupcraft_measure::units::LengthUnit,
    /// "" = this page, "all", or a range like `1-3, 7`
    pub pages: String,
    pub apply_to_markups: bool,
}

/// A question waiting for the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Prompt {
    /// Close this document, which has unsaved changes.
    Close(u64),
    /// Exit with these documents unsaved.
    Exit(Vec<u64>),
}

/// Everything except the dock layout (so panels can borrow it while the dock draws them).
pub struct AppState {
    pub docs: Vec<DocTab>,
    pub active: usize,
    /// Active tool id (`tools::TOOLS`).
    pub tool: &'static str,
    /// Keep the tool after drawing (else back to Select, Revu's default).
    pub tool_locked: bool,
    /// The Tool Chest item drawing with the active tool: (set id, item id).
    pub active_item: Option<(String, String)>,
    pub toolchest: ToolChest,
    /// Stamp design for the Stamp tool.
    pub stamp: &'static str,
    pub snaps: Snaps,
    /// Draw the grid over the pages (Show Grid; Snap to Grid is a separate toggle).
    pub show_grid: bool,
    /// Last message for the status bar.
    pub status: String,
    pub hide_markups: bool,
    /// The wheel zooms (Revu's default); Ctrl+wheel always zooms.
    pub wheel_zooms: bool,
    /// Author written on new markups.
    pub author: String,
    /// Render worker threads (0 = render on the UI thread).
    pub threads: usize,
    pub page_entry: String,
    pub show_shortcuts: bool,
    pub show_about: bool,
    pub show_properties: bool,
    /// Panels open in the dock (refreshed every frame, for menu checkmarks).
    pub open_panels: Vec<&'static str>,
    /// The Markups List's view and editing state.
    pub list: panels::markups_list::ListState,
    /// The Measurements panel's entry fields.
    pub measure: panels::measurements::MeasureState,
    /// The Thumbnails panel was drawn this frame / last frame.
    pub thumbs_wanted: bool,
    pub thumbs_wanted_last: bool,
    pub dialogs: Dialogs,
    pub prompts: Vec<Prompt>,
    pub calibrate: Option<Calibrate>,
    /// Search, compare, spaces, layers, summary and the other document features.
    pub features: features::FeatureState,
    /// Format Painter, Highlight Viewports, Sketch to Scale.
    pub edit: editing::EditState,
    /// The user's keyboard shortcuts.
    pub keys: keyprefs::KeyPrefs,
    /// Commands to run at the end of the frame.
    queued: Vec<String>,
    /// Dock changes the state cannot make itself.
    panel_toggles: Vec<&'static str>,
    panel_shows: Vec<&'static str>,
    reset_layout: bool,
    /// The window may close now (prompts answered).
    pub close_now: bool,
    next_uid: u64,
    /// Window furniture, view history, splits, recent files, preferences (`shell`).
    pub shell: shell::Shell,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            docs: Vec::new(),
            active: 0,
            tool: "select",
            tool_locked: false,
            active_item: None,
            toolchest: ToolChest::default(),
            stamp: "Approved",
            snaps: Snaps::default(),
            show_grid: false,
            status: String::new(),
            hide_markups: false,
            wheel_zooms: true,
            author: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default(),
            threads: RenderOptions::default_threads(),
            page_entry: String::new(),
            show_shortcuts: false,
            show_about: false,
            show_properties: false,
            open_panels: Vec::new(),
            list: Default::default(),
            measure: Default::default(),
            thumbs_wanted: false,
            thumbs_wanted_last: true,
            dialogs: Dialogs::default(),
            prompts: Vec::new(),
            calibrate: None,
            features: Default::default(),
            edit: Default::default(),
            keys: Default::default(),
            queued: Vec::new(),
            panel_toggles: Vec::new(),
            panel_shows: Vec::new(),
            reset_layout: false,
            close_now: false,
            next_uid: 1,
            shell: shell::Shell::default(),
        }
    }
}

/// Commands that need the operating system: a system printer, a camera or scanner, the mail
/// program, shell integration, a desktop browser, quitting the process. Greyed out in the
/// browser build.
pub const NEEDS_DESKTOP: &[&str] = &[
    "file.print",
    "file.create_from_scanner",
    "markup.camera",
    "markup.image_scanner",
    "file.email",
    "file.shell_integration",
    "view.web_tab",
    "file.exit",
];

/// The page ids (0-based) a `pages` field names: "" = `current`, "all", or a range text.
pub fn pages_from_text(text: &str, current: usize, count: usize) -> Option<Vec<usize>> {
    let t = text.trim();
    if t.is_empty() {
        return (current < count).then(|| vec![current]);
    }
    if t.eq_ignore_ascii_case("all") {
        return Some((0..count).collect());
    }
    markupcraft_measure::units::parse_page_range(t, count).filter(|v| !v.is_empty())
}

impl AppState {
    pub fn has_doc(&self) -> bool {
        self.docs.get(self.active).is_some()
    }

    pub fn doc(&self) -> Option<&DocTab> {
        self.docs.get(self.active)
    }

    pub fn doc_mut(&mut self) -> Option<&mut DocTab> {
        self.docs.get_mut(self.active)
    }

    fn doc_by_uid(&mut self, uid: u64) -> Option<&mut DocTab> {
        self.docs.iter_mut().find(|d| d.uid == uid)
    }

    /// Run `id` at the end of this frame (menus, toolbar, shortcuts and panels all queue).
    pub fn queue(&mut self, id: &str) {
        self.queued.push(id.to_string());
    }

    /// Show a panel (open it if closed) at the end of the frame.
    pub fn show_panel(&mut self, id: &'static str) {
        self.panel_shows.push(id);
    }

    /// Open a PDF from bytes. `path` is where Save writes.
    pub fn open_bytes(&mut self, name: &str, path: Option<PathBuf>, bytes: Vec<u8>) -> Result<(), String> {
        let at = path.clone().unwrap_or_else(|| PathBuf::from(name));
        let mut session = Session::from_bytes(bytes, &at).map_err(|e| e.to_string())?;
        session.set_author(&self.author);
        let file_bytes = session.pdf().bytes();
        let opts = RenderOptions {
            hide: actions::drawn_objects(session.doc()),
            threads: self.threads,
            hide_all_markups: false,
        };
        let render = RenderDoc::open(file_bytes.clone(), &opts).map_err(|e| e.to_string())?;
        let uid = self.next_uid;
        self.next_uid += 1;
        let mut tab = DocTab {
            uid,
            name: name.to_string(),
            path,
            session,
            render: Some(render),
            bytes: file_bytes,
            view: DocView::default(),
            snaps: SnapCache::new(self.threads == 0),
            text: None,
            page_refs: Vec::new(),
        };
        tab.page_refs = tab.current_refs();
        self.docs.push(tab);
        self.active = self.docs.len() - 1;
        self.status = format!("Opened {name}");
        // Preferences > Advanced: its JavaScript runs on opening when allowed.
        features::more6::script::on_open(self);
        Ok(())
    }

    /// Open a file from disk (argv, File > Open, a drop).
    pub fn open_path(&mut self, path: &Path) {
        if shell::files::intercept_open(self, path) {
            return;
        }
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(path)) {
            self.active = i;
            return;
        }
        let r = markupcraft_revu::fsio::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| self.open_bytes(&name, Some(path.to_path_buf()), b));
        match r {
            Ok(()) => shell::recent::opened(self, path),
            Err(e) => {
                self.status = format!("Could not open {name}: {e}");
                log::warn!("{}", self.status);
            }
        }
    }

    /// Close document `i`, asking first when it has unsaved changes.
    pub fn close_doc(&mut self, i: usize) {
        let Some(d) = self.docs.get(i) else { return };
        if d.session.is_dirty() {
            let p = Prompt::Close(d.uid);
            if !self.prompts.contains(&p) {
                self.prompts.push(p);
            }
            return;
        }
        self.force_close(d.uid);
    }

    /// Close a document without asking.
    pub fn force_close(&mut self, uid: u64) {
        shell::recent::closing(self, uid);
        if let Some(i) = self.docs.iter().position(|d| d.uid == uid) {
            self.docs.remove(i);
            if self.active > i || self.active >= self.docs.len() {
                self.active = self.active.saturating_sub(1).min(self.docs.len().saturating_sub(1));
            }
        }
    }

    /// Save a document to its file; without one, ask where (then close when `then_close`).
    pub fn save_doc(&mut self, uid: u64, as_new: bool, then_close: bool) {
        let Some(i) = self.docs.iter().position(|d| d.uid == uid) else {
            return;
        };
        let Some(d) = self.docs.get(i) else { return };
        match (&d.path, as_new) {
            (Some(p), false) => {
                let p = p.clone();
                self.save_to(uid, &p, then_close);
            }
            _ => {
                let name = d.name.clone();
                self.dialogs
                    .save(Purpose::SaveAs { doc: i, then_close }, dialogs::PDF, &name);
            }
        }
    }

    fn save_to(&mut self, uid: u64, path: &Path, then_close: bool) {
        let threads = self.threads;
        let compressed = self.shell.prefs.save_mode == "compressed";
        let full_saves = self.shell.prefs.save_mode == "full" || compressed;
        let Some(d) = self.doc_by_uid(uid) else { return };
        interact::commit_editor(d, &mut CanvasOut::default());
        let same = d.path.as_deref() == Some(path);
        // Preferences > Document: keep revisions (incremental) or publish (a full rewrite).
        let full = full_saves;
        let r = if same {
            d.session.save(full)
        } else {
            d.session.save_as(path, full)
        };
        // Publish compressed: the saved file is rewritten with compressed object streams
        let r = r.and_then(|()| {
            if compressed {
                d.session
                    .publish_as(path, markupcraft_engine::docfile::PublishMode::Compressed)
                    .map(|_| ())
            } else {
                Ok(())
            }
        });
        match r {
            Ok(()) => {
                d.path = Some(path.to_path_buf());
                d.name = path
                    .file_name()
                    .map_or_else(|| d.name.clone(), |n| n.to_string_lossy().into_owned());
                d.rerender(threads);
                self.status = format!("Saved {}", path.display());
                if then_close {
                    self.force_close(uid);
                    self.after_prompt_save();
                }
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// An Exit prompt finishes once every document it waited on is saved.
    fn after_prompt_save(&mut self) {
        let open: Vec<u64> = self.docs.iter().map(|d| d.uid).collect();
        let mut done = false;
        for p in &mut self.prompts {
            if let Prompt::Exit(list) = p {
                list.retain(|u| open.contains(u));
                done |= list.is_empty();
            }
        }
        if done {
            self.prompts.retain(|p| !matches!(p, Prompt::Exit(l) if l.is_empty()));
            self.close_now = true;
        }
    }

    /// The window was asked to close: true when it may (nothing unsaved, or already asked).
    pub fn may_exit(&mut self) -> bool {
        if self.close_now {
            return true;
        }
        let dirty: Vec<u64> = self
            .docs
            .iter()
            .filter(|d| d.session.is_dirty())
            .map(|d| d.uid)
            .collect();
        if dirty.is_empty() {
            return true;
        }
        if !self.prompts.iter().any(|p| matches!(p, Prompt::Exit(_))) {
            self.prompts.push(Prompt::Exit(dirty));
        }
        false
    }

    /// Answers from file dialogs.
    fn take_dialogs(&mut self) {
        for (purpose, paths) in self.dialogs.poll() {
            let Some(first) = paths.first().cloned() else {
                if let Purpose::SaveAs { then_close: true, .. } = purpose {
                    self.status = "Not saved; the document stays open".into();
                }
                continue;
            };
            match purpose {
                Purpose::Open => {
                    for p in &paths {
                        self.open_path(p);
                    }
                }
                Purpose::SaveAs { doc, then_close } => {
                    if let Some(uid) = self.docs.get(doc).map(|d| d.uid) {
                        self.save_to(uid, &first, then_close);
                    }
                }
                Purpose::Edit(tag, arg) => self.status = editing::dialog_answer(self, tag, &arg, &first),
                Purpose::ExportCsv | Purpose::ExportTotals | Purpose::ExportXml => {
                    self.status = panels::markups_list::export(self, &purpose, &first);
                }
                Purpose::InsertPages { at } => {
                    let threads = self.threads;
                    if let Some(d) = self.doc_mut() {
                        let items: Vec<_> = paths.iter().map(|p| (p.clone(), None)).collect();
                        let r = d.session.insert_files(at, &items);
                        d.sync_pages(threads);
                        self.status = actions::report(r, |rep| {
                            format!(
                                "Inserted {}",
                                actions::plural(rep.pages_after.saturating_sub(rep.pages_before), "page")
                            )
                        });
                    }
                }
                Purpose::Shell { tag } => shell::dialog_answer(self, &tag, &paths),
                Purpose::Feature(ask) => features::answer(self, ask, paths),
                Purpose::ExtractPages { pages } => {
                    if let Some(d) = self.doc_mut() {
                        let r = d.session.extract_pages(&pages, &first, false);
                        self.status = actions::report(r, |n| {
                            format!("Extracted {} to {}", actions::plural(n, "page"), first.display())
                        });
                    }
                }
            }
        }
    }

    /// Whether a command can run now.
    pub fn enabled(&self, id: &str) -> bool {
        if cfg!(target_arch = "wasm32") && NEEDS_DESKTOP.contains(&id) {
            return false;
        }
        if let Some(e) = shell::enabled(self, id) {
            return e;
        }
        let doc = self.doc();
        let selected = doc.is_some_and(|d| !d.selection().is_empty());
        match id {
            "file.open"
            | "file.exit"
            | "help.shortcuts"
            | "help.about"
            | "window.reset_layout"
            | "tools.keep_tool"
            | "tools.customize_keys" => true,
            "edit.undo" => doc.is_some_and(|d| d.session.can_undo()),
            "edit.redo" => doc.is_some_and(|d| d.session.can_redo()),
            "edit.paste" | "edit.paste_in_place" => doc.is_some_and(|d| !d.session.clipboard().is_empty()),
            "edit.delete"
            | "edit.cut"
            | "edit.copy"
            | "edit.duplicate"
            | "markup.lock"
            | "markup.unlock"
            | "markup.group"
            | "markup.ungroup"
            | "markup.set_default"
            | "markup.add_to_toolchest"
            | "markup.edit_text"
            | "markup.autosize" => selected,
            _ if id.starts_with("arrange.") || editing::needs_selection(id) => selected,
            "edit.deselect" => doc.is_some(),
            "window.next_document" | "window.prev_document" => self.docs.len() > 1,
            "tool.select" | "tool.pan" => true,
            _ if id.starts_with("panel.") || id.starts_with("snap.") => true,
            _ if features::handles(id) => features::enabled(self, id),
            _ if more::handles(id) => more::enabled(self, id),
            _ => doc.is_some(),
        }
    }

    /// Checkmark state for toggles (`None` = not a toggle).
    pub fn checked(&self, id: &str) -> Option<bool> {
        if let Some(c) = shell::checked(self, id) {
            return Some(c);
        }
        if let Some(c) = more::checked(self, id) {
            return Some(c);
        }
        if let Some(c) = features::more6::checked(self, id) {
            return Some(c);
        }
        if let Some(t) = id.strip_prefix("tool.") {
            return Some(self.tool == t);
        }
        if let Some(p) = id.strip_prefix("panel.") {
            return Some(self.open_panels.contains(&p));
        }
        match id {
            "view.single_page" => chrome::mode_checked(self, PageMode::Single),
            "view.continuous" => chrome::mode_checked(self, PageMode::Continuous),
            "view.fit_page" => chrome::fit_checked(self, Fit::Page),
            "view.fit_width" => chrome::fit_checked(self, Fit::Width),
            "view.hide_markups" => Some(self.hide_markups),
            "snap.grid" => Some(self.snaps.grid),
            "view.show_grid" => Some(self.show_grid),
            "snap.content" => Some(self.snaps.content),
            "snap.markup" => Some(self.snaps.markup),
            "tools.keep_tool" => Some(self.tool_locked),
            "view.highlight_viewports" => Some(self.edit.highlight_viewports),
            _ => None,
        }
    }

    /// The scale at the pointer (a viewport's, else the page's), for the status bar.
    pub fn scale_readout(&self) -> String {
        let Some(d) = self.doc() else { return String::new() };
        let page = d.view.pointer.map_or(d.view.current, |(p, _)| p);
        let Some(info) = d.session.doc().pages.get(page) else {
            return String::new();
        };
        let scale = match d.view.pointer {
            Some((_, at)) => info.scale_at(at),
            None => info
                .scale
                .as_ref()
                .filter(|s| s.valid())
                .or(info.viewports.first().map(|v| &v.scale)),
        };
        match scale {
            Some(s) if !s.ratio.is_empty() => format!("Scale {}", s.ratio),
            Some(_) => "Scale set".into(),
            None => "Scale Not Set".into(),
        }
    }

    pub fn set_zoom(&mut self, zoom: f32, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some(d) = self.doc_mut()
            && let Some(r) = d.render.as_ref()
        {
            d.view.zoom_to(zoom, None, r.pages(), now);
        }
    }

    fn zoom_step(&mut self, factor: f32, ctx: &egui::Context) {
        if let Some(z) = self.doc().map(|d| d.view.zoom) {
            self.set_zoom(z * factor, ctx);
        }
    }

    fn page_step(&mut self, to: impl Fn(usize, usize) -> usize) {
        if let Some(d) = self.doc_mut() {
            let count = d.render.as_ref().map_or(0, RenderDoc::page_count);
            if count > 0 {
                let p = to(d.view.current, count);
                d.view.go_to_page(p, count);
            }
        }
    }

    /// The look a new markup from the active tool takes (Tool Chest item, else the tool's
    /// Set as Default), and whether it is a Drawing-mode item.
    pub fn template(&self) -> (Option<&Markup>, bool) {
        if let Some((set, item)) = &self.active_item
            && let Some(it) = self.toolchest.item(set, item)
            && it.tool == self.tool
        {
            return (Some(&it.markup), it.mode == chest::Mode::Drawing);
        }
        (self.toolchest.defaults.get(self.tool), false)
    }

    /// The canvas inputs for this frame.
    pub fn canvas_cx(&self) -> CanvasCx<'_> {
        let tool = tools::find(self.tool).unwrap_or(&tools::select::TOOL);
        let (template, drawing_mode) = self.template();
        CanvasCx {
            tool,
            wheel_zooms: self.wheel_zooms,
            hide_markups: self.hide_markups,
            want_thumbs: self.thumbs_wanted_last,
            snaps: self.snaps,
            show_grid: self.show_grid,
            template,
            drawing_mode,
            stamp: self.stamp,
            author: &self.author,
            edit: &self.edit,
        }
    }

    /// Switch tools (drops a half-drawn markup, keeps typed text).
    pub fn set_tool(&mut self, id: &'static str) {
        if self.tool != id {
            let mut out = CanvasOut::default();
            if let Some(d) = self.doc_mut() {
                interact::commit_editor(d, &mut out);
                d.view.draft = None;
            }
            self.apply_canvas_out(out);
        }
        self.tool = id;
        // Measure (M) comes back to the measurement used last, however it was picked.
        if matches!(
            id,
            "length" | "polylength" | "area" | "perimeter" | "count" | "volume" | "angle" | "diameter" | "radius"
        ) {
            self.edit.more.measure_tool = id;
        }
        if self
            .active_item
            .as_ref()
            .and_then(|(s, i)| self.toolchest.item(s, i))
            .is_none_or(|it| it.tool != id)
        {
            self.active_item = None;
        }
    }

    /// Use a Tool Chest item: its tool, with its look (or its copy, in Drawing mode).
    pub fn use_item(&mut self, set: &str, item: &str) {
        let Some(tool) = self
            .toolchest
            .item(set, item)
            .and_then(|it| tools::find(&it.tool))
            .map(|t| t.id)
        else {
            return;
        };
        self.set_tool(tool);
        self.active_item = Some((set.to_string(), item.to_string()));
    }

    /// Handle what the canvas reported.
    pub fn apply_canvas_out(&mut self, mut out: CanvasOut) {
        shell::extra::canvas_out(self, &mut out);
        if let Some(s) = out.status {
            self.status = s;
        }
        if let Some((tool, m)) = out.created {
            self.toolchest.add_recent(tool, &m);
            more::created(self, &m);
            let keeps = tool == "count" || self.tool_locked;
            if !keeps && self.tool == tool {
                self.tool = "select";
            }
        }
        if out.done && !self.tool_locked {
            self.tool = "select";
        }
        if let Some((page, a, b)) = out.calibrate
            && let Some(uid) = self.doc().map(|d| d.uid)
        {
            self.calibrate = Some(Calibrate {
                doc: uid,
                page,
                a,
                b,
                length: String::new(),
                unit: markupcraft_measure::units::LengthUnit::Foot,
                pages: String::new(),
                apply_to_markups: true,
            });
        }
        for c in out.commands {
            self.queue(&c);
        }
        for a in out.actions {
            match a {
                CanvasAction::AddToToolChest(m) => {
                    self.toolchest.add_markup(chest::MY_TOOLS, &m);
                    self.status = format!("Added {} to My Tools", m.subject);
                    self.show_panel("toolchest");
                }
                CanvasAction::SetDefault(m) => {
                    self.status = match self.toolchest.set_default(&m) {
                        Some(t) => format!("New {t} markups take this look"),
                        None => "This markup has no tool to set a default for".into(),
                    };
                }
                CanvasAction::ShowPanel(p) => self.show_panel(p),
                CanvasAction::NewViewport(page, r) => viewports::open_dialog(self, page, r),
            }
        }
    }

    /// Run one command (see `commands::COMMANDS`, plus `tool.<id>` and `panel.<id>`).
    pub fn run(&mut self, id: &str, ctx: &egui::Context) {
        if !self.enabled(id) || shell::run(self, id, ctx) || shell::extra::page_edit_blocked(self, id) {
            return;
        }
        if let Some(t) = id.strip_prefix("tool.").and_then(tools::find) {
            self.set_tool(t.id);
            return;
        }
        if let Some(p) = id.strip_prefix("panel.").and_then(panels::find) {
            self.panel_toggles.push(p.id);
            return;
        }
        let threads = self.threads;
        match id {
            "file.open" => self.dialogs.open(Purpose::Open, dialogs::PDF_OR_IMAGE, true),
            "file.close" => self.close_doc(self.active),
            "file.save" | "file.save_as" => {
                if let Some(uid) = self.doc().map(|d| d.uid) {
                    self.save_doc(uid, id == "file.save_as", false);
                }
            }
            "file.exit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "edit.undo" | "edit.redo" => {
                if let Some(d) = self.doc_mut() {
                    interact::commit_editor(d, &mut CanvasOut::default());
                    let r = if id == "edit.undo" {
                        d.session.undo()
                    } else {
                        d.session.redo()
                    };
                    d.sync_pages(threads);
                    self.status = match r {
                        Ok(label) => format!("{} {label}", if id == "edit.undo" { "Undid" } else { "Redid" }),
                        Err(e) => e.to_string(),
                    };
                }
            }
            "edit.deselect" => {
                // Esc: end the text editor or the drawing (Count keeps what it counted), else
                // clear the selection; then back to Select.
                let tool = tools::find(self.tool).unwrap_or(&tools::select::TOOL);
                let (template, drawing_mode) = {
                    let (t, d) = self.template();
                    (t.cloned(), d)
                };
                let (author, stamp, snaps) = (self.author.clone(), self.stamp, self.snaps);
                let mut out = CanvasOut::default();
                if let Some(d) = self.docs.get_mut(self.active) {
                    let cx = CanvasCx {
                        tool,
                        wheel_zooms: true,
                        hide_markups: false,
                        want_thumbs: false,
                        snaps,
                        show_grid: false,
                        template: template.as_ref(),
                        drawing_mode,
                        stamp,
                        author: &author,
                        edit: &self.edit,
                    };
                    if !interact::escape(d, &cx, &mut out) {
                        d.session.clear_selection();
                    }
                }
                self.apply_canvas_out(out);
                editing::escape(self);
                self.tool = "select";
                self.active_item = None;
            }
            "edit.select_all" => {
                if let Some(d) = self.doc_mut() {
                    d.session.select_all(None);
                }
            }
            "edit.paste" => {
                if let Some(d) = self.doc_mut() {
                    let (page, at) = match d.view.pointer {
                        Some((p, at)) => (p, Some(at)),
                        None => (d.view.current, None),
                    };
                    let r = d.session.paste(Some(page), at);
                    self.status = actions::report(r, |v| format!("Pasted {}", actions::plural(v.len(), "markup")));
                }
            }
            "markup.edit_text" => {
                if let Some(d) = self.doc_mut()
                    && let Some(sel) = d.selection().first().cloned()
                    && !interact::edit_existing(d, &sel)
                {
                    self.status = "Only text boxes, callouts, typewriter text and notes have text to edit".into();
                }
            }
            "markup.autosize" => {
                if let Some(d) = self.doc_mut() {
                    let ids = d.selection().to_vec();
                    d.session.set_merge_key(Some("autosize"));
                    for id in &ids {
                        if let Some(m) = d.session.doc().find(id).filter(|m| m.kind.is_text()).cloned() {
                            let mut s = m.clone();
                            markupcraft_revu::kinds::text::fit_text_box(&mut s);
                            if s.pts != m.pts {
                                let _ = d.session.set_points(id, s.pts);
                            }
                        }
                    }
                    d.session.set_merge_key(None);
                    d.session.seal();
                }
            }
            "markup.set_default" | "markup.add_to_toolchest" => {
                let m = self
                    .doc()
                    .and_then(|d| d.selection().first().and_then(|id| d.session.doc().find(id)).cloned());
                if let Some(m) = m {
                    let a = if id == "markup.set_default" {
                        CanvasAction::SetDefault(m)
                    } else {
                        CanvasAction::AddToToolChest(m)
                    };
                    self.apply_canvas_out(CanvasOut {
                        actions: vec![a],
                        ..Default::default()
                    });
                }
            }
            "measure.calibrate" => self.set_tool("calibrate"),
            "tools.keep_tool" => self.tool_locked = !self.tool_locked,
            "view.zoom_in" => self.zoom_step(1.25, ctx),
            "view.zoom_out" => self.zoom_step(0.8, ctx),
            "view.actual_size" => self.set_zoom(1.0, ctx),
            "view.fit_page" | "view.fit_width" => {
                if let Some(d) = self.doc_mut() {
                    d.view
                        .set_fit(if id == "view.fit_page" { Fit::Page } else { Fit::Width });
                }
            }
            "view.single_page" | "view.continuous" => {
                if let Some(d) = self.doc_mut() {
                    d.view.set_mode(if id == "view.single_page" {
                        PageMode::Single
                    } else {
                        PageMode::Continuous
                    });
                }
            }
            "view.first_page" => self.page_step(|_, _| 0),
            "view.prev_page" => self.page_step(|c, _| c.saturating_sub(1)),
            "view.next_page" => self.page_step(|c, n| (c + 1).min(n - 1)),
            "view.last_page" => self.page_step(|_, n| n - 1),
            "view.hide_markups" => self.hide_markups = !self.hide_markups,
            "document.properties" => self.show_properties = true,
            "document.rotate_cw" | "document.rotate_ccw" | "document.delete_page" | "document.insert_blank" => {
                // Preferences > "Rotate all Pages by Default": the quick buttons turn every page.
                let all = self.shell.ui.extra.rotate_all_pages;
                if let Some(d) = self.doc_mut() {
                    let page = d.view.current;
                    let turn: Vec<usize> = if all {
                        (0..d.session.page_count()).collect()
                    } else {
                        vec![page]
                    };
                    let r = match id {
                        "document.rotate_cw" => d.session.rotate_pages(&turn, 90),
                        "document.rotate_ccw" => d.session.rotate_pages(&turn, -90),
                        "document.delete_page" => d.session.delete_pages(&[page]),
                        _ => d.session.insert_blank_pages(page + 1, 1, None),
                    };
                    d.sync_pages(threads);
                    self.status = actions::report(r, |_| {
                        match id {
                            "document.rotate_cw" | "document.rotate_ccw" => "Page rotated",
                            "document.delete_page" => "Page deleted",
                            _ => "Blank page inserted",
                        }
                        .to_string()
                    });
                }
            }
            "document.insert_pages" => {
                if let Some(at) = self.doc().map(|d| d.view.current + 1) {
                    self.dialogs.open(Purpose::InsertPages { at }, dialogs::PDF, false);
                }
            }
            "document.extract_page" => {
                if let Some((page, name)) = self.doc().map(|d| {
                    (
                        d.view.current,
                        format!("{} page {}.pdf", d.name.trim_end_matches(".pdf"), d.view.current + 1),
                    )
                }) {
                    self.dialogs
                        .save(Purpose::ExtractPages { pages: vec![page] }, dialogs::PDF, &name);
                }
            }
            "window.reset_layout" => self.reset_layout = true,
            "window.next_document" => self.active = (self.active + 1) % self.docs.len().max(1),
            "window.prev_document" => self.active = (self.active + self.docs.len().max(1) - 1) % self.docs.len().max(1),
            "help.shortcuts" => self.show_shortcuts = true,
            "help.about" => self.show_about = true,
            "snap.grid" => self.snaps.grid = !self.snaps.grid,
            "view.show_grid" => self.show_grid = !self.show_grid,
            "snap.content" => self.snaps.content = !self.snaps.content,
            "snap.markup" => self.snaps.markup = !self.snaps.markup,
            _ if features::handles(id) => features::run(self, id, ctx),
            _ if editing::handles(id) => editing::run(self, id),
            "edit.paste_in_place" => {
                // Same position, on the page in view (Revu: e.g. onto another sheet).
                if let Some(d) = self.doc_mut() {
                    let page = d.view.current;
                    let r = d.session.paste(Some(page), None);
                    self.status =
                        actions::report(r, |v| format!("Pasted {} in place", actions::plural(v.len(), "markup")));
                }
            }
            "markup.lock" => {
                // Lock is a toggle (Revu's menu item is checked on a locked markup): when every
                // selected markup is locked already, Ctrl+Shift+L unlocks them.
                if let Some(d) = self.doc_mut() {
                    let all_locked = !d.selection().is_empty()
                        && d.selection()
                            .iter()
                            .all(|id| d.session.doc().find(id).is_some_and(|m| m.locked()));
                    let cmd = if all_locked { "markup.unlock" } else { "markup.lock" };
                    let r = markupcraft_engine::commands::run(&mut d.session, cmd);
                    self.status = actions::report(r, |s| s);
                }
            }
            "edit.copy" | "edit.cut" => {
                if let Some(d) = self.doc_mut() {
                    let r = markupcraft_engine::commands::run(&mut d.session, id);
                    d.sync_pages(threads);
                    // The platform sends Ctrl+V on only while the system clipboard holds text,
                    // so the copied markups go there as text too.
                    let text: Vec<&str> = d
                        .session
                        .clipboard()
                        .iter()
                        .map(|m| {
                            if m.contents.is_empty() {
                                m.subject.as_str()
                            } else {
                                m.contents.as_str()
                            }
                        })
                        .collect();
                    let text = text.join("\n");
                    ctx.copy_text(if text.trim().is_empty() {
                        "markups".to_string()
                    } else {
                        text
                    });
                    // Preferences > Tablet: pen strokes also go out as a picture.
                    let picture = tablet_prefs::opts()
                        .ink_copy_picture
                        .then(|| tablet_prefs::ink_picture(d.session.clipboard(), 1024))
                        .flatten();
                    self.status = actions::report(r, |s| s);
                    if let Some(img) = picture {
                        self.status = format!(
                            "{}; pen strokes copied as a {} x {} picture",
                            self.status, img.size[0], img.size[1]
                        );
                        ctx.copy_image(img);
                    }
                }
            }
            _ if markupcraft_engine::commands::find(id).is_some() => {
                if let Some(d) = self.doc_mut() {
                    let r = markupcraft_engine::commands::run(&mut d.session, id);
                    d.sync_pages(threads);
                    self.status = actions::report(r, |s| s);
                }
            }
            _ => log::debug!("command {id} is not wired yet"),
        }
    }

    /// Settings by name: the desktop app's command-line flags and the screenshot example.
    pub fn set_option(&mut self, key: &str, value: &str, ctx: &egui::Context) -> Result<(), String> {
        match key {
            "page" => {
                let n: usize = value.parse().map_err(|_| format!("page number: {value}"))?;
                self.page_step(|_, c| n.saturating_sub(1).min(c - 1));
            }
            "zoom" => match value {
                "fit-page" | "page" => self.run("view.fit_page", ctx),
                "fit-width" | "width" => self.run("view.fit_width", ctx),
                v => {
                    let pct: f32 = v.trim_end_matches('%').parse().map_err(|_| format!("zoom: {v}"))?;
                    self.set_zoom(pct / 100.0, ctx);
                }
            },
            "mode" => self.run(
                if value == "single" {
                    "view.single_page"
                } else {
                    "view.continuous"
                },
                ctx,
            ),
            "tool" => self.set_tool(tools::find(value).ok_or(format!("no tool {value}"))?.id),
            "select" => {
                let d = self.doc_mut().ok_or("no document")?;
                let ids: Vec<String> = d.session.doc().markups.iter().map(|m| m.id.clone()).collect();
                let sel = match value {
                    "all" => ids,
                    v => {
                        let i: usize = v.parse().map_err(|_| format!("select: {v}"))?;
                        ids.get(i).cloned().into_iter().collect()
                    }
                };
                actions::select(&mut d.session, sel);
            }
            "show" => {
                // Select markup number `value` (0-based) and go to its page.
                let d = self.doc_mut().ok_or("no document")?;
                let i: usize = value.parse().map_err(|_| format!("show: {value}"))?;
                let m = d
                    .session
                    .doc()
                    .markups
                    .get(i)
                    .cloned()
                    .ok_or(format!("no markup {i}"))?;
                actions::select(&mut d.session, vec![m.id.clone()]);
                let count = d.session.page_count();
                d.view.go_to_page(m.page, count);
            }
            "hide-markups" => self.hide_markups = value != "false",
            "panel" => self
                .panel_shows
                .push(panels::find(value).ok_or(format!("no panel {value}"))?.id),
            "wheel" => self.wheel_zooms = value == "zoom",
            "snap" => {
                for s in value.split(',') {
                    match s.trim() {
                        "grid" => self.snaps.grid = true,
                        "content" => self.snaps.content = true,
                        "markup" => self.snaps.markup = true,
                        "" => {}
                        o => return Err(format!("snap: {o}")),
                    }
                }
            }
            "group-by" => self.list.view.group_by = value.split(',').map(str::to_string).collect(),
            "columns" => self.list.view.visible = value.split(',').map(str::to_string).collect(),
            _ => return Err(format!("unknown option {key}")),
        }
        Ok(())
    }

    /// Visible pages, tiles or thumbnails still rendering (headless screenshots wait for 0).
    pub fn render_pending(&self) -> bool {
        self.doc().is_some_and(|d| d.render.is_some() && d.view.missing > 0)
    }
}

/// F1..F12: keys a text field never uses.
fn is_function_key(k: egui::Key) -> bool {
    use egui::Key::*;
    matches!(k, F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12)
}

/// The app: state plus the dock layout.
pub struct MarkupCraftApp {
    pub state: AppState,
    dock: DockState<Tab>,
    styled: bool,
    title: String,
    /// Options to apply once the first frame has a context.
    pending_options: Vec<(String, String)>,
}

impl Default for MarkupCraftApp {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkupCraftApp {
    /// An app with an in-memory Tool Chest (tests, screenshots).
    pub fn new() -> Self {
        // The log file of Preferences > Admin (written once a folder is set).
        shell::admin_prefs::install();
        Self {
            state: AppState::default(),
            dock: dock::default_layout(),
            styled: false,
            title: String::new(),
            pending_options: Vec::new(),
        }
    }

    /// The desktop app: the user's Tool Chest from the configuration folder.
    pub fn with_user_settings() -> Self {
        let mut app = Self::new();
        if let Some(dir) = chest::config_dir() {
            app.state.toolchest = ToolChest::load(&dir.join("toolchest.json"));
            app.state.keys = keyprefs::KeyPrefs::load(&dir.join("keyboard.json"));
            app.state.list.prefs = panels::list_views::ListPrefs::load(&dir.join("markups-list.json"));
            if let Some(e) = &app.state.toolchest.error {
                app.state.status = e.clone();
            }
        }
        shell::load_user(&mut app.state);
        app
    }

    pub fn open_path(&mut self, path: &Path) {
        self.state.open_path(path);
    }

    /// The panel layout (tests, the layout preference).
    pub fn dock(&self) -> &DockState<Tab> {
        &self.dock
    }

    pub fn open_bytes(&mut self, name: &str, path: Option<PathBuf>, bytes: Vec<u8>) -> Result<(), String> {
        self.state.open_bytes(name, path, bytes)
    }

    /// Apply an option (see [`AppState::set_option`]) on the next frame.
    pub fn set_option(&mut self, key: &str, value: &str) {
        self.pending_options.push((key.to_string(), value.to_string()));
    }

    pub fn render_pending(&self) -> bool {
        !self.pending_options.is_empty() || self.state.render_pending()
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.state.keys.capturing.is_some() {
            return;
        }
        // While a text field has focus only function keys reach commands (text editing has
        // no use for them): F3 steps through results from the search box, as in Revu.
        if ctx.egui_wants_keyboard_input() {
            for (k, id) in self.state.keys.bindings() {
                if is_function_key(k.key)
                    && features::more6::prefs::key_allowed(&self.state, &k, &id)
                    && ctx.input_mut(|i| i.consume_shortcut(&k.shortcut()))
                {
                    self.state.queue(&id);
                }
            }
            return;
        }
        // The platform layer (egui-winit) delivers Ctrl+C / Ctrl+X / Ctrl+V, with any other
        // modifier held, as Copy / Cut / Paste events and no key event: turn them back into
        // keys so Copy, Format Painter (Ctrl+Shift+C), Copy Page to Snapshot (Ctrl+Alt+C),
        // Extract Pages (Ctrl+Shift+X), Paste and Paste in Place (Ctrl+Shift+V) all fire.
        ctx.input_mut(|i| {
            let m = i.modifiers;
            if !m.command {
                return;
            }
            for e in &mut i.events {
                let key = match e {
                    egui::Event::Copy => egui::Key::C,
                    egui::Event::Cut => egui::Key::X,
                    egui::Event::Paste(_) => egui::Key::V,
                    _ => continue,
                };
                *e = egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: m,
                };
            }
        });
        for (k, id) in self.state.keys.bindings() {
            if !features::more6::prefs::key_allowed(&self.state, &k, &id) {
                continue;
            }
            // Arrow keys nudge only a selection; with none they stay free for the panels
            // (Thumbnails: Up / Down move between pages).
            // Delete likewise (Bookmarks: Delete removes the selected bookmark).
            if (id.starts_with("edit.nudge_") || id == "edit.delete")
                && self.state.doc().is_none_or(|d| d.selection().is_empty())
            {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&k.shortcut())) {
                self.state.queue(&id);
            }
        }
    }

    fn take_dropped(&mut self, ctx: &egui::Context) {
        if shell::files::drop_on_thumbnails(&mut self.state, ctx) {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        // The browser reads dropped files asynchronously: they open when they arrive.
        #[cfg(target_arch = "wasm32")]
        {
            browser::set_context(ctx);
            browser::read_dropped(dropped);
            for (name, r) in browser::take_arrived() {
                match r {
                    Ok(path) => self.state.open_path(&path),
                    Err(e) => self.state.status = format!("Could not open {name}: {e}"),
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        for f in dropped {
            let path = f.path().to_path_buf();
            if path.is_file() {
                self.state.open_path(&path);
                continue;
            }
            let name = path
                .file_name()
                .map_or_else(|| "dropped.pdf".to_string(), |n| n.to_string_lossy().into_owned());
            let r = f.bytes().and_then(|b| self.state.open_bytes(&name, None, b));
            if let Err(e) = r {
                self.state.status = format!("Could not open {name}: {e}");
            }
        }
    }
}

impl eframe::App for MarkupCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        i18n::set_language(&self.state.shell.prefs.language);
        markup_prefs::frame(&mut self.state);
        tablet_prefs::frame(&self.state);
        shell::admin_prefs::apply(&mut self.state);
        if !self.styled {
            egui_extras::install_image_loaders(&ctx);
            theme::apply(&ctx);
            self.styled = true;
        } else {
            // From the second frame: the canvas has a size, so zoom and page options apply
            // to a laid-out view.
            for (k, v) in std::mem::take(&mut self.pending_options) {
                if let Err(e) = self.state.set_option(&k, &v, &ctx) {
                    log::warn!("option {k}={v}: {e}");
                }
            }
        }
        // Closing the window waits for unsaved documents.
        if ctx.input(|i| i.viewport().close_requested()) && !self.state.may_exit() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.state.close_now {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        for d in &mut self.state.docs {
            if let Some(r) = &d.render {
                d.view.receive(&ctx, r);
            }
        }
        self.state.take_dialogs();
        shell::begin_frame(&mut self.state, &ctx);
        shell::panelbars::dedupe(&mut self.state, &self.dock);
        self.state.open_panels = shell::panelbars::open(&self.state, &self.dock);
        // The Thumbnails panel says each frame whether it is showing; the canvas requests
        // thumbnails while it is.
        self.state.thumbs_wanted_last = self.state.thumbs_wanted;
        self.state.thumbs_wanted = false;
        self.shortcuts(&ctx);
        self.take_dropped(&ctx);

        let title = self.state.doc().map_or_else(
            || "MarkupCraft".to_string(),
            |d| {
                let star = if d.session.is_dirty() { "*" } else { "" };
                format!("{}{star} - MarkupCraft", d.name)
            },
        );
        if title != self.title {
            // The browser has no window title: the tab's title is the page's.
            #[cfg(target_arch = "wasm32")]
            browser::set_title(&title);
            #[cfg(not(target_arch = "wasm32"))]
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        // The default arrangement (docs/UI_LAYOUT.md): menu bar, document bar, optional
        // toolbars; the bottom bar under everything; the panel bar on the left edge and the
        // tool strip on the right; the bottom panel, the left panel area, then the document.
        chrome::menu_bar(&mut self.state, ui);
        shell::docbar::bar(&mut self.state, ui);
        chrome::toolbar(&mut self.state, ui);
        shell::proptoolbar::bar(&mut self.state, ui);
        chrome::bottom_bar(&mut self.state, ui);
        shell::panelbars::bars(&mut self.state, ui);
        shell::toolstrip::strip(&mut self.state, ui);
        shell::panelbars::bottom_area(&mut self.state, ui, true);
        shell::toolbars_more::strips(&mut self.state, ui);
        shell::panelbars::left_area(&mut self.state, ui);
        shell::panelbars::bottom_area(&mut self.state, ui, false);
        shell::edges::strips(&mut self.state, ui);
        let chrome_fill = theme::Tokens::get(&ctx).chrome;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(chrome_fill))
            .show(ui, |ui| {
                // With nothing docked the document area fills the middle (no dock tab over it).
                if self.dock.iter_all_tabs().all(|(_, t)| *t == Tab::Document) {
                    chrome::document_area(&mut self.state, ui);
                    return;
                }
                let style = egui_dock::Style::from_egui(ui.style().as_ref());
                DockArea::new(&mut self.dock)
                    .style(style)
                    .show_leaf_collapse_buttons(true)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(ui, &mut dock::Viewer { app: &mut self.state });
            });
        chrome::windows(&mut self.state, &ctx);
        windows::show(&mut self.state, &ctx);
        shell::windows(&mut self.state, &ctx);
        features::frame(&mut self.state, &ctx);
        more::frame(&mut self.state, &ctx);
        viewports::dialog(&mut self.state, &ctx);
        sketch::bar(&mut self.state, &ctx);
        keyprefs::window(&mut self.state, &ctx);

        for id in std::mem::take(&mut self.state.queued) {
            self.state.run(&id, &ctx);
        }
        for p in std::mem::take(&mut self.state.panel_toggles) {
            shell::panelbars::toggle(&mut self.state, &mut self.dock, p);
        }
        for p in std::mem::take(&mut self.state.panel_shows) {
            shell::panelbars::show(&mut self.state, &mut self.dock, p);
        }
        if std::mem::take(&mut self.state.reset_layout) {
            self.dock = dock::default_layout();
            let d = shell::UiPrefs::default();
            self.state.shell.ui.left_panel = d.left_panel;
            self.state.shell.ui.bottom_panel = d.bottom_panel;
            self.state.shell.panels_hidden = false;
            self.state.shell.layout_dirty = true;
        }
        shell::dock_frame(&mut self.state, &mut self.dock);
        shell::end_frame(&mut self.state, &ctx);
        // A finished gesture (slider drag, typing) closes its undo step.
        let idle = !ctx.input(|i| i.pointer.any_down()) && ctx.memory(|m| m.focused().is_none());
        if idle && let Some(d) = self.state.doc_mut() {
            d.session.seal();
        }
        let snapping = self.state.doc().is_some_and(|d| d.snaps.building());
        if self.state.render_pending() || self.state.dialogs.busy() || snapping {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_sample() -> AppState {
        let mut s = AppState {
            threads: 0,
            ..Default::default()
        };
        s.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
            .unwrap();
        s
    }

    #[test]
    fn desktop_only_commands_are_real_commands() {
        for id in NEEDS_DESKTOP {
            assert!(commands::find(id).is_some(), "{id} is not in the command table");
        }
        // On the desktop they stay available.
        assert!(app_with_sample().enabled("file.print"));
    }

    #[test]
    fn opens_the_sample_and_hides_what_it_draws() {
        let s = app_with_sample();
        let d = s.doc().unwrap();
        assert_eq!(d.render.as_ref().unwrap().page_count(), 2);
        assert_eq!(d.session.doc().markups.len(), 6);
        // Every sample markup has a writer, so all are drawn live from the model and editable.
        assert_eq!(d.render.as_ref().unwrap().hidden_count(), 6);
        assert!(crate::actions::editable(
            d.session.doc().find("SAMPLETEXTAAAAAA").unwrap()
        ));
        assert_eq!(s.scale_readout(), "Scale 1/8 in = 1 ft");
    }

    #[test]
    fn delete_undo_through_commands() {
        let ctx = egui::Context::default();
        let mut s = app_with_sample();
        s.set_option("select", "0", &ctx).unwrap();
        s.run("edit.delete", &ctx);
        assert_eq!(s.doc().unwrap().session.doc().markups.len(), 5);
        assert!(s.doc().unwrap().selection().is_empty());
        s.run("edit.undo", &ctx);
        assert_eq!(s.doc().unwrap().session.doc().markups.len(), 6);
        assert!(s.status.starts_with("Undid"), "{}", s.status);
        s.run("tool.rectangle", &ctx);
        assert_eq!(s.tool, "rectangle");
        assert!(s.set_option("tool", "nope", &ctx).is_err());
    }

    #[test]
    fn engine_commands_and_page_operations() {
        let ctx = egui::Context::default();
        let mut s = app_with_sample();
        s.set_option("select", "0", &ctx).unwrap();
        s.run("edit.copy", &ctx);
        s.run("edit.paste", &ctx);
        assert_eq!(s.doc().unwrap().session.doc().markups.len(), 7);
        s.run("arrange.send_to_back", &ctx);
        assert!(!s.status.is_empty());
        s.run("edit.select_all", &ctx);
        assert_eq!(s.doc().unwrap().selection().len(), 7);
        s.run("edit.cut", &ctx);
        assert_eq!(s.doc().unwrap().session.doc().markups.len(), 0);
        s.run("edit.paste_in_place", &ctx);
        assert_eq!(s.doc().unwrap().session.doc().markups.len(), 7);
        // Rotating a page re-renders from the rewritten document; undo puts it back.
        s.run("document.rotate_cw", &ctx);
        assert_eq!(s.doc().unwrap().render.as_ref().unwrap().pages()[0].rotation, 90);
        s.run("document.insert_blank", &ctx);
        assert_eq!(s.doc().unwrap().render.as_ref().unwrap().page_count(), 3);
        s.run("edit.undo", &ctx);
        s.run("edit.undo", &ctx);
        let d = s.doc().unwrap();
        assert_eq!(d.render.as_ref().unwrap().page_count(), 2);
        assert_eq!(d.render.as_ref().unwrap().pages()[0].rotation, 0);
    }

    #[test]
    fn closing_an_edited_document_asks_first() {
        let ctx = egui::Context::default();
        let mut s = app_with_sample();
        s.set_option("select", "0", &ctx).unwrap();
        s.run("edit.delete", &ctx);
        s.close_doc(0);
        assert_eq!(s.docs.len(), 1);
        assert_eq!(s.prompts.len(), 1);
        assert!(!s.may_exit());
        let uid = s.docs[0].uid;
        s.prompts.clear();
        s.force_close(uid);
        assert!(s.docs.is_empty());
        assert!(s.may_exit());
    }

    #[test]
    fn saves_and_reopens_an_edit() {
        let dir = std::env::temp_dir().join(format!("markupcraft-ui-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.pdf");
        std::fs::write(&path, markupcraft_render::synthetic::sample_pdf()).unwrap();
        let ctx = egui::Context::default();
        let mut s = AppState {
            threads: 0,
            ..Default::default()
        };
        s.open_path(&path);
        {
            let d = s.doc_mut().unwrap();
            d.session
                .move_markups(&["SAMPLESQUAREAAAA".to_string()], -100.0, -50.0)
                .unwrap();
        }
        s.run("file.save", &ctx);
        assert!(s.status.starts_with("Saved"), "{}", s.status);
        assert!(!s.doc().unwrap().session.is_dirty());
        let mut again = AppState {
            threads: 0,
            ..Default::default()
        };
        again.open_path(&path);
        let m = again
            .doc()
            .unwrap()
            .session
            .doc()
            .find("SAMPLESQUAREAAAA")
            .unwrap()
            .clone();
        // Moved 100 left: centred at x = 850 (the writer pads /Rect by the line width).
        assert!(((m.rect.x0 + m.rect.x1) / 2.0 - 850.0).abs() < 0.5, "{:?}", m.rect);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn page_ranges() {
        assert_eq!(pages_from_text("", 2, 5), Some(vec![2]));
        assert_eq!(pages_from_text("all", 0, 3), Some(vec![0, 1, 2]));
        assert_eq!(pages_from_text("1-2", 0, 3), Some(vec![0, 1]));
        assert_eq!(pages_from_text("9", 0, 3), None);
    }
}
