//! Files in and out: images opened as PDF pages, the locked-file prompt (a read-only copy of a
//! file in use), document recovery (unsaved work written every few minutes and offered back
//! after a crash), Email (a draft with the PDF attached), and PDFs dropped on Thumbnails
//! (inserted where they land).

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::docfile;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// A document left by a session that did not close cleanly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recovered {
    /// Its tab name.
    pub name: String,
    /// The file it was opened from (`None` = never saved).
    pub path: Option<PathBuf>,
    /// The recovery copy.
    #[serde(skip)]
    pub copy: PathBuf,
}

#[derive(Default)]
pub struct FileState {
    /// Where recovery copies go (`None` = recovery off: tests, screenshots).
    pub recovery_dir: Option<PathBuf>,
    /// Documents (uid) with a recovery copy written by this session.
    pub written: Vec<u64>,
    /// When recovery copies were last written (egui time).
    pub last_autosave: Option<f64>,
    /// Copies from a session that crashed, offered back.
    pub offered: Vec<Recovered>,
    /// A file in use elsewhere (or read-only): offer a read-only copy.
    pub locked: Option<PathBuf>,
    /// The last email draft written.
    pub last_email: Option<PathBuf>,
}

/// Can we write `path`? (A file open in another program, or marked read-only, cannot be.)
pub fn is_locked(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if meta.permissions().readonly() {
        return true;
    }
    match std::fs::OpenOptions::new().append(true).open(path) {
        Ok(_) => false,
        // Windows: ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
        Err(e) => e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33)),
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// File > Open of a path: images become a new PDF; a locked file asks first. True when handled
/// here (the caller opens nothing).
pub fn intercept_open(app: &mut AppState, path: &Path) -> bool {
    if app.docs.iter().any(|d| d.path.as_deref() == Some(path)) {
        return false;
    }
    if docfile::is_image_path(path) {
        let stem = path
            .file_stem()
            .map_or_else(|| "image".to_string(), |s| s.to_string_lossy().into_owned());
        let name = format!("{stem}.pdf");
        let r = docfile::image_file_pdf(path)
            .map_err(|e| e.to_string())
            .and_then(|b| app.open_bytes(&name, None, b));
        app.status = match r {
            Ok(()) => format!("Opened {} as a PDF (Save writes {name})", name_of(path)),
            Err(e) => format!("Could not open {}: {e}", name_of(path)),
        };
        return true;
    }
    if app.shell.ui.extra.locked_prompt && is_locked(path) {
        app.shell.extra.files.locked = Some(path.to_path_buf());
        return true;
    }
    false
}

/// Open a read-only copy of `path` (it saves elsewhere).
pub fn open_copy(app: &mut AppState, path: &Path) {
    let name = name_of(path);
    let stem = name.trim_end_matches(".pdf");
    let r = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| app.open_bytes(&format!("{stem} (read-only copy).pdf"), None, b));
    app.status = match r {
        Ok(()) => format!("Opened a read-only copy of {name}: Save As keeps your changes"),
        Err(e) => format!("Could not open {name}: {e}"),
    };
}

// ---- recovery ----------------------------------------------------------------------------------

fn copy_paths(dir: &Path, uid: u64) -> (PathBuf, PathBuf) {
    let stem = format!("{}-{uid}", std::process::id());
    (dir.join(format!("{stem}.pdf")), dir.join(format!("{stem}.json")))
}

/// Write a recovery copy of every document with unsaved changes now.
pub fn autosave_now(app: &mut AppState) -> usize {
    let Some(dir) = app.shell.extra.files.recovery_dir.clone() else {
        return 0;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return 0;
    }
    let mut n = 0;
    for d in app.docs.iter().filter(|d| d.session.is_dirty()) {
        let Ok(bytes) = d.session.current_bytes() else { continue };
        let (pdf, meta) = copy_paths(&dir, d.uid);
        let info = Recovered {
            name: d.name.clone(),
            path: d.path.clone(),
            copy: pdf.clone(),
        };
        let ok = crate::chest::write_atomic(&pdf, &bytes).is_ok()
            && serde_json::to_vec(&info)
                .ok()
                .is_some_and(|j| crate::chest::write_atomic(&meta, &j).is_ok());
        if ok {
            n += 1;
            if !app.shell.extra.files.written.contains(&d.uid) {
                app.shell.extra.files.written.push(d.uid);
            }
        }
    }
    n
}

/// Remove recovery copies of documents saved or closed since.
fn clean(app: &mut AppState) {
    let Some(dir) = app.shell.extra.files.recovery_dir.clone() else {
        return;
    };
    let docs = &app.docs;
    app.shell.extra.files.written.retain(|uid| {
        let keep = docs.iter().any(|d| d.uid == *uid && d.session.is_dirty());
        if !keep {
            let (pdf, meta) = copy_paths(&dir, *uid);
            let _ = std::fs::remove_file(pdf);
            let _ = std::fs::remove_file(meta);
        }
        keep
    });
}

/// Copies another session left behind (it crashed). Read at startup, before this session writes.
pub fn scan(dir: &Path) -> Vec<Recovered> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut metas: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .take(200)
        .collect();
    metas.sort();
    metas
        .into_iter()
        .filter_map(|m| {
            let pdf = m.with_extension("pdf");
            let text = std::fs::read(&m).ok().filter(|b| b.len() < 64 * 1024)?;
            let mut r: Recovered = serde_json::from_slice(&text).ok()?;
            pdf.is_file().then(|| {
                r.copy = pdf;
                r
            })
        })
        .collect()
}

/// Recovery copies on: `dir` holds them; anything there from a crashed session is offered.
pub fn enable_recovery(app: &mut AppState, dir: PathBuf) {
    app.shell.extra.files.offered = scan(&dir);
    app.shell.extra.files.recovery_dir = Some(dir);
}

fn forget(r: &Recovered) {
    let _ = std::fs::remove_file(&r.copy);
    let _ = std::fs::remove_file(r.copy.with_extension("json"));
}

/// Open a recovered document; Save writes to the file it came from.
pub fn restore(app: &mut AppState, r: &Recovered) {
    let res = std::fs::read(&r.copy)
        .map_err(|e| e.to_string())
        .and_then(|b| app.open_bytes(&r.name, r.path.clone(), b));
    app.status = match res {
        Ok(()) => format!("Recovered {}: save to keep it", r.name),
        Err(e) => format!("Could not recover {}: {e}", r.name),
    };
    forget(r);
}

pub fn begin_frame(app: &mut AppState, ctx: &egui::Context) {
    if app.shell.extra.files.recovery_dir.is_none() {
        return;
    }
    clean(app);
    let minutes = app.shell.prefs.autosave_minutes;
    if minutes == 0 {
        return;
    }
    let now = ctx.input(|i| i.time);
    let last = *app.shell.extra.files.last_autosave.get_or_insert(now);
    if now - last >= f64::from(minutes) * 60.0 {
        app.shell.extra.files.last_autosave = Some(now);
        autosave_now(app);
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(30));
}

// ---- email -------------------------------------------------------------------------------------

/// File > Email: a new message with the document (as it is now) attached, opened in the mail
/// program.
pub fn email(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    let bytes = match d.session.current_bytes() {
        Ok(b) => b,
        Err(e) => {
            app.status = e.to_string();
            return;
        }
    };
    let name = if d.name.to_lowercase().ends_with(".pdf") {
        d.name.clone()
    } else {
        format!("{}.pdf", d.name)
    };
    let draft = docfile::email_draft(name.trim_end_matches(".pdf"), "", &name, &bytes);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "markupcraft-email-{}-{}.eml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if let Err(e) = crate::chest::write_atomic(&path, &draft) {
        app.status = format!("Could not write the email: {e}");
        return;
    }
    app.shell.extra.files.last_email = Some(path.clone());
    if app.shell.extra.launch {
        launch(&path);
    }
    app.status = format!("Email draft with {name} attached");
}

/// Open `path` with the program the system uses for it.
pub fn launch(path: &Path) {
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_arch = "wasm32")]
    let r: std::io::Result<()> = Ok(());
    if let Err(e) = r {
        log::warn!("open {}: {e}", path.display());
    }
}

// ---- drops on Thumbnails -----------------------------------------------------------------------

/// PDFs dropped on the Thumbnails panel go into the document before the page they land on.
/// True when the drop was handled here.
pub fn drop_on_thumbnails(app: &mut AppState, ctx: &egui::Context) -> bool {
    let (files, at) = ctx.input(|i| (i.raw.dropped_files.clone(), i.pointer.latest_pos()));
    if files.is_empty() {
        return false;
    }
    let (Some(r), Some(at)) = (app.shell.extra.thumbs_rect, at) else {
        return false;
    };
    if !r.contains(at) || !app.has_doc() {
        return false;
    }
    let before = app.shell.thumbs.drop_before.unwrap_or(usize::MAX);
    let threads = app.threads;
    let mut pages = 0usize;
    let mut errors = Vec::new();
    for f in files {
        let path = f.path().to_path_buf();
        let Some(d) = app.doc_mut() else { break };
        let at = before.min(d.session.page_count()) + pages;
        let r = if docfile::is_image_path(&path) {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = std::env::temp_dir().join(format!(
                "markupcraft-drop-{}-{}.pdf",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            docfile::image_file_pdf(&path)
                .and_then(|b| {
                    std::fs::write(&tmp, b).map_err(|e| markupcraft_engine::EngineError::Io {
                        path: tmp.display().to_string(),
                        source: e,
                    })
                })
                .and_then(|_| d.session.insert_file_pages(at, &tmp, None))
        } else {
            d.session.insert_file_pages(at, &path, None)
        };
        match r {
            Ok(rep) => pages += rep.pages_after.saturating_sub(rep.pages_before),
            Err(e) => errors.push(format!("{}: {e}", name_of(&path))),
        }
        d.sync_pages(threads);
    }
    app.status = if errors.is_empty() {
        format!("Inserted {}", crate::actions::plural(pages, "page"))
    } else {
        errors.join("; ")
    };
    true
}

// ---- windows -----------------------------------------------------------------------------------

pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    if let Some(path) = app.shell.extra.files.locked.clone() {
        let mut answer = None;
        egui::Window::new("File in Use")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} is in use by another program or read-only, so changes cannot be saved to it.",
                    name_of(&path)
                ));
                ui.horizontal(|ui| {
                    if ui.button("Open Read-Only Copy").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(a) = answer {
            app.shell.extra.files.locked = None;
            if a {
                open_copy(app, &path);
            }
        }
    }
    if !app.shell.extra.files.offered.is_empty() {
        let list = app.shell.extra.files.offered.clone();
        let mut act: Option<(usize, bool)> = None;
        let mut all = None;
        egui::Window::new("Document Recovery")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("MarkupCraft did not close normally. These documents had unsaved changes:");
                for (i, r) in list.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&r.name).strong());
                        if let Some(p) = &r.path {
                            ui.label(RichText::new(p.display().to_string()).weak());
                        }
                        if ui.button("Recover").clicked() {
                            act = Some((i, true));
                        }
                        if ui.button("Discard").clicked() {
                            act = Some((i, false));
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Recover All").clicked() {
                        all = Some(true);
                    }
                    if ui.button("Discard All").clicked() {
                        all = Some(false);
                    }
                });
            });
        let take: Vec<(Recovered, bool)> = match (all, act) {
            (Some(a), _) => list.iter().map(|r| (r.clone(), a)).collect(),
            (None, Some((i, a))) => list.get(i).map(|r| vec![(r.clone(), a)]).unwrap_or_default(),
            _ => Vec::new(),
        };
        for (r, keep) in take {
            app.shell.extra.files.offered.retain(|o| o.copy != r.copy);
            if keep {
                restore(app, &r);
            } else {
                forget(&r);
            }
        }
    }
}
