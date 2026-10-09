//! The page dialogs' further options: Insert Pages from several files (each with its own page
//! range), blank pages ruled with a grid or copied from a template page, Extract Pages to one
//! file per page (named by page label, with or without overwriting), and Page Setup's content
//! placement (fit or scale, offsets, rotation, centring, binding margins and a border).

use std::path::PathBuf;

use egui::RichText;
use markupcraft_engine::finish::pages::{GridStyle, PageSetup};

use super::pages::{Kind, PageDialog};
use crate::dialogs::{self, Purpose};
use crate::{AppState, actions};

/// The extra options of a page dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct PageExtras {
    /// Insert Pages: the files, each with a page range ("" = all).
    pub files: Vec<(PathBuf, String)>,
    /// Insert Blank: rule a grid, its spacing (points) and dots instead of lines.
    pub grid: bool,
    pub grid_spacing: f64,
    pub grid_dots: bool,
    /// Insert Blank: copy the first page of this PDF instead.
    pub template: Option<PathBuf>,
    /// Extract: one file per page into a folder; name by page label; overwrite.
    pub each: bool,
    pub by_label: bool,
    pub overwrite: bool,
    /// Links between the extracted pages point to their new files.
    pub links: bool,
    /// Page Setup: place the content (else only the media size changes at the anchor).
    pub place: bool,
    /// Fit the content inside the margins (else `scale_pct`).
    pub fit: bool,
    pub scale_pct: f64,
    /// Offset, inches (x right, y up); rotation degrees counter-clockwise.
    pub offset: (f64, f64),
    pub rotation: f64,
    pub center: bool,
    /// Margins, inches: left, right, top, bottom.
    pub margins: [f64; 4],
    /// Border line width, points (0 = none).
    pub border: f64,
}

impl Default for PageExtras {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            grid: false,
            grid_spacing: 18.0,
            grid_dots: false,
            template: None,
            each: false,
            by_label: false,
            overwrite: false,
            links: true,
            place: false,
            fit: true,
            scale_pct: 100.0,
            offset: (0.0, 0.0),
            rotation: 0.0,
            center: true,
            margins: [0.0; 4],
            border: 0.0,
        }
    }
}

/// Insert Pages: the file list with per-file page ranges. True when "Add Files..." was clicked.
pub fn insert_files_ui(ui: &mut egui::Ui, ex: &mut PageExtras) -> bool {
    ui.label("Files (pages of each: a range like 1-3, 7, or empty for all)");
    let mut remove = None;
    let mut swap = None;
    for (i, (p, range)) in ex.files.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let name = p
                .file_name()
                .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
            ui.label(name).on_hover_text(p.display().to_string());
            ui.add(egui::TextEdit::singleline(range).hint_text("all").desired_width(80.0));
            if ui.small_button("Up").clicked() && i > 0 {
                swap = Some(i);
            }
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = swap {
        ex.files.swap(i, i - 1);
    }
    if let Some(i) = remove
        && i < ex.files.len()
    {
        ex.files.remove(i);
    }
    ui.button("Add Files...").clicked()
}

/// Insert Blank: grid and template. True when "Choose Template..." was clicked.
pub fn blank_ui(ui: &mut egui::Ui, ex: &mut PageExtras) -> bool {
    let mut pick = false;
    ui.horizontal(|ui| {
        ui.checkbox(&mut ex.grid, "Grid");
        ui.add_enabled_ui(ex.grid, |ui| {
            ui.add(
                egui::DragValue::new(&mut ex.grid_spacing)
                    .range(4.0..=288.0)
                    .suffix(" pt"),
            );
            ui.checkbox(&mut ex.grid_dots, "Dots");
        });
    });
    ui.horizontal(|ui| {
        let name = ex
            .template
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "none".to_string(), |n| n.to_string_lossy().into_owned());
        ui.label(format!("Template page: {name}"));
        if ui.small_button("Choose Template...").clicked() {
            pick = true;
        }
        if ex.template.is_some() && ui.small_button("Clear").clicked() {
            ex.template = None;
        }
    });
    pick
}

pub fn extract_ui(ui: &mut egui::Ui, ex: &mut PageExtras) {
    ui.checkbox(&mut ex.each, "One file per page (into a folder)");
    ui.add_enabled_ui(ex.each, |ui| {
        ui.checkbox(&mut ex.by_label, "Name the files by page label");
        ui.checkbox(&mut ex.overwrite, "Overwrite files of the same name");
        ui.checkbox(&mut ex.links, "Update links between the extracted pages");
    });
}

pub fn setup_ui(ui: &mut egui::Ui, ex: &mut PageExtras) {
    ui.checkbox(&mut ex.place, "Place the content on the new size");
    if !ex.place {
        return;
    }
    ui.horizontal(|ui| {
        ui.radio_value(&mut ex.fit, true, "Fit to the page");
        ui.radio_value(&mut ex.fit, false, "Scale");
        ui.add_enabled(
            !ex.fit,
            egui::DragValue::new(&mut ex.scale_pct).range(5.0..=2000.0).suffix(" %"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Offset");
        ui.add(egui::DragValue::new(&mut ex.offset.0).speed(0.05).suffix(" in"));
        ui.add(egui::DragValue::new(&mut ex.offset.1).speed(0.05).suffix(" in"));
        ui.label("Rotation");
        ui.add(
            egui::DragValue::new(&mut ex.rotation)
                .range(-360.0..=360.0)
                .suffix(" deg"),
        );
    });
    ui.checkbox(&mut ex.center, "Centre the content");
    egui::Grid::new("setup-margins").num_columns(4).show(ui, |ui| {
        for (i, label) in ["Left", "Right", "Top", "Bottom"].iter().enumerate() {
            ui.label(*label);
            if let Some(v) = ex.margins.get_mut(i) {
                ui.add(egui::DragValue::new(v).range(0.0..=10.0).speed(0.05).suffix(" in"));
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Border");
        ui.add(egui::DragValue::new(&mut ex.border).range(0.0..=72.0).suffix(" pt"));
    });
    ui.label(RichText::new("Markups move with the content.").weak().small());
}

/// The dialog's operation when its extra options are in use; `None` = the basic operation runs.
pub fn apply(app: &mut AppState, dlg: &PageDialog, at: usize, pages: &[usize]) -> Option<Result<String, String>> {
    let threads = app.threads;
    let ex = &dlg.more;
    match dlg.kind {
        Kind::InsertPages if !ex.files.is_empty() => {
            let mut items = Vec::new();
            for (p, range) in &ex.files {
                let pages = if range.trim().is_empty() {
                    None
                } else {
                    let n = match markupcraft_engine::pages::ForeignPdf::open(p) {
                        Ok(f) => f.page_count(),
                        Err(e) => return Some(Err(e.to_string())),
                    };
                    match crate::pages_from_text(range, 0, n) {
                        Some(v) if !v.is_empty() => Some(v),
                        _ => return Some(Err(format!("{range}: no pages of {}", p.display()))),
                    }
                };
                items.push((p.clone(), pages));
            }
            let d = app.doc_mut()?;
            let r = d.session.insert_files(at, &items);
            d.sync_pages(threads);
            Some(r.map_err(|e| e.to_string()).map(|rep| {
                format!(
                    "Inserted {} from {}",
                    actions::plural(rep.pages_after.saturating_sub(rep.pages_before), "page"),
                    actions::plural(items.len(), "file")
                )
            }))
        }
        Kind::InsertBlank if ex.grid || ex.template.is_some() => {
            let grid = ex.grid.then_some(GridStyle {
                spacing: ex.grid_spacing,
                dots: ex.grid_dots,
                gray: 0.75,
            });
            let count = dlg.count.clamp(1, 500) as usize;
            let d = app.doc_mut()?;
            let r = d
                .session
                .insert_blank_styled(at, count, Some(dlg.size_pt()), grid, ex.template.as_deref());
            d.sync_pages(threads);
            Some(
                r.map_err(|e| e.to_string())
                    .map(|_| format!("Inserted {}", actions::plural(count, "page"))),
            )
        }
        Kind::Extract if ex.each => {
            let tag = format!(
                "extract-each:{}",
                pages.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
            );
            app.features.partials.extract_each = Some(dlg.clone());
            app.dialogs.folder(Purpose::Shell { tag });
            Some(Ok(String::new()))
        }
        Kind::PageSetup if ex.place => {
            let (w, h) = dlg.size_pt();
            let st = PageSetup {
                width: w,
                height: h,
                scale: (!ex.fit).then_some(ex.scale_pct / 100.0),
                offset: (ex.offset.0 * 72.0, ex.offset.1 * 72.0),
                rotation: ex.rotation,
                center: ex.center,
                margins: ex.margins.map(|m| m * 72.0),
                border: ex.border,
            };
            let d = app.doc_mut()?;
            let r = d.session.page_setup(pages, &st);
            d.sync_pages(threads);
            d.rerender(threads);
            Some(
                r.map_err(|e| e.to_string())
                    .map(|_| format!("Set up {}", actions::plural(pages.len(), "page"))),
            )
        }
        _ => None,
    }
}

/// A file dialog of these options was answered; true when `tag` was one of them.
pub fn answer(app: &mut AppState, tag: &str, paths: &[PathBuf]) -> bool {
    let threads = app.threads;
    match tag {
        "pages-insert-add" => {
            if let Some(d) = app.shell.page_dialog.as_mut() {
                for p in paths {
                    if !d.more.files.iter().any(|(f, _)| f == p) {
                        d.more.files.push((p.clone(), String::new()));
                    }
                }
            }
        }
        "pages-template" => {
            if let Some(d) = app.shell.page_dialog.as_mut() {
                d.more.template = paths.first().cloned();
            }
        }
        t if t.starts_with("extract-each:") => {
            let pages: Vec<usize> = t
                .trim_start_matches("extract-each:")
                .split(',')
                .filter_map(|v| v.parse().ok())
                .collect();
            let Some(dir) = paths.first().cloned() else { return true };
            let Some(dlg) = app.features.partials.extract_each.take() else {
                return true;
            };
            let ex = dlg.more;
            let Some(d) = app.doc_mut() else { return true };
            let stem = d.name.trim_end_matches(".pdf").to_string();
            let r = d.session.extract_each_linked(
                &pages,
                &dir,
                &stem,
                ex.by_label,
                ex.overwrite,
                dlg.delete_after,
                ex.links,
            );
            d.sync_pages(threads);
            app.status = actions::report(r, |v| {
                format!("Extracted {} to {}", actions::plural(v.len(), "file"), dir.display())
            });
        }
        _ => return false,
    }
    true
}

/// Ask for files for these options.
pub fn ask_insert_files(app: &mut AppState) {
    app.dialogs.open(
        Purpose::Shell {
            tag: "pages-insert-add".into(),
        },
        dialogs::PDF,
        true,
    );
}

pub fn ask_template(app: &mut AppState) {
    app.dialogs.open(
        Purpose::Shell {
            tag: "pages-template".into(),
        },
        dialogs::PDF,
        false,
    );
}
