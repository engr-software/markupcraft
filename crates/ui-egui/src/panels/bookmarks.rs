//! Bookmarks (Alt+B): the PDF outline. Click an entry to go to its page; add (Ctrl+B), rename,
//! re-target to the current page, delete, reorder and nest, copy, expand and collapse (saved in
//! the file), create one per page from the page labels or from a title-block region (AutoMark),
//! properties (colour, bold, italic; Ctrl+click selects many), the action (Edit Action), saved
//! structures, Audit (broken ones marked), Export, or clear them all. Every edit is an engine
//! call and undoable.

use egui::RichText;
use markupcraft_engine::bookmarks::{BookmarkItem, BookmarkTitles};
use markupcraft_engine::bookmarks_more::BookmarkStyle;
use markupcraft_model::Color;

use super::{PanelDef, Slot};
use crate::commands::alt;
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "bookmarks",
    title: "Bookmarks",
    icon: "bookmark",
    slot: Slot::Left,
    keys: alt(egui::Key::B),
    ui,
};

/// The panel's selection and rename field (kept in `AppState::features`).
#[derive(Default)]
pub struct Fields {
    pub selected: Option<Vec<usize>>,
    pub rename: String,
    /// More bookmarks selected with Ctrl+click (for Properties).
    pub also: Vec<Vec<usize>>,
    pub color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    /// Bookmarks Audit found broken: (path, reason).
    pub broken: Vec<(Vec<usize>, String)>,
    pub audited: bool,
    /// A bookmark was just added: put the cursor in its title (Revu: immediately editable).
    pub edit_now: bool,
}

enum Act {
    /// Run the bookmark's action (its page and zoom, view, Place, file or web address).
    Follow(Vec<usize>, Option<usize>),
    Toggle(Vec<usize>, bool),
    Add,
    Rename(Vec<usize>, String),
    SetPage(Vec<usize>),
    Delete(Vec<usize>),
    Move(Vec<usize>, Vec<usize>, Option<usize>),
    FromPages,
    Clear,
    Copy(Vec<usize>),
    Style(Vec<Vec<usize>>),
    Action(Vec<usize>),
    AutoMark,
    Audit,
    SaveStructure,
    ApplyStructure,
    Export,
}

fn split(path: &[usize]) -> (Vec<usize>, usize) {
    match path.split_last() {
        Some((last, parent)) => (parent.to_vec(), *last),
        None => (Vec::new(), 0),
    }
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let Some(d) = app.doc() else {
        super::empty(ui, "No document open.");
        return;
    };
    let items: Vec<BookmarkItem> = d.session.bookmarks();
    let current = d.view.current;
    // Each bookmark's look (for the first 500).
    let styles: std::collections::HashMap<Vec<usize>, BookmarkStyle> = items
        .iter()
        .take(500)
        .filter_map(|b| {
            d.session
                .bookmark_details(&b.path)
                .ok()
                .map(|x| (b.path.clone(), x.style))
        })
        .collect();
    let mut f = std::mem::take(&mut app.features.bookmarks);
    let mut act = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Add")
            .on_hover_text("Bookmark the current page (Ctrl+B)")
            .clicked()
        {
            act = Some(Act::Add);
        }
        if ui
            .button("From Pages")
            .on_hover_text("One bookmark per page, titled by its label")
            .clicked()
        {
            act = Some(Act::FromPages);
        }
        if ui
            .button("AutoMark")
            .on_hover_text("Bookmarks from the text in a title-block region you drag")
            .clicked()
        {
            act = Some(Act::AutoMark);
        }
        ui.add_enabled_ui(!items.is_empty(), |ui| {
            if ui.button("Clear").clicked() {
                act = Some(Act::Clear);
            }
            if ui
                .button("Audit")
                .on_hover_text("Find bookmarks that go nowhere")
                .clicked()
            {
                act = Some(Act::Audit);
            }
            if ui.button("Export...").clicked() {
                act = Some(Act::Export);
            }
        });
        ui.menu_button("Structures", |ui| {
            if ui.button("Save Structure...").clicked() {
                act = Some(Act::SaveStructure);
                ui.close();
            }
            if ui.button("Apply Structure...").clicked() {
                act = Some(Act::ApplyStructure);
                ui.close();
            }
        });
    });
    if f.audited {
        ui.label(
            RichText::new(if f.broken.is_empty() {
                "Audit: every bookmark goes somewhere".to_string()
            } else {
                format!("Audit: {} broken (marked !)", f.broken.len())
            })
            .small(),
        );
    }
    if let Some(sel) = f.selected.clone().filter(|s| items.iter().any(|i| &i.path == s)) {
        let (parent, idx) = split(&sel);
        let siblings = items
            .iter()
            .filter(|i| i.path.len() == sel.len() && i.path.get(..i.path.len() - 1) == Some(&parent[..]))
            .count();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Up").clicked() && idx > 0 {
                act = Some(Act::Move(sel.clone(), parent.clone(), Some(idx - 1)));
            }
            if ui.small_button("Down").clicked() && idx + 1 < siblings {
                act = Some(Act::Move(sel.clone(), parent.clone(), Some(idx + 1)));
            }
            if ui.small_button("Indent").clicked() && idx > 0 {
                let mut new_parent = parent.clone();
                new_parent.push(idx - 1);
                act = Some(Act::Move(sel.clone(), new_parent, None));
            }
            if ui.small_button("Outdent").clicked() && !parent.is_empty() {
                let (gp, pi) = split(&parent);
                act = Some(Act::Move(sel.clone(), gp, Some(pi + 1)));
            }
            if ui
                .small_button("Set Page")
                .on_hover_text("Go to the current page")
                .clicked()
            {
                act = Some(Act::SetPage(sel.clone()));
            }
            if ui.small_button("Delete").clicked() {
                act = Some(Act::Delete(sel.clone()));
            }
            if ui
                .small_button("Copy")
                .on_hover_text("A copy (with its children) after it")
                .clicked()
            {
                act = Some(Act::Copy(sel.clone()));
            }
            if ui
                .small_button("Action...")
                .on_hover_text("Where it goes (Edit Action)")
                .clicked()
            {
                act = Some(Act::Action(sel.clone()));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Properties:");
            let mut on = f.color.is_some();
            if ui.checkbox(&mut on, "").on_hover_text("Text colour").changed() {
                f.color = on.then_some(Color::rgb(0.8, 0.0, 0.0));
            }
            if let Some(c) = f.color.as_mut() {
                crate::features::color_edit(ui, c);
            }
            ui.toggle_value(&mut f.bold, RichText::new("B").strong());
            ui.toggle_value(&mut f.italic, RichText::new("I").italics());
            if ui.small_button("Apply").clicked() {
                let mut all = vec![sel.clone()];
                all.extend(f.also.iter().filter(|p| **p != sel).cloned());
                act = Some(Act::Style(all));
            }
        });
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut f.rename).desired_width(150.0));
            if std::mem::take(&mut f.edit_now) {
                r.request_focus();
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.small_button("Rename").clicked() || enter) && !f.rename.trim().is_empty() {
                act = Some(Act::Rename(sel.clone(), f.rename.trim().to_string()));
            }
        });
    }
    ui.separator();
    if items.is_empty() {
        super::empty(ui, "This document has no bookmarks.");
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Children of a collapsed entry are hidden.
        let mut hide_below: Option<usize> = None;
        for e in &items {
            let depth = e.path.len().saturating_sub(1);
            if let Some(h) = hide_below {
                if depth > h {
                    continue;
                }
                hide_below = None;
            }
            ui.horizontal(|ui| {
                ui.add_space(4.0 + 14.0 * depth.min(12) as f32);
                if e.children > 0 {
                    let icon = if e.open { "−" } else { "+" };
                    if ui.small_button(icon).clicked() {
                        act = Some(Act::Toggle(e.path.clone(), !e.open));
                    }
                } else {
                    ui.add_space(18.0);
                }
                let label = if e.title.trim().is_empty() {
                    "(untitled)"
                } else {
                    e.title.as_str()
                };
                let sel = f.selected.as_ref() == Some(&e.path) || f.also.contains(&e.path);
                let style = styles.get(&e.path).copied().unwrap_or_default();
                let mut text = RichText::new(label);
                if e.page == Some(current) || style.bold {
                    text = text.strong();
                }
                if style.italic {
                    text = text.italics();
                }
                if let Some(c) = style.color {
                    text = text.color(egui::Color32::from_rgb(
                        (c.r * 255.0) as u8,
                        (c.g * 255.0) as u8,
                        (c.b * 255.0) as u8,
                    ));
                }
                if let Some((_, why)) = f.broken.iter().find(|b| b.0 == e.path) {
                    ui.label(RichText::new("!").color(egui::Color32::RED).strong())
                        .on_hover_text(why);
                }
                let r = ui.selectable_label(sel, text);
                if r.double_clicked() {
                    f.edit_now = true;
                }
                if r.clicked() {
                    if ui.input(|i| i.modifiers.command) {
                        if let Some(k) = f.also.iter().position(|p| p == &e.path) {
                            f.also.remove(k);
                        } else {
                            f.also.push(e.path.clone());
                        }
                    } else {
                        f.also.clear();
                        f.selected = Some(e.path.clone());
                        f.rename = e.title.clone();
                        f.bold = style.bold;
                        f.italic = style.italic;
                        f.color = style.color;
                        act = Some(Act::Follow(e.path.clone(), e.page));
                    }
                }
                if let Some(p) = e.page {
                    r.on_hover_text(format!("Page {}", p + 1));
                }
            });
            if e.children > 0 && !e.open {
                hide_below = Some(depth);
            }
        }
    });
    // Keys while the panel has the pointer: F2 renames the selected bookmark, Delete deletes it.
    if act.is_none()
        && let Some(sel) = f.selected.clone()
        && ui.rect_contains_pointer(ui.max_rect())
        && !ui.ctx().egui_wants_keyboard_input()
    {
        let (f2, del) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::F2),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
            )
        });
        if f2 {
            f.edit_now = true;
        } else if del {
            act = Some(Act::Delete(sel));
        }
    }
    let Some(act) = act else {
        app.features.bookmarks = f;
        return;
    };
    // Acts handled by the features (dialogs and picks).
    match act {
        Act::Follow(path, page) => {
            app.features.bookmarks = f;
            let target = app
                .doc()
                .and_then(|d| d.session.bookmark_details(&path).ok())
                .and_then(|b| b.target);
            match (target, page) {
                (Some(t), _) => crate::features::links::follow_target(app, &t, ui.ctx()),
                (None, Some(p)) => {
                    if let Some(d) = app.doc_mut() {
                        let n = d.session.page_count();
                        d.view.go_to_page(p, n);
                    }
                }
                (None, None) => {}
            }
            return;
        }
        Act::Action(p) => {
            app.features.bookmarks = f;
            crate::features::links::edit_bookmark_action(app, p);
            return;
        }
        Act::AutoMark => {
            app.features.bookmarks = f;
            crate::features::start_pick(
                app,
                crate::features::Pick::AutoMark,
                "Drag a box around the sheet number in the title block",
            );
            return;
        }
        Act::SaveStructure | Act::ApplyStructure | Act::Export => {
            app.features.bookmarks = f;
            let (ask, save, name, filter): (crate::features::Ask, bool, &str, crate::dialogs::Filter) = match act {
                Act::SaveStructure => (
                    crate::features::Ask::BookmarkStructureSave,
                    true,
                    "Structure.json",
                    crate::features::JSON,
                ),
                Act::ApplyStructure => (
                    crate::features::Ask::BookmarkStructureApply,
                    false,
                    "",
                    crate::features::JSON,
                ),
                _ => (
                    crate::features::Ask::BookmarkExport,
                    true,
                    "Bookmarks.pdf",
                    ("PDF or CSV", &["pdf", "csv"]),
                ),
            };
            let purpose = crate::dialogs::Purpose::Feature(ask);
            if save {
                app.dialogs.save(purpose, filter, name);
            } else {
                app.dialogs.open(purpose, filter, false);
            }
            return;
        }
        _ => {}
    }
    let Some(d) = app.doc_mut() else { return };
    let count = d.session.page_count();
    let s = &mut d.session;
    let status = match act {
        Act::Follow(..) => None,
        Act::Toggle(p, open) => Some(actions::report(s.set_bookmark_open(&p, open), |_| String::new())),
        Act::Add => {
            let label = s.page_labels().get(current).cloned().unwrap_or_default();
            let title = if label.is_empty() {
                format!("Page {}", current + 1)
            } else {
                label
            };
            // A new bookmark goes after the selected one (else at the end).
            let (parent, index) = f.selected.as_ref().map_or((Vec::new(), None), |p| {
                let (pa, i) = split(p);
                (pa, Some(i + 1))
            });
            Some(actions::report(s.add_bookmark(&parent, index, &title, current), |p| {
                f.selected = Some(p);
                f.rename = title.clone();
                f.edit_now = true;
                format!("Bookmark added: {title}")
            }))
        }
        Act::Rename(p, t) => Some(actions::report(s.rename_bookmark(&p, &t), |_| {
            format!("Renamed to {t}")
        })),
        Act::SetPage(p) => Some(actions::report(s.set_bookmark_page(&p, current), |_| {
            format!("Bookmark goes to page {}", current + 1)
        })),
        Act::Delete(p) => {
            f.selected = None;
            Some(actions::report(s.delete_bookmark(&p), |_| "Bookmark deleted".into()))
        }
        Act::Move(from, parent, index) => Some(actions::report(s.move_bookmark(&from, &parent, index), |p| {
            f.selected = Some(p);
            "Bookmark moved".into()
        })),
        Act::FromPages => {
            let all: Vec<usize> = (0..count).collect();
            Some(actions::report(
                s.bookmarks_from_pages(&all, BookmarkTitles::Labels, true),
                |n| format!("Made {}", actions::plural(n, "bookmark")),
            ))
        }
        Act::Clear => {
            f.selected = None;
            Some(actions::report(s.clear_bookmarks(), |n| {
                format!("Removed {}", actions::plural(n, "bookmark"))
            }))
        }
        Act::Copy(p) => {
            let (parent, i) = split(&p);
            Some(actions::report(s.copy_bookmark(&p, &parent, Some(i + 1)), |np| {
                f.selected = Some(np);
                "Bookmark copied".into()
            }))
        }
        Act::Style(paths) => {
            let st = BookmarkStyle {
                color: f.color,
                bold: f.bold,
                italic: f.italic,
            };
            Some(actions::report(s.set_bookmark_style(&paths, st), |_| {
                format!("Properties of {}", actions::plural(paths.len(), "bookmark"))
            }))
        }
        Act::Audit => {
            f.broken = s.audit_bookmarks().into_iter().map(|b| (b.path, b.reason)).collect();
            f.audited = true;
            Some(format!("{} broken", actions::plural(f.broken.len(), "bookmark")))
        }
        Act::Action(_) | Act::AutoMark | Act::SaveStructure | Act::ApplyStructure | Act::Export => None,
    };
    app.features.bookmarks = f;
    if let Some(s) = status.filter(|s| !s.is_empty()) {
        app.status = s;
    }
}
