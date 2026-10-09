//! Search (Alt+1): text search of the page text and markup comments, and Visual Search for a
//! symbol picked on the page. Results list with check boxes, Previous/Next, hits highlighted on
//! the pages, and bulk actions (highlight, count, redact).

use egui::RichText;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::commands::alt;
use crate::features::search::{self, Bulk, Scope};

pub static PANEL: PanelDef = PanelDef {
    id: "search",
    title: "Search",
    icon: "search",
    slot: Slot::Left,
    keys: alt(egui::Key::Num1),
    ui,
};

fn scope_ui(ui: &mut egui::Ui, scope: &mut Scope, range: &mut String, salt: &str) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(salt)
            .selected_text(match scope {
                Scope::AllPages => "All pages",
                Scope::CurrentPage => "Current page",
                Scope::Pages => "Pages",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(scope, Scope::AllPages, "All pages");
                ui.selectable_value(scope, Scope::CurrentPage, "Current page");
                ui.selectable_value(scope, Scope::Pages, "Pages");
            });
        if *scope == Scope::Pages {
            ui.add(
                egui::TextEdit::singleline(range)
                    .desired_width(80.0)
                    .hint_text("1-3, 5"),
            );
        }
    });
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.has_doc() {
        super::empty(ui, "No document open.");
        return;
    }
    ui.horizontal(|ui| {
        let s = &mut app.features.search;
        ui.selectable_value(&mut s.visual, false, "Text");
        ui.selectable_value(&mut s.visual, true, "Visual Search");
    });
    ui.separator();
    if app.features.search.visual {
        visual_ui(app, ui);
    } else {
        text_ui(app, ui);
    }
    results_ui(app, ui);
}

fn text_ui(app: &mut AppState, ui: &mut egui::Ui) {
    let mut go = false;
    {
        let s = &mut app.features.search;
        ui.horizontal(|ui| {
            let id = egui::Id::new("search-query");
            let r = ui.add(
                egui::TextEdit::singleline(&mut s.query)
                    .id(id)
                    .desired_width(ui.available_width() - 70.0)
                    .hint_text("Find text"),
            );
            if std::mem::take(&mut s.focus) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if ui.button("Search").clicked() {
                go = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut s.case_sensitive, "Match case");
            ui.checkbox(&mut s.whole_words, "Whole words");
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut s.page_text, "Page text");
            ui.checkbox(&mut s.markups, "Markups");
        });
        scope_ui(ui, &mut s.scope, &mut s.range, "search-scope");
    }
    if go {
        search::run_text(app);
    }
}

fn visual_ui(app: &mut AppState, ui: &mut egui::Ui) {
    let (mut pick, mut go) = (false, false);
    {
        let s = &mut app.features.search;
        ui.horizontal(|ui| {
            if ui
                .button("Select Region")
                .on_hover_text("Drag a box around one symbol")
                .clicked()
            {
                pick = true;
            }
            match s.region {
                Some((p, r)) => ui.label(format!("page {}, {:.0} x {:.0} pt", p + 1, r.width(), r.height())),
                None => ui.label(RichText::new("no region yet").weak()),
            };
        });
        ui.horizontal(|ui| {
            ui.label("Sensitivity");
            ui.add(egui::Slider::new(&mut s.sensitivity, 0.0..=1.0).fixed_decimals(2));
        });
        ui.checkbox(&mut s.rotations, "Find rotated (90, 180, 270)");
        scope_ui(ui, &mut s.visual_scope, &mut s.range, "visual-scope");
        ui.add_enabled_ui(s.region.is_some(), |ui| {
            if ui.button("Search").clicked() {
                go = true;
            }
        });
    }
    if pick {
        search::pick_region(app);
    }
    if go {
        search::run_visual(app);
    }
}

fn results_ui(app: &mut AppState, ui: &mut egui::Ui) {
    ui.separator();
    let (mut goto, mut stepv, mut bulk, mut clear) = (None, 0isize, None, false);
    {
        let s = &mut app.features.search;
        if !s.message.is_empty() {
            ui.label(RichText::new(&s.message).small());
        }
        ui.horizontal_wrapped(|ui| {
            let has = !s.results().is_empty();
            ui.add_enabled_ui(has, |ui| {
                if ui.small_button("< Prev").clicked() {
                    stepv = -1;
                }
                if ui.small_button("Next >").clicked() {
                    stepv = 1;
                }
                if ui.small_button("Check all").clicked() {
                    let all = s.results().iter().all(|h| h.checked);
                    for h in s.results_mut() {
                        h.checked = !all;
                    }
                }
                if ui.small_button("Clear").clicked() {
                    clear = true;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(s.results().iter().any(|h| h.checked && h.markup.is_none()), |ui| {
                if !s.visual
                    && ui
                        .small_button("Highlight")
                        .on_hover_text("Highlight checked results")
                        .clicked()
                {
                    bulk = Some(Bulk::Highlight);
                }
                if ui
                    .small_button("Count")
                    .on_hover_text("Apply a Count to checked results")
                    .clicked()
                {
                    bulk = Some(Bulk::Count);
                }
                if ui
                    .small_button("Redact")
                    .on_hover_text("Mark checked results for redaction")
                    .clicked()
                {
                    bulk = Some(Bulk::Redact);
                }
            });
        });
        let current = s.current;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut last_page = None;
            for (i, h) in s.results_mut().iter_mut().enumerate().take(5_000) {
                if last_page != Some(h.page) {
                    ui.label(RichText::new(format!("Page {}", h.page + 1)).strong().small());
                    last_page = Some(h.page);
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut h.checked, "");
                    let text = if h.context.is_empty() {
                        h.text.clone()
                    } else {
                        format!("{}  {}", h.text, h.context)
                    };
                    let label = if h.markup.is_some() {
                        format!("[markup] {text}")
                    } else {
                        text
                    };
                    if ui.selectable_label(current == Some(i), label).clicked() {
                        goto = Some(i);
                    }
                });
            }
        });
    }
    if let Some(i) = goto {
        search::go_to(app, i);
    }
    if stepv != 0 {
        search::step(app, stepv);
    }
    if let Some(b) = bulk {
        search::apply(app, b);
    }
    if clear {
        search::clear(app);
    }
}
