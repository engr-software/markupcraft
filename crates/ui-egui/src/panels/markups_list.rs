//! Markups List (Alt+L): every markup in the document as a table. Click a row to select the
//! markup and bring it into view; Shift/Ctrl+click adds to the selection; click a header to
//! sort. Filters, grouping, custom columns and export arrive with the Markups List wave.

use egui::{Color32, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use markupcraft_model::Markup;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::actions::markup_bbox;
use crate::commands::alt;
use crate::theme::{Tokens, color32};

pub static PANEL: PanelDef = PanelDef {
    id: "markups",
    title: "Markups List",
    icon: "list",
    slot: Slot::Bottom,
    keys: alt(egui::Key::L),
    ui,
};

/// Header, width, cell text.
type ColumnDef = (&'static str, f32, fn(&Markup) -> String);

/// The columns, in order.
const COLUMNS: &[ColumnDef] = &[
    ("Subject", 140.0, |m| m.subject.clone()),
    ("Page", 44.0, |m| format!("{}", m.page + 1)),
    ("Label", 110.0, |m| m.label.clone()),
    ("Measurement", 110.0, |m| m.quantity_text()),
    ("Type", 90.0, |m| m.kind.name().to_string()),
    ("Author", 100.0, |m| m.author.clone()),
    ("Date", 130.0, |m| pretty_date(&m.modified)),
    ("Color", 50.0, |m| m.color.hex()),
    ("Status", 80.0, |m| m.status.clone()),
    ("Layer", 90.0, |m| m.layer.clone()),
    ("Comments", 220.0, |m| {
        m.contents.lines().next().unwrap_or_default().to_string()
    }),
];

/// `D:20261009153000Z` -> `2026-10-09 15:30`.
fn pretty_date(d: &str) -> String {
    let s = d.strip_prefix("D:").unwrap_or(d);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    match digits.len() {
        n if n >= 12 => format!(
            "{}-{}-{} {}:{}",
            &digits[0..4],
            &digits[4..6],
            &digits[6..8],
            &digits[8..10],
            &digits[10..12]
        ),
        n if n >= 8 => format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8]),
        _ => d.to_string(),
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let (sort_col, ascending) = app.list_sort;
    let Some(doc) = app.doc_mut() else {
        super::empty(ui, "No document open.");
        return;
    };
    let markups = &doc.session.doc.markups;
    let mut order: Vec<usize> = (0..markups.len()).collect();
    if let Some((_, _, cell)) = COLUMNS.get(sort_col) {
        order.sort_by(|a, b| {
            let (ma, mb) = (&markups[*a], &markups[*b]);
            let o = if sort_col == 1 {
                ma.page.cmp(&mb.page)
            } else {
                cell(ma).to_lowercase().cmp(&cell(mb).to_lowercase())
            };
            if ascending { o } else { o.reverse() }
        });
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} markups", markups.len()))
                .color(t.text_muted)
                .size(11.0),
        );
        if !doc.selection.is_empty() {
            ui.label(
                RichText::new(format!("{} selected", doc.selection.len()))
                    .color(t.text_muted)
                    .size(11.0),
            );
        }
    });
    let mut clicked: Option<usize> = None;
    let mut new_sort = None;
    let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    let selection = &doc.selection;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .auto_shrink([false, false])
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .columns(
            Column::initial(100.0).at_least(30.0).clip(true),
            COLUMNS.len().saturating_sub(1),
        )
        .column(Column::remainder().at_least(60.0).clip(true))
        .header(22.0, |mut row| {
            for (i, (name, _, _)) in COLUMNS.iter().enumerate() {
                row.col(|ui| {
                    let arrow = if i == sort_col {
                        if ascending { " ^" } else { " v" }
                    } else {
                        ""
                    };
                    if ui
                        .add(egui::Button::new(RichText::new(format!("{name}{arrow}")).strong()).frame(false))
                        .clicked()
                    {
                        new_sort = Some(if i == sort_col { (i, !ascending) } else { (i, true) });
                    }
                });
            }
        })
        .body(|body| {
            body.rows(20.0, order.len(), |mut row| {
                let Some(m) = order.get(row.index()).and_then(|i| markups.get(*i)) else {
                    return;
                };
                row.set_selected(selection.contains(&m.id));
                for (ci, (_, _, cell)) in COLUMNS.iter().enumerate() {
                    row.col(|ui| {
                        if ci == 7 {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, color32(&m.color, 1.0));
                            ui.painter().rect_stroke(
                                r,
                                2.0,
                                egui::Stroke::new(1.0, Color32::from_black_alpha(80)),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            ui.label(cell(m));
                        }
                    });
                }
                if row.response().clicked() {
                    clicked = order.get(row.index()).copied();
                }
            });
        });
    if let Some(s) = new_sort {
        app.list_sort = s;
    }
    let Some(doc) = app.doc_mut() else { return };
    if let Some(i) = clicked
        && let Some(m) = doc.session.doc.markups.get(i)
    {
        let id = m.id.clone();
        if add {
            if let Some(k) = doc.selection.iter().position(|x| *x == id) {
                doc.selection.remove(k);
            } else {
                doc.selection.push(id);
            }
        } else {
            doc.selection = vec![id];
        }
        let b = markup_bbox(m);
        let page = m.page;
        if let Some(render) = doc.render.as_ref() {
            doc.view.center_on(
                page,
                markupcraft_geom::Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
                render.pages(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_read_well() {
        assert_eq!(super::pretty_date("D:20261009153000Z"), "2026-10-09 15:30");
        assert_eq!(super::pretty_date("D:2026"), "D:2026");
        assert_eq!(super::pretty_date(""), "");
    }
}
