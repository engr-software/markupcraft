//! File > Print: the printer, pages (all, current, a range, the current view or a dragged
//! window), paper, orientation, scaling with margins and position, copies (collated, reverse
//! order), what prints (document and markups, document, markups) and emphasis (dimmed content
//! or filtered-out markups, Spaces, visible hyperlinks). Print lays the sheets out as a
//! print-ready PDF and hands it to the system's print command; Save Print PDF keeps the file.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::printing::{PrintJob, list_printers, send_to_printer};
use markupcraft_engine::printout::{PrintLayout, PrintSettings, paper_size};
use markupcraft_geom::{Point, Rect};

use super::Ask;
use crate::dialogs::{PDF, Purpose};
use crate::{AppState, actions};

pub const PAPERS: &[&str] = &[
    "Letter", "Legal", "Tabloid", "ANSI C", "ANSI D", "ANSI E", "ARCH A", "ARCH B", "ARCH C", "ARCH D", "ARCH E", "A4",
    "A3", "A2", "A1", "A0",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    #[default]
    All,
    Current,
    Pages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scaling {
    #[default]
    Fit,
    Actual,
    Shrink,
    Percent,
}

pub struct PrintState {
    pub open: bool,
    pub range: Range,
    pub pages: String,
    pub paper: String,
    pub orientation: Orientation,
    pub scaling: Scaling,
    pub percent: f64,
    pub markups: bool,
    pub message: String,
    /// The system's printers (read on a thread when the dialog opens) and the chosen one
    /// ("" = the default printer).
    pub printers: Vec<String>,
    pub printers_rx: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    pub printer: String,
    pub copies: usize,
    pub collate: bool,
    pub reverse: bool,
    pub markups_only: bool,
    /// Get Window: a dragged region of a page.
    pub region: Option<(usize, Rect)>,
    pub margin: f64,
    pub offset: [f64; 2],
    pub dim_content: bool,
    pub dim_filtered: bool,
    pub spaces: bool,
    pub links: bool,
}

impl Default for PrintState {
    fn default() -> Self {
        Self {
            open: false,
            range: Range::All,
            pages: String::new(),
            paper: "Letter".into(),
            orientation: Orientation::Auto,
            scaling: Scaling::Fit,
            percent: 100.0,
            markups: true,
            message: String::new(),
            printers: Vec::new(),
            printers_rx: None,
            printer: String::new(),
            copies: 1,
            collate: true,
            reverse: false,
            markups_only: false,
            region: None,
            margin: 0.0,
            offset: [0.0, 0.0],
            dim_content: false,
            dim_filtered: false,
            spaces: false,
            links: false,
        }
    }
}

impl PrintState {
    pub fn open(&mut self) {
        self.open = true;
        self.message.clear();
        if self.printers.is_empty() && self.printers_rx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let started = std::thread::Builder::new()
                .name("markupcraft-printers".into())
                .spawn(move || {
                    let _ = tx.send(list_printers());
                });
            if started.is_ok() {
                self.printers_rx = Some(rx);
            }
        }
    }

    /// The whole job: layout settings and the rest of the dialog. `filtered` are the markups
    /// the Markups List shows (for dimming the others).
    pub fn job(&self, current: usize, count: usize, filtered: Option<Vec<String>>) -> Result<PrintJob, String> {
        Ok(PrintJob {
            settings: self.settings(current, count)?,
            markups_only: self.markups_only,
            region: self.region,
            copies: self.copies.clamp(1, 999),
            collate: self.collate,
            reverse: self.reverse,
            margin: self.margin,
            offset: (self.offset[0], self.offset[1]),
            dim_content: self.dim_content,
            dim_except: if self.dim_filtered { filtered } else { None },
            spaces: self.spaces,
            links: self.links,
        })
    }

    /// The settings for a document of `count` pages showing page `current`.
    pub fn settings(&self, current: usize, count: usize) -> Result<PrintSettings, String> {
        let pages = match self.range {
            Range::All => (0..count).collect(),
            Range::Current => vec![current.min(count.saturating_sub(1))],
            Range::Pages => super::parse_pages(&self.pages, count).ok_or("Pages: a range like 1-3, 5")?,
        };
        Ok(PrintSettings {
            pages,
            paper: paper_size(&self.paper).ok_or("Choose a paper size")?,
            landscape: match self.orientation {
                Orientation::Auto => None,
                Orientation::Portrait => Some(false),
                Orientation::Landscape => Some(true),
            },
            layout: match self.scaling {
                Scaling::Fit => PrintLayout::Fit,
                Scaling::Actual => PrintLayout::ActualSize,
                Scaling::Shrink => PrintLayout::Shrink,
                Scaling::Percent => PrintLayout::Percent(self.percent.clamp(1.0, 1000.0)),
            },
            markups: self.markups,
        })
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.print.open {
        return;
    }
    let Some(name) = app.doc().map(|d| d.name.trim_end_matches(".pdf").to_string()) else {
        app.features.print.open = false;
        return;
    };
    if let Some(rx) = &app.features.print.printers_rx
        && let Ok(list) = rx.try_recv()
    {
        app.features.print.printers = list;
        app.features.print.printers_rx = None;
    }
    let (mut open, mut go, mut print, mut window, mut view) = (true, false, false, false, false);
    super::window("Print").open(&mut open).show(ctx, |ui| {
        let p = &mut app.features.print;
        egui::Grid::new("print-grid").num_columns(2).show(ui, |ui| {
            ui.label("Printer");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("print-printer")
                    .selected_text(if p.printer.is_empty() {
                        "Default printer"
                    } else {
                        p.printer.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.printer, String::new(), "Default printer");
                        for n in &p.printers {
                            ui.selectable_value(&mut p.printer, n.clone(), n);
                        }
                    });
                ui.label("Copies");
                ui.add(egui::DragValue::new(&mut p.copies).range(1..=999));
                ui.checkbox(&mut p.collate, "Collate");
                ui.checkbox(&mut p.reverse, "Reverse");
            });
            ui.end_row();
            ui.label("Pages");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.range, Range::All, "All");
                ui.selectable_value(&mut p.range, Range::Current, "Current");
                if ui
                    .button("Current View")
                    .on_hover_text("Print what the view shows")
                    .clicked()
                {
                    view = true;
                }
                if ui
                    .button("Get Window")
                    .on_hover_text("Drag the region to print")
                    .clicked()
                {
                    window = true;
                }
                if let Some((pg, r)) = p.region {
                    ui.label(format!("p.{} {:.0} x {:.0} pt", pg + 1, r.width(), r.height()));
                    if ui.small_button("x").clicked() {
                        p.region = None;
                    }
                }
                ui.selectable_value(&mut p.range, Range::Pages, "Pages");
                if p.range == Range::Pages {
                    ui.add(
                        egui::TextEdit::singleline(&mut p.pages)
                            .desired_width(80.0)
                            .hint_text("1-3, 5"),
                    );
                }
            });
            ui.end_row();
            ui.label("Paper");
            egui::ComboBox::from_id_salt("print-paper")
                .selected_text(&p.paper)
                .show_ui(ui, |ui| {
                    for n in PAPERS {
                        ui.selectable_value(&mut p.paper, (*n).to_string(), *n);
                    }
                });
            ui.end_row();
            ui.label("Orientation");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.orientation, Orientation::Auto, "Auto");
                ui.selectable_value(&mut p.orientation, Orientation::Portrait, "Portrait");
                ui.selectable_value(&mut p.orientation, Orientation::Landscape, "Landscape");
            });
            ui.end_row();
            ui.label("Scaling");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.scaling, Scaling::Fit, "Fit");
                ui.selectable_value(&mut p.scaling, Scaling::Actual, "Actual size");
                ui.selectable_value(&mut p.scaling, Scaling::Shrink, "Shrink");
                ui.selectable_value(&mut p.scaling, Scaling::Percent, "Custom");
                if p.scaling == Scaling::Percent {
                    ui.add(egui::DragValue::new(&mut p.percent).range(1.0..=1000.0).suffix("%"));
                }
            });
            ui.end_row();
            ui.label("Margins");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut p.margin).range(0.0..=144.0).suffix(" pt"));
                ui.label("Position");
                ui.add(egui::DragValue::new(&mut p.offset[0]).prefix("x ").suffix(" pt"));
                ui.add(egui::DragValue::new(&mut p.offset[1]).prefix("y ").suffix(" pt"));
            });
            ui.end_row();
            ui.label("Print");
            ui.horizontal(|ui| {
                let mut what = if p.markups_only {
                    2
                } else if p.markups {
                    0
                } else {
                    1
                };
                ui.selectable_value(&mut what, 0, "Document and markups");
                ui.selectable_value(&mut what, 1, "Document");
                ui.selectable_value(&mut what, 2, "Markups only");
                p.markups = what != 1;
                p.markups_only = what == 2;
            });
            ui.end_row();
            ui.label("Emphasis");
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut p.dim_content, "Dim page content");
                ui.checkbox(&mut p.dim_filtered, "Dim filtered-out markups");
                ui.checkbox(&mut p.spaces, "Print Spaces");
                ui.checkbox(&mut p.links, "Print hyperlinks");
            });
            ui.end_row();
        });
        if !p.message.is_empty() {
            ui.label(RichText::new(&p.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("Print").clicked() {
                print = true;
            }
            if ui.button("Save Print PDF...").clicked() {
                go = true;
            }
            if ui.button("Cancel").clicked() {
                p.open = false;
            }
        });
    });
    if !open {
        app.features.print.open = false;
    }
    if go {
        app.dialogs
            .save(Purpose::Feature(Ask::PrintOut), PDF, &format!("{name} print.pdf"));
    }
    if window {
        super::start_pick(app, super::Pick::PrintRegion, "Drag the region to print");
    }
    if view {
        current_view(app);
    }
    if print {
        let out = std::env::temp_dir().join(format!("markupcraft-print-{}.pdf", std::process::id()));
        if write(app, &out) {
            let p = &app.features.print;
            let printer = (!p.printer.is_empty()).then_some(p.printer.as_str());
            let r = send_to_printer(&out, printer, 1);
            app.status = actions::report(r, |_| "Sent to the printer".into());
            app.features.print.message = app.status.clone();
        }
    }
}

/// Get Window: the region to print was dragged.
pub fn region_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    if let Some(r) = super::rect_of(pts) {
        app.features.print.region = Some((page, r));
        app.features.print.open = true;
    }
}

/// Current View: the region the canvas shows.
fn current_view(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    let Some(render) = d.render.as_ref() else { return };
    let Some((page, c)) = d.view.center_point(render.pages()) else {
        return;
    };
    let vp = d.view.viewport();
    let k = f64::from(d.view.zoom * crate::canvas::PT).max(1e-6);
    let (hw, hh) = (f64::from(vp.width()) / k / 2.0, f64::from(vp.height()) / k / 2.0);
    app.features.print.region = Some((page, Rect::new(c.x - hw, c.y - hh, c.x + hw, c.y + hh)));
}

/// Write the print-ready PDF; returns whether it was written.
pub fn write(app: &mut AppState, out: &Path) -> bool {
    let Some(d) = app.docs.get(app.active) else {
        return false;
    };
    let filtered: Vec<String> = d.session.doc().markups.iter().map(|m| m.id.clone()).collect();
    let job = match app
        .features
        .print
        .job(d.view.current, d.session.page_count(), Some(filtered))
    {
        Ok(s) => s,
        Err(e) => {
            app.features.print.message = e;
            return false;
        }
    };
    match d.session.print_job_to_pdf(out, &job) {
        Ok(n) => {
            app.status = format!("Wrote {} to {}", actions::plural(n, "sheet"), out.display());
            app.features.print.message = app.status.clone();
            true
        }
        Err(e) => {
            app.features.print.message = e.to_string();
            false
        }
    }
}
