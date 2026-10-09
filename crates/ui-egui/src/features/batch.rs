//! Batch dialogs over many files: Combine, Batch Link, Batch Summary, Batch Flatten, and Slip
//! Sheet (the active document's sheets replaced by a newer revision's).

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::Session;
use markupcraft_engine::batch::{BatchLinkOptions, SlipSheetOptions, batch_link, batch_summary_csv};
use markupcraft_engine::combine::combine_files;
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
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Kind::Combine => "Combine Files",
            Kind::Link => "Batch Link",
            Kind::Summary => "Batch Summary",
            Kind::Flatten => "Batch Flatten",
            Kind::SlipSheet => "Slip Sheet",
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
    pub slip: SlipSheetOptions,
    pub message: String,
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
            slip: SlipSheetOptions {
                append_unmatched: true,
                ..Default::default()
            },
            message: String::new(),
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
    let (mut open, mut add, mut go) = (true, false, false);
    super::window(kind.title()).open(&mut open).default_width(500.0).show(ctx, |ui| {
        let b = &mut app.features.batch;
        ui.label(match kind {
            Kind::Combine => "Combine these PDFs, in order, into one new PDF.",
            Kind::Link => "Link sheet references: wherever a page shows another sheet's number, it links to that sheet. Files are saved in place.",
            Kind::Summary => "The Markups List of every file in one CSV (a File column first).",
            Kind::Flatten => "Flatten every markup of these files into their pages. Files are saved in place.",
            Kind::SlipSheet => "Replace the active document's sheets with the matching sheets (by page label) of a newer revision; markups stay.",
        });
        let mut action = None;
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for (i, f) in b.files.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(file_name(f)).on_hover_text(f.display().to_string());
                    if kind == Kind::Combine {
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
        match kind {
            Kind::Combine => {
                ui.checkbox(&mut b.bookmarks, "A bookmark for each file");
            }
            Kind::Link => {
                ui.checkbox(&mut b.link.match_case, "Match case");
                ui.horizontal(|ui| {
                    ui.label("Border width");
                    ui.add(egui::DragValue::new(&mut b.link.width).range(0.0..=12.0));
                    super::color_edit(ui, &mut b.link.color);
                });
            }
            Kind::Summary => {
                ui.checkbox(&mut b.measurements_only, "Measurements only");
            }
            Kind::Flatten => {}
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
        app.dialogs
            .open(Purpose::Feature(Ask::BatchFiles), PDF, kind != Kind::SlipSheet);
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
        Kind::Summary => app
            .dialogs
            .save(Purpose::Feature(Ask::BatchOut), CSV, "Batch Summary.csv"),
        Kind::Link => {
            let b = &mut app.features.batch;
            b.message = actions::report(batch_link(&b.files, &b.link), |r| {
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
        Kind::Combine => match combine_files(&b.files, out, b.bookmarks) {
            Ok(n) => {
                app.features.batch.message =
                    format!("Combined into {} ({})", out.display(), actions::plural(n, "page"));
                app.features.batch.open = false;
                app.open_path(out);
            }
            Err(e) => app.features.batch.message = e.to_string(),
        },
        Kind::Summary => {
            let (csv, n, errors) = batch_summary_csv(&b.files, b.measurements_only);
            let mut msg = match crate::chest::write_atomic(out, csv.as_bytes()) {
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
        _ => {}
    }
}
