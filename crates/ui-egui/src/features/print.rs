//! File > Print: pages, paper, orientation, scaling and whether markups print, written as a
//! print-ready PDF. Sending it to a printer is left to the system's PDF viewer for now: the
//! dialog says so and saves the sheets where the user chooses.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::printout::{PrintLayout, PrintSettings, paper_size};

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
        }
    }
}

impl PrintState {
    pub fn open(&mut self) {
        self.open = true;
        self.message.clear();
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
    let (mut open, mut go) = (true, false);
    super::window("Print").open(&mut open).show(ctx, |ui| {
        let p = &mut app.features.print;
        egui::Grid::new("print-grid").num_columns(2).show(ui, |ui| {
            ui.label("Printer");
            ui.label(RichText::new("Print-ready PDF (open it in your PDF viewer to send to a printer)").small());
            ui.end_row();
            ui.label("Pages");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.range, Range::All, "All");
                ui.selectable_value(&mut p.range, Range::Current, "Current");
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
            ui.label("Print");
            ui.checkbox(&mut p.markups, "Document and markups");
            ui.end_row();
        });
        if !p.message.is_empty() {
            ui.label(RichText::new(&p.message).small());
        }
        ui.horizontal(|ui| {
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
}

pub fn write(app: &mut AppState, out: &Path) {
    let Some(d) = app.docs.get(app.active) else { return };
    let s = match app.features.print.settings(d.view.current, d.session.page_count()) {
        Ok(s) => s,
        Err(e) => {
            app.features.print.message = e;
            return;
        }
    };
    match d.session.print_to_pdf(out, &s) {
        Ok(n) => {
            app.status = format!("Wrote {} to {}", actions::plural(n, "sheet"), out.display());
            app.features.print.message = app.status.clone();
            app.features.print.open = false;
        }
        Err(e) => app.features.print.message = e.to_string(),
    }
}
