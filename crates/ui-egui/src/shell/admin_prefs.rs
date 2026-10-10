//! Preferences > Admin (in `UiPrefs::admin`), beside backing up, restoring and resetting the
//! settings and the profiles: the log folder and extended debugging (a log file MarkupCraft
//! writes itself), the shared (network) stamp folder, the email template folder, and making
//! MarkupCraft the default PDF viewer (the files a user applies on their own: MarkupCraft never
//! changes system settings itself).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use egui::RichText;
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AdminPrefs {
    /// Where `markupcraft.log` is written ("" = no log file).
    pub log_folder: String,
    /// The log also records debugging detail (else warnings and errors only).
    pub extended_logging: bool,
    /// A shared stamp library folder (a network folder a team uses; "" = the profile's own).
    pub stamp_folder: String,
    /// The folder holding the email templates ("" = the profile's own).
    pub email_template_folder: String,
}

impl AdminPrefs {
    pub fn sanitize(&mut self) {
        for s in [
            &mut self.log_folder,
            &mut self.stamp_folder,
            &mut self.email_template_folder,
        ] {
            if s.chars().count() > 1024 || s.chars().any(char::is_control) {
                s.clear();
            }
        }
    }
}

/// Largest log file before it is rolled over to `markupcraft.log.1`.
const MAX_LOG: u64 = 8 << 20;

/// Where the log goes and how much it records.
static SINK: Mutex<Option<(PathBuf, bool)>> = Mutex::new(None);

struct FileLog;

impl log::Log for FileLog {
    fn enabled(&self, meta: &log::Metadata<'_>) -> bool {
        let sink = SINK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*sink {
            Some((_, extended)) => meta.level() <= if *extended { log::Level::Debug } else { log::Level::Warn },
            None => false,
        }
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Some((dir, _)) = SINK.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone() else {
            return;
        };
        let line = format!(
            "{} {:<5} {}: {}\n",
            markupcraft_revu::pdf_date_now(),
            record.level(),
            record.target(),
            record.args()
        );
        append(&dir, &line);
    }

    fn flush(&self) {}
}

/// The log file in `dir`.
pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("markupcraft.log")
}

fn append(dir: &Path, line: &str) {
    let path = log_path(dir);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG) {
        let _ = std::fs::rename(&path, dir.join("markupcraft.log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Install MarkupCraft's log (once per process; a logger set before stays, and then the log
/// file is written only by [`note`]).
pub fn install() {
    if log::set_boxed_logger(Box::new(FileLog)).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

/// Point the log at the preference's folder (none: no log file).
pub fn configure(p: &AdminPrefs) {
    let dir = p.log_folder.trim();
    let want = (!dir.is_empty() && Path::new(dir).is_dir()).then(|| (PathBuf::from(dir), p.extended_logging));
    let mut sink = SINK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if *sink != want {
        *sink = want;
    }
}

/// Write a line to the log file now (whatever logger is installed): a warning, or with
/// `detail` a debugging detail (written only with extended debugging).
pub fn note(detail: bool, msg: &str) {
    let Some((dir, extended)) = SINK.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone() else {
        return;
    };
    if !detail || extended {
        let level = if detail { log::Level::Debug } else { log::Level::Warn };
        append(
            &dir,
            &format!("{} {:<5} markupcraft: {msg}\n", markupcraft_revu::pdf_date_now(), level),
        );
    }
}

thread_local! {
    /// The Admin preferences this thread's interface applied last (the defaults at first, so an
    /// interface that never sets a log folder never touches the process's log).
    static APPLIED: std::cell::RefCell<AdminPrefs> = std::cell::RefCell::new(AdminPrefs::default());
}

/// Pass the folders on: the log, the stamp library, the email templates.
pub fn apply(app: &mut AppState) {
    let p = app.shell.ui.admin.clone();
    let changed = APPLIED.with(|a| {
        let mut a = a.borrow_mut();
        let changed = *a != p;
        if changed {
            *a = p.clone();
        }
        changed
    });
    if changed {
        configure(&p);
    }
    let stamps = p.stamp_folder.trim();
    if !stamps.is_empty() {
        let dir = PathBuf::from(stamps);
        if app.features.stamps.dir.as_ref() != Some(&dir) {
            app.features.stamps.dir = Some(dir);
            app.features.stamps.loaded = false;
        }
        app.features.more6.stamps.settings.folder = stamps.to_string();
    }
}

/// The email templates file: in the Admin folder when one is set.
pub fn email_templates_file(app: &AppState) -> Option<PathBuf> {
    let d = app.shell.ui.admin.email_template_folder.trim();
    (!d.is_empty()).then(|| PathBuf::from(d).join("email_templates.json"))
}

/// The page's options (drawn under Admin). Returns an action: "default-viewer".
pub fn section(ui: &mut egui::Ui, a: &mut AdminPrefs) -> Option<&'static str> {
    ui.add_space(6.0);
    egui::CollapsingHeader::new(RichText::new("Folders, logging and the default PDF viewer").strong())
        .id_salt("admin-more")
        .default_open(false)
        .show(ui, |ui| section_body(ui, a))
        .body_returned
        .flatten()
}

fn section_body(ui: &mut egui::Ui, a: &mut AdminPrefs) -> Option<&'static str> {
    let mut out = None;
    ui.label(RichText::new("Folders").strong());
    let folder = |ui: &mut egui::Ui, label: &str, s: &mut String| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::TextEdit::singleline(s)
                    .desired_width(240.0)
                    .hint_text("(the profile's own)"),
            );
        });
    };
    folder(ui, "Shared stamp folder", &mut a.stamp_folder);
    folder(ui, "Email template folder", &mut a.email_template_folder);
    ui.add_space(6.0);
    ui.label(RichText::new("Logging").strong());
    ui.horizontal(|ui| {
        ui.label("Log folder");
        ui.add(
            egui::TextEdit::singleline(&mut a.log_folder)
                .desired_width(240.0)
                .hint_text("(no log file)"),
        );
    });
    ui.checkbox(&mut a.extended_logging, "Extended debugging (record debugging detail)");
    ui.add_space(6.0);
    ui.label(RichText::new("Default PDF viewer").strong());
    if ui.button("Make MarkupCraft the Default PDF Viewer...").clicked() {
        out = Some("default-viewer");
    }
    ui.label(
        RichText::new("Writes the files that do it into a folder you choose, with a README; MarkupCraft never changes system settings itself.")
            .weak()
            .size(11.0),
    );
    out
}

/// The folder for the default-viewer files was chosen.
pub fn write_default_viewer(app: &mut AppState, dir: &Path) {
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("markupcraft"));
    let os = markupcraft_engine::shell_integration::TargetOs::current();
    app.status = crate::actions::report(
        markupcraft_engine::shell_integration::write_default_viewer(dir, &program, os),
        |f| {
            format!(
                "Wrote {} to {}: see its README",
                crate::actions::plural(f.len(), "file"),
                dir.display()
            )
        },
    );
}
