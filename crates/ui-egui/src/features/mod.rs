//! The interface of the document and review features: their menu commands, dialogs, canvas
//! picks and highlights. Panels for them live in `panels/` (Search, Compare results, Spaces,
//! Links, Signatures, Layers, Sets); everything else is here, one file per feature.
//!
//! The rest of the interface reaches this module through a few one-line hooks:
//! - [`COMMANDS`] joins the command table (menus, shortcuts, Help > Keyboard Shortcuts);
//! - [`handles`] / [`enabled`] / [`run`] are arms of `AppState::enabled` and `AppState::run`;
//! - [`Ask`] is the `Purpose::Feature` of a file dialog, answered by [`answer`];
//! - [`frame`] runs once a frame (dialog windows, canvas picks);
//! - [`canvas::layer`] draws highlights on the pages and runs picks before the active tool.

pub mod batch;
pub mod canvas;
pub mod compare;
pub mod docops;
pub mod fill;
pub mod forms;
pub mod links;
pub mod ocr;
pub mod overlay;
pub mod print;
pub mod redact;
pub mod search;
pub mod sets;
pub mod signatures;
pub mod spaces;
pub mod spell;
pub mod stamps;
pub mod summary;

use std::path::PathBuf;

use egui::Key;
use markupcraft_geom::Point;

use crate::AppState;
use crate::commands::{Command, Keys, ctrl, ctrl_shift, key, shift};

pub use canvas::{Mark, PickKind};

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

/// Commands of these features (Revu's default keys). Groups 20+ keep them apart from the core
/// rows of the same menu.
#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    // File
    c("file.combine", "Combine Files...", "File", 20, None, "files"),
    c("file.overlay", "Overlay Pages...", "File", 20, None, "layers"),
    // Markup
    c("markup.stamps", "Stamp Library...", "Markup", 20, None, "stamp"),
    c("markup.image", "Image...", "Markup", 20, None, ""),
    c("markup.hyperlink", "Hyperlink", "Markup", 20, shift(Key::H), ""),
    c("markup.summary", "Markup Summary...", "Markup", 21, None, "file-spreadsheet"),
    c("markup.spell", "Check Spelling...", "Markup", 21, key(Key::F7), "type"),
    // Measure
    c("measure.dynamic_fill", "Dynamic Fill", "Measure", 20, key(Key::J), "scan"),
    c("measure.legend", "Legend...", "Measure", 20, None, "list"),
    // Tools
    c("tools.search", "Search", "Tools", 20, ctrl(Key::F), "search"),
    c("tools.visual_search", "Visual Search", "Tools", 20, None, "scan"),
    c("tools.compare", "Compare Documents...", "Tools", 21, None, "columns-3"),
    c("tools.ocr", "OCR...", "Tools", 22, ctrl_shift(Key::O), "text-select"),
    c("tools.forms", "Form Fields...", "Tools", 22, None, "text-cursor-input"),
    c("tools.sign", "Sign Document...", "Tools", 22, None, "pen-line"),
    c("tools.digital_id", "Create Digital ID...", "Tools", 22, None, ""),
    // Document
    c("document.add_bookmark", "Add Bookmark", "Document", 20, ctrl(Key::B), "bookmark"),
    c("document.headers_footers", "Header & Footer...", "Document", 21, None, ""),
    c("document.watermark", "Watermark...", "Document", 21, None, ""),
    c("document.bates", "Bates Numbering...", "Document", 21, None, "hash"),
    c("document.reduce", "Reduce File Size...", "Document", 21, None, ""),
    c("document.security", "Security...", "Document", 22, ctrl(Key::L), "lock"),
    c("document.mark_redaction", "Mark for Redaction", "Document", 22, shift(Key::R), "square"),
    c("document.apply_redactions", "Apply Redactions...", "Document", 22, shift(Key::A), ""),
    c("document.attachments", "Attachments...", "Document", 23, None, "file-plus"),
    c("document.slip_sheet", "Slip Sheet...", "Document", 23, None, ""),
    // Batch
    c("batch.link", "Batch Link...", "Batch", 20, None, ""),
    c("batch.combine", "Combine...", "Batch", 20, None, "files"),
    c("batch.sets", "Sets", "Batch", 21, None, "files"),
];

/// Panels these features add (for the Window menu keys see the panel rows).
pub fn handles(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
        || matches!(
            id,
            "file.print" | "document.flatten" | "batch.summary" | "batch.flatten"
        )
}

/// Whether a feature command can run now.
pub fn enabled(app: &AppState, id: &str) -> bool {
    match id {
        "file.combine" | "file.overlay" | "batch.link" | "batch.combine" | "batch.sets" | "batch.summary"
        | "batch.flatten" | "markup.stamps" | "tools.digital_id" => true,
        _ => app.has_doc(),
    }
}

/// Run a feature command.
pub fn run(app: &mut AppState, id: &str, _ctx: &egui::Context) {
    let f = &mut app.features;
    match id {
        "file.print" => f.print.open(),
        "file.combine" | "batch.combine" => f.batch.open(batch::Kind::Combine),
        "batch.link" => f.batch.open(batch::Kind::Link),
        "batch.summary" => f.batch.open(batch::Kind::Summary),
        "batch.flatten" => f.batch.open(batch::Kind::Flatten),
        "document.slip_sheet" => f.batch.open(batch::Kind::SlipSheet),
        "batch.sets" => app.show_panel("sets"),
        "file.overlay" => f.overlay.open = true,
        "markup.stamps" => f.stamps.open = true,
        "markup.image" => app.dialogs.open(Purpose::Feature(Ask::PlaceImage), IMAGES, false),
        "markup.hyperlink" => {
            start_pick(app, Pick::Hyperlink, "Drag a box over the link area");
        }
        "markup.summary" => f.summary.open = true,
        "markup.spell" => spell::start(app),
        "measure.dynamic_fill" => fill::start(app),
        "measure.legend" => f.fill.legend_open = true,
        "tools.search" => {
            f.search.focus = true;
            f.search.visual = false;
            app.show_panel("search");
        }
        "tools.visual_search" => {
            f.search.visual = true;
            app.show_panel("search");
        }
        "tools.compare" => compare::open(app),
        "tools.ocr" => f.ocr.open = true,
        "tools.forms" => forms::open(app),
        "tools.sign" => f.signatures.sign_open = true,
        "tools.digital_id" => f.signatures.new_id_open = true,
        "document.add_bookmark" => add_bookmark(app),
        "document.flatten" => f.docops.open(docops::Tab::Flatten),
        "document.headers_footers" => f.docops.open(docops::Tab::HeaderFooter),
        "document.watermark" => f.docops.open(docops::Tab::Watermark),
        "document.bates" => f.docops.open(docops::Tab::Bates),
        "document.reduce" => f.docops.open(docops::Tab::Reduce),
        "document.security" => f.docops.open(docops::Tab::Security),
        "document.attachments" => f.docops.open(docops::Tab::Attachments),
        "document.mark_redaction" => {
            start_pick(app, Pick::Redact, "Drag boxes over what to redact; Esc when done");
        }
        "document.apply_redactions" => f.redact.confirm = true,
        _ => {}
    }
}

/// Document > Add Bookmark: the current page, titled by its label.
fn add_bookmark(app: &mut AppState) {
    let Some(d) = app.doc_mut() else { return };
    let page = d.view.current;
    let label = d.session.page_labels().get(page).cloned().unwrap_or_default();
    let title = if label.is_empty() {
        format!("Page {}", page + 1)
    } else {
        label
    };
    let r = d.session.add_bookmark(&[], None, &title, page);
    app.status = crate::actions::report(r, |_| format!("Bookmark added: {title}"));
    app.show_panel("bookmarks");
}

/// What a file dialog's answer is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    CompareOld,
    OverlayAdd,
    OverlayOut,
    SummaryOut(markupcraft_engine::summary::SummaryFormat),
    PrintOut,
    SetOpen,
    SetSave,
    SetAddFiles,
    SpacesExport,
    SpacesImport,
    StampImage,
    PlaceImage,
    Attachment,
    AttachmentExtract(String),
    SignId,
    SignOut,
    NewIdOut,
    TrustCerts,
    BatchFiles,
    BatchOut,
}

pub const IMAGES: crate::dialogs::Filter = ("Images", &["png", "jpg", "jpeg"]);
pub const ANY: crate::dialogs::Filter = ("All files", &["*"]);
pub const P12: crate::dialogs::Filter = ("Digital ID", &["p12", "pfx"]);
pub const CERTS: crate::dialogs::Filter = ("Certificates", &["pem", "cer", "crt", "der", "p7b"]);
pub const SET: crate::dialogs::Filter = ("MarkupCraft Set", &["pcset", "json"]);
pub const JSON: crate::dialogs::Filter = ("JSON", &["json"]);

use crate::dialogs::Purpose;

/// A file dialog answered (never called with an empty list: that is a cancel).
pub fn answer(app: &mut AppState, ask: Ask, paths: Vec<PathBuf>) {
    let Some(first) = paths.first().cloned() else { return };
    match ask {
        Ask::CompareOld => app.features.compare.old_file = Some(first),
        Ask::OverlayAdd => overlay::add_files(app, &paths),
        Ask::OverlayOut => overlay::write(app, &first),
        Ask::SummaryOut(fmt) => summary::write(app, fmt, &first),
        Ask::PrintOut => print::write(app, &first),
        Ask::SetOpen => sets::open_set(app, &first),
        Ask::SetSave => sets::save_set(app, &first),
        Ask::SetAddFiles => sets::add_files(app, &paths),
        Ask::SpacesExport | Ask::SpacesImport => spaces::file(app, &ask, &first),
        Ask::StampImage => stamps::add_image(app, &first),
        Ask::PlaceImage => {
            app.features.stamps.place = Some(stamps::Placing::Image(first));
            start_pick(app, Pick::Stamp, "Click to place the image, or drag its box");
        }
        Ask::Attachment | Ask::AttachmentExtract(_) => docops::attachment_file(app, &ask, &first),
        Ask::SignId | Ask::SignOut | Ask::NewIdOut | Ask::TrustCerts => signatures::file(app, &ask, &first),
        Ask::BatchFiles => app.features.batch.add_files(&paths),
        Ask::BatchOut => batch::output(app, &first),
    }
}

/// Something the user picks on the canvas, and what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Visual Search: the symbol's box.
    VisualRegion,
    /// Dynamic Fill: click inside a region (repeats).
    Fill,
    /// A new space outline (click points; double-click or Enter closes).
    Space,
    /// A space by Dynamic Fill (click inside a room).
    SpaceFill,
    /// A link's box.
    Hyperlink,
    /// Boxes to mark for redaction (repeats).
    Redact,
    /// Where a stamp or image goes (click or box).
    Stamp,
    /// A new form field's box.
    FormField,
    /// The signature's box.
    Signature,
    /// Where a legend goes.
    Legend,
}

impl Pick {
    pub fn kind(self) -> PickKind {
        match self {
            Pick::Fill | Pick::SpaceFill | Pick::Legend => PickKind::Point,
            Pick::Space => PickKind::Polygon,
            Pick::Stamp => PickKind::PointOrRect,
            _ => PickKind::Rect,
        }
    }

    /// Picks that stay on after each answer (Esc ends them).
    pub fn repeats(self) -> bool {
        matches!(self, Pick::Fill | Pick::Redact)
    }
}

/// State of every feature's panels and dialogs.
#[derive(Default)]
pub struct FeatureState {
    /// The canvas pick waiting for the user: (document uid, what for).
    pub pick: Option<(u64, Pick)>,
    pub hint: String,
    pub search: search::SearchState,
    pub compare: compare::CompareState,
    pub overlay: overlay::OverlayState,
    pub summary: summary::SummaryState,
    pub print: print::PrintState,
    pub docops: docops::DocOpsState,
    pub ocr: ocr::OcrState,
    pub redact: redact::RedactState,
    pub forms: forms::FormsState,
    pub spell: spell::SpellState,
    pub stamps: stamps::StampsState,
    pub batch: batch::BatchState,
    pub fill: fill::FillState,
    pub spaces: spaces::SpacesState,
    pub links: links::LinksState,
    pub signatures: signatures::SignaturesState,
    pub sets: sets::SetsState,
    pub layers: crate::panels::layers::Fields,
    pub bookmarks: crate::panels::bookmarks::Fields,
}

/// Ask the user to pick on the active document's canvas.
pub fn start_pick(app: &mut AppState, what: Pick, hint: &str) {
    let Some(uid) = app.doc().map(|d| d.uid) else { return };
    app.set_tool("select");
    app.features.pick = Some((uid, what));
    app.features.hint = hint.to_string();
    app.status = hint.to_string();
}

/// End the pick.
pub fn end_pick(app: &mut AppState) {
    app.features.pick = None;
    app.features.hint.clear();
}

/// Once a frame, after the dock: picked points, dialog windows, and the canvas layer for the
/// next frame.
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    // Esc (or another tool) ends a pick.
    if app.features.pick.is_some() && (app.queued.iter().any(|q| q == "edit.deselect") || app.tool != "select") {
        end_pick(app);
        app.status = "Cancelled".into();
    }
    for p in canvas::take_picked(ctx) {
        let Some((uid, what)) = app.features.pick else { break };
        if uid != p.doc {
            continue;
        }
        if !what.repeats() {
            end_pick(app);
        }
        picked(app, what, p.page, p.pts);
    }
    // While the user picks on the page, dialogs step aside (they come back after the pick).
    if app.features.pick.is_some() {
        fill::window(app, ctx);
        canvas::publish(app, ctx);
        return;
    }
    search::window(app, ctx);
    compare::window(app, ctx);
    overlay::window(app, ctx);
    summary::window(app, ctx);
    print::window(app, ctx);
    docops::window(app, ctx);
    ocr::window(app, ctx);
    redact::window(app, ctx);
    forms::window(app, ctx);
    spell::window(app, ctx);
    stamps::window(app, ctx);
    batch::window(app, ctx);
    fill::window(app, ctx);
    links::window(app, ctx);
    signatures::window(app, ctx);
    spaces::window(app, ctx);
    canvas::publish(app, ctx);
}

/// A pick finished.
fn picked(app: &mut AppState, what: Pick, page: usize, pts: Vec<Point>) {
    match what {
        Pick::VisualRegion => search::region_picked(app, page, &pts),
        Pick::Fill | Pick::SpaceFill => fill::picked(app, what, page, &pts),
        Pick::Space => spaces::outline_picked(app, page, pts),
        Pick::Hyperlink => links::rect_picked(app, page, &pts),
        Pick::Redact => redact::rect_picked(app, page, &pts),
        Pick::Stamp => stamps::picked(app, page, &pts),
        Pick::FormField => forms::rect_picked(app, page, &pts),
        Pick::Signature => signatures::rect_picked(app, page, &pts),
        Pick::Legend => fill::legend_picked(app, page, &pts),
    }
}

/// The rectangle spanned by the first two points (any order).
pub(crate) fn rect_of(pts: &[Point]) -> Option<markupcraft_geom::Rect> {
    let (a, b) = (pts.first()?, pts.get(1)?);
    let r = markupcraft_geom::Rect::new(a.x, a.y, b.x, b.y).normalized();
    (r.width() >= 2.0 && r.height() >= 2.0).then_some(r)
}

/// A colour picker for a model colour.
pub(crate) fn color_edit(ui: &mut egui::Ui, c: &mut markupcraft_model::Color) -> bool {
    let mut rgb = [c.r as f32, c.g as f32, c.b as f32];
    let r = ui.color_edit_button_rgb(&mut rgb);
    if r.changed() {
        *c = markupcraft_model::Color::rgb(f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2]));
        return true;
    }
    false
}

/// A modal-like feature window, centred, closable.
pub(crate) fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(egui::pos2(700.0, 380.0))
}

/// Page text field: "" = all pages, else a range like `1-3, 5` (1-based).
pub(crate) fn pages_field(ui: &mut egui::Ui, text: &mut String) {
    ui.horizontal(|ui| {
        ui.label("Pages:");
        ui.add(egui::TextEdit::singleline(text).desired_width(120.0).hint_text("all"));
    });
}

/// The pages a pages field names (0-based); `None` = not a valid range.
pub(crate) fn parse_pages(text: &str, count: usize) -> Option<Vec<usize>> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("all") {
        return Some((0..count).collect());
    }
    markupcraft_measure::units::parse_page_range(t, count).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_commands_are_consistent() {
        let mut ids = std::collections::HashSet::new();
        for c in crate::commands::COMMANDS.iter().chain(COMMANDS) {
            assert!(ids.insert(c.id), "duplicate command {}", c.id);
            assert!(c.icon.is_empty() || crate::icons::exists(c.icon), "icon {}", c.icon);
            assert!(
                c.menu.is_empty() || crate::commands::MENUS.contains(&c.menu),
                "menu {}",
                c.menu
            );
        }
        for c in COMMANDS {
            assert!(handles(c.id));
        }
        assert_eq!(parse_pages("", 3), Some(vec![0, 1, 2]));
    }

    /// Revu 21's default keys (docs/revu_features/05_shortcuts.md) for these features.
    #[test]
    fn feature_shortcuts_are_bound() {
        use crate::commands::bindings;
        let k = |ctrl: bool, shift: bool, alt: bool, key: Key| Keys::new(ctrl, shift, alt, key);
        let (c, s, a, n) = (true, true, true, false);
        #[rustfmt::skip]
        let expected: &[(Keys, &str)] = &[
            (k(n, n, n, Key::F7), "markup.spell"), (k(n, s, n, Key::H), "markup.hyperlink"),
            (k(n, n, n, Key::J), "measure.dynamic_fill"), (k(c, n, n, Key::P), "file.print"),
            (k(c, n, n, Key::F), "tools.search"), (k(c, n, n, Key::B), "document.add_bookmark"),
            (k(n, s, n, Key::A), "document.apply_redactions"), (k(c, s, n, Key::M), "document.flatten"),
            (k(n, s, n, Key::R), "document.mark_redaction"), (k(c, s, n, Key::O), "tools.ocr"),
            (k(c, n, n, Key::L), "document.security"),
            (k(n, n, a, Key::Y), "panel.layers"), (k(n, n, a, Key::N), "panel.links"),
            (k(n, n, a, Key::Num1), "panel.search"), (k(n, n, a, Key::Num2), "panel.sets"),
            (k(n, n, a, Key::Num4), "panel.signatures"), (k(n, n, a, Key::S), "panel.spaces"),
        ];
        let b = bindings();
        for (keys, id) in expected {
            assert!(
                b.iter().any(|(bk, bid)| bk == keys && bid == id),
                "{} should run {id}",
                keys.label()
            );
        }
        assert_eq!(parse_pages("2", 3), Some(vec![1]));
        assert_eq!(parse_pages("9", 3), None);
    }
}
