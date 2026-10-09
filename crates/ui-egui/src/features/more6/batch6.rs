//! Batch > Sign & Seal and Batch > Smart Overlay.

use std::path::PathBuf;

use markupcraft_engine::Rect;
use markupcraft_engine::batch_compare::{BatchJob, collect_pdfs};
use markupcraft_engine::batch_sign::{BatchSign, DateText, SignOutcome, batch_sign};
use markupcraft_engine::smart_overlay::{SmartReport, smart_overlay};

use super::Ask6;
use crate::AppState;
use crate::dialogs::Purpose;
use crate::features::{Ask, IMAGES, P12};

pub struct Batch6State {
    pub sign_open: bool,
    pub files: Vec<PathBuf>,
    pub p12: Option<PathBuf>,
    pub password: String,
    pub field: String,
    /// 1-based; 0 = the last page.
    pub page: usize,
    /// The signature box, PDF points.
    pub rect: [f64; 4],
    pub visible: bool,
    pub seal: Option<PathBuf>,
    pub date: bool,
    pub date_format: String,
    /// 0 = approval signature, 1..=3 certify.
    pub certify: u8,
    pub reason: String,
    pub out_dir: Option<PathBuf>,
    pub suffix: String,
    pub results: Vec<SignOutcome>,
    pub smart_open: bool,
    pub current: Vec<PathBuf>,
    pub revised: Vec<PathBuf>,
    pub smart_out: Option<PathBuf>,
    pub shading: bool,
    pub report: Option<SmartReport>,
    pub message: String,
}

impl Default for Batch6State {
    fn default() -> Self {
        Self {
            sign_open: false,
            files: Vec::new(),
            p12: None,
            password: String::new(),
            field: String::new(),
            page: 0,
            rect: [400.0, 40.0, 570.0, 100.0],
            visible: true,
            seal: None,
            date: true,
            date_format: "yyyy-MM-dd".into(),
            certify: 0,
            reason: String::new(),
            out_dir: None,
            suffix: " signed".into(),
            results: Vec::new(),
            smart_open: false,
            current: Vec::new(),
            revised: Vec::new(),
            smart_out: None,
            shading: false,
            report: None,
            message: String::new(),
        }
    }
}

pub fn answer(app: &mut AppState, ask: &Ask6, paths: &[PathBuf]) {
    let Some(first) = paths.first().cloned() else { return };
    let st = &mut app.features.more6.batch;
    let pdfs = |p: &[PathBuf]| collect_pdfs(p, true).unwrap_or_default();
    match ask {
        Ask6::SignFiles => st.files.extend(pdfs(paths)),
        Ask6::SignP12 => st.p12 = Some(first),
        Ask6::SignSeal => st.seal = Some(first),
        Ask6::SignOutDir => st.out_dir = Some(first),
        Ask6::SmartCurrent => st.current.extend(pdfs(paths)),
        Ask6::SmartRevised => st.revised.extend(pdfs(paths)),
        Ask6::SmartOutDir => st.smart_out = Some(first),
        _ => {}
    }
}

fn run_sign(app: &mut AppState) {
    let st = &mut app.features.more6.batch;
    let Some(p12) = st.p12.clone() else {
        st.message = "Choose the digital ID first".into();
        return;
    };
    let id = match std::fs::metadata(&p12).map(|m| m.len()) {
        Ok(n) if n <= 1 << 20 => std::fs::read(&p12).map_err(|e| e.to_string()),
        Ok(_) => Err("the digital ID file is too large".into()),
        Err(e) => Err(e.to_string()),
    };
    let p12 = match id {
        Ok(b) => b,
        Err(e) => {
            st.message = e;
            return;
        }
    };
    let rect = Rect::new(st.rect[0], st.rect[1], st.rect[2], st.rect[3]);
    let job = BatchSign {
        p12,
        password: st.password.clone(),
        field: (!st.field.trim().is_empty()).then(|| st.field.trim().to_string()),
        page: st.page.checked_sub(1).unwrap_or(usize::MAX),
        rect: st.visible.then_some(rect),
        seal: st.seal.clone(),
        seal_rect: st
            .seal
            .as_ref()
            .map(|_| Rect::new(rect.x0, rect.y1 + 6.0, rect.x0 + 60.0, rect.y1 + 66.0)),
        date: st.date.then(|| DateText {
            format: st.date_format.clone(),
            rect: Rect::new(rect.x0 + 66.0, rect.y1 + 6.0, rect.x1, rect.y1 + 24.0),
        }),
        certify: (st.certify > 0).then_some(st.certify.min(3)),
        reason: (!st.reason.trim().is_empty()).then(|| st.reason.trim().to_string()),
        location: None,
        out_dir: st.out_dir.clone(),
        suffix: if st.out_dir.is_some() {
            st.suffix.clone()
        } else {
            String::new()
        },
    };
    match batch_sign(&st.files, &job) {
        Ok(r) => {
            let ok = r.iter().filter(|o| o.error.is_empty()).count();
            st.message = format!("Signed {} of {}", ok, crate::actions::plural(r.len(), "file"));
            st.results = r;
        }
        Err(e) => st.message = e.to_string(),
    }
}

fn run_smart(app: &mut AppState) {
    let st = &mut app.features.more6.batch;
    let Some(out) = st.smart_out.clone() else {
        st.message = "Choose a folder for the overlays".into();
        return;
    };
    let job = BatchJob {
        current: st.current.clone(),
        revised: st.revised.clone(),
        ..Default::default()
    };
    match smart_overlay(&job, &out, st.shading) {
        Ok(r) => {
            st.message = format!("Overlaid {}", crate::actions::plural(r.sheets.len(), "sheet"));
            st.report = Some(r);
        }
        Err(e) => st.message = e.to_string(),
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    sign_window(app, ctx);
    smart_window(app, ctx);
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sign_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.batch.sign_open {
        return;
    }
    let mut open = true;
    let mut ask: Option<(Ask6, bool)> = None;
    let mut go = false;
    let st = &mut app.features.more6.batch;
    crate::features::window("Batch Sign & Seal")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Files: {}", st.files.len()));
                if ui.button("Add Files...").clicked() {
                    ask = Some((Ask6::SignFiles, true));
                }
                if ui.small_button("Clear").clicked() {
                    st.files.clear();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Digital ID");
                ui.label(st.p12.as_deref().map(file_name).unwrap_or_else(|| "none".into()));
                if ui.button("Choose...").clicked() {
                    ask = Some((Ask6::SignP12, false));
                }
                ui.label("Password");
                ui.add(
                    egui::TextEdit::singleline(&mut st.password)
                        .password(true)
                        .desired_width(100.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Sign the field named");
                ui.add(
                    egui::TextEdit::singleline(&mut st.field)
                        .desired_width(120.0)
                        .hint_text("(none)"),
                );
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.visible, "Else a visible signature on page");
                ui.add(egui::DragValue::new(&mut st.page).range(0..=9999));
                ui.weak("(0 = last)");
            });
            ui.horizontal(|ui| {
                ui.label("Box x0 y0 x1 y1");
                for v in &mut st.rect {
                    ui.add(egui::DragValue::new(v).speed(1.0));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Seal image");
                ui.label(st.seal.as_deref().map(file_name).unwrap_or_else(|| "none".into()));
                if ui.button("Choose...").clicked() {
                    ask = Some((Ask6::SignSeal, false));
                }
                if st.seal.is_some() && ui.small_button("Remove").clicked() {
                    st.seal = None;
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.date, "Add the date");
                ui.add(egui::TextEdit::singleline(&mut st.date_format).desired_width(100.0));
            });
            ui.horizontal(|ui| {
                ui.label("Signature");
                ui.selectable_value(&mut st.certify, 0, "Approval");
                ui.selectable_value(&mut st.certify, 1, "Certify: no changes");
                ui.selectable_value(&mut st.certify, 2, "Certify: forms");
                ui.selectable_value(&mut st.certify, 3, "Certify: forms and comments");
            });
            ui.horizontal(|ui| {
                ui.label("Reason");
                ui.text_edit_singleline(&mut st.reason);
            });
            ui.horizontal(|ui| {
                ui.label("Save to");
                ui.label(
                    st.out_dir
                        .as_ref()
                        .map_or_else(|| "each file, in place".into(), |d| d.display().to_string()),
                );
                if ui.button("Folder...").clicked() {
                    ask = Some((Ask6::SignOutDir, false));
                }
                if st.out_dir.is_some() {
                    ui.label("suffix");
                    ui.add(egui::TextEdit::singleline(&mut st.suffix).desired_width(70.0));
                }
            });
            go = ui
                .add_enabled(!st.files.is_empty(), egui::Button::new("Sign & Seal"))
                .clicked();
            for r in &st.results {
                if r.error.is_empty() {
                    ui.label(format!("{}: signed ({})", file_name(&r.file), r.field));
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xB0, 0x20, 0x20),
                        format!("{}: {}", file_name(&r.file), r.error),
                    );
                }
            }
            if !st.message.is_empty() {
                ui.label(&st.message);
            }
        });
    app.features.more6.batch.sign_open = open;
    match ask {
        Some((a @ Ask6::SignFiles, _)) => app
            .dialogs
            .open(Purpose::Feature(Ask::More6(a)), crate::dialogs::PDF, true),
        Some((a @ Ask6::SignP12, _)) => app.dialogs.open(Purpose::Feature(Ask::More6(a)), P12, false),
        Some((a @ Ask6::SignSeal, _)) => app.dialogs.open(Purpose::Feature(Ask::More6(a)), IMAGES, false),
        Some((a, _)) => app.dialogs.folder(Purpose::Feature(Ask::More6(a))),
        None => {}
    }
    if go {
        run_sign(app);
    }
}

fn smart_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.batch.smart_open {
        return;
    }
    let mut open = true;
    let mut ask: Option<Ask6> = None;
    let mut go = false;
    let st = &mut app.features.more6.batch;
    crate::features::window("Smart Overlay").open(&mut open).show(ctx, |ui| {
        ui.label("Overlay whole sets sheet by sheet: each revised sheet is registered on its current sheet by matching the drawing, with a match score per sheet and per discipline.");
        ui.horizontal(|ui| {
            ui.label(format!("Current: {}", crate::actions::plural(st.current.len(), "sheet file")));
            if ui.button("Add...").clicked() {
                ask = Some(Ask6::SmartCurrent);
            }
            ui.label(format!("Revised: {}", crate::actions::plural(st.revised.len(), "sheet file")));
            if ui.button("Add...").clicked() {
                ask = Some(Ask6::SmartRevised);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Output folder");
            ui.label(st.smart_out.as_ref().map_or_else(|| "none".into(), |d| d.display().to_string()));
            if ui.button("Folder...").clicked() {
                ask = Some(Ask6::SmartOutDir);
            }
        });
        ui.checkbox(&mut st.shading, "Advanced color shading (keep grey tones and fills)");
        go = ui
            .add_enabled(!st.current.is_empty() && !st.revised.is_empty(), egui::Button::new("Overlay"))
            .clicked();
        if let Some(r) = &st.report {
            egui::Grid::new("smart-report").striped(true).show(ui, |ui| {
                ui.strong("Sheet");
                ui.strong("Discipline");
                ui.strong("Match");
                ui.end_row();
                for s in &r.sheets {
                    ui.label(&s.sheet);
                    ui.label(&s.discipline);
                    if s.error.is_empty() {
                        ui.label(format!("{:.0}%", s.score * 100.0));
                    } else {
                        ui.label(&s.error);
                    }
                    ui.end_row();
                }
                for (d, n, avg) in &r.disciplines {
                    ui.label(format!("{d} ({n})"));
                    ui.label("");
                    ui.label(format!("{:.0}%", avg * 100.0));
                    ui.end_row();
                }
            });
        }
        if !st.message.is_empty() {
            ui.label(&st.message);
        }
    });
    app.features.more6.batch.smart_open = open;
    match ask {
        Some(a @ Ask6::SmartOutDir) => app.dialogs.folder(Purpose::Feature(Ask::More6(a))),
        Some(a) => app
            .dialogs
            .open(Purpose::Feature(Ask::More6(a)), crate::dialogs::PDF, true),
        None => {}
    }
    if go {
        run_smart(app);
    }
}
