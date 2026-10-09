//! Batch dialogs over many files: Combine, Batch Link, Batch Summary, Batch Flatten, and Slip
//! Sheet (the active document's sheets replaced by a newer revision's).

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::Session;
use markupcraft_engine::batch::{
    BatchLinkOptions, LinkTerms, SlipSheetOptions, batch_link, batch_summary_csv, link_terms_csv,
};
use markupcraft_engine::docs_more::{
    CombineOptions, combine_with, create_pdf_from_files, layered_pdf, merge_form_data,
};
use markupcraft_engine::flatten::FlattenFilter;

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
    pub slip: SlipSheetOptions,
    pub message: String,
    /// Batch Print: also send the sheets to the printer chosen in the Print dialog.
    pub print_send: bool,
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
            slip: SlipSheetOptions {
                append_unmatched: true,
                ..Default::default()
            },
            message: String::new(),
            print_send: false,
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
            if self.kind == Kind::SlipSheet {
                self.files.clear();
            }
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
    let (mut open, mut add, mut go, mut add_folder) = (true, false, false, false);
    super::window(kind.title()).open(&mut open).default_width(500.0).show(ctx, |ui| {
        let b = &mut app.features.batch;
        ui.label(match kind {
            Kind::Combine => "Combine these PDFs, in order, into one new PDF.",
            Kind::Link => "Link sheet references: wherever a page shows another sheet's number, it links to that sheet. Files are saved in place.",
            Kind::Summary => "The Markups List of every file in one CSV (a File column first).",
            Kind::Flatten => "Flatten every markup of these files into their pages. Files are saved in place.",
            Kind::SlipSheet => "Replace the active document's sheets with the matching sheets (by page label) of a newer revision; markups stay.",
            Kind::Unflatten => "Restore the markups flattened with recovery in these files. Files are saved in place.",
            Kind::Print => "Print these PDFs in list order with the Print dialog's settings: each laid out as a print-ready PDF in the folder you choose, then sent to the chosen printer.",
            Kind::Create => "Make one PDF from these files in order: images (PNG, JPEG, TIFF, BMP) and text files become pages, PDFs are appended.",
            Kind::Layered => "Draw the first page of each file on one page, each in its own layer named after its file.",
            Kind::FormMerge => "Collect the form field values of these PDFs into one CSV: a row per file.",
        });
        let mut action = None;
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for (i, f) in b.files.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(file_name(f)).on_hover_text(f.display().to_string());
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
        if ui
            .button(if kind == Kind::SlipSheet {
                "Choose Revision..."
            } else {
                "Add Files..."
            })
            .clicked()
        {
            add = true;
        }
        if kind == Kind::Summary && ui.button("Add Folder...").on_hover_text("Every PDF in a folder and its subfolders").clicked() {
            add_folder = true;
        }
        match kind {
            Kind::Combine => {
                ui.checkbox(&mut b.combine.bookmarks, "A bookmark for each file");
                ui.checkbox(&mut b.combine.attachments, "Keep every file's attachments");
                ui.checkbox(&mut b.combine.properties, "Merge document properties");
                ui.checkbox(&mut b.combine.layers, "Keep every file's layers");
                ui.checkbox(&mut b.combine.labels_from_names, "Page labels from file names");
            }
            Kind::Create | Kind::Layered | Kind::FormMerge => {}
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
            }
            Kind::Summary => {
                ui.checkbox(&mut b.measurements_only, "Measurements only");
            }
            Kind::Flatten | Kind::Unflatten => {}
            Kind::Print => {
                ui.checkbox(&mut b.print_send, "Send to the printer");
            }
            Kind::SlipSheet => {
                ui.horizontal(|ui| {
                    ui.label("Match the label before");
                    ui.add(
                        egui::TextEdit::singleline(&mut b.slip.number_filter)
                            .desired_width(60.0)
                            .hint_text("e.g.  - "),
                    );
                });
                ui.checkbox(&mut b.slip.match_case, "Match case");
                ui.checkbox(&mut b.slip.append_unmatched, "Append new sheets that match nothing");
            }
        }
        if !b.message.is_empty() {
            ui.label(RichText::new(&b.message).small());
        }
        ui.horizontal(|ui| {
            let ready = match kind {
                Kind::Combine => b.files.len() >= 2,
                Kind::SlipSheet => b.files.len() == 1 && has_doc,
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
        app.dialogs
            .open(Purpose::Feature(Ask::BatchFiles), filter, kind != Kind::SlipSheet);
    }
    if add_folder {
        app.dialogs.folder(Purpose::Feature(Ask::BatchFolder));
    }
    if go {
        run(app);
    }
}

/// Run the batch (asking where to write when it makes a file).
pub fn run(app: &mut AppState) {
    let kind = app.features.batch.kind;
    match kind {
        Kind::Combine => app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Combined.pdf"),
        Kind::Create => app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Created.pdf"),
        Kind::Layered => app.dialogs.save(Purpose::Feature(Ask::BatchOut), PDF, "Layered.pdf"),
        Kind::FormMerge => app.dialogs.save(Purpose::Feature(Ask::BatchOut), CSV, "Form Data.csv"),
        Kind::Summary => app
            .dialogs
            .save(Purpose::Feature(Ask::BatchOut), CSV, "Batch Summary.csv"),
        Kind::Print => app.dialogs.folder(Purpose::Feature(Ask::BatchOut)),
        Kind::Unflatten => {
            let mut done = 0;
            let mut errors = Vec::new();
            for f in app.features.batch.files.clone() {
                let r = Session::open(&f).and_then(|mut s| {
                    let n = s.unflatten(&[])?;
                    s.save(true)?;
                    Ok(n)
                });
                match r {
                    Ok(n) => done += n,
                    Err(e) => errors.push(format!("{}: {e}", file_name(&f))),
                }
            }
            let b = &mut app.features.batch;
            b.message = format!("Unflattened {}", actions::plural(done, "markup"));
            if !errors.is_empty() {
                b.message.push_str(&format!("; {}", errors.join("; ")));
            }
        }
        Kind::Link => {
            let b = &mut app.features.batch;
            let mut o = b.link.clone();
            o.terms = match b.link_source {
                1 => LinkTerms::FileNames,
                2 => LinkTerms::Custom(link_terms_csv(&b.link_custom, &b.files)),
                _ => LinkTerms::PageLabels,
            };
            o.filter_char = b.link_filter.trim().chars().next();
            o.highlight = b.link_highlight.then_some(markupcraft_model::Color::rgb(1.0, 1.0, 0.0));
            if matches!(&o.terms, LinkTerms::Custom(l) if l.is_empty()) {
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
        Kind::Flatten => {
            let mut done = 0;
            let mut errors = Vec::new();
            for f in app.features.batch.files.clone() {
                let r = Session::open(&f).and_then(|mut s| {
                    let n = s.flatten_markups(&FlattenFilter::default())?;
                    if n > 0 {
                        s.save(false)?;
                    }
                    Ok(n)
                });
                match r {
                    Ok(n) => done += n,
                    Err(e) => errors.push(format!("{}: {e}", file_name(&f))),
                }
            }
            let b = &mut app.features.batch;
            b.message = format!("Flattened {}", actions::plural(done, "markup"));
            if !errors.is_empty() {
                b.message.push_str(&format!("; {}", errors.join("; ")));
            }
        }
        Kind::SlipSheet => {
            let threads = app.threads;
            let Some(f) = app.features.batch.files.first().cloned() else {
                return;
            };
            let opts = app.features.batch.slip.clone();
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.slip_sheet(&f, &opts);
            d.sync_pages(threads);
            d.rerender(threads);
            let msg = actions::report(r, |r| {
                format!(
                    "Replaced {}, appended {}; {} old and {} new sheets did not match",
                    actions::plural(r.matched.len(), "sheet"),
                    r.appended,
                    r.unmatched_old.len(),
                    r.unmatched_new.len()
                )
            });
            app.status = msg.clone();
            app.features.batch.message = msg;
        }
    }
}

/// Where a batch writes was chosen.
pub fn output(app: &mut AppState, out: &Path) {
    let b = &app.features.batch;
    match b.kind {
        Kind::Create | Kind::Layered | Kind::FormMerge => {
            let r = match b.kind {
                Kind::Create => create_pdf_from_files(&b.files, out)
                    .map(|n| format!("Created {} ({})", out.display(), actions::plural(n, "page"))),
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
