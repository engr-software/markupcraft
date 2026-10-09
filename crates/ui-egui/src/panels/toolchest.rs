//! Tool Chest (Alt+X): Recent Tools (this session), My Tools and the user's tool sets (saved
//! markup looks, persisted in the configuration folder), the stamp designs, and the drawing
//! tools by group. Click an item to draw with it; double-click to keep the tool after each
//! markup. Right-click an item: Properties or Drawing mode, apply it to the selection, copy it
//! to a set, rename, delete. A markup's right-click menu (or Properties) adds it to My Tools.

use egui::{RichText, Sense, vec2};

use super::{PanelDef, Slot};
use crate::chest::{MY_TOOLS, Mode, ToolItem};
use crate::chest_sets::ChestView;
use crate::commands::alt;
use crate::theme::{Tokens, color32};
use crate::tools::{TOOLS, ToolDef};
use crate::{AppState, actions};

pub static PANEL: PanelDef = PanelDef {
    id: "toolchest",
    title: "Tool Chest",
    icon: "wrench",
    slot: Slot::Right,
    keys: alt(egui::Key::X),
    ui,
};

/// What a click in the panel asks for (applied after drawing).
enum Act {
    Use(String, String, bool),
    Tool(&'static str, bool),
    Mode(String, String, Mode),
    Apply(String, String),
    CopyTo(String, String, String),
    Delete(String, String),
    Rename(String, String, String),
    StartRename(String, String, String),
    NewSet,
    DeleteSet(String),
    RenameSet(String, String),
    Stamp(&'static str),
    MoveSet(String, i32),
    MoveItem(String, String, i32),
    SetScale(String, Option<markupcraft_model::Scale>),
    Export(String),
    Import,
    View(ChestView, f32),
}

const RENAME: &str = "markupcraft-chest-rename";

fn row(
    ui: &mut egui::Ui,
    t: &Tokens,
    icon: &str,
    label: &str,
    hint: &str,
    active: bool,
    swatch: Option<egui::Color32>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, label));
    let p = ui.painter();
    if active {
        p.rect_filled(rect, t.radius, t.accent_soft);
    } else if resp.hovered() {
        p.rect_filled(rect, t.radius, t.hover);
    }
    let icon_rect = egui::Rect::from_min_size(rect.min + vec2(4.0, 1.0), vec2(24.0, 24.0));
    crate::icons::paint(ui, icon_rect, icon, 16.0, if active { t.accent_text } else { t.icon });
    if let Some(c) = swatch {
        p.rect_filled(
            egui::Rect::from_center_size(icon_rect.right_bottom() - vec2(3.0, 4.0), vec2(8.0, 8.0)),
            2.0,
            c,
        );
    }
    p.text(
        rect.left_center() + vec2(34.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        t.text,
    );
    p.text(
        rect.right_center() - vec2(6.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        hint,
        egui::FontId::proportional(11.0),
        t.text_faint,
    );
    resp
}

#[allow(clippy::too_many_arguments)]
fn item_row(
    ui: &mut egui::Ui,
    t: &Tokens,
    app: &AppState,
    set: &str,
    it: &ToolItem,
    sets: &[(String, String)],
    acts: &mut Vec<Act>,
    more: &mut Vec<crate::chest_more::ChestAct>,
) {
    let Some(tool) = crate::tools::find(&it.tool) else {
        return;
    };
    let active = app.active_item.as_ref().is_some_and(|(s, i)| s == set && *i == it.id);
    let renaming: Option<(String, String, String)> = ui.data(|d| d.get_temp(egui::Id::new(RENAME)));
    if let Some((s, i, mut text)) = renaming.filter(|(s, i, _)| s == set && *i == it.id) {
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        r.request_focus();
        if r.lost_focus() {
            acts.push(Act::Rename(s, i, text));
        } else {
            acts.push(Act::StartRename(s, i, text));
        }
        return;
    }
    let hint = match it.mode {
        Mode::Drawing => "drawing",
        Mode::Properties => "",
    };
    let r = if app.toolchest.view == ChestView::Symbol {
        let size = app.toolchest.icon_size;
        let r = crate::icons::button(ui, tool.icon, size, active, &it.name);
        let swatch = egui::Rect::from_center_size(r.rect.right_bottom() - vec2(4.0, 4.0), vec2(7.0, 7.0));
        ui.painter().rect_filled(swatch, 2.0, color32(&it.markup.color, 1.0));
        r
    } else {
        row(
            ui,
            t,
            tool.icon,
            &it.name,
            hint,
            active,
            Some(color32(&it.markup.color, 1.0)),
        )
    };
    if r.double_clicked() {
        acts.push(Act::Use(set.into(), it.id.clone(), true));
    } else if r.clicked() {
        acts.push(Act::Use(set.into(), it.id.clone(), false));
    }
    r.context_menu(|ui| {
        let (s, i) = (set.to_string(), it.id.clone());
        if ui
            .add(egui::Button::new("Properties Mode").selected(it.mode == Mode::Properties))
            .clicked()
        {
            acts.push(Act::Mode(s.clone(), i.clone(), Mode::Properties));
            ui.close();
        }
        if ui
            .add(egui::Button::new("Drawing Mode").selected(it.mode == Mode::Drawing))
            .clicked()
        {
            acts.push(Act::Mode(s.clone(), i.clone(), Mode::Drawing));
            ui.close();
        }
        ui.separator();
        if ui.button("Apply to Selection").clicked() {
            acts.push(Act::Apply(s.clone(), i.clone()));
            ui.close();
        }
        ui.menu_button("Copy to", |ui| {
            for (sid, title) in sets {
                if *sid != s && ui.button(title).clicked() {
                    acts.push(Act::CopyTo(s.clone(), i.clone(), sid.clone()));
                    ui.close();
                }
            }
        });
        if s != "recent" {
            if ui.button("Move Up").clicked() {
                acts.push(Act::MoveItem(s.clone(), i.clone(), -1));
                ui.close();
            }
            if ui.button("Move Down").clicked() {
                acts.push(Act::MoveItem(s.clone(), i.clone(), 1));
                ui.close();
            }
        }
        if ui.button("Rename").clicked() {
            acts.push(Act::StartRename(s.clone(), i.clone(), it.name.clone()));
            ui.close();
        }
        if ui.button("Delete").clicked() {
            acts.push(Act::Delete(s.clone(), i.clone()));
            ui.close();
        }
        crate::chest_more::item_menu(ui, app, &s, &i, more);
    });
}

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    crate::chest_more::drop_target(ui, app);
    let t = Tokens::get(ui.ctx());
    let mut acts: Vec<Act> = Vec::new();
    let mut more: Vec<crate::chest_more::ChestAct> = Vec::new();
    let sets: Vec<(String, String)> = app
        .toolchest
        .sets
        .iter()
        .map(|s| (s.id.clone(), s.title.clone()))
        .collect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if let Some(e) = &app.toolchest.error {
            ui.label(RichText::new(e).color(t.text_faint).size(11.0));
        }
        ui.horizontal(|ui| {
            let mut keep = app.tool_locked;
            if ui
                .checkbox(&mut keep, "Keep tool selected")
                .on_hover_text("Stay in the tool after each markup (double-click a tool does this too)")
                .changed()
            {
                app.tool_locked = keep;
            }
        });
        crate::chest_more::options_ui(ui, app);
        ui.horizontal(|ui| {
            let mut view = app.toolchest.view;
            let mut size = app.toolchest.icon_size;
            ui.selectable_value(&mut view, ChestView::Detail, "Detail");
            ui.selectable_value(&mut view, ChestView::Symbol, "Symbol");
            if view == ChestView::Symbol {
                ui.add(egui::Slider::new(&mut size, 16.0..=96.0).show_value(false))
                    .on_hover_text("Icon size");
            }
            if view != app.toolchest.view || (size - app.toolchest.icon_size).abs() > 0.01 {
                acts.push(Act::View(view, size));
            }
        });
        // Recent Tools
        egui::CollapsingHeader::new(RichText::new("Recent Tools").strong())
            .default_open(true)
            .show(ui, |ui| {
                if app.toolchest.recent.is_empty() {
                    ui.label(
                        RichText::new("Tools you draw with appear here.")
                            .color(t.text_faint)
                            .size(11.0),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if app.toolchest.view == ChestView::Detail {
                        ui.vertical(|ui| {
                            for it in app.toolchest.recent.clone() {
                                item_row(ui, &t, app, "recent", &it, &sets, &mut acts, &mut more);
                            }
                        });
                    } else {
                        for it in app.toolchest.recent.clone() {
                            item_row(ui, &t, app, "recent", &it, &sets, &mut acts, &mut more);
                        }
                    }
                });
            });
        // My Tools and the user's sets
        for set in app.toolchest.sets.clone() {
            let mut title = match &set.scale {
                Some(s) => format!("{}  ({})", set.title, s.ratio),
                None => set.title.clone(),
            };
            if app.toolchest.is_locked(&set.id) {
                title.push_str("  (locked)");
            }
            let header = egui::CollapsingHeader::new(RichText::new(title).strong())
                .id_salt(("chest-set", &set.id))
                .default_open(!set.collapsed)
                .show(ui, |ui| {
                    if set.items.is_empty() {
                        let hint = if set.id == MY_TOOLS {
                            "Right-click a markup > Add to Tool Chest to save its look here."
                        } else {
                            "Copy tools here from their right-click menu."
                        };
                        ui.label(RichText::new(hint).color(t.text_faint).size(11.0));
                    }
                    if app.toolchest.view == ChestView::Symbol {
                        ui.horizontal_wrapped(|ui| {
                            for it in &set.items {
                                item_row(ui, &t, app, &set.id, it, &sets, &mut acts, &mut more);
                            }
                        });
                    } else {
                        for it in &set.items {
                            item_row(ui, &t, app, &set.id, it, &sets, &mut acts, &mut more);
                        }
                    }
                });
            if header.openness < 0.5 && !set.items.is_empty() {
                crate::chest_more::flyout(ui, app, &set, &mut more);
            }
            header.header_response.context_menu(|ui| {
                crate::chest_more::set_menu(ui, app, &set.id, &mut more);
                ui.menu_button("Scale", |ui| {
                    if ui.selectable_label(set.scale.is_none(), "None").clicked() {
                        acts.push(Act::SetScale(set.id.clone(), None));
                        ui.close();
                    }
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for p in markupcraft_measure::units::scale_presets() {
                            let on = set.scale.as_ref().is_some_and(|s| s.ratio == p.scale.ratio);
                            if ui.selectable_label(on, &p.name).clicked() {
                                acts.push(Act::SetScale(set.id.clone(), Some(p.scale.clone())));
                                ui.close();
                            }
                        }
                    });
                });
                if ui.button("Export...").clicked() {
                    acts.push(Act::Export(set.id.clone()));
                    ui.close();
                }
                if set.id != MY_TOOLS {
                    ui.separator();
                    if ui.button("Move Up").clicked() {
                        acts.push(Act::MoveSet(set.id.clone(), -1));
                        ui.close();
                    }
                    if ui.button("Move Down").clicked() {
                        acts.push(Act::MoveSet(set.id.clone(), 1));
                        ui.close();
                    }
                    let renaming: Option<String> = ui.data(|d| d.get_temp(egui::Id::new(("set-rename", &set.id))));
                    let mut title = renaming.unwrap_or_else(|| set.title.clone());
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        if ui.text_edit_singleline(&mut title).changed() {
                            ui.data_mut(|d| d.insert_temp(egui::Id::new(("set-rename", &set.id)), title.clone()));
                        }
                    });
                    if ui.button("Rename").clicked() {
                        acts.push(Act::RenameSet(set.id.clone(), title));
                        ui.close();
                    }
                    if ui.button("Delete Tool Set").clicked() {
                        acts.push(Act::DeleteSet(set.id.clone()));
                        ui.close();
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("New Tool Set").clicked() {
                acts.push(Act::NewSet);
            }
            if ui.button("Import...").clicked() {
                acts.push(Act::Import);
            }
        });
        ui.add_space(6.0);
        // Stamps
        egui::CollapsingHeader::new(RichText::new("Stamps").strong())
            .default_open(false)
            .show(ui, |ui| {
                for d in markupcraft_revu::kinds::draw::STAMP_DESIGNS {
                    let active = app.tool == "stamp" && app.stamp == d.id;
                    let r = row(ui, &t, "stamp", d.text, "", active, Some(color32(&d.color, 1.0)));
                    if r.clicked() {
                        acts.push(Act::Stamp(d.id));
                    }
                }
            });
        // The tools
        for group in ["Markup", "Measure"] {
            egui::CollapsingHeader::new(RichText::new(group).strong())
                .default_open(true)
                .show(ui, |ui| {
                    let tools: Vec<&&ToolDef> = TOOLS.iter().filter(|tl| tl.menu == group && tl.draws()).collect();
                    for tool in tools {
                        let active = app.tool == tool.id && app.active_item.is_none();
                        let key = tool.keys.map(|k| k.label()).unwrap_or_default();
                        let r = row(ui, &t, tool.icon, tool.label, &key, active, None);
                        if r.double_clicked() {
                            acts.push(Act::Tool(tool.id, true));
                        } else if r.clicked() {
                            acts.push(Act::Tool(tool.id, false));
                        }
                    }
                });
        }
    });
    crate::chest_more::apply(app, more);
    for a in acts {
        match a {
            Act::Use(set, item, keep) => {
                app.edit.item_scale = app.toolchest.item_set_scale(&set).cloned();
                app.use_item(&set, &item);
                if keep {
                    app.tool_locked = true;
                }
            }
            Act::Tool(id, keep) => {
                app.set_tool(id);
                app.active_item = None;
                if keep {
                    app.tool_locked = true;
                }
            }
            Act::Mode(set, item, mode) => app.toolchest.update_item(&set, &item, |i| i.mode = mode),
            Act::Apply(set, item) => {
                let Some(tpl) = app.toolchest.item(&set, &item).map(|i| i.markup.clone()) else {
                    continue;
                };
                if let Some(d) = app.doc_mut() {
                    let ids = d.selection().to_vec();
                    let r = d.session.set_properties(&ids, &actions::patch_from(&tpl));
                    app.status = actions::report(r, |n| format!("Applied to {}", actions::plural(n, "markup")));
                }
            }
            Act::CopyTo(from, item, to) => app.toolchest.copy_item_to(&from, &item, &to),
            Act::Delete(set, item) => {
                app.toolchest.remove_item(&set, &item);
                if app.active_item.as_ref().is_some_and(|(s, i)| *s == set && *i == item) {
                    app.active_item = None;
                }
            }
            Act::Rename(set, item, name) => {
                ui.data_mut(|d| d.remove::<(String, String, String)>(egui::Id::new(RENAME)));
                if !name.trim().is_empty() {
                    app.toolchest
                        .update_item(&set, &item, |i| i.name = name.trim().to_string());
                }
            }
            Act::StartRename(set, item, text) => {
                ui.data_mut(|d| d.insert_temp(egui::Id::new(RENAME), (set, item, text)));
            }
            Act::NewSet => {
                let n = app.toolchest.sets.len();
                app.toolchest.add_set(&format!("Tool Set {n}"));
            }
            Act::DeleteSet(id) => app.toolchest.remove_set(&id),
            Act::RenameSet(id, title) => {
                if !title.trim().is_empty() {
                    app.toolchest.rename_set(&id, title.trim());
                }
            }
            Act::MoveSet(id, d) => app.toolchest.move_set(&id, d),
            Act::MoveItem(set, item, d) => app.toolchest.move_item(&set, &item, d),
            Act::SetScale(id, s) => app.toolchest.set_scale(&id, s),
            Act::View(v, size) => app.toolchest.set_view(v, size),
            Act::Export(id) => {
                let name = app.toolchest.find_set(&id).map_or("Tools".into(), |s| s.title.clone());
                app.dialogs.save(
                    crate::dialogs::Purpose::Edit("export_toolset", id),
                    crate::dialogs::TOOLSET,
                    &format!("{name}.mctools"),
                );
            }
            Act::Import => app.dialogs.open(
                crate::dialogs::Purpose::Edit("import_toolset", String::new()),
                crate::dialogs::TOOLSET,
                false,
            ),
            Act::Stamp(id) => {
                app.stamp = id;
                app.set_tool("stamp");
                app.active_item = None;
            }
        }
    }
}
