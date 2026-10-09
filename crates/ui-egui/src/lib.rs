//! The Revu-style interface of MarkupCraft (egui + eframe on wgpu).
//!
//! Layout: menu bar and toolbar on top, a docking area (egui_dock) with the document canvas in
//! the middle and the panels around it, the status bar at the bottom. Three tables drive it:
//! [`commands::COMMANDS`] (menus, toolbar, shortcuts), [`tools::TOOLS`] (what the mouse does)
//! and [`panels::PANELS`] (dockable panels). Every document change goes through
//! [`actions::Session`], the thin layer the engine crate will replace.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod actions;
pub mod canvas;
pub mod chrome;
pub mod commands;
pub mod dock;
pub mod icon_data;
pub mod icons;
pub mod painter;
pub mod panels;
pub mod theme;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_dock::{DockArea, DockState};
use markupcraft_render::{RenderDoc, RenderOptions};

use crate::actions::Session;
use crate::canvas::{DocView, Fit, PageMode};
use crate::dock::Tab;

/// One open document.
pub struct DocTab {
    /// File name shown on the tab.
    pub name: String,
    /// Where Save writes; `None` = Save As first.
    pub path: Option<PathBuf>,
    pub session: Session,
    pub render: Option<RenderDoc>,
    pub view: DocView,
    /// Selected markups by `/NM`.
    pub selection: Vec<String>,
}

/// Status bar toggles.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snaps {
    pub grid: bool,
    pub content: bool,
    pub markup: bool,
}

/// Everything except the dock layout (so panels can borrow it while the dock draws them).
pub struct AppState {
    pub docs: Vec<DocTab>,
    pub active: usize,
    /// Active tool id (`tools::TOOLS`).
    pub tool: &'static str,
    pub snaps: Snaps,
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
    /// Markups List sort: column, ascending.
    pub list_sort: (usize, bool),
    /// The Thumbnails panel was drawn this frame / last frame.
    pub thumbs_wanted: bool,
    pub thumbs_wanted_last: bool,
    /// Commands to run at the end of the frame.
    queued: Vec<String>,
    /// Dock changes the state cannot make itself.
    panel_toggles: Vec<&'static str>,
    reset_layout: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            docs: Vec::new(),
            active: 0,
            tool: "select",
            snaps: Snaps::default(),
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
            list_sort: (1, true),
            thumbs_wanted: false,
            thumbs_wanted_last: true,
            queued: Vec::new(),
            panel_toggles: Vec::new(),
            reset_layout: false,
        }
    }
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

    /// Run `id` at the end of this frame (menus, toolbar, shortcuts and panels all queue).
    pub fn queue(&mut self, id: &str) {
        self.queued.push(id.to_string());
    }

    /// Open a PDF from bytes. `path` is where Save writes.
    pub fn open_bytes(&mut self, name: &str, path: Option<PathBuf>, bytes: Vec<u8>) -> Result<(), String> {
        let bytes = Arc::new(bytes);
        let session = Session::open_bytes(bytes.clone(), path.as_deref())?;
        let render = self.make_renderer(&session, bytes)?;
        self.docs.push(DocTab {
            name: name.to_string(),
            path,
            session,
            render: Some(render),
            view: DocView::default(),
            selection: Vec::new(),
        });
        self.active = self.docs.len() - 1;
        self.status = format!("Opened {name}");
        Ok(())
    }

    fn make_renderer(&self, session: &Session, bytes: Arc<Vec<u8>>) -> Result<RenderDoc, String> {
        let opts = RenderOptions {
            hide: session.drawn_objects(),
            threads: self.threads,
            hide_all_markups: false,
        };
        RenderDoc::open(bytes, &opts).map_err(|e| e.to_string())
    }

    /// Open a file from disk (argv, File > Open, a drop).
    pub fn open_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(path)) {
            self.active = i;
            return;
        }
        let r = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| self.open_bytes(&name, Some(path.to_path_buf()), b));
        if let Err(e) = r {
            self.status = format!("Could not open {name}: {e}");
            log::warn!("{}", self.status);
        }
    }

    pub fn close_doc(&mut self, i: usize) {
        if i < self.docs.len() {
            self.docs.remove(i);
        }
        if self.active >= self.docs.len() {
            self.active = self.docs.len().saturating_sub(1);
        }
    }

    /// Save the active document (`as_new` or no path: ask where).
    fn save(&mut self, as_new: bool) {
        let Some(doc) = self.docs.get(self.active) else { return };
        let target = match (&doc.path, as_new) {
            (Some(p), false) => Some(p.clone()),
            _ => pick_save_path(&doc.name),
        };
        let Some(target) = target else { return };
        let threads = self.threads;
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        match doc.session.save(Some(&target)) {
            Ok(p) => {
                doc.path = Some(p.clone());
                doc.name = p
                    .file_name()
                    .map_or_else(|| doc.name.clone(), |n| n.to_string_lossy().into_owned());
                // The file changed on disk: render from the saved bytes.
                if let Some(bytes) = doc.session.bytes() {
                    let opts = RenderOptions {
                        hide: doc.session.drawn_objects(),
                        threads,
                        hide_all_markups: false,
                    };
                    doc.render = RenderDoc::open(bytes, &opts).ok();
                    doc.view.invalidate();
                }
                self.status = format!("Saved {}", p.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Whether a command can run now.
    pub fn enabled(&self, id: &str) -> bool {
        let doc = self.doc();
        match id {
            "file.open" | "file.exit" | "help.shortcuts" | "help.about" | "window.reset_layout" => true,
            "edit.undo" => doc.is_some_and(|d| d.session.can_undo()),
            "edit.redo" => doc.is_some_and(|d| d.session.can_redo()),
            "edit.delete" | "markup.lock" | "edit.deselect" => doc.is_some_and(|d| !d.selection.is_empty()),
            "window.next_document" | "window.prev_document" => self.docs.len() > 1,
            "tool.select" | "tool.pan" => true,
            _ if id.starts_with("panel.") || id.starts_with("snap.") => true,
            _ => doc.is_some(),
        }
    }

    /// Checkmark state for toggles (`None` = not a toggle).
    pub fn checked(&self, id: &str) -> Option<bool> {
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
            "snap.content" => Some(self.snaps.content),
            "snap.markup" => Some(self.snaps.markup),
            _ => None,
        }
    }

    /// The scale at the pointer (a viewport's, else the page's), for the status bar.
    pub fn scale_readout(&self) -> String {
        let Some(d) = self.doc() else { return String::new() };
        let page = d.view.pointer.map_or(d.view.current, |(p, _)| p);
        let Some(info) = d.session.doc.pages.get(page) else {
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
            None => "No scale".into(),
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

    /// Run one command (see `commands::COMMANDS`, plus `tool.<id>` and `panel.<id>`).
    pub fn run(&mut self, id: &str, ctx: &egui::Context) {
        if !self.enabled(id) {
            return;
        }
        if let Some(t) = id.strip_prefix("tool.").and_then(tools::find) {
            self.tool = t.id;
            return;
        }
        if let Some(p) = id.strip_prefix("panel.").and_then(panels::find) {
            self.panel_toggles.push(p.id);
            return;
        }
        match id {
            "file.open" => {
                for p in pick_open_paths() {
                    self.open_path(&p);
                }
            }
            "file.close" => self.close_doc(self.active),
            "file.save" => self.save(false),
            "file.save_as" => self.save(true),
            "file.exit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "edit.undo" | "edit.redo" => {
                if let Some(d) = self.doc_mut() {
                    let done = if id == "edit.undo" {
                        d.session.undo()
                    } else {
                        d.session.redo()
                    };
                    let doc = &d.session.doc;
                    d.selection.retain(|s| doc.find(s).is_some());
                    if done {
                        self.status = if id == "edit.undo" {
                            "Undone".into()
                        } else {
                            "Redone".into()
                        };
                    }
                }
            }
            "edit.delete" => {
                if let Some(d) = self.doc_mut() {
                    let n = d.session.delete(&d.selection);
                    let doc = &d.session.doc;
                    d.selection.retain(|s| doc.find(s).is_some());
                    self.status = format!("Deleted {n} markup{}", if n == 1 { "" } else { "s" });
                }
            }
            "edit.select_all" => {
                if let Some(d) = self.doc_mut() {
                    d.selection = d.session.doc.markups.iter().map(|m| m.id.clone()).collect();
                }
            }
            "edit.deselect" => {
                if let Some(d) = self.doc_mut() {
                    d.selection.clear();
                }
                self.tool = "select";
            }
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
            "markup.lock" => {
                if let Some(d) = self.doc_mut() {
                    let all = d
                        .selection
                        .iter()
                        .filter_map(|s| d.session.doc.find(s))
                        .all(|m| m.locked());
                    d.session.edit(&d.selection, "lock-cmd", |m| m.set_locked(!all));
                    d.session.seal();
                }
            }
            "document.properties" => self.show_properties = true,
            "window.reset_layout" => self.reset_layout = true,
            "window.next_document" => self.active = (self.active + 1) % self.docs.len().max(1),
            "window.prev_document" => self.active = (self.active + self.docs.len().max(1) - 1) % self.docs.len().max(1),
            "help.shortcuts" => self.show_shortcuts = true,
            "help.about" => self.show_about = true,
            "snap.grid" => self.snaps.grid = !self.snaps.grid,
            "snap.content" => self.snaps.content = !self.snaps.content,
            "snap.markup" => self.snaps.markup = !self.snaps.markup,
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
            "tool" => self.tool = tools::find(value).ok_or(format!("no tool {value}"))?.id,
            "select" => {
                let d = self.doc_mut().ok_or("no document")?;
                let ids: Vec<String> = d.session.doc.markups.iter().map(|m| m.id.clone()).collect();
                d.selection = match value {
                    "all" => ids,
                    v => {
                        let i: usize = v.parse().map_err(|_| format!("select: {v}"))?;
                        ids.get(i).cloned().into_iter().collect()
                    }
                };
            }
            "hide-markups" => self.hide_markups = value != "false",
            "panel" => self
                .panel_toggles
                .push(panels::find(value).ok_or(format!("no panel {value}"))?.id),
            "wheel" => self.wheel_zooms = value == "zoom",
            _ => return Err(format!("unknown option {key}")),
        }
        Ok(())
    }

    /// Visible pages, tiles or thumbnails still rendering (headless screenshots wait for 0).
    pub fn render_pending(&self) -> bool {
        self.doc().is_some_and(|d| d.render.is_some() && d.view.missing > 0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_open_paths() -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_files()
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn pick_open_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_save_path(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(name)
        .save_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_save_path(_name: &str) -> Option<PathBuf> {
    None
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
    pub fn new() -> Self {
        Self {
            state: AppState::default(),
            dock: dock::default_layout(),
            styled: false,
            title: String::new(),
            pending_options: Vec::new(),
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        self.state.open_path(path);
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
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        for (k, id) in commands::bindings() {
            if ctx.input_mut(|i| i.consume_shortcut(&k.shortcut())) {
                self.state.queue(&id);
            }
        }
    }

    fn take_dropped(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
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
        for d in &mut self.state.docs {
            if let Some(r) = &d.render {
                d.view.receive(&ctx, r);
            }
        }
        self.state.open_panels = dock::open_panels(&self.dock);
        // The Thumbnails panel says each frame whether it is showing; the canvas requests
        // thumbnails while it is.
        self.state.thumbs_wanted_last = self.state.thumbs_wanted;
        self.state.thumbs_wanted = false;
        self.shortcuts(&ctx);
        self.take_dropped(&ctx);

        let title = self
            .state
            .doc()
            .map_or_else(|| "MarkupCraft".to_string(), |d| format!("{} - MarkupCraft", d.name));
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        chrome::menu_bar(&mut self.state, ui);
        chrome::toolbar(&mut self.state, ui);
        chrome::status_bar(&mut self.state, ui);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let style = egui_dock::Style::from_egui(ui.style().as_ref());
            DockArea::new(&mut self.dock)
                .style(style)
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut dock::Viewer { app: &mut self.state });
        });
        chrome::windows(&mut self.state, &ctx);

        for id in std::mem::take(&mut self.state.queued) {
            self.state.run(&id, &ctx);
        }
        for p in std::mem::take(&mut self.state.panel_toggles) {
            dock::toggle_panel(&mut self.dock, p);
        }
        if std::mem::take(&mut self.state.reset_layout) {
            self.dock = dock::default_layout();
        }
        // A finished gesture (slider drag, typing) closes its undo step.
        let idle = !ctx.input(|i| i.pointer.any_down()) && ctx.memory(|m| m.focused().is_none());
        if idle && let Some(d) = self.state.doc_mut() {
            d.session.seal();
        }
        if self.state.render_pending() {
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
    fn opens_the_sample_and_hides_what_it_draws() {
        let s = app_with_sample();
        let d = s.doc().unwrap();
        assert_eq!(d.render.as_ref().unwrap().page_count(), 2);
        assert_eq!(d.session.doc.markups.len(), 6);
        // Every sample markup has a writer, so all are drawn live from the model and editable.
        assert_eq!(d.render.as_ref().unwrap().hidden_count(), 6);
        assert!(crate::actions::editable(
            d.session.doc.find("SAMPLETEXTAAAAAA").unwrap()
        ));
        assert_eq!(s.scale_readout(), "Scale 1/8 in = 1 ft");
    }

    #[test]
    fn delete_undo_through_commands() {
        let ctx = egui::Context::default();
        let mut s = app_with_sample();
        s.set_option("select", "0", &ctx).unwrap();
        s.run("edit.delete", &ctx);
        assert_eq!(s.doc().unwrap().session.doc.markups.len(), 5);
        assert!(s.doc().unwrap().selection.is_empty());
        s.run("edit.undo", &ctx);
        assert_eq!(s.doc().unwrap().session.doc.markups.len(), 6);
        s.run("tool.rectangle", &ctx);
        assert_eq!(s.tool, "rectangle");
        assert!(s.set_option("tool", "nope", &ctx).is_err());
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
            d.session.checkpoint();
            let ids = vec!["SAMPLESQUAREAAAA".to_string()];
            d.session.translate(&ids, markupcraft_geom::Point::new(-100.0, -50.0));
        }
        s.run("file.save", &ctx);
        assert!(s.status.starts_with("Saved"), "{}", s.status);
        let mut again = AppState {
            threads: 0,
            ..Default::default()
        };
        again.open_path(&path);
        let m = again
            .doc()
            .unwrap()
            .session
            .doc
            .find("SAMPLESQUAREAAAA")
            .unwrap()
            .clone();
        // Moved 100 left: centred at x = 850 (the writer pads /Rect by the line width).
        assert!(((m.rect.x0 + m.rect.x1) / 2.0 - 850.0).abs() < 0.5, "{:?}", m.rect);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
