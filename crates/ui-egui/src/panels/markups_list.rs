//! Markups List (Alt+L): every markup as a table on `markupcraft_model::MarkupTable`. Columns
//! (standard and custom) can be shown or hidden; click a header to sort, right-click it to
//! group by it, filter its values or hide it. Groups show subtotals; the last row is the grand
//! total per unit. Status is a menu, Checkmark a box, text cells edit on double-click. Click a
//! row to select its markup and bring it into view; right-click for its menu. Export writes
//! the list as CSV, a totals summary or XML; Manage Columns edits the custom columns.

use std::collections::BTreeSet;
use std::path::Path;

use egui::{Color32, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use markupcraft_model::csv::{group_name, table_csv, table_xml, totals_csv};
use markupcraft_model::{
    CellEdit, ColumnType, CustomColumn, Group, MarkupTable, Scope, View, default_visible_columns, make_column_id,
};

use super::{PanelDef, Slot};
use crate::actions::{self, markup_bbox};
use crate::commands::alt;
use crate::dialogs::{self, Purpose};
use crate::theme::{Tokens, color32};
use crate::{AppState, DocTab};

pub static PANEL: PanelDef = PanelDef {
    id: "markups",
    title: "Markups List",
    icon: "list",
    slot: Slot::Bottom,
    keys: alt(egui::Key::L),
    ui,
};

/// The list's view and editing state (kept across frames, shared by every document).
pub struct ListState {
    pub view: View,
    /// A cell being typed into: markup id, column id, text.
    pub editing: Option<(String, String, String)>,
    /// Manage Columns: the custom columns being edited.
    pub columns_editor: Option<Vec<CustomColumn>>,
    /// Saved views and column widths.
    pub prefs: super::list_views::ListPrefs,
}

impl Default for ListState {
    fn default() -> Self {
        Self {
            view: View {
                visible: default_visible_columns(),
                ..Default::default()
            },
            editing: None,
            columns_editor: None,
            prefs: Default::default(),
        }
    }
}

/// One line of the list as drawn.
enum Line<'a> {
    Group(&'a Group),
    Markup(usize),
    Total(&'a Group),
}

fn flatten<'a>(g: &'a Group, out: &mut Vec<Line<'a>>) {
    if g.column.is_some() {
        out.push(Line::Group(g));
    }
    for r in &g.rows {
        out.push(Line::Markup(*r));
    }
    for c in &g.children {
        flatten(c, out);
    }
}

/// The view with this frame's scope.
fn scoped(view: &View, doc: &DocTab) -> View {
    let mut v = view.clone();
    v.scope = match &view.scope {
        Scope::CurrentPage(_) => Scope::CurrentPage(doc.view.current),
        Scope::Selected(_) => {
            let sel = doc.selection();
            Scope::Selected(
                doc.session
                    .doc()
                    .markups
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| sel.contains(&m.id))
                    .map(|(i, _)| i)
                    .collect(),
            )
        }
        Scope::AllPages => Scope::AllPages,
    };
    v
}

/// What a click on the list asks for.
enum Act {
    Select(usize, bool),
    Sort(String),
    GroupBy(String),
    Hide(String),
    Filter(String, String, bool),
    ClearFilter(String),
    SetCell(String, String, String),
    StartEdit(String, String, String),
    Command(&'static str),
    /// Put the selection on this layer ("" = none)
    Layer(String),
    /// A legend of the selected rows' subjects
    Legend,
    CopyRows,
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(doc) = app.docs.get_mut(app.active) else {
        super::empty(ui, "No document open.");
        return;
    };
    let list = &mut app.list;
    let mut acts: Vec<Act> = Vec::new();
    let mut export: Option<Purpose> = None;

    // ---- toolbar ------------------------------------------------------------------------
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut list.view.search)
                .hint_text("Search")
                .desired_width(160.0),
        );
        let scope_name = match list.view.scope {
            Scope::AllPages => "All Pages",
            Scope::CurrentPage(_) => "Current Page",
            Scope::Selected(_) => "Selected",
        };
        egui::ComboBox::from_id_salt("list-scope")
            .selected_text(scope_name)
            .show_ui(ui, |ui| {
                if ui.selectable_label(scope_name == "All Pages", "All Pages").clicked() {
                    list.view.scope = Scope::AllPages;
                }
                if ui
                    .selectable_label(scope_name == "Current Page", "Current Page")
                    .clicked()
                {
                    list.view.scope = Scope::CurrentPage(0);
                }
                if ui.selectable_label(scope_name == "Selected", "Selected").clicked() {
                    list.view.scope = Scope::Selected(BTreeSet::new());
                }
            });
        let table = MarkupTable::new(doc.session.doc());
        let header_of = |id: &str| {
            table
                .column_index(id)
                .and_then(|i| table.columns().get(i))
                .map_or(id.to_string(), |c| c.header.clone())
        };
        let grouped = list
            .view
            .group_by
            .first()
            .map(|g| header_of(g))
            .unwrap_or("None".into());
        egui::ComboBox::from_id_salt("list-group")
            .selected_text(format!("Group: {grouped}"))
            .show_ui(ui, |ui| {
                if ui.selectable_label(list.view.group_by.is_empty(), "None").clicked() {
                    list.view.group_by.clear();
                }
                for c in table.columns() {
                    if ui
                        .selectable_label(list.view.group_by.first() == Some(&c.id), &c.header)
                        .clicked()
                    {
                        list.view.group_by = vec![c.id.clone()];
                    }
                }
            });
        ui.menu_button("Columns", |ui| {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for c in table.columns() {
                    let mut on = list.view.visible.contains(&c.id);
                    if ui.checkbox(&mut on, &c.header).changed() {
                        if on {
                            list.view.visible.push(c.id.clone());
                        } else {
                            list.view.visible.retain(|v| *v != c.id);
                        }
                    }
                }
            });
            ui.separator();
            if ui.button("Manage Columns...").clicked() {
                list.columns_editor = Some(doc.session.doc().columns.clone());
                ui.close();
            }
            if ui.button("Reset Columns").clicked() {
                list.view.visible = default_visible_columns();
                ui.close();
            }
        });
        if !list.view.filters.is_empty() {
            let n = list.view.filters.len();
            if ui
                .button(format!("Clear {}", actions::plural(n, "filter")))
                .on_hover_text("Show every markup again")
                .clicked()
            {
                list.view.filters.clear();
            }
        }
        super::list_views::views_menu(ui, &mut list.view, &mut list.prefs);
        ui.menu_button("Export", |ui| {
            if ui.button("Markups (CSV)...").clicked() {
                export = Some(Purpose::ExportCsv);
                ui.close();
            }
            if ui.button("Totals Summary (CSV)...").clicked() {
                export = Some(Purpose::ExportTotals);
                ui.close();
            }
            if ui.button("Append Summary with Links").clicked() {
                acts.push(Act::Command("markup.summary_append"));
                ui.close();
            }
            if ui.button("Summary (XML)...").clicked() {
                export = Some(Purpose::ExportXml);
                ui.close();
            }
        });
    });

    // ---- the table ----------------------------------------------------------------------
    let view = scoped(&list.view, doc);
    let table = MarkupTable::new(doc.session.doc());
    let root = table.build(&view);
    let cols: Vec<usize> = view.visible.iter().filter_map(|id| table.column_index(id)).collect();
    let mut lines = Vec::new();
    flatten(&root, &mut lines);
    lines.push(Line::Total(&root));
    let selection = doc.selection().to_vec();
    let layer_names: Vec<String> = doc.session.layers().into_iter().map(|l| l.name).collect();
    let custom_statuses = app.toolchest.extras.statuses.clone();
    let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    let markups = &doc.session.doc().markups;
    let editing = list.editing.clone();
    // Preferences > Markups List: dominant markups, rich and wrapped comments.
    let lp = app.shell.prefs.more.markups_list.clone();
    let not_dominant: std::collections::HashSet<usize> = if lp.dominant_only {
        let mut seen = std::collections::HashSet::new();
        markups
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.group.is_empty() && !seen.insert(m.group.clone()))
            .map(|(i, _)| i)
            .collect()
    } else {
        Default::default()
    };
    let comment_width = list.prefs.width("comments").unwrap_or(200.0).max(40.0);
    let heights: Vec<f32> = lines
        .iter()
        .map(|l| match l {
            Line::Markup(i) if lp.wrap_comments => {
                let n = markups.get(*i).map_or(0, |m| m.contents.chars().count());
                let per_line = (comment_width / 7.0).max(4.0);
                let rows = (n as f32 / per_line).ceil().clamp(1.0, 8.0);
                18.0 * rows + 2.0
            }
            _ => 20.0,
        })
        .collect();
    // The grand total stays in sight under the table.
    let footer = 24.0;
    let room = (ui.available_height() - footer - 28.0).max(40.0);
    let summary: Vec<String> = cols
        .iter()
        .filter_map(|&c| {
            let v = table.totals_text(c, &root.totals);
            let h = table.columns().get(c).map(|col| col.header.clone())?;
            (!v.is_empty()).then(|| format!("{h}: {v}"))
        })
        .collect();
    let full = ui.available_rect_before_wrap();
    let table_rect = egui::Rect::from_min_max(full.min, egui::pos2(full.max.x, (full.max.y - footer).max(full.min.y)));
    let footer_rect = egui::Rect::from_min_max(egui::pos2(full.min.x, table_rect.max.y), full.max);
    let mut widths: Vec<(String, f32)> = Vec::new();
    ui.scope_builder(egui::UiBuilder::new().max_rect(table_rect), |ui| {
        let mut builder = TableBuilder::new(ui)
            .max_scroll_height(room)
            .min_scrolled_height(0.0)
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .auto_shrink([false, true])
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for (k, &c) in cols.iter().enumerate() {
            let w = table
                .columns()
                .get(c)
                .map_or(100.0, |col| list.prefs.width(&col.id).unwrap_or(col.width as f32));
            builder = if k + 1 == cols.len() {
                builder.column(Column::remainder().at_least(60.0).clip(true))
            } else {
                builder.column(Column::initial(w).at_least(30.0).clip(true))
            };
        }
        if cols.is_empty() {
            ui.label("No columns are shown: pick some from Columns.");
            return;
        }
        builder
            .header(22.0, |mut row| {
                for &c in &cols {
                    let Some(col) = table.columns().get(c) else { continue };
                    let col_id = col.id.clone();
                    let (cell, _) = row.col(|ui| {
                        let arrow = if view.sort_column == col.id {
                            if view.sort_descending { " v" } else { " ^" }
                        } else {
                            ""
                        };
                        let filtered = view.filters.contains_key(&col.id);
                        let mut text =
                            RichText::new(format!("{}{arrow}{}", col.header, if filtered { " *" } else { "" }))
                                .strong();
                        if filtered {
                            text = text.color(t.accent_text);
                        }
                        let r = ui.add(egui::Button::new(text).frame(false));
                        if r.clicked() {
                            acts.push(Act::Sort(col.id.clone()));
                        }
                        r.context_menu(|ui| {
                            if ui.button("Group by This Column").clicked() {
                                acts.push(Act::GroupBy(col.id.clone()));
                                ui.close();
                            }
                            if ui.button("Hide Column").clicked() {
                                acts.push(Act::Hide(col.id.clone()));
                                ui.close();
                            }
                            ui.menu_button("Filter", |ui| {
                                let allowed = view.filters.get(&col.id);
                                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                    for v in table.distinct_values(
                                        c,
                                        &View {
                                            filters: Default::default(),
                                            ..view.clone()
                                        },
                                    ) {
                                        let mut on = allowed.is_none_or(|a| a.contains(&v));
                                        let label = if v.is_empty() { "(blank)".to_string() } else { v.clone() };
                                        if ui.checkbox(&mut on, label).changed() {
                                            acts.push(Act::Filter(col.id.clone(), v.clone(), on));
                                        }
                                    }
                                });
                                if ui.button("Show All").clicked() {
                                    acts.push(Act::ClearFilter(col.id.clone()));
                                    ui.close();
                                }
                            });
                        });
                    });
                    widths.push((col_id, cell.width()));
                }
            })
            .body(|body| {
                let row_ui = |mut row: egui_extras::TableRow<'_, '_>| {
                    let Some(line) = lines.get(row.index()) else { return };
                    match line {
                        Line::Group(g) | Line::Total(g) => {
                            let total = matches!(line, Line::Total(_));
                            for (k, &c) in cols.iter().enumerate() {
                                row.col(|ui| {
                                    let v = table.totals_text(c, &g.totals);
                                    let text = if k == 0 {
                                        let name = if total {
                                            format!("Total ({})", g.totals.count)
                                        } else {
                                            format!(
                                                "{}{} ({})",
                                                "  ".repeat(g.level),
                                                group_name(&table, g),
                                                g.totals.count
                                            )
                                        };
                                        if v.is_empty() { name } else { format!("{name}  {v}") }
                                    } else {
                                        v
                                    };
                                    let rt = RichText::new(text).strong();
                                    ui.label(if total { rt.color(t.accent_text) } else { rt });
                                });
                            }
                        }
                        Line::Markup(i) => {
                            let Some(m) = markups.get(*i) else { return };
                            row.set_selected(selection.contains(&m.id));
                            let mut cell_resps: Vec<egui::Response> = Vec::new();
                            for &c in &cols {
                                let Some(col) = table.columns().get(c) else { continue };
                                let cell = table.cell(*i, c);
                                row.col(|ui| {
                                    let editing_here = editing
                                        .as_ref()
                                        .is_some_and(|(id, cid, _)| *id == m.id && *cid == col.id);
                                    if editing_here {
                                        let mut text = editing.as_ref().map(|e| e.2.clone()).unwrap_or_default();
                                        let r =
                                            ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
                                        r.request_focus();
                                        if r.lost_focus() {
                                            acts.push(Act::SetCell(m.id.clone(), col.id.clone(), text));
                                        } else {
                                            acts.push(Act::StartEdit(m.id.clone(), col.id.clone(), text));
                                        }
                                        return;
                                    }
                                    match col.edit {
                                        CellEdit::Check => {
                                            let mut on = !cell.text.is_empty() && cell.text != "0";
                                            if ui.checkbox(&mut on, "").changed() {
                                                acts.push(Act::SetCell(
                                                    m.id.clone(),
                                                    col.id.clone(),
                                                    if on { "1".into() } else { String::new() },
                                                ));
                                            }
                                        }
                                        CellEdit::Choice => {
                                            let choices = table.choices(*i, c);
                                            let shown = if cell.text.is_empty() { "-" } else { cell.text.as_str() };
                                            let b = egui::Button::new(RichText::new(shown)).frame(false);
                                            let r = ui.add(b).on_hover_text("Click to choose");
                                            egui::Popup::menu(&r).show(|ui| {
                                                for ch in choices {
                                                    let label =
                                                        if ch.is_empty() { "(none)".into() } else { ch.clone() };
                                                    if ui.selectable_label(cell.text == ch, label).clicked() {
                                                        acts.push(Act::SetCell(m.id.clone(), col.id.clone(), ch));
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        }
                                        _ if col.id == "color" => {
                                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                                            ui.painter().rect_filled(r, 2.0, color32(&m.color, 1.0));
                                            ui.painter().rect_stroke(
                                                r,
                                                2.0,
                                                egui::Stroke::new(1.0, Color32::from_black_alpha(80)),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        _ if not_dominant.contains(i) && MEASURE_COLUMNS.contains(&col.id.as_str()) => {
                                            // The group's dominant markup carries the value.
                                            ui.label("");
                                        }
                                        _ if col.id == "comments" && lp.rich_comments && !m.rich.is_empty() => {
                                            let mut job = crate::richedit::layout_job(
                                                &m.contents,
                                                &m.text,
                                                &m.rich,
                                                12.0,
                                                if lp.wrap_comments {
                                                    ui.available_width()
                                                } else {
                                                    f32::INFINITY
                                                },
                                            );
                                            job.wrap.max_rows = if lp.wrap_comments { 8 } else { 1 };
                                            let lr = ui.add(egui::Label::new(job).sense(Sense::click()));
                                            if lr.clicked() {
                                                acts.push(Act::Select(*i, add));
                                            }
                                        }
                                        edit => {
                                            let mut rt = RichText::new(&cell.text);
                                            if col.markup_color {
                                                rt = rt.color(color32(&m.color, 1.0));
                                            }
                                            if cell.error {
                                                rt = rt.color(Color32::from_rgb(0xB0, 0x20, 0x20));
                                            }
                                            let lr = if col.right_align {
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.add(egui::Label::new(rt).sense(Sense::click()))
                                                })
                                                .inner
                                            } else if col.id == "comments" {
                                                let l = egui::Label::new(rt).sense(Sense::click());
                                                ui.add(if lp.wrap_comments { l.wrap() } else { l.truncate() })
                                            } else {
                                                ui.add(egui::Label::new(rt).sense(Sense::click()))
                                            };
                                            cell_resps.push(lr.clone());
                                            if lr.double_clicked() && edit != CellEdit::None {
                                                let start = if col.id == "comments" {
                                                    m.contents.clone()
                                                } else {
                                                    cell.text.clone()
                                                };
                                                acts.push(Act::StartEdit(m.id.clone(), col.id.clone(), start));
                                            } else if lr.clicked() {
                                                acts.push(Act::Select(*i, add));
                                            }
                                        }
                                    }
                                });
                            }
                            let resp = row.response();
                            if resp.clicked() {
                                acts.push(Act::Select(*i, add));
                            }
                            // A right-click on a cell's text opens the row's menu too.
                            let resp = cell_resps.into_iter().fold(resp, |a, b| a.union(b));
                            resp.context_menu(|ui| {
                                if !selection.contains(&m.id) {
                                    acts.push(Act::Select(*i, false));
                                }
                                ui.menu_button("Status", |ui| {
                                    let all = markupcraft_model::review_statuses()
                                        .iter()
                                        .map(|s| s.to_string())
                                        .chain(custom_statuses.iter().cloned());
                                    for st in all {
                                        if ui.button(&st).clicked() {
                                            acts.push(Act::SetCell(m.id.clone(), "status".into(), st));
                                            ui.close();
                                        }
                                    }
                                });
                                let check = if m.checked { "Clear Checkmark" } else { "Checkmark" };
                                if ui.button(check).clicked() {
                                    acts.push(Act::SetCell(
                                        m.id.clone(),
                                        "checkmark".into(),
                                        if m.checked { String::new() } else { "1".into() },
                                    ));
                                    ui.close();
                                }
                                let (label, cmd) = if m.locked() {
                                    ("Unlock", "markup.unlock")
                                } else {
                                    ("Lock", "markup.lock")
                                };
                                if ui.button(label).clicked() {
                                    acts.push(Act::Command(cmd));
                                    ui.close();
                                }
                                ui.menu_button("Layer", |ui| {
                                    if ui.button("None").clicked() {
                                        acts.push(Act::Layer(String::new()));
                                        ui.close();
                                    }
                                    for l in &layer_names {
                                        if ui.button(l).clicked() {
                                            acts.push(Act::Layer(l.clone()));
                                            ui.close();
                                        }
                                    }
                                });
                                if ui.button("Create Legend").clicked() {
                                    acts.push(Act::Legend);
                                    ui.close();
                                }
                                if ui.button("Copy Rows").clicked() {
                                    acts.push(Act::CopyRows);
                                    ui.close();
                                }
                                if ui.button("Delete").clicked() {
                                    acts.push(Act::Command("edit.delete"));
                                    ui.close();
                                }
                                if ui.button("Properties").clicked() {
                                    acts.push(Act::Command("panel-properties"));
                                    ui.close();
                                }
                            });
                        }
                    }
                };
                if lp.wrap_comments {
                    body.heterogeneous_rows(heights.into_iter(), row_ui);
                } else {
                    body.rows(20.0, lines.len(), row_ui);
                }
            });
    });
    ui.scope_builder(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Total ({})", root.totals.count)).strong());
            for s in &summary {
                ui.label(RichText::new(s).strong().color(t.accent_text));
            }
            if !selection.is_empty() {
                ui.label(RichText::new(format!("{} selected", selection.len())).color(t.text_muted));
            }
        });
    });

    // ---- apply ----------------------------------------------------------------------------
    // The last column fills the rest of the panel: its width is not the user's choice.
    widths.pop();
    let down = ui.input(|i| i.pointer.any_down());
    list.prefs.note_widths(&widths, down);
    let mut commands: Vec<&'static str> = Vec::new();
    for a in acts {
        match a {
            Act::Select(i, add) => select_row(doc, i, add),
            Act::Sort(id) => {
                if list.view.sort_column == id {
                    list.view.sort_descending = !list.view.sort_descending;
                } else {
                    list.view.sort_column = id;
                    list.view.sort_descending = false;
                }
            }
            Act::GroupBy(id) => list.view.group_by = vec![id],
            Act::Hide(id) => list.view.visible.retain(|v| *v != id),
            Act::Filter(col, value, on) => {
                let table = MarkupTable::new(doc.session.doc());
                let all: BTreeSet<String> = table
                    .column_index(&col)
                    .map(|c| table.distinct_values(c, &View::default()))
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                let set = list.view.filters.entry(col.clone()).or_insert(all.clone());
                if on {
                    set.insert(value);
                } else {
                    set.remove(&value);
                }
                if *set == all {
                    list.view.filters.remove(&col);
                }
            }
            Act::ClearFilter(col) => {
                list.view.filters.remove(&col);
            }
            Act::StartEdit(id, col, text) => list.editing = Some((id, col, text)),
            Act::SetCell(id, col, text) => {
                list.editing = None;
                let r = doc.session.set_cell(&id, &col, &text);
                if let Ok(false) = r {
                    log::info!("the list did not take {text:?} for {col}");
                }
                if let Err(e) = r {
                    log::info!("list: {e}");
                }
            }
            Act::Command(c) => commands.push(c),
            Act::Layer(name) => app.status = crate::more::list_layer(doc, &name),
            Act::Legend => app.status = crate::more::list_legend(doc),
            Act::CopyRows => {
                let sel = doc.selection().to_vec();
                let idx: Vec<usize> = doc
                    .session
                    .doc()
                    .markups
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| sel.contains(&m.id))
                    .map(|(i, _)| i)
                    .collect();
                let table = MarkupTable::new(doc.session.doc());
                let text = super::list_views::rows_tsv(&table, &list.view, &idx);
                ui.ctx().copy_text(text);
                app.status = format!("Copied {} to the clipboard", actions::plural(idx.len(), "row"));
            }
        }
    }
    let mut editor_out = EditorOut::default();
    if let Some(cols) = list.columns_editor.as_mut() {
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Manage Columns")
            .open(&mut open)
            .default_width(520.0)
            .show(ui.ctx(), |ui| {
                apply = columns_editor(ui, cols, &mut editor_out);
            });
        if editor_out.save_profile {
            app.toolchest.save_profile_columns(cols);
            app.status = format!("Saved {} to the profile", actions::plural(cols.len(), "column"));
        }
        if editor_out.load_profile {
            let mut added = 0;
            for c in app.toolchest.extras.columns.clone() {
                if !cols.iter().any(|o| o.name == c.name) {
                    let id = make_column_id(&c.name, cols);
                    cols.push(CustomColumn { id, ..c });
                    added += 1;
                }
            }
            app.status = format!("Added {} from the profile", actions::plural(added, "column"));
        }
        if apply {
            let r = doc.session.set_custom_columns(cols.clone());
            match r {
                Ok(()) => {
                    for c in cols.iter() {
                        let id = format!("c:{}", c.id);
                        if !list.view.visible.contains(&id) {
                            list.view.visible.push(id);
                        }
                    }
                    list.columns_editor = None;
                }
                Err(e) => log::info!("columns: {e}"),
            }
        } else if !open {
            list.columns_editor = None;
        }
    }
    if let Some(id) = editor_out.import_csv {
        app.dialogs.open(Purpose::Edit("choice_csv", id), dialogs::CSV, false);
    }
    let name = doc.name.trim_end_matches(".pdf").to_string();
    for c in commands {
        if c == "panel-properties" {
            app.show_panel("properties");
        } else {
            app.queue(c);
        }
    }
    if let Some(p) = export {
        let (filter, ext) = match p {
            Purpose::ExportXml => (dialogs::XML, "xml"),
            _ => (dialogs::CSV, "csv"),
        };
        let suffix = if p == Purpose::ExportTotals {
            " totals"
        } else {
            " markups"
        };
        app.dialogs.save(p, filter, &format!("{name}{suffix}.{ext}"));
    }
}

fn select_row(doc: &mut DocTab, i: usize, add: bool) {
    let Some(m) = doc.session.doc().markups.get(i).cloned() else {
        return;
    };
    if add {
        actions::toggle_selected(&mut doc.session, &m.id);
    } else {
        actions::select(&mut doc.session, vec![m.id.clone()]);
    }
    let b = markup_bbox(&m);
    if !crate::features::more6::prefs::zoom_to_selected() {
        return;
    }
    if let Some(render) = doc.render.as_ref() {
        doc.view.center_on(
            m.page,
            markupcraft_geom::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
            render.pages(),
        );
    }
}

/// The Manage Columns editor; true when Apply was clicked.
/// What the Manage Columns window asks for besides Apply.
#[derive(Debug, Default)]
struct EditorOut {
    /// import a Choice column's items from CSV (its id)
    import_csv: Option<String>,
    save_profile: bool,
    load_profile: bool,
}

fn columns_editor(ui: &mut egui::Ui, cols: &mut Vec<CustomColumn>, out: &mut EditorOut) -> bool {
    let mut remove = None;
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        egui::Grid::new("columns-editor")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                ui.label(RichText::new("Name").strong());
                ui.label(RichText::new("Type").strong());
                ui.label(RichText::new("Decimals").strong());
                ui.label(RichText::new("Total").strong());
                ui.label(RichText::new("Formula / choices").strong());
                ui.label("");
                ui.end_row();
                for (i, c) in cols.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(120.0));
                    egui::ComboBox::from_id_salt(("col-type", i))
                        .selected_text(c.kind.name())
                        .show_ui(ui, |ui| {
                            for k in [
                                ColumnType::Text,
                                ColumnType::Number,
                                ColumnType::Currency,
                                ColumnType::Percent,
                                ColumnType::Date,
                                ColumnType::Choice,
                                ColumnType::Formula,
                                ColumnType::Checkmark,
                            ] {
                                ui.selectable_value(&mut c.kind, k, k.name());
                            }
                        });
                    ui.add(egui::DragValue::new(&mut c.decimals).range(0..=6));
                    ui.checkbox(&mut c.total, "");
                    match c.kind {
                        ColumnType::Formula => {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut c.formula)
                                        .hint_text("e.g. Measurement * Unit Cost")
                                        .desired_width(180.0),
                                );
                                egui::ComboBox::from_id_salt(("col-display", i))
                                    .selected_text(c.display.name())
                                    .show_ui(ui, |ui| {
                                        for k in [ColumnType::Number, ColumnType::Currency, ColumnType::Percent] {
                                            ui.selectable_value(&mut c.display, k, k.name());
                                        }
                                    });
                            });
                        }
                        ColumnType::Date => {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("col-date", i))
                                    .selected_text(c.date_format.as_str())
                                    .show_ui(ui, |ui| {
                                        for f in markupcraft_model::columns::DATE_FORMATS {
                                            ui.selectable_value(&mut c.date_format, f.to_string(), *f);
                                        }
                                    });
                                let mut today = c.default_value == markupcraft_model::columns::TODAY;
                                if ui
                                    .checkbox(&mut today, "Default: current date")
                                    .on_hover_text("New rows show the date the markup was made")
                                    .changed()
                                {
                                    c.default_value = if today {
                                        markupcraft_model::columns::TODAY.into()
                                    } else {
                                        String::new()
                                    };
                                }
                            });
                        }
                        ColumnType::Choice => {
                            let mut text: String =
                                c.items.iter().map(|i| i.text.clone()).collect::<Vec<_>>().join(", ");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut text)
                                        .hint_text("one, two, three")
                                        .desired_width(180.0),
                                )
                                .changed()
                            {
                                c.items = text
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(|s| markupcraft_model::ChoiceItem {
                                        text: s.to_string(),
                                        ..Default::default()
                                    })
                                    .collect();
                            }
                            if ui
                                .small_button("Import...")
                                .on_hover_text("Items from a CSV file: item, subject, value")
                                .clicked()
                            {
                                out.import_csv = Some(c.id.clone());
                            }
                        }
                        _ => {
                            ui.label("");
                        }
                    }
                    if crate::icons::button(ui, "trash-2", 20.0, false, "Remove column").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
    });
    if let Some(i) = remove
        && i < cols.len()
    {
        cols.remove(i);
    }
    let mut apply = false;
    ui.horizontal(|ui| {
        if ui.button("Add Column").clicked() {
            let name = format!("Column {}", cols.len() + 1);
            let id = make_column_id(&name, cols);
            cols.push(CustomColumn {
                id,
                name,
                ..Default::default()
            });
        }
        if ui.button("Apply").clicked() {
            apply = true;
        }
        ui.separator();
        if ui
            .button("Save to Profile")
            .on_hover_text("Keep these columns for other documents")
            .clicked()
        {
            out.save_profile = true;
        }
        if ui
            .button("Load from Profile")
            .on_hover_text("Add the columns saved to the profile")
            .clicked()
        {
            out.load_profile = true;
        }
    });
    apply
}

/// Columns that show a measurement value (blank on a group's other markups when only the
/// dominant markup shows it).
const MEASURE_COLUMNS: &[&str] = &["measurement", "length", "area", "volume", "perimeter", "depth"];

/// The markups the list's filters (and quick search) hide, by page and box: the page dims
/// them by Preferences > Markups List's percentage.
pub fn filtered_out(doc: &DocTab, view: &View) -> Vec<(usize, markupcraft_geom::Rect)> {
    if view.filters.is_empty() && view.search.trim().is_empty() {
        return Vec::new();
    }
    let mut v = view.clone();
    v.scope = Scope::AllPages;
    let table = MarkupTable::new(doc.session.doc());
    let shown: std::collections::HashSet<usize> = table.build(&v).all_rows().into_iter().collect();
    doc.session
        .doc()
        .markups
        .iter()
        .enumerate()
        .filter(|(i, _)| !shown.contains(i))
        .map(|(_, m)| (m.page, actions::markup_bbox(m)))
        .take(20_000)
        .collect()
}

/// Write the list (as the panel shows it) to `path`; returns the status text.
pub fn export(app: &mut AppState, purpose: &Purpose, path: &Path) -> String {
    let Some(doc) = app.docs.get(app.active) else {
        return "No document open".into();
    };
    let mut view = scoped(&app.list.view, doc);
    if !app.shell.prefs.more.markups_list.exclude_filtered_from_export {
        // Preferences > Markups List: exports keep the markups the filters hide.
        view.filters.clear();
        view.search.clear();
    }
    let table = MarkupTable::new(doc.session.doc());
    let root = table.build(&view);
    let text = match purpose {
        Purpose::ExportTotals => totals_csv(&table, &root, &view.visible),
        Purpose::ExportXml => table_xml(&table, &root, &view.visible, &doc.name),
        _ => table_csv(&table, &root, &view.visible, true),
    };
    match crate::chest::write_atomic(path, text.as_bytes()) {
        Ok(()) => format!(
            "Exported {} to {}",
            actions::plural(root.totals.count, "markup"),
            path.display()
        ),
        Err(e) => format!("Export failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_writes_the_list_with_totals() {
        let mut app = AppState {
            threads: 0,
            ..Default::default()
        };
        app.open_bytes("sample.pdf", None, markupcraft_render::synthetic::sample_pdf())
            .unwrap();
        app.list.view.group_by = vec!["type".into()];
        let dir = std::env::temp_dir().join(format!("markupcraft-list-{}", std::process::id()));
        let csv = dir.join("list.csv");
        let s = export(&mut app, &Purpose::ExportCsv, &csv);
        assert!(s.starts_with("Exported 6 markups"), "{s}");
        let text = std::fs::read_to_string(&csv).unwrap();
        assert!(text.starts_with("Subject,"), "{text}");
        assert!(text.contains("Total (6)"), "{text}");
        let xml = dir.join("list.xml");
        export(&mut app, &Purpose::ExportXml, &xml);
        assert!(std::fs::read_to_string(&xml).unwrap().contains("<MarkupSummary"));
        let tot = dir.join("totals.csv");
        export(&mut app, &Purpose::ExportTotals, &tot);
        assert!(std::fs::read_to_string(&tot).unwrap().starts_with("Group,Count"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
