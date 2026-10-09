//! Markup Summary: choose the columns, pages, grouping and order, then write the summary as
//! CSV, XML, an Excel workbook or a PDF report.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::summary::{PdfLayout, SummaryContent, SummaryFormat, SummaryOptions};
use markupcraft_model::table::MarkupTable;

use super::Ask;
use crate::dialogs::Purpose;
use crate::{AppState, actions};

pub struct SummaryState {
    pub open: bool,
    /// Column ids chosen (empty until the dialog first opens: then the list's defaults).
    pub columns: Vec<String>,
    pub pages: String,
    pub group_by: String,
    pub sort: String,
    pub descending: bool,
    pub measurements_only: bool,
    pub title: String,
    pub message: String,
    /// Then sort by (Filter and Sort).
    pub then_sort: String,
    pub then_descending: bool,
    /// Filter: a column and the text its cells must have (comma-separated values).
    pub filter_column: String,
    pub filter_values: String,
    pub date_in_title: bool,
    pub content: SummaryContent,
    pub headers: bool,
    pub per_value: bool,
    pub layout: PdfLayout,
}

impl Default for SummaryState {
    fn default() -> Self {
        Self {
            open: false,
            columns: Vec::new(),
            pages: String::new(),
            group_by: String::new(),
            sort: String::new(),
            descending: false,
            measurements_only: false,
            title: "Markup Summary".into(),
            message: String::new(),
            then_sort: String::new(),
            then_descending: false,
            filter_column: String::new(),
            filter_values: String::new(),
            date_in_title: false,
            content: SummaryContent::Both,
            headers: true,
            per_value: false,
            layout: PdfLayout::default(),
        }
    }
}

/// The options the dialog describes for a document of `count` pages.
pub fn options(s: &SummaryState, count: usize) -> Result<SummaryOptions, String> {
    let pages = if s.pages.trim().is_empty() {
        Vec::new()
    } else {
        super::parse_pages(&s.pages, count).ok_or("Pages: leave empty for all, or a range like 1-3, 5")?
    };
    Ok(SummaryOptions {
        columns: s.columns.clone(),
        pages,
        measurements_only: s.measurements_only,
        group_by: if s.group_by.is_empty() {
            Vec::new()
        } else {
            vec![s.group_by.clone()]
        },
        sort: s.sort.clone(),
        descending: s.descending,
        title: s.title.clone(),
        then_by: if s.then_sort.is_empty() {
            Vec::new()
        } else {
            vec![(s.then_sort.clone(), s.then_descending)]
        },
        filters: if s.filter_column.is_empty() || s.filter_values.trim().is_empty() {
            Default::default()
        } else {
            [(
                s.filter_column.clone(),
                s.filter_values
                    .split(',')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect(),
            )]
            .into_iter()
            .collect()
        },
        date_in_title: s.date_in_title,
        content: s.content,
        no_headers: !s.headers,
        layout: s.layout.clone(),
    })
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.summary.open {
        return;
    }
    let Some(d) = app.docs.get(app.active) else {
        app.features.summary.open = false;
        return;
    };
    let table = MarkupTable::new(d.session.doc());
    let cols: Vec<(String, String, bool)> = table
        .columns()
        .iter()
        .map(|c| (c.id.clone(), c.header.clone(), c.visible_by_default))
        .collect();
    let name = d.name.trim_end_matches(".pdf").to_string();
    let s = &mut app.features.summary;
    if s.columns.is_empty() {
        s.columns = cols.iter().filter(|c| c.2).map(|c| c.0.clone()).collect();
    }
    let (mut open, mut save) = (true, None);
    super::window("Markup Summary").open(&mut open).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Title:");
            ui.text_edit_singleline(&mut s.title);
        });
        ui.label(RichText::new("Columns").strong());
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            egui::Grid::new("summary-cols").num_columns(3).show(ui, |ui| {
                for (k, (id, header, _)) in cols.iter().enumerate() {
                    let mut on = s.columns.contains(id);
                    if ui.checkbox(&mut on, header).changed() {
                        if on {
                            s.columns.push(id.clone());
                        } else {
                            s.columns.retain(|c| c != id);
                        }
                    }
                    if k % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        });
        let header_of = |id: &str| {
            cols.iter()
                .find(|c| c.0 == id)
                .map_or_else(|| "(none)".to_string(), |c| c.1.clone())
        };
        ui.horizontal(|ui| {
            ui.label("Group by:");
            egui::ComboBox::from_id_salt("summary-group")
                .selected_text(header_of(&s.group_by))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.group_by, String::new(), "(none)");
                    ui.selectable_value(&mut s.group_by, "type".to_string(), "Type");
                    for (id, h, _) in &cols {
                        ui.selectable_value(&mut s.group_by, id.clone(), h);
                    }
                });
            ui.label("Sort by:");
            egui::ComboBox::from_id_salt("summary-sort")
                .selected_text(header_of(&s.sort))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.sort, String::new(), "(document order)");
                    for (id, h, _) in &cols {
                        ui.selectable_value(&mut s.sort, id.clone(), h);
                    }
                });
            ui.checkbox(&mut s.descending, "Descending");
        });
        ui.horizontal(|ui| {
            ui.label("Then by:");
            egui::ComboBox::from_id_salt("summary-then")
                .selected_text(header_of(&s.then_sort))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.then_sort, String::new(), "(none)");
                    for (id, h, _) in &cols {
                        ui.selectable_value(&mut s.then_sort, id.clone(), h);
                    }
                });
            ui.checkbox(&mut s.then_descending, "Descending");
        });
        ui.horizontal(|ui| {
            ui.label("Filter:");
            egui::ComboBox::from_id_salt("summary-filter")
                .selected_text(header_of(&s.filter_column))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter_column, String::new(), "(none)");
                    for (id, h, _) in &cols {
                        ui.selectable_value(&mut s.filter_column, id.clone(), h);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut s.filter_values)
                    .hint_text("values, comma-separated")
                    .desired_width(160.0),
            );
        });
        super::pages_field(ui, &mut s.pages);
        ui.checkbox(&mut s.measurements_only, "Measurements only");
        ui.collapsing("Output", |ui| {
            ui.checkbox(&mut s.date_in_title, "Append the date to the title");
            ui.checkbox(&mut s.per_value, "One report per value of the first column");
            ui.horizontal(|ui| {
                ui.label("CSV/XML:");
                ui.selectable_value(&mut s.content, SummaryContent::Both, "Markups and totals");
                ui.selectable_value(&mut s.content, SummaryContent::Markups, "Markups");
                ui.selectable_value(&mut s.content, SummaryContent::Totals, "Totals");
                ui.checkbox(&mut s.headers, "Headers");
            });
            ui.horizontal(|ui| {
                ui.label("PDF:");
                ui.selectable_value(&mut s.layout.flow, false, "Table");
                ui.selectable_value(&mut s.layout.flow, true, "Flow");
                ui.checkbox(&mut s.layout.break_per_group, "Page per group");
                ui.checkbox(&mut s.layout.links, "Links to pages");
                ui.checkbox(&mut s.layout.totals, "Totals");
            });
            ui.horizontal(|ui| {
                ui.label("Padding");
                ui.add(egui::DragValue::new(&mut s.layout.padding).range(0.0..=20.0));
                ui.label("Page");
                let portrait = s.layout.page_size.0 < s.layout.page_size.1;
                if ui.selectable_label(!portrait, "Landscape").clicked() {
                    s.layout.page_size = (792.0, 612.0);
                }
                if ui.selectable_label(portrait, "Portrait").clicked() {
                    s.layout.page_size = (612.0, 792.0);
                }
            });
        });
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Save as:");
            for (label, f) in [
                ("Excel (.xlsx)", SummaryFormat::Xlsx),
                ("PDF", SummaryFormat::Pdf),
                ("CSV", SummaryFormat::Csv),
                ("XML", SummaryFormat::Xml),
            ] {
                if ui.button(label).clicked() {
                    save = Some(f);
                }
            }
        });
    });
    if !open {
        app.features.summary.open = false;
    }
    if let Some(f) = save {
        let ext = f.extension();
        let filter: crate::dialogs::Filter = match f {
            SummaryFormat::Xlsx => ("Excel workbook", &["xlsx"]),
            SummaryFormat::Pdf => ("PDF", &["pdf"]),
            SummaryFormat::Csv => ("CSV", &["csv"]),
            SummaryFormat::Xml => ("XML", &["xml"]),
        };
        app.dialogs.save(
            Purpose::Feature(Ask::SummaryOut(f)),
            filter,
            &format!("{name} Summary.{ext}"),
        );
    }
}

/// Write the summary of the active document.
pub fn write(app: &mut AppState, f: SummaryFormat, out: &Path) {
    let Some(d) = app.docs.get(app.active) else { return };
    let o = match options(&app.features.summary, d.session.page_count()) {
        Ok(o) => o,
        Err(e) => {
            app.features.summary.message = e;
            return;
        }
    };
    if app.features.summary.per_value {
        let r = d.session.export_summary_per_value(out, Some(f), &o);
        app.status = actions::report(r, |files| format!("Wrote {}", actions::plural(files.len(), "report")));
        app.features.summary.message = app.status.clone();
        return;
    }
    match d.session.export_summary(out, Some(f), &o) {
        Ok(n) => {
            app.status = format!(
                "Summary of {} written to {}",
                actions::plural(n, "markup"),
                out.display()
            );
            app.features.summary.message = app.status.clone();
        }
        Err(e) => app.features.summary.message = e.to_string(),
    }
}
