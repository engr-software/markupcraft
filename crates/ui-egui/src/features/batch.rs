//! Batch dialogs over many files: Combine, Batch Link, Batch Summary, Batch Flatten, and Slip
//! Sheet (the active document's sheets replaced by a newer revision's).

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::batch::{
    BatchLinkOptions, LinkTerms, TermDest, TermTarget, batch_link, batch_summary_csv, link_terms_from_csv,
};
use markupcraft_engine::docs_more::{CombineOptions, combine_with, layered_pdf, merge_form_data};
use markupcraft_engine::jobs;

use super::Ask;
use crate::dialogs::{CSV, PDF, Purpose};
use crate::{AppState, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Combine,
    Link,
    Summary,
    Flatten,
    SlipSheet,
    Unflatten,
    Print,
    /// Create PDF from images and text files.
    Create,
    /// One page with each file's first page in its own layer.
    Layered,
    /// The form data of many PDFs in one CSV.
    FormMerge,
    /// Split every file into parts.
    Split,
    /// Run a script's tool steps on every file.
    Script,
    /// One stamp at one spot on many pages of many files.
    ApplyStamp,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Kind::Combine => "Combine Files",
            Kind::Link => "Batch Link",
            Kind::Summary => "Batch Summary",
            Kind::Flatten => "Batch Flatten",
            Kind::SlipSheet => "Slip Sheet",
            Kind::Unflatten => "Batch Unflatten",
            Kind::Print => "Batch Print",
            Kind::Create => "Create PDF from Files",
            Kind::Layered => "Create Layered PDF",
            Kind::FormMerge => "Merge Form Data",
            Kind::Split => "Batch Split",
            Kind::Script => "Batch Script",
            Kind::ApplyStamp => "Batch Apply Stamp",
        }
    }
}

pub struct BatchState {
    pub open: bool,
    pub kind: Kind,
    pub files: Vec<PathBuf>,
    pub bookmarks: bool,
    pub measurements_only: bool,
    pub link: BatchLinkOptions,
    /// Batch Link term source: 0 page labels, 1 file names, 2 custom terms.
    pub link_source: usize,
    /// Custom terms, one `term,file name,page` per line.
    pub link_custom: String,
    pub link_filter: String,
    pub link_highlight: bool,
    pub combine: CombineOptions,
    /// Slip Sheet, Combine ranges, Split naming, Batch Link runs, Apply Stamp.
    pub extra: super::batch_more::BatchMore,
    pub message: String,
    /// Batch Print: also send the sheets to the printer chosen in the Print dialog.
    pub print_send: bool,
    /// The list's further sources and the Split / Script / one-per-file options.
    pub more: super::batch_list::ListExtras,
}

impl Default for BatchState {
    fn default() -> Self {
        Self {
            open: false,
            kind: Kind::Combine,
            files: Vec::new(),
            bookmarks: true,
            measurements_only: false,
            link: BatchLinkOptions::default(),
            link_source: 0,
            link_custom: String::new(),
            link_filter: String::new(),
            link_highlight: false,
            combine: CombineOptions {
                bookmarks: true,
                ..Default::default()
            },
            extra: Default::default(),
            message: String::new(),
            print_send: false,
            more: Default::default(),
        }
    }
}

impl BatchState {
    pub fn open(&mut self, kind: Kind) {
        if self.kind != kind {
            self.files.clear();
        }
        self.kind = kind;
        self.open = true;
        self.message.clear();
    }

    pub fn add_files(&mut self, paths: &[PathBuf]) {
        for p in paths {
            if !self.files.contains(p) {
                self.files.push(p.clone());
            }
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.batch.open {
        return;
    }
    let kind = app.features.batch.kind;
    let has_doc = app.has_doc();
    let (mut open, mut add, mut go) = (true, false, false);
    let open_docs: Vec<PathBuf> = app.docs.iter().filter_map(|d| d.path.clone()).collect();
    let set_files = app.features.sets.set.files.clone();
    let mut want = None;
    let mut extra_want = None;
    super::window(kind.title()).open(&mut open).default_width(500.0).show(ctx, |ui| {
        let b = &mut app.features.batch;
        ui.label(match kind {
            Kind::Combine => "Combine these PDFs, in order, into one new PDF.",
            Kind::Link => "Link sheet references: wherever a page shows another sheet's number, it links to that sheet. Files are saved in place.",
            Kind::Summary => "The Markups List of every file in one CSV (a File column first).",
            Kind::Flatten => "Flatten every markup of these files into their pages. Files are saved in place.",
            Kind::SlipSheet => "Pair the active document's sheets with the sheets of the revised files and replace them, or insert the revisions ahead; markups come forward.",
            Kind::Unflatten => "Restore the markups flattened with recovery in these files. Files are saved in place.",
            Kind::Print => "Print these PDFs in list order with the Print dialog's settings: each laid out as a print-ready PDF in the folder you choose, then sent to the chosen printer.",
            Kind::Create => "Make one PDF from these files in order: images (PNG, JPEG, TIFF, BMP) and text files become pages, PDFs are appended.",
            Kind::Layered => "Draw the first page of each file on one page, each in its own layer named after its file.",
            Kind::FormMerge => "Collect the form field values of these PDFs into one CSV: a row per file.",
            Kind::Split => "Split each PDF into parts (every N pages, or at its top-level bookmarks) in a folder you choose.",
            Kind::Script => "Run a script's tool steps (a JSON list of {\"tool\", \"params\"}) on each PDF; files are saved in place.",
            Kind::ApplyStamp => "Place one stamp at the same spot on the chosen pages of each PDF; files are saved in place.",
        });

        let mut action = None;
        let mut moved = None;
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for (i, f) in b.files.iter().enumerate() {
                ui.horizontal(|ui| {
                    super::batch_list::row(ui, i, f, &mut moved);
                    if kind == Kind::Combine {
                        super::batch_more::range_cell(ui, &mut b.extra, f);
                    }
                    if matches!(kind, Kind::Combine | Kind::Print | Kind::Create | Kind::Layered) {
                        if ui.small_button("Up").clicked() {
                            action = Some((i, 0));
                        }
                        if ui.small_button("Down").clicked() {
                            action = Some((i, 1));
                        }
                    }
                    if ui.small_button("Remove").clicked() {
                        action = Some((i, 2));
                    }
                });
            }
        });
        match action {
            Some((i, 0)) if i > 0 => b.files.swap(i, i - 1),
            Some((i, 1)) if i + 1 < b.files.len() => b.files.swap(i, i + 1),
            Some((i, 2)) if i < b.files.len() => {
                b.files.remove(i);
            }
            _ => {}
        }
        if let Some((from, to)) = moved {
            super::batch_list::move_row(&mut b.files, from, to);
        }
        if ui
            .button(if kind == Kind::SlipSheet {
                "Add Revised Files..."
            } else {
                "Add Files..."
            })
            .clicked()
        {
            add = true;
        }
        if kind != Kind::SlipSheet {
            want = super::batch_list::sources_ui(ui, &mut b.files, &open_docs, &set_files, &mut b.more);
        }
        match kind {
            Kind::Combine => {
                ui.checkbox(&mut b.combine.bookmarks, "A bookmark for each file");
                ui.checkbox(&mut b.combine.attachments, "Keep every file's attachments");
                ui.checkbox(&mut b.combine.properties, "Merge document properties");
                ui.checkbox(&mut b.combine.layers, "Keep every file's layers");
                ui.checkbox(&mut b.combine.labels_from_names, "Page labels from file names");
            }
            Kind::Create => {
                ui.checkbox(&mut b.more.each, "One PDF per file");
                ui.add_enabled_ui(b.more.each, |ui| {
                    ui.radio_value(&mut b.more.beside_source, true, "Beside each source file");
                    ui.radio_value(&mut b.more.beside_source, false, "In a folder I choose");
                });
            }
            Kind::Layered | Kind::FormMerge => {}
            Kind::Split => {
                ui.horizontal(|ui| {
                    ui.label("Pages per part");
                    ui.add(egui::DragValue::new(&mut b.more.split_pages).range(0..=10_000));
                    ui.label(RichText::new("0 = at top-level bookmarks").weak());
                });
                super::batch_more::split_ui(ui, &mut b.extra);
            }
            Kind::ApplyStamp => {
                let ids: Vec<String> = markupcraft_engine::stamps::builtin_stamps()
                    .into_iter()
                    .map(|e| e.id)
                    .collect();
                super::batch_more::stamp_ui(ui, &mut b.extra, &ids);
            }
            Kind::Script => {
                ui.horizontal(|ui| {
                    let name = b.more.script.as_deref().map_or_else(|| "none".to_string(), file_name);
                    ui.label(format!("Script: {name}"));
                    if ui.button("Choose Script...").clicked() {
                        want = Some(super::batch_list::Want::Script);
                    }
                });
            }
            Kind::Link => {
                ui.horizontal(|ui| {
                    ui.label("Search for");
                    ui.selectable_value(&mut b.link_source, 0, "Page labels");
                    ui.selectable_value(&mut b.link_source, 1, "File names");
                    ui.selectable_value(&mut b.link_source, 2, "Custom terms");
                });
                if b.link_source == 2 {
                    ui.label(RichText::new("One term per line: term, file name, page").small());
                    ui.add(
                        egui::TextEdit::multiline(&mut b.link_custom)
                            .desired_rows(3)
                            .hint_text("DETAIL 5, A-501.pdf, 1"),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Filter at");
                    ui.add(egui::TextEdit::singleline(&mut b.link_filter).desired_width(30.0));
                    ui.selectable_value(&mut b.link.keep_start, true, "keep before");
                    ui.selectable_value(&mut b.link.keep_start, false, "keep after");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut b.link.match_case, "Match case");
                    ui.checkbox(&mut b.link.full_paths, "Full paths");
                    ui.checkbox(&mut b.link.replace_existing, "Replace existing links");
                    ui.checkbox(&mut b.link_highlight, "Highlight links");
                });
                ui.horizontal(|ui| {
                    ui.label("Border width");
                    ui.add(egui::DragValue::new(&mut b.link.width).range(0.0..=12.0));
                    super::color_edit(ui, &mut b.link.color);
                });
                extra_want = super::batch_more::link_ui(ui, &mut b.link, b.link_highlight);
            }
            Kind::Summary => {
                ui.checkbox(&mut b.measurements_only, "Measurements only");
            }
            Kind::Flatten | Kind::Unflatten => {}
            Kind::Print => {
                ui.checkbox(&mut b.print_send, "Send to the printer");
            }
            Kind::SlipSheet => super::batch_more::slip_ui(ui, &mut b.extra),
        }
        if !b.message.is_empty() {
            ui.label(RichText::new(&b.message).small());
        }
        ui.horizontal(|ui| {
            let ready = match kind {
                Kind::Combine => b.files.len() >= 2,
                Kind::SlipSheet => !b.files.is_empty() && has_doc,
                Kind::Script => !b.files.is_empty() && b.more.script.is_some(),
                _ => !b.files.is_empty(),
            };
            ui.add_enabled_ui(ready, |ui| {
                if ui.button("Run").clicked() {
                    go = true;
                }
            });
            if ui.button("Close").clicked() {
                b.open = false;
            }
        });
    });
    if !open {
        app.features.batch.open = false;
    }
    if add {
        let filter = if kind == Kind::Create { super::ANY } else { PDF };
        app.dialogs.open(Purpose::Feature(Ask::BatchFiles), filter, true);
    }
    if let Some(e) = extra_want {
        super::batch_more::ask(app, e);
    }
    if want == Some(super::batch_list::Want::Folder) {
        app.dialogs.folder(Purpose::Feature(Ask::BatchFolder));
    }
    match want {
        Some(super::batch_list::Want::SaveList) => {
            app.dialogs
                .save(Purpose::Feature(Ask::BatchListSave), super::JSON, "File List.json");
        }
        Some(super::batch_list::Want::LoadList) => {
            app.dialogs
                .open(Purpose::Feature(Ask::BatchListLoad), super::JSON, false);
        }
        Some(super::batch_list::Want::Script) => {
            app.dialogs.open(Purpose::Feature(Ask::BatchScript), super::JSON, false);
        }
        _ => {}
    }
    if go {
        run(app);
    }
}

/// Run the batch (asking where to write when it makes a file).
pub fn run(app: &mut AppState) {
    let kind = app.features.batch.kind;
    let more = app.features.batch.more.clone();
    match kind {
        Kind::Combine => {
            let b = &mut app.features.batch;
            match super::batch_more::combine_pages(&b.extra, &b.files) {
                Ok(p) => {
                    b.combine.pages = p;
                    app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Combined.pdf");
                }
                Err(e) => b.message = e,
            }
        }
        Kind::ApplyStamp => {
            let b = &mut app.features.batch;
            b.message = super::batch_more::stamp_all(&b.files, &b.extra, None);
            app.status = b.message.clone();
        }
        Kind::Create if more.each && more.beside_source => {
            let pictures = app.shell.prefs.more.import_export.image_to_pdf();
            let built = jobs::create_each_job(app.features.batch.files.clone(), None, pictures);
            super::jobs::submit(app, "Create PDF from Files (one per file)", built);
        }
        Kind::Create if more.each => app.dialogs.folder(Purpose::Feature(Ask::BatchOut)),
        Kind::Split => app.dialogs.folder(Purpose::Feature(Ask::BatchOut)),
        Kind::Script => {
            let Some(script) = more.script.clone() else { return };
            let b = &mut app.features.batch;
            b.message = match super::batch_list::run_script(&b.files, &script) {
                Ok((n, errors)) => {
                    let mut m = format!("Ran the script on {}", actions::plural(n, "file"));
                    if !errors.is_empty() {
                        m.push_str(&format!("; {}", errors.join("; ")));
                    }
                    m
                }
                Err(e) => e,
            };
        }
        Kind::Create => app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Created.pdf"),
        Kind::Layered => app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Layered.pdf"),
        Kind::FormMerge => app.dialogs.save(Purpose::Feature(Ask::BatchOut), CSV, "Form Data.csv"),
        Kind::Summary => app
            .dialogs
            .save(Purpose::Feature(Ask::BatchOut), CSV, "Batch Summary.csv"),
        Kind::Print => app.dialogs.folder(Purpose::Feature(Ask::BatchOut)),
        Kind::Unflatten | Kind::Flatten => {
            let unflatten = kind == Kind::Unflatten;
            let built = jobs::flatten_job(app.features.batch.files.clone(), unflatten);
            super::jobs::submit(app, kind.title(), built);
        }
        Kind::Link => {
            let b = &mut app.features.batch;
            let o = link_options(b);
            if matches!(&o.terms, LinkTerms::Targets(l) if l.is_empty()) {
                b.message =
                    "Custom terms: one line per term, as term, file name, page (the file must be in the list)".into();
                return;
            }
            b.message = actions::report(batch_link(&b.files, &o), |r| {
                let mut s = format!(
                    "Added {} in {} ({} already linked)",
                    actions::plural(r.links, "link"),
                    actions::plural(r.files.iter().filter(|f| f.1 > 0).count(), "file"),
                    r.existing
                );
                if !r.errors.is_empty() {
                    s.push_str(&format!("; {}", r.errors.join("; ")));
                }
                s
            });
            app.status = app.features.batch.message.clone();
        }

        Kind::SlipSheet => super::batch_more::slip_run(app),
    }
}

/// The Batch Link options the dialog runs with: its terms (custom lines are
/// `term, file, page[, Place[, URL]]`), filter and highlight.
pub fn link_options(b: &BatchState) -> BatchLinkOptions {
    let mut o = b.link.clone();
    o.terms = match b.link_source {
        1 => LinkTerms::FileNames,
        2 => {
            let mut list: Vec<TermTarget> = link_terms_from_csv(&b.link_custom, &b.files);
            if list.is_empty() {
                list = markupcraft_engine::batch::link_terms_csv(&b.link_custom, &b.files)
                    .into_iter()
                    .map(|(term, file, page)| TermTarget {
                        term,
                        dest: TermDest::Page { file, page },
                    })
                    .collect();
            }
            LinkTerms::Targets(list)
        }
        _ => LinkTerms::PageLabels,
    };
    o.filter_char = b.link_filter.trim().chars().next();
    o.highlight = b
        .link_highlight
        .then_some(o.highlight.unwrap_or(markupcraft_model::Color::rgb(1.0, 1.0, 0.0)));
    o
}

/// Where a batch writes was chosen.
pub fn output(app: &mut AppState, out: &Path) {
    let b = &app.features.batch;
    match b.kind {
        Kind::Create if b.more.each => {
            let pictures = app.shell.prefs.more.import_export.image_to_pdf();
            let built = jobs::create_each_job(b.files.clone(), Some(out.to_path_buf()), pictures);
            super::jobs::submit(app, "Create PDF from Files (one per file)", built);
        }
        Kind::Create => {
            let pictures = app.shell.prefs.more.import_export.image_to_pdf();
            let built = jobs::create_combined_job(b.files.clone(), out.to_path_buf(), pictures);
            super::jobs::submit(app, "Create PDF from Files", built);
        }
        Kind::Split => {
            let (n, errors) = super::batch_more::split_all(&b.files, out, b.more.split_pages, &b.extra);
            let mut m = format!("Split into {} in {}", actions::plural(n, "part"), out.display());
            if !errors.is_empty() {
                m.push_str(&format!("; {}", errors.join("; ")));
            }
            app.features.batch.message = m;
        }
        Kind::Layered | Kind::FormMerge => {
            let r = match b.kind {
                Kind::Layered => layered_pdf(&b.files, out)
                    .map(|n| format!("Layered PDF {} ({})", out.display(), actions::plural(n, "page"))),
                _ => merge_form_data(&b.files, out).map(|n| {
                    format!(
                        "Form data of {} written to {}",
                        actions::plural(n, "file"),
                        out.display()
                    )
                }),
            };
            let open_it = b.kind != Kind::FormMerge;
            match r {
                Ok(m) => {
                    app.features.batch.message = m.clone();
                    app.status = m;
                    if open_it {
                        app.features.batch.open = false;
                        app.open_path(out);
                    }
                }
                Err(e) => app.features.batch.message = e.to_string(),
            }
        }
        Kind::Combine => match combine_with(&b.files, out, &b.combine) {
            Ok((n, _warnings)) => {
                app.features.batch.message =
                    format!("Combined into {} ({})", out.display(), actions::plural(n, "page"));
                app.features.batch.open = false;
                app.open_path(out);
            }
            Err(e) => app.features.batch.message = e.to_string(),
        },
        Kind::Summary => {
            let excel = out.extension().is_some_and(|e| e.eq_ignore_ascii_case("xlsx"));
            let (bytes, n, errors) = if excel {
                markupcraft_engine::batch::batch_summary_xlsx(&b.files, b.measurements_only)
            } else {
                let (csv, n, errors) = batch_summary_csv(&b.files, b.measurements_only);
                (csv.into_bytes(), n, errors)
            };
            let mut msg = match crate::chest::write_atomic(out, &bytes) {
                Ok(()) => format!(
                    "Summary of {} written to {}",
                    actions::plural(n, "markup"),
                    out.display()
                ),
                Err(e) => format!("Could not write: {e}"),
            };
            if !errors.is_empty() {
                msg.push_str(&format!("; {}", errors.join("; ")));
            }
            app.features.batch.message = msg;
        }
        Kind::Print => {
            let job = {
                let count = 1;
                match app.features.print.job(0, count, None) {
                    Ok(j) => j,
                    Err(e) => {
                        app.features.batch.message = e;
                        return;
                    }
                }
            };
            let printer = app.features.print.printer.clone();
            let send = app.features.batch.print_send;
            let p = (!printer.is_empty()).then_some(printer.as_str());
            let r = markupcraft_engine::printing::batch_print(&b.files, &job, out, send.then_some(p));
            app.features.batch.message = actions::report(r, |list| {
                let ok = list.iter().filter(|x| x.1.is_ok()).count();
                let errs: Vec<String> = list.iter().filter_map(|x| x.1.as_ref().err().cloned()).collect();
                let mut m = format!("Printed {} into {}", actions::plural(ok, "file"), out.display());
                if !errs.is_empty() {
                    m.push_str(&format!("; {}", errs.join("; ")));
                }
                m
            });
        }
        _ => {}
    }
}
