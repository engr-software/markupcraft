//! Recent files, pinned files and the last session (`<config>/recent.json`): File > Open
//! Recent, the File Access panel, reopening last session's files at startup, and each file
//! reopening at the page, zoom and layout it was closed with. Also File > New (a blank PDF) and
//! File > Refresh (reload the file from disk).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::canvas::{Fit, PageMode};

/// Most recent files kept whatever the preference says.
const MAX_RECENT: usize = 200;
const MAX_PINNED: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RecentFile {
    pub path: PathBuf,
    /// Unix seconds of the last open.
    pub opened: u64,
    /// How many times it was opened.
    pub count: u32,
    /// The view it was closed with: 0-based page, zoom (1 = 100 %; 0 = fit page), layout.
    pub page: usize,
    pub zoom: f32,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Pinned {
    pub path: PathBuf,
    /// A user-named group ("" = ungrouped).
    pub category: String,
    pub added: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RecentStore {
    pub files: Vec<RecentFile>,
    pub pinned: Vec<Pinned>,
    /// The files open when the app last closed.
    pub last_session: Vec<PathBuf>,
}

/// How the File Access recents are sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Date,
    Folder,
    MostUsed,
    Name,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl RecentStore {
    pub fn load(path: &Path) -> Self {
        let mut s: RecentStore = std::fs::read(path)
            .ok()
            .filter(|b| b.len() < (8 << 20))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        s.files.truncate(MAX_RECENT);
        s.pinned.truncate(MAX_PINNED);
        s.last_session.truncate(64);
        for f in &mut s.files {
            if !f.zoom.is_finite() || f.zoom < 0.0 {
                f.zoom = 0.0;
            }
        }
        s
    }

    pub fn find(&self, path: &Path) -> Option<&RecentFile> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Recents in the requested order (newest first for Date).
    pub fn sorted(&self, sort: Sort) -> Vec<RecentFile> {
        let mut v = self.files.clone();
        match sort {
            Sort::Date => v.sort_by_key(|f| std::cmp::Reverse(f.opened)),
            Sort::MostUsed => v.sort_by_key(|f| (std::cmp::Reverse(f.count), std::cmp::Reverse(f.opened))),
            Sort::Folder => v.sort_by(|a, b| a.path.parent().cmp(&b.path.parent()).then(a.path.cmp(&b.path))),
            Sort::Name => v.sort_by_key(|f| f.path.file_name().map(|n| n.to_string_lossy().to_lowercase())),
        }
        v
    }

    pub fn is_pinned(&self, path: &Path) -> bool {
        self.pinned.iter().any(|p| p.path == path)
    }

    pub fn toggle_pin(&mut self, path: &Path, category: &str) {
        if self.is_pinned(path) {
            self.pinned.retain(|p| p.path != path);
        } else if self.pinned.len() < MAX_PINNED {
            self.pinned.push(Pinned {
                path: path.to_path_buf(),
                category: category.chars().take(80).collect(),
                added: now_secs(),
            });
        }
    }
}

fn mode_name(m: PageMode) -> &'static str {
    match m {
        PageMode::Single => "single",
        PageMode::Continuous => "continuous",
        PageMode::SideBySide => "side",
        PageMode::ContinuousSideBySide => "continuous-side",
    }
}

fn mode_of(s: &str) -> Option<PageMode> {
    Some(match s {
        "single" => PageMode::Single,
        "continuous" => PageMode::Continuous,
        "side" => PageMode::SideBySide,
        "continuous-side" => PageMode::ContinuousSideBySide,
        _ => return None,
    })
}

/// Write the store (when the app has a config folder).
pub fn persist(app: &mut AppState) {
    let Some(path) = app.shell.recent_path.clone() else {
        return;
    };
    let r = serde_json::to_vec_pretty(&app.shell.recent)
        .map_err(|e| e.to_string())
        .and_then(|b| crate::chest::write_atomic(&path, &b));
    if let Err(e) = r {
        log::warn!("saving recent files: {e}");
    }
}

/// A file was opened into the active tab: list it first and give it its default (or
/// remembered) view.
pub fn opened(app: &mut AppState, path: &Path) {
    let now = now_secs();
    let keep_days = u64::from(app.shell.ui.recents_days);
    let limit = (app.shell.prefs.recent_files as usize).min(MAX_RECENT);
    let st = &mut app.shell.recent;
    let prev = st.files.iter().position(|f| f.path == path).map(|i| st.files.remove(i));
    let mut entry = prev.clone().unwrap_or_default();
    entry.path = path.to_path_buf();
    entry.opened = now;
    entry.count = entry.count.saturating_add(1);
    st.files.insert(0, entry);
    if keep_days > 0 {
        st.files.retain(|f| {
            now.saturating_sub(f.opened) <= keep_days * 86_400 || st.pinned.iter().any(|p| p.path == f.path)
        });
    }
    st.files.truncate(limit.max(1));
    // The view: remembered, else the default layout rule.
    let ui = app.shell.ui.clone();
    if let Some(d) = app.doc_mut() {
        let first = d.render.as_ref().and_then(|r| r.page(0)).map(|g| (g.width, g.height));
        d.view.set_mode(ui.mode_for(first));
        d.view.set_fit(if ui.default_fit == "width" {
            Fit::Width
        } else {
            Fit::Page
        });
        if ui.remember_last_page
            && let Some(p) = prev
        {
            if let Some(m) = mode_of(&p.mode) {
                d.view.set_mode(m);
            }
            let n = d.session.page_count();
            d.view.go_to_page(p.page, n);
            if p.zoom > 0.0 {
                d.view.zoom = p.zoom;
                d.view.fit = Fit::None;
            }
        }
    }
    persist(app);
}

/// A document is closing: remember where it was.
pub fn closing(app: &mut AppState, uid: u64) {
    let Some(d) = app.docs.iter().find(|d| d.uid == uid) else {
        return;
    };
    let Some(path) = d.path.clone() else { return };
    let (page, zoom, mode) = (
        d.view.current,
        if d.view.fit == Fit::Page { 0.0 } else { d.view.zoom },
        mode_name(d.view.mode),
    );
    if let Some(f) = app.shell.recent.files.iter_mut().find(|f| f.path == path) {
        f.page = page;
        f.zoom = zoom;
        f.mode = mode.to_string();
    }
    app.shell.history.remove(&uid);
    if app.shell.split.as_ref().is_some_and(|s| s.pane.uid == uid) {
        app.shell.split = None;
    }
    persist(app);
}

/// Keep the last session's file list current (called each frame; writes only on change).
pub fn track_session(app: &mut AppState) {
    if app.shell.recent_path.is_none() {
        return;
    }
    let open: Vec<PathBuf> = app.docs.iter().filter_map(|d| d.path.clone()).collect();
    if open != app.shell.recent.last_session {
        // Remember each open file's view too, so a crash or exit keeps it.
        let uids: Vec<u64> = app.docs.iter().map(|d| d.uid).collect();
        for u in uids {
            closing_view_only(app, u);
        }
        app.shell.recent.last_session = open;
        persist(app);
    }
}

fn closing_view_only(app: &mut AppState, uid: u64) {
    let Some(d) = app.docs.iter().find(|d| d.uid == uid) else {
        return;
    };
    let Some(path) = d.path.clone() else { return };
    let (page, zoom, mode) = (
        d.view.current,
        if d.view.fit == Fit::Page { 0.0 } else { d.view.zoom },
        mode_name(d.view.mode),
    );
    if let Some(f) = app.shell.recent.files.iter_mut().find(|f| f.path == path) {
        f.page = page;
        f.zoom = zoom;
        f.mode = mode.to_string();
    }
}

/// Startup: reopen the files of the last session (preference).
pub fn reopen_last_session(app: &mut AppState) {
    if !app.shell.ui.reopen_last_session {
        return;
    }
    let files = app.shell.recent.last_session.clone();
    for f in files {
        if f.is_file() {
            app.open_path(&f);
        }
    }
}

/// File > New: an untitled blank Letter page.
pub fn new_blank(app: &mut AppState) {
    match markupcraft_engine::blank::pdf_bytes(&[(612.0, 792.0)]) {
        Ok(bytes) => {
            app.shell.untitled += 1;
            let name = format!("Untitled {}.pdf", app.shell.untitled);
            if let Err(e) = app.open_bytes(&name, None, bytes) {
                app.status = format!("New PDF: {e}");
            }
        }
        Err(e) => app.status = format!("New PDF: {e}"),
    }
}

/// File > Refresh (Shift+F5): read the file again from disk.
pub fn reload(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    if d.session.is_dirty() {
        app.status = "This document has unsaved changes: save or close it before refreshing".into();
        return;
    }
    let Some(path) = d.path.clone() else {
        app.status = "This document has no file to refresh from".into();
        return;
    };
    let (uid, page) = (d.uid, d.view.current);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            app.status = format!("Could not read {}: {e}", path.display());
            return;
        }
    };
    let author = app.author.clone();
    let threads = app.threads;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    match markupcraft_engine::Session::from_bytes(bytes, &path) {
        Ok(mut s) => {
            s.set_author(&author);
            d.session = s;
            d.rerender(threads);
            let n = d.session.page_count();
            d.view.go_to_page(page, n);
            app.status = format!("Refreshed {}", path.display());
        }
        Err(e) => app.status = format!("Could not refresh: {e}"),
    }
}

/// File > Open Recent (submenu items).
pub fn menu(app: &mut AppState, ui: &mut egui::Ui) {
    ui.menu_button("Open Recent", |ui| {
        ui.set_min_width(300.0);
        let files = app.shell.recent.sorted(Sort::Date);
        if files.is_empty() {
            ui.label(egui::RichText::new("No recent files").weak());
        }
        for (i, f) in files.iter().take(20).enumerate() {
            let name = f
                .path
                .file_name()
                .map_or_else(|| f.path.display().to_string(), |n| n.to_string_lossy().into_owned());
            if ui
                .button(format!("{} {name}", i + 1))
                .on_hover_text(f.path.display().to_string())
                .clicked()
            {
                app.open_path(&f.path);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Clear Recent Files").clicked() {
            app.queue("file.clear_recent");
            ui.close();
        }
    });
}
