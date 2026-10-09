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
pub mod batch_compare;
pub mod batch_list;
pub mod canvas;
pub mod compare;
pub mod docops;
pub mod docs5b;
pub mod export;
pub mod fill;
pub mod flatten_ui;
pub mod forms;
pub mod forms_more;
pub mod links;
pub mod more6;
pub mod ocr;
pub mod overlay;
pub mod partials;
pub mod partials_more;
pub mod partials_more2;
pub mod partials_more3;
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
    c("markup.image", "Image...", "Markup", 20, key(Key::I), ""),
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
    // wave 5B
    c("file.export_images", "Export Pages as Images...", "File", 22, None, ""),
    c("file.export_document", "Export to Word, Excel, PowerPoint...", "File", 22, None, ""),
    c("file.export_region", "Export Page Region to Excel", "File", 22, None, ""),
    c("document.repair", "Repair PDF", "Document", 24, None, ""),
    c("document.pdfa", "Archive as PDF/A...", "Document", 24, None, ""),
    c("document.color_processing", "Color Processing...", "Document", 24, None, ""),
    c("document.unflatten", "Unflatten", "Document", 24, ctrl_shift(Key::U), ""),
    c("batch.compare", "Compare Documents...", "Batch", 22, None, "columns-3"),
    c("batch.overlay", "Overlay Pages...", "Batch", 22, None, "layers"),
    c("batch.unflatten", "Unflatten...", "Batch", 22, None, ""),
    c("batch.print", "Print...", "Batch", 22, None, ""),
    c("search.next", "Next Result", "Tools", 23, key(Key::F3), ""),
    c("search.prev", "Previous Result", "Tools", 23, shift(Key::F3), ""),
    c("tools.search_selection", "Search Selected Text", "Tools", 23, None, "search"),
    c("forms.signature_field", "Add Signature Field", "Tools", 23, key(Key::X), ""),
    c("forms.editor", "Form Editor", "Tools", 23, ctrl_shift(Key::F), ""),
    c("markup.edit_action", "Edit Action...", "Markup", 22, ctrl_shift(Key::E), ""),
    c("markup.file_attachment", "File Attachment...", "Markup", 22, None, "file-plus"),
    c("markup.capture_summary", "Capture Summary...", "Markup", 22, None, ""),
    c("document.links_from_urls", "Create Hyperlinks from URLs", "Document", 25, None, ""),
    c("file.create_from_files", "Create PDF from Files...", "File", 23, None, ""),
    c("file.layered", "Create Layered PDF...", "File", 23, None, "layers"),
    c("forms.export_data", "Export Form Data...", "Tools", 24, None, ""),
    c("forms.import_data", "Import Form Data...", "Tools", 24, None, ""),
    c("forms.merge_data", "Merge Form Data...", "Tools", 24, None, ""),
    c("forms.typewriter_to_fields", "Migrate Typewriter Text to Fields", "Tools", 24, None, ""),
    c("forms.auto_fields", "Automatically Create Form Fields", "Tools", 24, None, ""),
    c("document.redaction_properties", "Redaction Properties...", "Document", 22, None, ""),
    c("document.hf_update", "Update Header & Footer", "Document", 21, None, ""),
    c("measure.legend_copy", "Copy Legend to Pages", "Measure", 21, None, ""),
    c("measure.legend_freeze", "Snapshot Legend", "Measure", 21, None, ""),
    c("measure.quantity_link", "Quantity Link...", "Measure", 21, None, "file-spreadsheet"),
    // wave 6B
    c("file.new_from_template", "New PDF from Template...", "File", 23, None, ""),
    c("file.email_templates", "Email Templates...", "File", 23, None, ""),
    c("tools.add_shared_toolset", "Add Shared Tool Set...", "Tools", 25, None, ""),
    c("markup.profile_columns", "Profile Columns...", "Markup", 22, None, ""),
    c("measure.status_report", "Count Status Report...", "Measure", 21, None, ""),
    c("edit.snapshot_cut", "Cut Snapshot", "Edit", 20, None, "scissors"),
    c("tools.digital_ids", "Digital IDs...", "Tools", 22, None, ""),
    c("tools.clear_certification", "Clear Certification", "Tools", 22, None, ""),
    c("batch.split", "Split...", "Batch", 22, None, ""),
    c("batch.script", "Run Script...", "Batch", 22, None, ""),
];

/// Panels these features add (for the Window menu keys see the panel rows).
pub fn handles(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
        || more6::handles(id)
        || matches!(
            id,
            "file.print" | "document.flatten" | "batch.summary" | "batch.flatten"
        )
}

/// Whether a feature command can run now.
pub fn enabled(app: &AppState, id: &str) -> bool {
    if more6::handles(id) {
        return more6::enabled(app, id);
    }
    match id {
        "file.combine"
        | "file.overlay"
        | "batch.link"
        | "batch.combine"
        | "batch.sets"
        | "batch.summary"
        | "batch.flatten"
        | "markup.stamps"
        | "tools.digital_id"
        | "batch.compare"
        | "batch.overlay"
        | "batch.unflatten"
        | "batch.print"
        | "file.create_from_files"
        | "file.layered"
        | "forms.merge_data"
        | "document.redaction_properties"
        | "file.new_from_template"
        | "batch.split"
        | "tools.add_shared_toolset"
        | "markup.profile_columns"
        | "tools.digital_ids"
        | "batch.script"
        | "file.email_templates" => true,
        _ => app.has_doc(),
    }
}

/// Run a feature command.
pub fn run(app: &mut AppState, id: &str, _ctx: &egui::Context) {
    if export::run(app, id)
        || batch_compare::run(app, id)
        || docs5b::run(app, id)
        || more6::run(app, id)
        || partials::run(app, id)
    {
        return;
    }
    let f = &mut app.features;
    match id {
        "file.print" => f.print.open(),
        "file.combine" | "batch.combine" => f.batch.open(batch::Kind::Combine),
        "batch.link" => f.batch.open(batch::Kind::Link),
        "batch.summary" => f.batch.open(batch::Kind::Summary),
        "batch.flatten" => f.batch.open(batch::Kind::Flatten),
        "batch.unflatten" => f.batch.open(batch::Kind::Unflatten),
        "batch.print" => f.batch.open(batch::Kind::Print),
        "batch.split" => f.batch.open(batch::Kind::Split),
        "batch.script" => f.batch.open(batch::Kind::Script),
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
        "search.next" => search::step(app, 1),
        "search.prev" => search::step(app, -1),
        "tools.search_selection" => {
            start_pick(app, Pick::SearchSelection, "Drag over the text to search for");
        }
        "forms.signature_field" => {
            app.features.forms.new_kind = forms::NewKind::Signature;
            start_pick(app, Pick::FormField, "Drag the signature field's box");
        }
        "forms.editor" => forms::open(app),
        "markup.edit_action" => links::edit_markup_action(app),
        "markup.file_attachment" => app
            .dialogs
            .open(Purpose::Feature(Ask::AttachmentMarkupFile), ANY, false),
        "markup.capture_summary" => app.dialogs.folder(Purpose::Feature(Ask::CaptureExportDir)),
        "document.links_from_urls" => links::from_urls(app),
        "edit.copy_page_snapshot" => {
            let Some(d) = app.doc_mut() else { return };
            let page = d.view.current;
            let r = d.session.snapshot_to_clipboard(page, None, None);
            app.status = crate::actions::report(r, |_| "Page copied as a snapshot: Ctrl+V pastes it".into());
        }
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
    let f = &mut app.features.bookmarks;
    app.status = crate::actions::report(r, |p| {
        // Selected with its title ready to type over, as the panel's Add does.
        f.also.clear();
        f.selected = Some(p);
        f.rename = title.clone();
        f.edit_now = true;
        format!("Bookmark added: {title}")
    });
    app.show_panel("bookmarks");
}

/// AutoMark: bookmarks from the text in the dragged title-block region, every page.
fn automark_picked(app: &mut AppState, pts: &[Point]) {
    let Some(r) = rect_of(pts) else { return };
    let Some(d) = app.doc_mut() else { return };
    let res = d.session.bookmarks_from_region(&[], r, true);
    app.status = crate::actions::report(res, |n| {
        format!("AutoMark made {}", crate::actions::plural(n, "bookmark"))
    });
    app.show_panel("bookmarks");
}

/// The File Attachment icon's place was clicked.
fn attachment_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(at) = pts.first().copied() else { return };
    let Some(file) = app.features.attach_file.take() else {
        return;
    };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.add_file_attachment(
        page,
        Point::new(at.x - 10.0, at.y - 10.0),
        &file,
        markupcraft_engine::capture::AttachIcon::Paperclip,
        "",
    );
    d.rerender(threads);
    app.status = crate::actions::report(r, |_| format!("Attached {}", file.display()));
}

/// Capture Summary: the attached files saved into a folder, with a summary CSV.
fn capture_export(app: &mut AppState, dir: &std::path::Path) {
    let Some(d) = app.doc() else { return };
    let csv = d.session.capture_summary_csv();
    let r = d.session.export_attachment_markups(dir).and_then(|files| {
        crate::chest::write_atomic(&dir.join("Capture Summary.csv"), csv.as_bytes())
            .map_err(|e| markupcraft_engine::EngineError::Invalid(e.to_string()))?;
        Ok(files.len())
    });
    app.status = crate::actions::report(r, |n| {
        format!("Exported {} and the capture summary", crate::actions::plural(n, "file"))
    });
}

/// Import Layer and Export Layer files.
fn layer_file(app: &mut AppState, ask: &Ask, path: &std::path::Path) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let msg = match ask {
        Ask::LayerImport => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Imported".into());
            let page = d.view.current;
            let r = d.session.import_layer(path, 0, page, &name);
            d.rerender(threads);
            crate::actions::report(r, |_| format!("Imported {name} as a layer"))
        }
        Ask::LayerExport(n) => crate::actions::report(d.session.export_layer(n, path), |_| {
            format!("Layer {n} exported to {}", path.display())
        }),
        _ => String::new(),
    };
    app.status = msg;
}

/// Bookmark structure and export files.
fn bookmark_file(app: &mut AppState, ask: &Ask, path: &std::path::Path) {
    use markupcraft_engine::bookmarks_more::{BookmarkExport, export_bookmarks, load_structure, save_structure};
    let Some(d) = app.doc_mut() else { return };
    let msg = match ask {
        Ask::BookmarkStructureSave => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let st = d.session.bookmark_structure(&name);
            crate::actions::report(save_structure(path, &st), |_| {
                format!("Structure saved: {}", path.display())
            })
        }
        Ask::BookmarkStructureApply => crate::actions::report(
            load_structure(path).and_then(|st| d.session.apply_bookmark_structure(&st)),
            |n| format!("Filed {}", crate::actions::plural(n, "bookmark")),
        ),
        _ => match d.path.clone() {
            Some(src) => crate::actions::report(export_bookmarks(&[src], path, &BookmarkExport::default()), |n| {
                format!("Exported {}", crate::actions::plural(n, "bookmark"))
            }),
            None => "Save the document first".into(),
        },
    };
    app.status = msg;
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
    ExportImagesDir,
    ExportDocOut,
    ExportRegionOut,
    BatchCmpCurrent,
    BatchCmpRevised,
    BatchCmpOut,
    BatchCmpJobSave,
    BatchCmpJobOpen,
    BatchCmpReport,
    BookmarkStructureSave,
    BookmarkStructureApply,
    BookmarkExport,
    AttachmentMarkupFile,
    CaptureExportDir,
    LayerImport,
    LayerExport(String),
    SetPublishPdf,
    SetPackageDir,
    SetPrintOut,
    FormDataOut,
    FormDataIn,
    QuantityOut,
    QuantityLinksSave,
    QuantityLinksOpen,
    BatchFolder,
    More6(more6::Ask6),
    BatchListSave,
    BatchListLoad,
    BatchScript,
    SharedToolSet,
    StatusReportPdf,
    StatusReportCsv,
    IdImport,
    IdExportCert,
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
        Ask::PrintOut => {
            if print::write(app, &first) {
                app.features.print.open = false;
            }
        }
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
        Ask::ExportImagesDir => export::images_to(app, &first),
        Ask::ExportDocOut => export::document_to(app, &first),
        Ask::ExportRegionOut => export::region_to(app, &first),
        Ask::BatchCmpCurrent
        | Ask::BatchCmpRevised
        | Ask::BatchCmpOut
        | Ask::BatchCmpJobSave
        | Ask::BatchCmpJobOpen
        | Ask::BatchCmpReport => batch_compare::answer(app, &ask, &paths),
        Ask::BookmarkStructureSave | Ask::BookmarkStructureApply | Ask::BookmarkExport => {
            bookmark_file(app, &ask, &first);
        }
        Ask::AttachmentMarkupFile => {
            app.features.attach_file = Some(first);
            start_pick(app, Pick::AttachmentAt, "Click where the attachment icon goes");
        }
        Ask::CaptureExportDir => capture_export(app, &first),
        Ask::LayerImport | Ask::LayerExport(_) => layer_file(app, &ask, &first),
        Ask::SetPublishPdf | Ask::SetPackageDir | Ask::SetPrintOut => sets::publish_file(app, &ask, &first),
        Ask::FormDataOut | Ask::FormDataIn => docs5b::form_data_file(app, &ask, &first),
        Ask::QuantityOut | Ask::QuantityLinksSave | Ask::QuantityLinksOpen => docs5b::quantity_file(app, &ask, &first),
        Ask::More6(a) => more6::answer(app, a, paths),
        Ask::BatchFolder => {
            match markupcraft_engine::search_more::folder_pdfs(&first, app.features.batch.more.recursive) {
                Ok(files) => app.features.batch.add_files(&files),
                Err(e) => app.features.batch.message = e.to_string(),
            }
        }
        Ask::BatchListSave => {
            let r = batch_list::save_list(&first, &app.features.batch.files);
            app.features.batch.message = crate::actions::report(r, |_| format!("List saved to {}", first.display()));
        }
        Ask::BatchListLoad => match batch_list::load_list(&first) {
            Ok(files) => app.features.batch.add_files(&files),
            Err(e) => app.features.batch.message = e.to_string(),
        },
        Ask::BatchScript => app.features.batch.more.script = Some(first),
        Ask::IdImport => partials_more3::ids_file(app, true, &first),
        Ask::IdExportCert => partials_more3::ids_file(app, false, &first),
        Ask::StatusReportPdf => partials_more2::status_file(app, true, &first),
        Ask::StatusReportCsv => partials_more2::status_file(app, false, &first),
        Ask::SharedToolSet => {
            app.status = match app.toolchest.add_shared(&first) {
                Ok(_) => format!(
                    "Shared tool set added from {}: read-only until checked out",
                    first.display()
                ),
                Err(e) => e,
            };
            app.show_panel("toolchest");
        }
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
    /// The region to export to Excel.
    ExportRegion,
    /// Text to search for (Search Selected Text).
    SearchSelection,
    /// AutoMark: the title-block region.
    AutoMark,
    /// Where a File Attachment icon goes.
    AttachmentAt,
    /// Print > Get Window.
    PrintRegion,
    /// Dynamic Fill by dragging across regions (repeats).
    FillDrag,
    /// Dynamic Fill > Add Boundary: a polyline.
    FillBoundary,
    /// Wave 6A's picks.
    More6(more6::Pick6),
    /// A viewport's calibration: drag between the ends of a known length.
    ViewportCalibrate,
    /// Snapshot > Cut: the region to cut.
    SnapshotCut,
}

impl Pick {
    pub fn kind(self) -> PickKind {
        match self {
            Pick::More6(p) => p.kind(),
            Pick::Fill | Pick::SpaceFill | Pick::Legend | Pick::AttachmentAt => PickKind::Point,
            Pick::Space | Pick::FillBoundary => PickKind::Polygon,
            Pick::Stamp => PickKind::PointOrRect,
            _ => PickKind::Rect,
        }
    }

    /// Picks that stay on after each answer (Esc ends them).
    pub fn repeats(self) -> bool {
        matches!(self, Pick::Fill | Pick::Redact | Pick::FillDrag) || matches!(self, Pick::More6(p) if p.repeats())
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
    pub export: export::ExportState,
    /// File Attachment: the file waiting for its place.
    pub attach_file: Option<PathBuf>,
    pub batch_compare: batch_compare::BatchCompareState,
    pub layers: crate::panels::layers::Fields,
    pub bookmarks: crate::panels::bookmarks::Fields,
    pub quantity: docs5b::QuantityState,
    pub more6: more6::More6State,
    pub partials: partials::PartialsState,
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
    docs5b::window(app, ctx);
    partials::window(app, ctx);
    forms::window(app, ctx);
    spell::window(app, ctx);
    stamps::window(app, ctx);
    batch::window(app, ctx);
    fill::window(app, ctx);
    links::window(app, ctx);
    signatures::window(app, ctx);
    spaces::window(app, ctx);
    export::window(app, ctx);
    batch_compare::window(app, ctx);
    more6::window(app, ctx);
    canvas::publish(app, ctx);
}

/// A pick finished.
fn picked(app: &mut AppState, what: Pick, page: usize, pts: Vec<Point>) {
    match what {
        Pick::VisualRegion => search::region_picked(app, page, &pts),
        Pick::Fill | Pick::SpaceFill | Pick::FillDrag => fill::picked(app, what, page, &pts),
        Pick::FillBoundary => fill::boundary_picked(app, pts),
        Pick::Space => spaces::outline_picked(app, page, pts),
        Pick::Hyperlink => links::rect_picked(app, page, &pts),
        Pick::Redact => redact::rect_picked(app, page, &pts),
        Pick::Stamp => stamps::picked(app, page, &pts),
        Pick::FormField => forms::rect_picked(app, page, &pts),
        Pick::Signature => signatures::rect_picked(app, page, &pts),
        Pick::Legend => fill::legend_picked(app, page, &pts),
        Pick::ExportRegion => export::region_picked(app, page, &pts),
        Pick::SearchSelection => search::selection_picked(app, page, &pts),
        Pick::AutoMark => automark_picked(app, &pts),
        Pick::AttachmentAt => attachment_picked(app, page, &pts),
        Pick::PrintRegion => print::region_picked(app, page, &pts),
        Pick::More6(p) => more6::picked(app, p, page, &pts),
        Pick::ViewportCalibrate => crate::viewports_more::picked(app, &pts),
        Pick::SnapshotCut => partials_more2::cut_picked(app, page, &pts),
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
