//! Smaller document commands: form data exchange and automatic fields, Redaction Properties,
//! Update Header & Footer, legend options, Copy Legend to Pages and Snapshot Legend. Creating
//! PDFs from files, layered PDFs and Merge Form Data use the batch dialog.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::legend::LegendOptions;
use markupcraft_engine::quantity::{
    QuantityLink, QuantityMeasure, QuantityValue, load_links, quantity_totals, save_links, update_quantity_workbook,
};
use markupcraft_engine::redact::REDACTION_CODES;

use super::{Ask, batch};
use crate::dialogs::Purpose;
use crate::{AppState, actions};

pub const FORM_DATA: crate::dialogs::Filter = ("Form data", &["xfdf", "csv", "json"]);

/// Run one of these commands; false when `id` is not one of them.
pub fn run(app: &mut AppState, id: &str) -> bool {
    match id {
        "file.create_from_files" => app.features.batch.open(batch::Kind::Create),
        "file.layered" => app.features.batch.open(batch::Kind::Layered),
        "forms.merge_data" => app.features.batch.open(batch::Kind::FormMerge),
        "forms.export_data" => app
            .dialogs
            .save(Purpose::Feature(Ask::FormDataOut), FORM_DATA, "Form Data.xfdf"),
        "forms.import_data" => app.dialogs.open(Purpose::Feature(Ask::FormDataIn), FORM_DATA, false),
        "forms.typewriter_to_fields" => doc_step(app, |s| {
            s.typewriter_to_fields()
                .map(|n| format!("Moved {} into fields", actions::plural(n, "Typewriter markup")))
        }),
        "forms.auto_fields" => doc_step(app, |s| {
            s.auto_create_fields(&[])
                .map(|v| format!("Created {}", actions::plural(v.len(), "form field")))
        }),
        "document.redaction_properties" => app.features.redact.properties_open = true,
        "measure.quantity_link" => app.features.quantity.open = true,
        "document.hf_update" => doc_step(app, |s| {
            s.update_header_footer()
                .map(|_| "Header and footer updated".to_string())
        }),
        "measure.legend_copy" => {
            let Some(id) = legend_id(app) else { return true };
            doc_step(app, |s| {
                s.copy_legend_to_pages(&id)
                    .map(|v| format!("Legend copied to {}", actions::plural(v.len(), "page")))
            });
        }
        "measure.legend_freeze" => {
            let Some(id) = legend_id(app) else { return true };
            doc_step(app, |s| {
                s.freeze_legend(&id)
                    .map(|_| "Legend frozen: it no longer updates".to_string())
            });
        }
        _ => return false,
    }
    true
}

/// Run an engine step on the active document, re-render it and report.
fn doc_step(
    app: &mut AppState,
    f: impl FnOnce(&mut markupcraft_engine::Session) -> markupcraft_engine::Result<String>,
) {
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let r = f(&mut d.session);
    d.rerender(threads);
    app.status = actions::report(r, |m| m);
}

/// The selected legend, else the first legend on the current page.
fn legend_id(app: &mut AppState) -> Option<String> {
    let d = app.doc()?;
    let legends = d.session.legends();
    let sel = d.selection();
    let found = legends
        .iter()
        .find(|l| sel.contains(&l.id))
        .or_else(|| legends.iter().find(|l| l.page == d.view.current))
        .map(|l| l.id.clone());
    if found.is_none() {
        app.status = "Select a legend first".into();
    }
    found
}

/// Export or import form data files.
pub fn form_data_file(app: &mut AppState, ask: &Ask, path: &Path) {
    let p = path.to_path_buf();
    match ask {
        Ask::FormDataOut => doc_step(app, |s| {
            s.export_form_data(&p)
                .map(|n| format!("Exported {} to {}", actions::plural(n, "field"), p.display()))
        }),
        Ask::FormDataIn => doc_step(app, |s| {
            s.import_form_data(&p)
                .map(|n| format!("Filled {} from {}", actions::plural(n, "field"), p.display()))
        }),
        _ => {}
    }
}

/// The extra legend options (in the Legend dialog).
pub fn legend_options(ui: &mut egui::Ui, l: &mut LegendOptions, pages: &mut String, selection: &mut bool) {
    ui.collapsing("More options", |ui| {
        super::pages_field(ui, pages);
        ui.checkbox(selection, "Only the selected markups");
        ui.checkbox(&mut l.show_empty, "List every chosen subject, even with none drawn");
        ui.horizontal(|ui| {
            ui.label("Subjects");
            let mut text = l.subjects.join(", ");
            if ui.add(egui::TextEdit::singleline(&mut text).hint_text("all")).changed() {
                l.subjects = split(&text);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Custom columns");
            let mut text = l.custom_columns.join(", ");
            if ui
                .add(egui::TextEdit::singleline(&mut text).hint_text("none"))
                .changed()
            {
                l.custom_columns = split(&text);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Border");
            super::color_edit(ui, &mut l.border_color);
            ui.add(egui::DragValue::new(&mut l.line_width).range(0.0..=12.0).speed(0.1));
            let mut fill = l.fill_color.is_some();
            ui.checkbox(&mut fill, "Fill");
            let mut c = l.fill_color.unwrap_or(markupcraft_model::Color::rgb(1.0, 1.0, 1.0));
            if fill {
                super::color_edit(ui, &mut c);
            }
            l.fill_color = fill.then_some(c);
        });
        ui.horizontal(|ui| {
            ui.label("Opacity");
            ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0));
        });
        ui.horizontal(|ui| {
            ui.label("Symbol size");
            ui.add(egui::DragValue::new(&mut l.symbol_scale).range(0.25..=4.0).speed(0.05));
            ui.checkbox(&mut l.header, "Header row");
        });
    });
}

fn split(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Redaction Properties: how new marks look once applied.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    quantity_window(app, ctx);
    if !app.features.redact.properties_open {
        return;
    }
    let mut open = true;
    super::window("Redaction Properties").open(&mut open).show(ctx, |ui| {
        let st = &mut app.features.redact.style;
        ui.horizontal(|ui| {
            let mut fill = st.fill.is_some();
            ui.checkbox(&mut fill, "Fill box");
            let mut c = st.fill.unwrap_or(markupcraft_model::Color::BLACK);
            if fill {
                super::color_edit(ui, &mut c);
            }
            st.fill = fill.then_some(c);
        });
        ui.horizontal(|ui| {
            ui.label("Overlay text");
            ui.text_edit_singleline(&mut st.overlay);
        });
        egui::ComboBox::from_label("Redaction code")
            .selected_text(if st.overlay.is_empty() {
                "None"
            } else {
                st.overlay.as_str()
            })
            .show_ui(ui, |ui| {
                for (code, what) in REDACTION_CODES {
                    if ui
                        .selectable_label(st.overlay == *code, *code)
                        .on_hover_text(*what)
                        .clicked()
                    {
                        st.overlay = (*code).to_string();
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.label("Font");
            for f in ["Helvetica", "Times", "Courier"] {
                if ui.selectable_label(st.font == f, f).clicked() {
                    st.font = f.to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Size");
            ui.add(egui::DragValue::new(&mut st.font_size).range(0.0..=144.0));
            ui.label(RichText::new("0 = fit").small());
            super::color_edit(ui, &mut st.text_color);
        });
        ui.horizontal(|ui| {
            ui.label("Align");
            ui.selectable_value(&mut st.align, 0, "Left");
            ui.selectable_value(&mut st.align, 1, "Centre");
            ui.selectable_value(&mut st.align, 2, "Right");
            ui.checkbox(&mut st.repeat, "Repeat to fill");
        });
        ui.label(RichText::new("New marks use these; Document > Mark for Redaction.").small());
    });
    if !open {
        app.features.redact.properties_open = false;
    }
}

// ---- Quantity Link ----------------------------------------------------------------------------

const MEASURES: [&str; 5] = ["count", "length", "area", "volume", "quantity"];
pub const XLSX: crate::dialogs::Filter = ("Excel workbook", &["xlsx"]);

/// The Quantity Link dialog: links from workbook cells to measurement totals.
pub struct QuantityState {
    pub open: bool,
    pub links: Vec<QuantityLink>,
    pub name: String,
    pub sheet: String,
    pub cell: String,
    /// Index into the measure list; a custom column when `column` is set.
    pub measure: usize,
    pub column: String,
    pub subjects: String,
    pub layers: String,
    pub pages: String,
    /// Totals from the last Update.
    pub values: Vec<QuantityValue>,
    pub message: String,
}

impl Default for QuantityState {
    fn default() -> Self {
        Self {
            open: false,
            links: Vec::new(),
            name: String::new(),
            sheet: "Quantities".into(),
            cell: "B2".into(),
            measure: 0,
            column: String::new(),
            subjects: String::new(),
            layers: String::new(),
            pages: String::new(),
            values: Vec::new(),
            message: String::new(),
        }
    }
}

/// Add a link over the active document (saved) from the dialog's fields.
fn add_link(app: &mut AppState) {
    let file = app.doc().and_then(|d| d.path.clone());
    let q = &mut app.features.quantity;
    let Some(file) = file else {
        q.message = "Save the document first: a link totals saved files".into();
        return;
    };
    let measure = if q.column.trim().is_empty() {
        MEASURES
            .get(q.measure)
            .and_then(|m| QuantityMeasure::from_name(m))
            .unwrap_or_default()
    } else {
        QuantityMeasure::Column(q.column.trim().to_string())
    };
    let l = QuantityLink {
        name: q.name.trim().to_string(),
        sheet: q.sheet.trim().to_string(),
        cell: q.cell.trim().to_string(),
        files: vec![file],
        measure,
        subjects: split(&q.subjects),
        layers: split(&q.layers),
        pages: split(&q.pages),
        ..Default::default()
    };
    match markupcraft_engine::quantity::check_link(&l) {
        Ok(()) => {
            q.links.retain(|x| x.name != l.name);
            q.message = format!("Link {} added", l.name);
            q.links.push(l);
            q.values.clear();
        }
        Err(e) => q.message = e.to_string(),
    }
}

pub fn quantity_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.quantity.open {
        return;
    }
    let (mut open, mut add, mut update, mut save, mut load) = (true, false, false, false, false);
    super::window("Quantity Link").open(&mut open).show(ctx, |ui| {
        let q = &mut app.features.quantity;
        ui.label(RichText::new("Link a workbook cell to a measurement total of this document.").small());
        egui::Grid::new("qty").num_columns(2).show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut q.name);
            ui.end_row();
            ui.label("Sheet / cell");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut q.sheet).desired_width(110.0));
                ui.add(egui::TextEdit::singleline(&mut q.cell).desired_width(50.0));
            });
            ui.end_row();
            ui.label("Measure");
            ui.horizontal_wrapped(|ui| {
                for (i, m) in MEASURES.iter().enumerate() {
                    ui.selectable_value(&mut q.measure, i, *m);
                }
            });
            ui.end_row();
            ui.label("Custom column");
            ui.add(egui::TextEdit::singleline(&mut q.column).hint_text("none"));
            ui.end_row();
            ui.label("Subjects");
            ui.add(egui::TextEdit::singleline(&mut q.subjects).hint_text("any"));
            ui.end_row();
            ui.label("Layers");
            ui.add(egui::TextEdit::singleline(&mut q.layers).hint_text("any"));
            ui.end_row();
            ui.label("Page labels");
            ui.add(egui::TextEdit::singleline(&mut q.pages).hint_text("any"));
            ui.end_row();
        });
        if ui.button("Add Link").clicked() {
            add = true;
        }
        let mut remove = None;
        egui::Grid::new("qty-links")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                for (i, l) in q.links.iter().enumerate() {
                    ui.label(&l.name);
                    ui.label(format!("{}!{}", l.sheet, l.cell.to_ascii_uppercase()));
                    ui.label(
                        q.values
                            .get(i)
                            .map_or_else(|| l.measure.name(), |v| format!("{} {}", v.value, v.units.join("/"))),
                    );
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove.filter(|i| *i < q.links.len()) {
            q.links.remove(i);
            q.values.clear();
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!q.links.is_empty(), |ui| {
                if ui.button("Update Workbook...").clicked() {
                    update = true;
                }
                if ui.button("Save Links...").clicked() {
                    save = true;
                }
            });
            if ui.button("Open Links...").clicked() {
                load = true;
            }
        });
        if !q.message.is_empty() {
            ui.label(RichText::new(&q.message).small());
        }
    });
    if !open {
        app.features.quantity.open = false;
    }
    if add {
        add_link(app);
    }
    if update {
        app.dialogs
            .save(Purpose::Feature(Ask::QuantityOut), XLSX, "Quantities.xlsx");
    }
    if save {
        app.dialogs.save(
            Purpose::Feature(Ask::QuantityLinksSave),
            super::JSON,
            "Quantity Links.json",
        );
    }
    if load {
        app.dialogs
            .open(Purpose::Feature(Ask::QuantityLinksOpen), super::JSON, false);
    }
}

/// Quantity Link files: the workbook, or the links saved and opened.
pub fn quantity_file(app: &mut AppState, ask: &Ask, path: &Path) {
    let q = &mut app.features.quantity;
    q.message = match ask {
        Ask::QuantityOut => match update_quantity_workbook(&q.links, path) {
            Ok(v) => {
                q.values = v;
                format!("Workbook written: {}", path.display())
            }
            Err(e) => e.to_string(),
        },
        Ask::QuantityLinksSave => actions::report(save_links(path, &q.links), |_| {
            format!("Links saved: {}", path.display())
        }),
        Ask::QuantityLinksOpen => match load_links(path) {
            Ok(l) => {
                q.links = l;
                q.values = quantity_totals(&q.links).unwrap_or_default();
                format!("Opened {}", actions::plural(q.links.len(), "link"))
            }
            Err(e) => e.to_string(),
        },
        _ => return,
    };
    app.status = app.features.quantity.message.clone();
}
