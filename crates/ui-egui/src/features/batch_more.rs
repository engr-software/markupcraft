//! More of the batch dialogs: Slip Sheet matching / apply / leftover options, Batch Link
//! destinations, highlight style, term table and saved runs, Combine page ranges, Split
//! naming and size, and Batch Apply Stamp.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::batch::{
    BatchLinkOptions, BatchLinkRun, HighlightStyle, batch_link_terms, link_terms_to_csv, load_link_run, save_link_run,
};
use markupcraft_engine::batch_compare::MatchBy;
use markupcraft_engine::slip::{Leftovers, SlipRun, SlipStatus, slip_report_csv, slip_report_pdf};
use serde_json::{Value, json};

use super::Ask;
use crate::dialogs::{CSV, Purpose};
use crate::{AppState, actions};

/// What a batch dialog's extra file dialog is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    LinkTermsCsv,
    LinkRunSave,
    LinkRunOpen,
}

pub const RUN: crate::dialogs::Filter = ("Batch Link run", &["xml", "pcset"]);

/// The dialogs' extra state.
pub struct BatchMore {
    /// Slip Sheet: 0 page label, 1 file name + page, 2 region.
    pub slip_match: usize,
    pub slip_region: [f64; 4],
    /// 0 append, 1 skip, 2 extract (beside the document).
    pub slip_leftovers: usize,
    pub slip_reports: bool,
    pub slip: SlipRun,
    /// Combine: each file's page range ("" = all).
    pub ranges: HashMap<PathBuf, String>,
    /// Split: size limit in MB (0 = by pages), prefix / suffix, bookmark names, subfolder,
    /// links, layers.
    pub split_mb: f64,
    pub split_prefix: String,
    pub split_suffix: String,
    pub split_bookmark_names: bool,
    pub split_subfolder: bool,
    pub split_links: bool,
    pub split_layers: bool,
    /// Apply Stamp.
    pub stamp_id: String,
    pub stamp_pages: String,
    pub stamp_filter: usize,
    pub stamp_anchor: usize,
    pub stamp_offset: [f64; 2],
    pub stamp_scale: f64,
    pub stamp_rotation: f64,
}

impl Default for BatchMore {
    fn default() -> Self {
        Self {
            slip_match: 0,
            slip_region: [400.0, 20.0, 600.0, 80.0],
            slip_leftovers: 0,
            slip_reports: false,
            slip: SlipRun::default(),
            ranges: HashMap::new(),
            split_mb: 0.0,
            split_prefix: String::new(),
            split_suffix: String::new(),
            split_bookmark_names: false,
            split_subfolder: false,
            split_links: false,
            split_layers: false,
            stamp_id: "Approved".into(),
            stamp_pages: String::new(),
            stamp_filter: 0,
            stamp_anchor: 4,
            stamp_offset: [0.0, 0.0],
            stamp_scale: 1.0,
            stamp_rotation: 0.0,
        }
    }
}

const FILTERS: [&str; 5] = ["all", "odd", "even", "portrait", "landscape"];
const ANCHORS: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

/// Slip Sheet options.
pub fn slip_ui(ui: &mut egui::Ui, m: &mut BatchMore) {
    ui.horizontal(|ui| {
        ui.label("Match by");
        ui.selectable_value(&mut m.slip_match, 0, "Page label");
        ui.selectable_value(&mut m.slip_match, 1, "File name + page");
        ui.selectable_value(&mut m.slip_match, 2, "Region");
    });
    if m.slip_match == 2 {
        ui.horizontal(|ui| {
            ui.label("Region (points)");
            for v in &mut m.slip_region {
                ui.add(egui::DragValue::new(v).range(0.0..=14_400.0));
            }
        });
    }
    let r = &mut m.slip;
    ui.horizontal(|ui| {
        ui.label("Match the part before");
        ui.add(
            egui::TextEdit::singleline(&mut r.number_filter)
                .desired_width(50.0)
                .hint_text(" - "),
        );
        ui.label("Wildcard filter");
        ui.add(
            egui::TextEdit::singleline(&mut r.filter)
                .desired_width(80.0)
                .hint_text("@?#"),
        );
    });
    ui.label(RichText::new("Wildcards: # digits, @ letters, * non-digits, ? a separator").small());
    ui.checkbox(&mut r.match_case, "Match case");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut r.insert_ahead, false, "Replace the old sheets");
        ui.selectable_value(&mut r.insert_ahead, true, "Insert revisions ahead");
    });
    ui.checkbox(&mut r.carry_markups, "Copy markups to the revisions");
    ui.add_enabled_ui(r.insert_ahead, |ui| {
        ui.checkbox(&mut r.superseded, "Stamp old sheets Superseded");
        ui.checkbox(&mut r.redirect_links, "Redirect links and bookmarks to the revisions");
    });
    if !r.insert_ahead {
        r.superseded = false;
    }
    ui.checkbox(&mut r.unflatten_first, "Unflatten markups first");
    ui.checkbox(&mut r.flatten_after, "Flatten markups after");
    ui.horizontal(|ui| {
        ui.label("Unmatched new sheets");
        ui.selectable_value(&mut m.slip_leftovers, 0, "Append");
        ui.selectable_value(&mut m.slip_leftovers, 1, "Leave out");
        ui.selectable_value(&mut m.slip_leftovers, 2, "Extract to files");
    });
    ui.checkbox(&mut m.slip_reports, "Write a CSV and a PDF report beside the document");
}

/// Run Slip Sheet on the active document with the revisions `files`.
pub fn slip_run(app: &mut AppState) {
    let threads = app.threads;
    let files = app.features.batch.files.clone();
    let m = &app.features.batch.extra;
    let mut run = m.slip.clone();
    run.new_files = files;
    run.matching = match m.slip_match {
        1 => MatchBy::FileAndPage,
        2 => MatchBy::Region { rect: m.slip_region },
        _ => MatchBy::PageLabel,
    };
    let (leftovers, reports) = (m.slip_leftovers, m.slip_reports);
    // Preferences > Document: links to a slip-sheeted page follow the new sheet, or stay put.
    run.redirect_links = app.shell.ui.extra2.redirect_slip_links;
    let Some(d) = app.doc_mut() else { return };
    let doc_path = d.session.path().to_path_buf();
    let folder = doc_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = doc_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into());
    run.leftovers = match leftovers {
        1 => Leftovers::Skip,
        2 => Leftovers::Extract(folder.join(format!("{stem} unmatched sheets"))),
        _ => Leftovers::Append,
    };
    let r = d.session.slip_sheet_run(&run);
    d.sync_pages(threads);
    d.rerender(threads);
    let msg = actions::report(r, |r| {
        let mut s = format!(
            "Slip-sheeted {}, appended {}; {} old and {} new sheets did not match",
            actions::plural(r.count(SlipStatus::Matched), "sheet"),
            r.appended,
            r.count(SlipStatus::OldUnmatched),
            r.count(SlipStatus::NewUnmatched)
        );
        if r.superseded > 0 {
            s.push_str(&format!("; {} stamped Superseded", r.superseded));
        }
        if reports {
            let csv = folder.join(format!("{stem} Slip Sheet Report.csv"));
            let pdf = folder.join(format!("{stem} Slip Sheet Report.pdf"));
            let w = std::fs::write(&csv, slip_report_csv(&r))
                .map_err(|e| e.to_string())
                .and_then(|_| slip_report_pdf(&r, &pdf).map_err(|e| e.to_string()));
            match w {
                Ok(()) => s.push_str(&format!("; report in {}", pdf.display())),
                Err(e) => s.push_str(&format!("; the report was not written: {e}")),
            }
        }
        s
    });
    app.status = msg.clone();
    app.features.batch.message = msg;
}

/// Batch Link: highlight style, overlap handling, term table and saved runs.
pub fn link_ui(ui: &mut egui::Ui, o: &mut BatchLinkOptions, highlight: bool) -> Option<Extra> {
    let mut want = None;
    ui.add_enabled_ui(highlight, |ui| {
        ui.horizontal(|ui| {
            ui.label("Highlight style");
            for s in [HighlightStyle::Fill, HighlightStyle::Outline, HighlightStyle::Highlight] {
                ui.selectable_value(&mut o.highlight_style, s, s.name());
            }
            ui.checkbox(&mut o.flatten_highlight, "Flatten highlights");
        });
    });
    ui.checkbox(
        &mut o.add_overlapping,
        "Keep existing links and add the new ones beside them",
    );
    ui.label(
        RichText::new("Custom terms may go to a page (term, file, page), a Place (term, file, , Place) or a web address (term, , , , URL)")
            .small(),
    );
    ui.horizontal(|ui| {
        if ui.button("Export Terms...").clicked() {
            want = Some(Extra::LinkTermsCsv);
        }
        if ui.button("Save Run...").clicked() {
            want = Some(Extra::LinkRunSave);
        }
        if ui.button("Load Run...").clicked() {
            want = Some(Extra::LinkRunOpen);
        }
    });
    want
}

/// Ask for the file an extra button needs.
pub fn ask(app: &mut AppState, e: Extra) {
    let p = Purpose::Feature(Ask::BatchExtra(e));
    match e {
        Extra::LinkTermsCsv => app.dialogs.save(p, CSV, "Batch Link Terms.csv"),
        Extra::LinkRunSave => app.dialogs.save(p, RUN, "Batch Link.xml"),
        Extra::LinkRunOpen => app.dialogs.open(p, RUN, false),
    }
}

/// The options the Batch Link dialog would run with.
fn link_options(app: &AppState) -> BatchLinkOptions {
    super::batch::link_options(&app.features.batch)
}

/// A file was chosen for an extra button.
pub fn chosen(app: &mut AppState, e: Extra, path: &Path) {
    let files = app.features.batch.files.clone();
    let msg = match e {
        Extra::LinkTermsCsv => match batch_link_terms(&files, &link_options(app)) {
            Ok((terms, _)) => match std::fs::write(path, link_terms_to_csv(&terms, &files)) {
                Ok(()) => format!(
                    "Exported {} to {}",
                    actions::plural(terms.len(), "term"),
                    path.display()
                ),
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        },
        Extra::LinkRunSave => {
            let run = BatchLinkRun {
                files,
                options: link_options(app),
            };
            match save_link_run(path, &run) {
                Ok(()) => format!("Saved the run to {}", path.display()),
                Err(e) => e.to_string(),
            }
        }
        Extra::LinkRunOpen => match load_link_run(path) {
            Ok(run) => {
                let b = &mut app.features.batch;
                b.files = run.files;
                b.link_highlight = run.options.highlight.is_some();
                b.link_filter = run.options.filter_char.map(String::from).unwrap_or_default();
                b.link_source = match &run.options.terms {
                    markupcraft_engine::batch::LinkTerms::FileNames => 1,
                    markupcraft_engine::batch::LinkTerms::PageLabels => 0,
                    _ => 2,
                };
                if let markupcraft_engine::batch::LinkTerms::Targets(t) = &run.options.terms {
                    b.link_custom = link_terms_to_csv(t, &b.files)
                        .lines()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                b.link = run.options;
                format!("Loaded the run from {}", path.display())
            }
            Err(e) => e.to_string(),
        },
    };
    app.features.batch.message = msg;
}

/// A file row's page range box (Combine).
pub fn range_cell(ui: &mut egui::Ui, m: &mut BatchMore, file: &Path) {
    let r = m.ranges.entry(file.to_path_buf()).or_default();
    ui.add(egui::TextEdit::singleline(r).desired_width(70.0).hint_text("all pages"));
}

/// The page lists of the Combine ranges (None = all); an error names the bad range.
pub fn combine_pages(m: &BatchMore, files: &[PathBuf]) -> Result<Vec<Option<Vec<usize>>>, String> {
    let mut out = Vec::new();
    for f in files {
        let t = m.ranges.get(f).map(|s| s.trim().to_string()).unwrap_or_default();
        if t.is_empty() || t.eq_ignore_ascii_case("all") {
            out.push(None);
            continue;
        }
        let n = markupcraft_engine::pages::ForeignPdf::open(f)
            .map_err(|e| e.to_string())?
            .page_count();
        let p = markupcraft_automation::parse_range(&t, n).map_err(|e| format!("{}: {e}", f.display()))?;
        out.push(Some(p));
    }
    Ok(out)
}

/// Split naming and size options.
pub fn split_ui(ui: &mut egui::Ui, m: &mut BatchMore) {
    ui.horizontal(|ui| {
        ui.label("Or parts of at most");
        ui.add(egui::DragValue::new(&mut m.split_mb).range(0.0..=100_000.0).speed(0.1));
        ui.label(RichText::new("MB (0 = by pages)").weak());
    });
    ui.horizontal(|ui| {
        ui.label("Prefix");
        ui.add(egui::TextEdit::singleline(&mut m.split_prefix).desired_width(80.0));
        ui.label("Suffix");
        ui.add(egui::TextEdit::singleline(&mut m.split_suffix).desired_width(80.0));
    });
    ui.label(RichText::new("# in the prefix or suffix is the part number (### pads it)").small());
    ui.checkbox(&mut m.split_bookmark_names, "Name parts after their bookmarks");
    ui.checkbox(&mut m.split_subfolder, "Put the parts in a subfolder");
    ui.checkbox(&mut m.split_links, "Update links between the parts");
    ui.checkbox(&mut m.split_layers, "Remove layers a part does not use");
}

/// Split every file with the dialog's options. Returns (parts, errors).
pub fn split_all(files: &[PathBuf], dir: &Path, pages: u32, m: &BatchMore) -> (usize, Vec<String>) {
    let mut parts = 0;
    let mut errors = Vec::new();
    for f in files {
        let mut a = markupcraft_automation::Automation::new();
        let mut args = json!({
            "dir": dir.display().to_string(),
            "prefix": m.split_prefix, "suffix": m.split_suffix,
            "bookmark_names": m.split_bookmark_names, "subfolder": m.split_subfolder,
            "update_links": m.split_links, "drop_empty_layers": m.split_layers,
        });
        if let Some(o) = args.as_object_mut() {
            if m.split_mb > 0.0 {
                o.insert("by".into(), json!("size"));
                o.insert("max_mb".into(), json!(m.split_mb));
            } else if pages == 0 {
                o.insert("by".into(), json!("bookmarks"));
            } else {
                o.insert("pages_per_file".into(), json!(pages));
            }
        }
        let r = a
            .call("doc_open", &json!({ "path": f.display().to_string() }))
            .and_then(|_| a.call("doc_split", &args));
        match r {
            Ok(v) => parts += v.get("files").and_then(Value::as_array).map_or(0, Vec::len),
            Err(e) => errors.push(format!("{}: {e}", f.display())),
        }
    }
    (parts, errors)
}

/// Apply Stamp options.
pub fn stamp_ui(ui: &mut egui::Ui, m: &mut BatchMore, stamps: &[String]) {
    ui.horizontal(|ui| {
        ui.label("Stamp");
        egui::ComboBox::from_id_salt("batch_stamp_id")
            .selected_text(m.stamp_id.clone())
            .show_ui(ui, |ui| {
                for s in stamps {
                    ui.selectable_value(&mut m.stamp_id, s.clone(), s);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Pages");
        ui.add(
            egui::TextEdit::singleline(&mut m.stamp_pages)
                .desired_width(80.0)
                .hint_text("all"),
        );
        for (i, f) in FILTERS.iter().enumerate() {
            ui.selectable_value(&mut m.stamp_filter, i, *f);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Anchor");
        egui::Grid::new("batch_stamp_anchor").show(ui, |ui| {
            for (i, a) in ANCHORS.iter().enumerate() {
                ui.selectable_value(&mut m.stamp_anchor, i, "o")
                    .on_hover_text(a.replace('_', " "));
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("X offset");
                ui.add(egui::DragValue::new(&mut m.stamp_offset[0]).range(-14_400.0..=14_400.0));
            });
            ui.horizontal(|ui| {
                ui.label("Y offset");
                ui.add(egui::DragValue::new(&mut m.stamp_offset[1]).range(-14_400.0..=14_400.0));
            });
        });
    });
    ui.horizontal(|ui| {
        ui.label("Scale");
        ui.add(egui::DragValue::new(&mut m.stamp_scale).range(0.1..=10.0).speed(0.05));
        ui.label("Rotation");
        ui.add(egui::DragValue::new(&mut m.stamp_rotation).range(-360.0..=360.0));
    });
    ui.label(RichText::new("Opacity, blend mode and lock follow the stamp settings.").small());
}

/// Apply the stamp to every file (saved in place). The tool reads the stamp settings from
/// `config`.
pub fn stamp_all(files: &[PathBuf], m: &BatchMore, config: Option<&Path>) -> String {
    let mut a = markupcraft_automation::Automation::new();
    if let Some(c) = config {
        a = a.with_config_dir(c.to_path_buf());
    }
    let mut args = json!({
        "stamp": m.stamp_id,
        "filter": FILTERS.get(m.stamp_filter).copied().unwrap_or("all"),
        "anchor": ANCHORS.get(m.stamp_anchor).copied().unwrap_or("center"),
        "offset_x": m.stamp_offset[0], "offset_y": m.stamp_offset[1],
        "scale": m.stamp_scale, "rotation": m.stamp_rotation,
    });
    if !m.stamp_pages.trim().is_empty()
        && let Some(o) = args.as_object_mut()
    {
        o.insert("pages".into(), json!(m.stamp_pages.trim()));
    }
    let list: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    match a.call(
        "batch_apply",
        &json!({ "files": list, "operations": [{ "tool": "stamp_apply", "args": args }] }),
    ) {
        Ok(v) => {
            let done = v.get("done").and_then(Value::as_u64).unwrap_or(0);
            let errors: Vec<String> = v
                .get("files")
                .and_then(Value::as_array)
                .map(|l| {
                    l.iter()
                        .filter_map(|f| f.get("error").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let mut s = format!("Stamped {}", actions::plural(done as usize, "file"));
            if !errors.is_empty() {
                s.push_str(&format!("; {}", errors.join("; ")));
            }
            s
        }
        Err(e) => e.to_string(),
    }
}
