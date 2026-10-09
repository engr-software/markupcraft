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

fn scope_ui(ui: &mut egui::Ui, scope: &mut Scope, range: &mut String, salt: &str, files: bool) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(salt)
            .selected_text(match scope {
                Scope::AllPages => "All pages",
                Scope::CurrentPage => "Current page",
                Scope::Pages => "Pages",
                Scope::AllOpen => "All open documents",
                Scope::CurrentSet => "Current Set",
                Scope::Folder => "Folder",
                Scope::Recent => "Recent files",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(scope, Scope::AllPages, "All pages");
                ui.selectable_value(scope, Scope::CurrentPage, "Current page");
                ui.selectable_value(scope, Scope::Pages, "Pages");
                if files {
                    ui.selectable_value(scope, Scope::AllOpen, "All open documents");
                    ui.selectable_value(scope, Scope::CurrentSet, "Current Set");
                    ui.selectable_value(scope, Scope::Folder, "Folder");
                    ui.selectable_value(scope, Scope::Recent, "Recent files");
                }
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
    let (mut go, mut replace) = (false, false);
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
            ui.checkbox(&mut s.file_names, "File names");
            ui.checkbox(&mut s.properties, "Properties");
            ui.checkbox(&mut s.form_fields, "Form fields");
        });
        scope_ui(ui, &mut s.scope, &mut s.range, "search-scope", true);
        if s.scope == Scope::Folder {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut s.folder)
                        .desired_width(150.0)
                        .hint_text("folder"),
                );
                ui.checkbox(&mut s.subfolders, "Subfolders");
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut s.replace)
                    .desired_width(120.0)
                    .hint_text("Replace with"),
            );
            ui.checkbox(&mut s.fallback, "Font fallback")
                .on_hover_text("Lines whose font cannot show the new text use Helvetica (else they are skipped)");
            let can = !s.hits.is_empty() && !s.query.trim().is_empty();
            ui.add_enabled_ui(can, |ui| {
                if ui.button("Replace Checked").clicked() {
                    replace = true;
                }
            });
        });
    }
    if go {
        search::run_text(app);
    }
    if replace {
        search::replace_checked(app);
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
        ui.checkbox(&mut s.rotations, "Find rotated");
        if s.rotations {
            ui.checkbox(&mut s.fine_rotations, "In 45-degree steps");
        }
        ui.checkbox(&mut s.color_filter, "Filter by color");
        ui.checkbox(&mut s.limit_to_selection, "Limit to selection");
        scope_ui(ui, &mut s.visual_scope, &mut s.range, "visual-scope", false);
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
    // Visual results show a thumbnail of each match (made on first show, at most 200).
    let thumbs: Vec<Option<egui::TextureHandle>> = if app.features.search.visual {
        let n = app.features.search.visual_hits.len().min(200);
        let ctx = ui.ctx().clone();
        (0..n).map(|i| search::thumb(app, &ctx, i)).collect()
    } else {
        Vec::new()
    };
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
                    if let Some(Some(t)) = thumbs.get(i) {
                        let size = t.size_vec2();
                        let k = 40.0 / size.x.max(size.y).max(1.0);
                        if ui
                            .add(egui::Image::new((t.id(), size * k)).sense(egui::Sense::click()))
                            .clicked()
                        {
                            goto = Some(i);
                        }
                    }
                    if let Some(f) = &h.file {
                        ui.label(
                            RichText::new(
                                f.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            )
                            .small()
                            .weak(),
                        );
                    }
                    if !matches!(h.source.as_str(), "text" | "markup" | "visual" | "") {
                        ui.label(RichText::new(format!("[{}]", h.source)).small().weak());
                    }
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
