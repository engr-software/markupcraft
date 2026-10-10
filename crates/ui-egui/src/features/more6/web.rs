//! The Web Tab (View > Web Tab, Ctrl+T). MarkupCraft does not embed a web browser: the Web Tab
//! is a document tab listing the favourite web pages as links, plus an address bar. A web page
//! opens in the system browser, or is captured: printed to PDF by Edge or Chrome run headless
//! (`markupcraft_engine::webtab`) and opened as a document to mark up. Window > WebTab in the
//! Preferences sets what links to web pages do, the browser used to capture, the time limit
//! and the favourites.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use markupcraft_engine::prefs_pages::Favorite;
use markupcraft_engine::webtab::{capture_web_page, find_browser, link_page_pdf, normalize_url};

use super::Ask6;
use crate::AppState;
use crate::dialogs::Purpose;
use crate::features::Ask;

/// The name of the Web Tab's document tab.
pub const TAB_NAME: &str = "Web Tab";

#[derive(Default)]
pub struct WebState {
    pub open: bool,
    pub address: String,
    pub fav_name: String,
    /// The last address sent to the system browser.
    pub last_opened: Option<String>,
    /// The address the next capture prints.
    pub capture_url: String,
    /// A capture running on a worker: its result, and where it writes.
    pub capture: Option<(Receiver<Result<usize, String>>, PathBuf)>,
    pub message: String,
}

/// The favourites as (name, address) pairs.
fn favorites(app: &AppState) -> Vec<(String, String)> {
    app.shell
        .prefs
        .more
        .webtab
        .favorites
        .iter()
        .map(|f| (f.name.clone(), f.url.clone()))
        .collect()
}

/// View > Web Tab: show (or refresh) the Web Tab document and its address bar.
pub fn open_tab(app: &mut AppState) {
    app.features.more6.web.open = true;
    refresh_tab(app, true);
}

/// Rebuild the Web Tab document from the favourites (opening it when `create`).
fn refresh_tab(app: &mut AppState, create: bool) {
    let links = favorites(app);
    let made = link_page_pdf("Web Tab", &links, Path::new("Web Tab.pdf")).and_then(|s| s.render_bytes());
    let bytes = match made {
        Ok(b) => b.to_vec(),
        Err(e) => {
            app.status = e.to_string();
            return;
        }
    };
    let before = app.active;
    if let Some(i) = app.docs.iter().position(|d| d.name == TAB_NAME && d.path.is_none()) {
        // Replace the old tab in place.
        let was_active = app.active == i;
        app.docs.remove(i);
        if app.open_bytes(TAB_NAME, None, bytes).is_ok() {
            let last = app.docs.len() - 1;
            let tab = app.docs.remove(last);
            app.docs.insert(i, tab);
            app.active = if was_active { i } else { before.min(app.docs.len() - 1) };
        }
        return;
    }
    if !create {
        return;
    }
    match app.open_bytes(TAB_NAME, None, bytes) {
        Ok(()) => {
            if !app.shell.prefs.more.webtab.switch_to_new && !app.docs.is_empty() {
                app.active = before.min(app.docs.len() - 1);
            }
            app.status = "Web Tab: type an address, or click a favourite".into();
        }
        Err(e) => app.status = e,
    }
}

/// Open `url` in the system browser.
pub fn open_in_browser(app: &mut AppState, ctx: &egui::Context, url: &str) {
    match normalize_url(url) {
        Ok(u) => {
            ctx.open_url(egui::OpenUrl::new_tab(&u));
            app.status = format!("Opened {u} in your browser");
            app.features.more6.web.last_opened = Some(u);
        }
        Err(e) => app.features.more6.web.message = e.to_string(),
    }
}

/// A link to a web page was clicked: the browser, or a capture, as the Preferences say.
pub fn follow_url(app: &mut AppState, ctx: &egui::Context, url: &str) {
    if app.shell.prefs.more.webtab.open_links_in == "browser" {
        open_in_browser(app, ctx, url);
    } else {
        start_capture(app, url);
    }
}

/// Ask where to save a capture of `url`.
pub fn start_capture(app: &mut AppState, url: &str) {
    match normalize_url(url) {
        Ok(u) => {
            let name = format!(
                "{}.pdf",
                markupcraft_engine::webtab::host_of(&u)
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' })
                    .collect::<String>()
            );
            app.features.more6.web.capture_url = u;
            app.dialogs.save(
                Purpose::Feature(Ask::More6(Ask6::CaptureOut)),
                crate::dialogs::PDF,
                &name,
            );
        }
        Err(e) => app.features.more6.web.message = e.to_string(),
    }
}

/// Capture the chosen page to `out` on a worker thread.
pub fn capture_to(app: &mut AppState, out: &Path) {
    let w = &app.shell.prefs.more.webtab;
    let explicit = (!w.browser_path.trim().is_empty()).then(|| PathBuf::from(w.browser_path.trim()));
    let timeout = Duration::from_secs(u64::from(w.capture_timeout_secs.clamp(5, 600)));
    let st = &mut app.features.more6.web;
    let Some(browser) = find_browser(explicit.as_deref()) else {
        st.message = "No Edge, Chrome or Chromium was found to capture the page with: install one, or set its path in Preferences > WebTab. The page can still open in your browser.".into();
        return;
    };
    let url = st.capture_url.clone();
    let (tx, rx) = channel();
    let target = out.to_path_buf();
    let spawned = std::thread::Builder::new().name("web-capture".into()).spawn(move || {
        let r = capture_web_page(&url, &target, &browser, timeout).map_err(|e| e.to_string());
        let _ = tx.send(r);
    });
    match spawned {
        Ok(_) => {
            st.capture = Some((rx, out.to_path_buf()));
            st.message = format!("Capturing {} ...", st.capture_url);
        }
        Err(e) => st.message = e.to_string(),
    }
}

/// Add the address bar's page to the favourites (saved with the preferences).
fn add_favorite(app: &mut AppState) {
    let st = &mut app.features.more6.web;
    let url = match normalize_url(&st.address) {
        Ok(u) => u,
        Err(e) => {
            st.message = e.to_string();
            return;
        }
    };
    let name = if st.fav_name.trim().is_empty() {
        markupcraft_engine::webtab::host_of(&url)
    } else {
        st.fav_name.trim().to_string()
    };
    st.fav_name.clear();
    let favs = &mut app.shell.prefs.more.webtab.favorites;
    if !favs.iter().any(|f| f.url == url) {
        favs.push(Favorite { name, url });
        app.shell.save_prefs();
    }
    refresh_tab(app, false);
}

fn remove_favorite(app: &mut AppState, url: &str) {
    app.shell.prefs.more.webtab.favorites.retain(|f| f.url != url);
    app.shell.save_prefs();
    refresh_tab(app, false);
}

/// Open a captured page: as a tab, or (Preferences > WebTab) in a split view beside the
/// document that was showing.
pub fn open_capture(app: &mut AppState, out: &Path) {
    let before = app.active;
    let had = app.docs.len();
    super::open_written(app, out);
    if app.shell.prefs.more.webtab.captures_in_split && app.docs.len() > had && had > 0 {
        app.shell.split = None;
        crate::shell::split::split(app, true);
        app.active = before.min(app.docs.len() - 1);
    }
}

/// The address bar window, and a running capture.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    // A finished capture opens as a document.
    let done = app
        .features
        .more6
        .web
        .capture
        .as_ref()
        .and_then(|(rx, out)| rx.try_recv().ok().map(|r| (r, out.clone())));
    if let Some((r, out)) = done {
        app.features.more6.web.capture = None;
        match r {
            Ok(n) => {
                app.features.more6.web.message =
                    format!("Captured {} to {}", crate::actions::plural(n, "page"), out.display());
                open_capture(app, &out);
            }
            Err(e) => app.features.more6.web.message = e,
        }
    }
    if app.features.more6.web.capture.is_some() {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    if !app.features.more6.web.open {
        return;
    }
    let mut open = true;
    let mut go: Option<String> = None;
    let mut capture: Option<String> = None;
    let mut add = false;
    let mut remove: Option<String> = None;
    let favs = favorites(app);
    let st = &mut app.features.more6.web;
    crate::i18n::window("Web Tab")
        .id(egui::Id::new("web-tab"))
        .open(&mut open)
        .default_width(520.0)
        .default_pos(egui::pos2(640.0, 140.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Address");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut st.address)
                        .id(egui::Id::new("web-address"))
                        .desired_width(300.0)
                        .hint_text("example.com"),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = Some(st.address.clone());
                }
                if ui.button("Open in Browser").clicked() {
                    go = Some(st.address.clone());
                }
                if ui.button("Capture as PDF...").clicked() {
                    capture = Some(st.address.clone());
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut st.fav_name)
                        .desired_width(160.0)
                        .hint_text("favourite name"),
                );
                add = ui.button("Add to Favorites").clicked();
            });
            ui.separator();
            ui.label(egui::RichText::new("Favorites").strong());
            if favs.is_empty() {
                ui.weak("None yet: type an address and Add to Favorites.");
            }
            egui::Grid::new("web-favs").num_columns(4).striped(true).show(ui, |ui| {
                for (name, url) in &favs {
                    ui.label(if name.is_empty() { url } else { name });
                    if ui.small_button("Open").clicked() {
                        go = Some(url.clone());
                    }
                    if ui.small_button("Capture").clicked() {
                        capture = Some(url.clone());
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(url.clone());
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.weak(
                "Web pages open in your web browser (MarkupCraft does not embed one). Capture prints a page to PDF with Edge or Chrome, so it can be marked up and measured.",
            );
            if st.capture.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Capturing...");
                });
            }
            if !st.message.is_empty() {
                ui.label(&st.message);
            }
        });
    app.features.more6.web.open = open;
    if let Some(u) = go {
        open_in_browser(app, ctx, &u);
    }
    if let Some(u) = capture {
        start_capture(app, &u);
    }
    if add {
        add_favorite(app);
    }
    if let Some(u) = remove {
        remove_favorite(app, &u);
    }
}
