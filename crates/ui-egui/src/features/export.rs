//! File > Export (pages as images; the document as text, HTML, RTF, Word, Excel, PowerPoint; a
//! page region to Excel) and Document > Repair PDF, Archive as PDF/A, Color Processing and
//! Unflatten.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::archive::{ColorMode, PdfaLevel};
use markupcraft_engine::convert::{ImageExport, ImageFormat, OfficeFormat};
use markupcraft_geom::{Point, Rect};
use markupcraft_model::Color;

use super::{Ask, Pick, start_pick};
use crate::dialogs::Purpose;
use crate::{AppState, actions};

/// Which dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialog {
    #[default]
    None,
    Images,
    Document,
    Pdfa,
    Color,
}

pub struct ExportState {
    pub dialog: Dialog,
    pub image_format: usize,
    pub quality: u8,
    pub dpi: f64,
    pub suffix: String,
    pub pages: String,
    pub markups: bool,
    pub doc_format: usize,
    /// A region picked for Export to Excel.
    pub region: Option<(usize, Rect)>,
    pub pdfa_3b: bool,
    /// PDF/A-1b (PDF 1.4, no transparency) instead of 2b / 3b.
    pub pdfa_1b: bool,
    pub pdfa_report: Vec<String>,
    pub color_mode: usize,
    pub tint: Color,
    pub lighten: f64,
    pub message: String,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            dialog: Dialog::None,
            image_format: 0,
            quality: 85,
            dpi: 150.0,
            suffix: "_".into(),
            pages: String::new(),
            markups: true,
            doc_format: 3,
            region: None,
            pdfa_3b: false,
            pdfa_1b: false,
            pdfa_report: Vec::new(),
            color_mode: 0,
            tint: Color::rgb(0.2, 0.4, 0.8),
            lighten: 0.6,
            message: String::new(),
        }
    }
}

const IMAGE_FORMATS: [(&str, &str); 5] = [
    ("PNG", "png"),
    ("JPEG", "jpg"),
    ("TIFF", "tif"),
    ("BMP", "bmp"),
    ("GIF", "gif"),
];
const DOC_FORMATS: [(&str, &str); 6] = [
    ("Plain text", "txt"),
    ("HTML", "html"),
    ("Rich text (RTF)", "rtf"),
    ("Word", "docx"),
    ("Excel", "xlsx"),
    ("PowerPoint", "pptx"),
];

/// The filter for a document export format.
fn doc_filter(i: usize) -> crate::dialogs::Filter {
    match i {
        0 => ("Text", &["txt"]),
        1 => ("HTML", &["html"]),
        2 => ("RTF", &["rtf"]),
        4 => ("Excel", &["xlsx"]),
        5 => ("PowerPoint", &["pptx"]),
        _ => ("Word", &["docx"]),
    }
}

/// Commands of this file.
pub fn run(app: &mut AppState, id: &str) -> bool {
    let e = &mut app.features.export;
    match id {
        "file.export_images" => e.dialog = Dialog::Images,
        "file.export_document" => e.dialog = Dialog::Document,
        "file.export_region" => start_pick(app, Pick::ExportRegion, "Drag a box around the table to export"),
        "document.pdfa" => e.dialog = Dialog::Pdfa,
        "document.color_processing" => e.dialog = Dialog::Color,
        "document.repair" => repair(app),
        "document.unflatten" => unflatten(app),
        _ => return false,
    }
    true
}

fn stem(app: &AppState) -> String {
    app.doc()
        .and_then(|d| d.path.as_ref())
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .or_else(|| app.doc().map(|d| d.name.trim_end_matches(".pdf").to_string()))
        .unwrap_or_else(|| "Document".into())
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    let dialog = app.features.export.dialog;
    if dialog == Dialog::None {
        return;
    }
    let title = match dialog {
        Dialog::Images => "Export Pages as Images",
        Dialog::Document => "Export Document",
        Dialog::Pdfa => "Archive as PDF/A",
        Dialog::Color => "Color Processing",
        Dialog::None => "",
    };
    let (mut open, mut go, mut extra, mut unlock) = (true, false, false, false);
    super::window(title).open(&mut open).show(ctx, |ui| {
        let e = &mut app.features.export;
        match dialog {
            Dialog::Images => {
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    for (i, (n, _)) in IMAGE_FORMATS.iter().enumerate() {
                        ui.selectable_value(&mut e.image_format, i, *n);
                    }
                });
                if e.image_format == 1 {
                    ui.add(egui::Slider::new(&mut e.quality, 1..=100).text("JPEG quality"));
                }
                ui.add(egui::Slider::new(&mut e.dpi, 36.0..=600.0).text("dpi"));
                ui.horizontal(|ui| {
                    ui.label("File name suffix:");
                    ui.add(egui::TextEdit::singleline(&mut e.suffix).desired_width(60.0));
                });
                super::pages_field(ui, &mut e.pages);
                ui.checkbox(&mut e.markups, "Include markups");
            }
            Dialog::Document => {
                for (i, (n, _)) in DOC_FORMATS.iter().enumerate() {
                    ui.radio_value(&mut e.doc_format, i, *n);
                }
                super::pages_field(ui, &mut e.pages);
            }
            Dialog::Pdfa => {
                ui.label("Make the document a PDF/A archive (identification, colour profile, no forbidden actions); Verify only checks; Unlock removes the PDF/A claim so it can be edited.");
                ui.horizontal(|ui| {
                    if ui.selectable_label(e.pdfa_1b, "PDF/A-1b").clicked() {
                        e.pdfa_1b = true;
                        e.pdfa_3b = false;
                    }
                    if ui.selectable_label(!e.pdfa_1b && !e.pdfa_3b, "PDF/A-2b").clicked() {
                        e.pdfa_1b = false;
                        e.pdfa_3b = false;
                    }
                    if ui.selectable_label(!e.pdfa_1b && e.pdfa_3b, "PDF/A-3b").clicked() {
                        e.pdfa_1b = false;
                        e.pdfa_3b = true;
                    }
                });
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    for l in &e.pdfa_report {
                        ui.label(RichText::new(l).small());
                    }
                });
            }
            Dialog::Color => {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut e.color_mode, 0, "Grayscale");
                    ui.selectable_value(&mut e.color_mode, 1, "Tint");
                    ui.selectable_value(&mut e.color_mode, 2, "Lighten");
                });
                if e.color_mode == 1 {
                    super::color_edit(ui, &mut e.tint);
                }
                if e.color_mode == 2 {
                    ui.add(egui::Slider::new(&mut e.lighten, 0.0..=1.0).text("toward white"));
                }
                super::pages_field(ui, &mut e.pages);
            }
            Dialog::None => {}
        }
        if !e.message.is_empty() {
            ui.label(RichText::new(&e.message).small());
        }
        ui.horizontal(|ui| {
            let (ok, more) = match dialog {
                Dialog::Pdfa => ("Archive", Some("Verify")),
                Dialog::Color => ("Apply", Some("Remove")),
                _ => ("Export...", None),
            };
            if ui.button(ok).clicked() {
                go = true;
            }
            if let Some(m) = more
                && ui.button(m).clicked()
            {
                extra = true;
            }
            if dialog == Dialog::Pdfa && ui.button("Unlock").clicked() {
                unlock = true;
            }
            if ui.button("Close").clicked() {
                e.dialog = Dialog::None;
            }
        });
    });
    if !open {
        app.features.export.dialog = Dialog::None;
    }
    if unlock {
        pdfa_run(app, 2);
    }
    if go || extra {
        match dialog {
            Dialog::Images => app.dialogs.folder(Purpose::Feature(Ask::ExportImagesDir)),
            Dialog::Document => {
                let i = app.features.export.doc_format;
                let ext = DOC_FORMATS.get(i).map_or("docx", |f| f.1);
                let name = format!("{}.{ext}", stem(app));
                app.dialogs
                    .save(Purpose::Feature(Ask::ExportDocOut), doc_filter(i), &name);
            }
            Dialog::Pdfa => pdfa_run(app, if go { 0 } else { 1 }),
            Dialog::Color => color(app, extra),
            Dialog::None => {}
        }
    }
}

fn pages_of(app: &AppState) -> Option<Vec<usize>> {
    let count = app.doc().map_or(0, |d| d.session.page_count());
    super::parse_pages(&app.features.export.pages, count)
}

/// The folder for page images was chosen.
pub fn images_to(app: &mut AppState, dir: &Path) {
    let Some(pages) = pages_of(app) else {
        app.features.export.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
        return;
    };
    let stem = stem(app);
    let e = &app.features.export;
    let format = match e.image_format {
        1 => ImageFormat::Jpeg(e.quality),
        2 => ImageFormat::Tiff,
        3 => ImageFormat::Bmp,
        4 => ImageFormat::Gif,
        _ => ImageFormat::Png,
    };
    let o = ImageExport {
        format,
        dpi: e.dpi,
        pages: Some(pages),
        suffix: e.suffix.clone(),
        hide_markups: !e.markups,
    };
    let Some(d) = app.doc() else { return };
    let msg = actions::report(d.session.export_images(dir, &stem, &o), |f| {
        format!("Exported {} to {}", actions::plural(f.len(), "image"), dir.display())
    });
    app.status = msg.clone();
    app.features.export.message = msg;
}

/// Where the exported document goes was chosen.
pub fn document_to(app: &mut AppState, out: &Path) {
    let Some(pages) = pages_of(app) else {
        app.features.export.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
        return;
    };
    let format = OfficeFormat::from_path(out).or_else(|| {
        DOC_FORMATS
            .get(app.features.export.doc_format)
            .and_then(|f| OfficeFormat::from_name(f.1))
    });
    let Some(d) = app.doc() else { return };
    let msg = actions::report(d.session.export_document(out, format, Some(pages)), |n| {
        format!("Exported {} ({} KB)", out.display(), n / 1024)
    });
    app.status = msg.clone();
    app.features.export.message = msg;
}

/// The region to export was dragged.
pub fn region_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    app.features.export.region = Some((page, r));
    let name = format!("{} region.xlsx", stem(app));
    app.dialogs.save(
        Purpose::Feature(Ask::ExportRegionOut),
        ("Excel", &["xlsx", "csv"]),
        &name,
    );
}

/// Where the region goes was chosen.
pub fn region_to(app: &mut AppState, out: &Path) {
    let Some((page, rect)) = app.features.export.region.take() else {
        return;
    };
    let Some(d) = app.doc() else { return };
    app.status = actions::report(d.session.export_region(page, rect, out), |t| {
        format!(
            "Exported {} by {} to {}",
            actions::plural(t.rows.len(), "row"),
            actions::plural(t.columns(), "column"),
            out.display()
        )
    });
}

fn repair(app: &mut AppState) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let msg = actions::report(d.session.repair_pdf(), |r| {
        format!(
            "Repaired: {} read fixes, {} broken annotations and {} broken contents removed; save to rewrite the file ({} KB)",
            r.read_fixes.len(),
            r.bad_annotations,
            r.bad_contents,
            r.bytes_after / 1024
        )
    });
    d.rerender(threads);
    app.status = msg;
}

fn unflatten(app: &mut AppState) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let msg = actions::report(d.session.unflatten(&[]), |n| {
        format!("Unflattened {}", actions::plural(n, "markup"))
    });
    d.rerender(threads);
    app.status = msg;
}

/// 0 archive, 1 verify, 2 unlock.
fn pdfa_run(app: &mut AppState, what: u8) {
    let threads = app.threads;
    let level = if app.features.export.pdfa_3b {
        PdfaLevel::A3b
    } else {
        PdfaLevel::A2b
    };
    let one = app.features.export.pdfa_1b;
    let label = if one { "PDF/A-1b" } else { level.label() };
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let mut report = Vec::new();
    let msg = match what {
        0 => {
            let archived = if one {
                d.session.archive_pdfa1b()
            } else {
                d.session.archive_pdfa(level)
            };
            actions::report(archived, |r| {
                report.extend(r.fixed.iter().map(|f| format!("Fixed: {f}")));
                report.extend(r.remaining.iter().map(|i| format!("{} {}", i.clause, i.message)));
                if r.remaining.is_empty() {
                    format!("{label} ready; save to write it")
                } else {
                    format!("{} problems remain", r.remaining.len())
                }
            })
        }
        1 => {
            let v = if one {
                d.session.pdfa1b_verify()
            } else {
                d.session.pdfa_verify(level)
            };
            report.extend(v.iter().map(|i| format!("{} {}", i.clause, i.message)));
            if v.is_empty() {
                format!("The document conforms to {label}")
            } else {
                format!("{} problems", v.len())
            }
        }
        _ => actions::report(d.session.unlock_pdfa(), |had| {
            if had {
                "PDF/A identification removed".into()
            } else {
                "Not a PDF/A file".into()
            }
        }),
    };
    d.rerender(threads);
    let e = &mut app.features.export;
    e.pdfa_report = report;
    e.message = msg.clone();
    app.status = msg;
}

fn color(app: &mut AppState, remove: bool) {
    let threads = app.threads;
    let Some(pages) = pages_of(app) else {
        app.features.export.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
        return;
    };
    let e = &app.features.export;
    let mode = match e.color_mode {
        1 => ColorMode::Tint(e.tint),
        2 => ColorMode::Lighten(e.lighten),
        _ => ColorMode::Grayscale,
    };
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let msg = if remove {
        actions::report(d.session.color_process_remove(&pages), |n| {
            format!("Color Processing removed from {}", actions::plural(n, "page"))
        })
    } else {
        actions::report(d.session.color_process(&pages, mode), |n| {
            format!("Color Processing on {}", actions::plural(n, "page"))
        })
    };
    d.rerender(threads);
    app.features.export.message = msg.clone();
    app.status = msg;
}
