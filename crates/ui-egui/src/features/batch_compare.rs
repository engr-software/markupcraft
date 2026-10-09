//! Batch > Compare Documents and Batch > Overlay Pages: a wizard that pairs current sheets
//! with their revisions (files, folders, open documents; matched by file name + page, page
//! label or a title-block region, with a wildcard filter), shows the pairs to re-pair (drag a
//! revised sheet onto another row) or remove, saves and reopens the batch file, runs every
//! pair and lists the differences, with a CSV or PDF report.

use std::path::{Path, PathBuf};

use egui::{RichText, Sense};
use markupcraft_engine::batch_compare::{
    BatchJob, BatchReport, MatchBy, SheetPair, SheetRef, batch_compare, batch_overlay, batch_overlay_colors,
    collect_pdfs, load_job, match_sheets, report_csv, report_pdf, save_job, stamp_now,
};
use markupcraft_engine::compare::CompareOptions;
use markupcraft_engine::overlay::OverlayAlign;

use super::Ask;
use crate::dialogs::{CSV, PDF, Purpose};
use crate::{AppState, actions};

pub const BATCH: crate::dialogs::Filter = ("MarkupCraft batch", &["pcbatch", "json"]);

pub struct BatchCompareState {
    pub open: bool,
    /// Overlay instead of compare.
    pub overlay: bool,
    pub job: BatchJob,
    pub recursive: bool,
    /// 0 file + page, 1 label, 2 region.
    pub match_by: usize,
    pub region: [f64; 4],
    pub unmatched: (usize, usize),
    pub sensitivity: f64,
    pub out_dir: Option<PathBuf>,
    pub report: Option<BatchReport>,
    pub message: String,
}

impl Default for BatchCompareState {
    fn default() -> Self {
        Self {
            open: false,
            overlay: false,
            job: BatchJob::default(),
            recursive: true,
            match_by: 0,
            region: [500.0, 20.0, 760.0, 120.0],
            unmatched: (0, 0),
            sensitivity: 0.5,
            out_dir: None,
            report: None,
            message: String::new(),
        }
    }
}

fn name(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn sheet(s: &SheetRef) -> String {
    format!("{} p{}", name(&s.file), s.page + 1)
}

/// Commands of this file.
pub fn run(app: &mut AppState, id: &str) -> bool {
    let b = &mut app.features.batch_compare;
    match id {
        "batch.compare" => {
            b.open = true;
            b.overlay = false;
        }
        "batch.overlay" => {
            b.open = true;
            b.overlay = true;
        }
        _ => return false,
    }
    true
}

/// A file or folder answer for the wizard.
pub fn answer(app: &mut AppState, ask: &Ask, paths: &[PathBuf]) {
    let b = &mut app.features.batch_compare;
    let first = paths.first().cloned().unwrap_or_default();
    match ask {
        Ask::BatchCmpCurrent | Ask::BatchCmpRevised => match collect_pdfs(paths, b.recursive) {
            Ok(files) => {
                let list = if *ask == Ask::BatchCmpCurrent {
                    &mut b.job.current
                } else {
                    &mut b.job.revised
                };
                for f in files {
                    if !list.contains(&f) {
                        list.push(f);
                    }
                }
            }
            Err(e) => b.message = e.to_string(),
        },
        Ask::BatchCmpOut => {
            b.out_dir = Some(first);
            run_batch(app);
        }
        Ask::BatchCmpJobSave => {
            b.message = actions::report(save_job(&first, &b.job), |_| format!("Saved {}", first.display()));
        }
        Ask::BatchCmpJobOpen => match load_job(&first) {
            Ok(j) => {
                b.match_by = match j.matching {
                    MatchBy::PageLabel => 1,
                    MatchBy::Region { rect } => {
                        b.region = rect;
                        2
                    }
                    _ => 0,
                };
                b.job = j;
                b.message = format!("Opened {}", first.display());
            }
            Err(e) => b.message = e.to_string(),
        },
        Ask::BatchCmpReport => {
            let Some(r) = &b.report else { return };
            let stamp = stamp_now();
            let pdf = first.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            let res = if pdf {
                report_pdf(r, &first, (792.0, 612.0), Some(&stamp))
            } else {
                crate::chest::write_atomic(&first, report_csv(r, Some(&stamp)).as_bytes())
                    .map_err(|e| markupcraft_engine::EngineError::Invalid(e.to_string()))
            };
            b.message = actions::report(res, |_| format!("Report written to {}", first.display()));
        }
        _ => {}
    }
}

fn do_match(app: &mut AppState) {
    let b = &mut app.features.batch_compare;
    b.job.matching = match b.match_by {
        1 => MatchBy::PageLabel,
        2 => MatchBy::Region { rect: b.region },
        _ => MatchBy::FileAndPage,
    };
    match match_sheets(&b.job) {
        Ok((pairs, lc, lr)) => {
            b.message = format!(
                "{} matched; {} current and {} revised sheets matched nothing",
                actions::plural(pairs.len(), "pair"),
                lc.len(),
                lr.len()
            );
            b.unmatched = (lc.len(), lr.len());
            b.job.pairs = pairs;
        }
        Err(e) => b.message = e.to_string(),
    }
}

fn run_batch(app: &mut AppState) {
    let b = &mut app.features.batch_compare;
    let Some(dir) = b.out_dir.clone() else { return };
    let r = if b.overlay {
        batch_overlay(
            &b.job.pairs,
            batch_overlay_colors(),
            OverlayAlign::Page,
            &dir,
            "_overlay",
        )
    } else {
        let o = CompareOptions {
            sensitivity: b.sensitivity,
            ..Default::default()
        };
        batch_compare(&b.job.pairs, &o, &dir, "_compared")
    };
    match r {
        Ok(rep) => {
            b.message = format!(
                "{} written to {}; {} differences",
                actions::plural(rep.outputs.len(), "file"),
                dir.display(),
                rep.total_differences()
            );
            b.report = Some(rep);
        }
        Err(e) => b.message = e.to_string(),
    }
    app.status = app.features.batch_compare.message.clone();
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.batch_compare.open {
        return;
    }
    let open_files: Vec<PathBuf> = app.docs.iter().filter_map(|d| d.path.clone()).collect();
    let title = if app.features.batch_compare.overlay {
        "Batch Overlay Pages"
    } else {
        "Batch Compare Documents"
    };
    let mut ask: Option<(Ask, bool)> = None;
    let (mut open, mut matching, mut go, mut save, mut load, mut report) = (true, false, false, false, false, false);
    super::window(title)
        .open(&mut open)
        .default_width(640.0)
        .show(ctx, |ui| {
            let b = &mut app.features.batch_compare;
            ui.columns(2, |cols| {
                for (k, col) in cols.iter_mut().enumerate() {
                    let (label, list) = if k == 0 {
                        ("Current", &mut b.job.current)
                    } else {
                        ("Revised", &mut b.job.revised)
                    };
                    col.label(RichText::new(format!("{label} ({})", list.len())).strong());
                    let mut remove = None;
                    egui::ScrollArea::vertical()
                        .id_salt(label)
                        .max_height(110.0)
                        .show(col, |ui| {
                            for (i, f) in list.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(name(f)).on_hover_text(f.display().to_string());
                                    if ui.small_button("x").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                        });
                    if let Some(i) = remove {
                        list.remove(i);
                    }
                    col.horizontal(|ui| {
                        let (files, dir) = if k == 0 {
                            (Ask::BatchCmpCurrent, Ask::BatchCmpCurrent)
                        } else {
                            (Ask::BatchCmpRevised, Ask::BatchCmpRevised)
                        };
                        if ui.small_button("Add Files...").clicked() {
                            ask = Some((files, false));
                        }
                        if ui.small_button("Add Folder...").clicked() {
                            ask = Some((dir, true));
                        }
                        if ui.small_button("Open Files").clicked() {
                            for f in &open_files {
                                if !list.contains(f) {
                                    list.push(f.clone());
                                }
                            }
                        }
                    });
                }
            });
            ui.checkbox(&mut b.recursive, "Include subfolders");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Match pages by:");
                ui.selectable_value(&mut b.match_by, 0, "File name + page");
                ui.selectable_value(&mut b.match_by, 1, "Page label");
                ui.selectable_value(&mut b.match_by, 2, "Title-block region");
            });
            if b.match_by == 2 {
                ui.horizontal(|ui| {
                    ui.label("Region (PDF points):");
                    for v in b.region.iter_mut() {
                        ui.add(egui::DragValue::new(v).speed(1.0));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(
                    egui::TextEdit::singleline(&mut b.job.filter)
                        .desired_width(120.0)
                        .hint_text("e.g. @?#"),
                )
                .on_hover_text("# digits, @ letters, * non-digits, ? a separator, \\ escapes the next character");
                if ui.button("Match").clicked() {
                    matching = true;
                }
            });
            ui.label(RichText::new("Drag a revised sheet onto another row to re-pair; x removes a pair.").small());
            let mut swap = None;
            let mut remove = None;
            egui::ScrollArea::vertical()
                .id_salt("pairs")
                .max_height(180.0)
                .show(ui, |ui| {
                    egui::Grid::new("batch-pairs")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            for h in ["Current", "Revised", "Differences", ""] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for (i, p) in b.job.pairs.iter().enumerate() {
                                ui.label(sheet(&p.current));
                                let r = ui.add(egui::Label::new(sheet(&p.revised)).sense(Sense::click_and_drag()));
                                r.dnd_set_drag_payload(i);
                                if let Some(from) = r.dnd_release_payload::<usize>() {
                                    swap = Some((*from, i));
                                }
                                let diffs = b
                                    .report
                                    .as_ref()
                                    .and_then(|rep| {
                                        rep.results
                                            .iter()
                                            .find(|l| l.current == p.current && l.revised == p.revised)
                                    })
                                    .map(|l| {
                                        if l.error.is_empty() {
                                            l.differences.to_string()
                                        } else {
                                            l.error.clone()
                                        }
                                    })
                                    .unwrap_or_default();
                                ui.label(diffs);
                                if ui.small_button("x").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                });
            if let Some((a, z)) = swap {
                repair_swap(&mut b.job.pairs, a, z);
            }
            if let Some(i) = remove {
                b.job.pairs.remove(i);
            }
            if !b.overlay {
                ui.add(egui::Slider::new(&mut b.sensitivity, 0.0..=1.0).text("Sensitivity"));
            }
            if !b.message.is_empty() {
                ui.label(RichText::new(&b.message).small());
            }
            ui.horizontal(|ui| {
                if ui.button("Save Batch...").clicked() {
                    save = true;
                }
                if ui.button("Open Batch...").clicked() {
                    load = true;
                }
                ui.add_enabled_ui(!b.job.pairs.is_empty(), |ui| {
                    if ui.button("Run...").clicked() {
                        go = true;
                    }
                });
                ui.add_enabled_ui(b.report.is_some(), |ui| {
                    if ui.button("Report...").clicked() {
                        report = true;
                    }
                });
                if ui.button("Close").clicked() {
                    b.open = false;
                }
            });
        });
    if !open {
        app.features.batch_compare.open = false;
    }
    if let Some((a, folder)) = ask {
        if folder {
            app.dialogs.folder(Purpose::Feature(a));
        } else {
            app.dialogs.open(Purpose::Feature(a), PDF, true);
        }
    }
    if matching {
        do_match(app);
    }
    if save {
        app.dialogs
            .save(Purpose::Feature(Ask::BatchCmpJobSave), BATCH, "Batch.pcbatch");
    }
    if load {
        app.dialogs.open(Purpose::Feature(Ask::BatchCmpJobOpen), BATCH, false);
    }
    if go {
        app.dialogs.folder(Purpose::Feature(Ask::BatchCmpOut));
    }
    if report {
        app.dialogs
            .save(Purpose::Feature(Ask::BatchCmpReport), CSV, "Batch Report.csv");
    }
}

/// Re-pair: the revised sheets of rows `a` and `z` change places.
pub fn repair_swap(pairs: &mut [SheetPair], a: usize, z: usize) {
    if a == z || a >= pairs.len() || z >= pairs.len() {
        return;
    }
    let ra = pairs.get(a).map(|p| p.revised.clone());
    let rz = pairs.get(z).map(|p| p.revised.clone());
    if let (Some(ra), Some(rz)) = (ra, rz) {
        if let Some(p) = pairs.get_mut(a) {
            p.revised = rz;
        }
        if let Some(p) = pairs.get_mut(z) {
            p.revised = ra;
        }
    }
}
