//! Wave 6A's document features: the Web Tab, the JavaScript Console, scanners and cameras,
//! PDF Packages, Insert Pages options and layered pages, stitching, bookmarks from the source
//! structure, Batch Sign & Seal, Smart Overlay, file manager integration, interactive stamps
//! and stamp settings, Mark Text for Redaction, Snapshot Content, links from the File Access
//! list and the Sketch commands. Their own command table joins the menus
//! (`commands::all`); `features` reaches the rest through a few one-line hooks.

pub mod acquire;
pub mod batch6;
pub mod docs;
pub mod prefs;
pub mod script;
pub mod stamps6;
pub mod web;

use std::path::{Path, PathBuf};

use egui::Key;
use markupcraft_geom::Point;

use super::{PickKind, start_pick};
use crate::AppState;
use crate::commands::{Command, Keys, alt, ctrl, ctrl_alt, shift};
use crate::dialogs::Purpose;

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

/// The commands (Revu 21's default keys where it has them).
#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    c("file.create_package", "Create PDF Package...", "File", 26, None, "files"),
    c("file.create_from_scanner", "Create from Scanner or Camera...", "File", 26, None, "scan"),
    c("file.shell_integration", "File Manager Integration...", "File", 26, None, ""),
    c("view.web_tab", "Web Tab", "View", 20, ctrl(Key::T), ""),
    c("markup.image_scanner", "Image From Scanner...", "Markup", 23, shift(Key::I), "scan"),
    c("markup.camera", "Camera...", "Markup", 23, ctrl_alt(Key::I), ""),
    c("markup.interactive_stamp", "Interactive Stamp...", "Markup", 23, None, "stamp"),
    c("markup.stamp_fields", "Edit Stamp Fields...", "Markup", 23, None, ""),
    c("markup.stamp_settings", "Stamp Settings...", "Markup", 23, None, "stamp"),
    c("markup.place_default_stamp", "Place Default Stamp", "Markup", 23, None, "stamp"),
    c("markup.symbol_from_selection", "Add Selection to Tool Chest as Symbol", "Markup", 23, None, "wrench"),
    c("tools.sketch_place", "Sketch: Place Typed Segment", "Tools", 25, None, ""),
    c("tools.sketch_finish", "Sketch: Finish Shape", "Tools", 25, None, ""),
    c("tools.sketch_relative", "Sketch: Relative Angles", "Tools", 25, None, ""),
    c("tools.document_javascript", "Run Document JavaScript", "Tools", 25, None, ""),
    c("window.javascript_console", "JavaScript Console", "Window", 12, alt(Key::J), ""),
    c("document.mark_text_redaction", "Mark Text for Redaction", "Document", 26, shift(Key::K), ""),
    c("document.snapshot_content", "Snapshot Content", "Document", 26, shift(Key::G), ""),
    c("document.bookmarks_from_source", "Bookmarks from Structure", "Document", 26, None, "bookmark"),
    c("document.insert_with_options", "Insert Pages with Options...", "Document", 26, None, ""),
    c("document.insert_layered", "Insert Layered Pages...", "Document", 26, None, "layers"),
    c("document.insert_scanner", "Insert from Scanner or Camera...", "Document", 26, None, "scan"),
    c("document.stitch", "Stitching...", "Document", 26, None, ""),
    c("batch.sign_seal", "Sign & Seal...", "Batch", 23, None, "pen-line"),
    c("batch.smart_overlay", "Smart Overlay...", "Batch", 23, None, "layers"),
];

pub fn handles(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
}

/// Whether a command can run now.
pub fn enabled(app: &AppState, id: &str) -> bool {
    match id {
        "file.create_package"
        | "file.create_from_scanner"
        | "file.shell_integration"
        | "view.web_tab"
        | "markup.stamp_settings"
        | "window.javascript_console"
        | "batch.sign_seal"
        | "batch.smart_overlay" => true,
        "tools.sketch_place" | "tools.sketch_finish" => crate::sketch::offer(app).is_some(),
        "tools.sketch_relative" => true,
        "markup.stamp_fields" => stamps6::selected_interactive(app).is_some(),
        "markup.symbol_from_selection" => app.doc().is_some_and(|d| !d.selection().is_empty()),
        _ => app.has_doc(),
    }
}

/// Checkmarks.
pub fn checked(app: &AppState, id: &str) -> Option<bool> {
    match id {
        "tools.sketch_relative" => Some(app.edit.sketch.relative),
        "window.javascript_console" => Some(app.features.more6.script.open),
        _ => None,
    }
}

/// Run a command; false when it is not one of these.
pub fn run(app: &mut AppState, id: &str) -> bool {
    if !handles(id) {
        return false;
    }
    let m = &mut app.features.more6;
    match id {
        "file.create_package" => m.docs.package_open = true,
        "file.create_from_scanner" => m.acquire.open_for(acquire::Target::NewPdf),
        "file.shell_integration" => m.docs.shell_open = true,
        "view.web_tab" => web::open_tab(app),
        "markup.image_scanner" => m.acquire.open_for(acquire::Target::ImageMarkup),
        "markup.camera" => {
            m.acquire.open_for(acquire::Target::ImageMarkup);
            m.acquire.camera_tab = true;
        }
        "markup.interactive_stamp" => m.stamps.place_open = true,
        "markup.stamp_fields" => stamps6::edit_fields(app),
        "markup.stamp_settings" => stamps6::open_settings(app),
        "markup.place_default_stamp" => start_pick(app, Pick6::DefaultStamp.into(), "Click to place the default stamp"),
        "markup.symbol_from_selection" => stamps6::symbol_from_selection(app),
        "tools.sketch_place" => app.status = crate::sketch::apply(app, false),
        "tools.sketch_finish" => app.status = crate::sketch::apply(app, true),
        "tools.sketch_relative" => app.edit.sketch.relative = !app.edit.sketch.relative,
        "tools.document_javascript" => script::run_document(app),
        "window.javascript_console" => m.script.open = !m.script.open,
        "document.mark_text_redaction" => start_pick(
            app,
            Pick6::RedactText.into(),
            "Drag over the text to mark for redaction; Esc when done",
        ),
        "document.snapshot_content" => start_pick(
            app,
            Pick6::SnapshotContent.into(),
            "Drag a box: its content becomes a snapshot markup in place",
        ),
        "document.bookmarks_from_source" => docs::bookmarks_from_source(app),
        "document.insert_with_options" => {
            app.dialogs.open(
                Purpose::Feature(super::Ask::More6(Ask6::InsertFile)),
                crate::dialogs::PDF,
                false,
            );
        }
        "document.insert_layered" => {
            app.dialogs.open(
                Purpose::Feature(super::Ask::More6(Ask6::LayeredFile)),
                crate::dialogs::PDF,
                false,
            );
        }
        "document.insert_scanner" => m.acquire.open_for(acquire::Target::InsertPages),
        "document.stitch" => m.docs.stitch_open = true,
        "batch.sign_seal" => m.batch.sign_open = true,
        "batch.smart_overlay" => m.batch.smart_open = true,
        _ => {}
    }
    true
}

/// What a file dialog answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask6 {
    PackageAdd,
    PackageOut,
    InsertFile,
    LayeredFile,
    StitchOut,
    ShellDir,
    SignFiles,
    SignP12,
    SignSeal,
    SignOutDir,
    SmartCurrent,
    SmartRevised,
    SmartOutDir,
    AcquireOut,
    CaptureOut,
    StampFolder,
}

/// A file dialog answered.
pub fn answer(app: &mut AppState, ask: Ask6, paths: Vec<PathBuf>) {
    let Some(first) = paths.first().cloned() else { return };
    match ask {
        Ask6::PackageAdd
        | Ask6::PackageOut
        | Ask6::InsertFile
        | Ask6::LayeredFile
        | Ask6::StitchOut
        | Ask6::ShellDir => docs::answer(app, &ask, &paths),
        Ask6::SignFiles
        | Ask6::SignP12
        | Ask6::SignSeal
        | Ask6::SignOutDir
        | Ask6::SmartCurrent
        | Ask6::SmartRevised
        | Ask6::SmartOutDir => batch6::answer(app, &ask, &paths),
        Ask6::AcquireOut => acquire::write_pdf(app, &first),
        Ask6::CaptureOut => web::capture_to(app, &first),
        Ask6::StampFolder => app.features.more6.stamps.settings.folder = first.display().to_string(),
    }
}

/// Canvas picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick6 {
    RedactText,
    SnapshotContent,
    InteractiveStamp,
    DefaultStamp,
    LinkFile,
    AcquiredImage,
}

impl Pick6 {
    pub fn kind(self) -> PickKind {
        match self {
            Pick6::InteractiveStamp | Pick6::DefaultStamp | Pick6::AcquiredImage => PickKind::PointOrRect,
            _ => PickKind::Rect,
        }
    }

    pub fn repeats(self) -> bool {
        matches!(self, Pick6::RedactText)
    }
}

impl From<Pick6> for super::Pick {
    fn from(p: Pick6) -> Self {
        super::Pick::More6(p)
    }
}

/// A pick finished.
pub fn picked(app: &mut AppState, what: Pick6, page: usize, pts: &[Point]) {
    match what {
        Pick6::RedactText => redact_text(app, page, pts),
        Pick6::SnapshotContent => snapshot_content(app, page, pts),
        Pick6::InteractiveStamp => stamps6::place_picked(app, page, pts),
        Pick6::DefaultStamp => stamps6::default_picked(app, page, pts),
        Pick6::LinkFile => docs::link_file_picked(app, page, pts),
        Pick6::AcquiredImage => acquire::image_picked(app, page, pts),
    }
}

/// The state of these dialogs.
#[derive(Default)]
pub struct More6State {
    pub web: web::WebState,
    pub script: script::ScriptState,
    pub acquire: acquire::AcquireState,
    pub docs: docs::DocsState,
    pub batch: batch6::Batch6State,
    pub stamps: stamps6::Stamps6State,
}

/// Once a frame: the dialog windows and finished workers.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    web::window(app, ctx);
    script::window(app, ctx);
    acquire::window(app, ctx);
    docs::window(app, ctx);
    batch6::window(app, ctx);
    stamps6::window(app, ctx);
}

/// Run an engine step on the active document, re-render it and report.
pub(crate) fn doc_step(
    app: &mut AppState,
    f: impl FnOnce(&mut markupcraft_engine::Session) -> markupcraft_engine::Result<String>,
) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else {
        app.status = "Open a document first".into();
        return;
    };
    let r = f(&mut d.session);
    d.sync_pages(threads);
    d.rerender(threads);
    app.status = crate::actions::report(r, |m| m);
}

/// Open a PDF written to `path` as a new tab.
pub(crate) fn open_written(app: &mut AppState, path: &Path) {
    match markupcraft_revu::fsio::read(path) {
        Ok(bytes) => {
            let name = path
                .file_name()
                .map_or_else(|| "document.pdf".to_string(), |n| n.to_string_lossy().into_owned());
            if let Err(e) = app.open_bytes(&name, Some(path.to_path_buf()), bytes) {
                app.status = e;
            }
        }
        Err(e) => app.status = format!("{}: {e}", path.display()),
    }
}

/// Mark Text for Redaction: the words under the dragged box.
fn redact_text(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    let style = app.features.redact.style.clone();
    doc_step(app, |s| {
        s.redact_mark_text(page, r, &style)
            .map(|n| format!("Marked {} for redaction", crate::actions::plural(n, "word")))
    });
}

/// Snapshot Content: the dragged region's content becomes a snapshot markup where it was.
fn snapshot_content(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    doc_step(app, |s| {
        let m = s.snapshot_markup(page, r)?;
        s.add_markup(m)
            .map(|_| "Snapshot placed: drag it, or copy it to another page".to_string())
    });
}
