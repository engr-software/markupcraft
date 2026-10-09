//! Window chrome: the menu bar, the toolbar, the document tabs, the status bar (snap toggles,
//! page navigation, zoom, scale) and the small windows (shortcuts, about, properties).

use egui::{Align, Layout, RichText, Stroke, vec2};

use crate::canvas::{self, CanvasCx, Fit, PageMode};
use crate::commands::{self, COMMANDS, MAIN_TOOLBAR, MENUS};
use crate::panels::PANELS;
use crate::theme::Tokens;
use crate::tools::{TOOLS, ToolKind};
use crate::{AppState, icons};

/// The menu bar, from the command, tool and panel tables.
pub fn menu_bar(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("menu_bar")
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(6, 2)),
        )
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                for menu in MENUS {
                    ui.menu_button(*menu, |ui| {
                        ui.set_min_width(220.0);
                        menu_items(app, ui, menu);
                    });
                }
            });
        });
}

fn item(
    app: &mut AppState,
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    keys: Option<commands::Keys>,
    enabled: bool,
    checked: Option<bool>,
) {
    // Checked items (active tool, open panel, view mode) are shown selected.
    let mut b = egui::Button::new(label).selected(checked == Some(true));
    if let Some(k) = keys {
        b = b.shortcut_text(k.label());
    }
    let r = ui.add_enabled(enabled, b);
    if r.clicked() {
        app.queue(id);
        ui.close();
    }
}

fn menu_items(app: &mut AppState, ui: &mut egui::Ui, menu: &str) {
    // Tools listed in this menu come first.
    let tools: Vec<_> = TOOLS.iter().filter(|tl| tl.menu == menu).collect();
    for tool in &tools {
        let id = format!("tool.{}", tool.id);
        let on = app.tool == tool.id;
        item(
            app,
            ui,
            &id,
            tool.label,
            tool.keys,
            app.has_doc() || matches!(tool.kind, ToolKind::Select | ToolKind::Pan),
            Some(on),
        );
    }
    if menu == "Window" {
        ui.label(RichText::new("Panels").size(11.0).weak());
        for p in PANELS {
            let on = app.open_panels.contains(&p.id);
            item(app, ui, &format!("panel.{}", p.id), p.title, p.keys, true, Some(on));
        }
    }
    let mut group = None;
    let mut first = tools.is_empty() && menu != "Window";
    for c in COMMANDS.iter().filter(|c| c.menu == menu) {
        if group != Some(c.group) {
            if !first {
                ui.separator();
            }
            group = Some(c.group);
        }
        first = false;
        let enabled = c.built && app.enabled(c.id);
        let label = if c.built {
            c.label.to_string()
        } else {
            format!("{} (not yet)", c.label)
        };
        item(app, ui, c.id, &label, c.keys, enabled, app.checked(c.id));
    }
}

fn separator(ui: &mut egui::Ui, t: &Tokens) {
    ui.add_space(3.0);
    let x = ui.cursor().left();
    let r = ui.max_rect();
    ui.painter()
        .vline(x, r.y_range().shrink(6.0), Stroke::new(1.0, t.border));
    ui.add_space(4.0);
}

/// The main toolbar and the markup tools.
pub fn toolbar(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("toolbar")
        .exact_size(36.0)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(6, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for id in MAIN_TOOLBAR {
                    if *id == "|" {
                        separator(ui, &t);
                        continue;
                    }
                    tool_button(app, ui, id);
                }
                separator(ui, &t);
                for tool in TOOLS.iter().filter(|tl| matches!(tl.kind, ToolKind::Drag(_))) {
                    tool_button(app, ui, &format!("tool.{}", tool.id));
                }
            });
        });
}

fn tool_button(app: &mut AppState, ui: &mut egui::Ui, id: &str) {
    let Some((label, icon, keys)) = commands::describe(id) else {
        return;
    };
    let tip = match keys {
        Some(k) => format!("{label} ({})", k.label()),
        None => label,
    };
    let selected = app.checked(id).unwrap_or(false);
    let enabled = app.enabled(id) && commands::find(id).is_none_or(|c| c.built);
    let r = ui
        .add_enabled_ui(enabled, |ui| icons::button(ui, icon, 28.0, selected, &tip))
        .inner;
    if r.clicked() {
        app.queue(id);
    }
}

/// The status bar: snap toggles, page navigation, zoom and scale.
pub fn status_bar(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status_bar")
        .exact_size(28.0)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(8, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (id, label) in [
                    ("snap.grid", "Grid"),
                    ("snap.content", "Content"),
                    ("snap.markup", "Markup"),
                ] {
                    let on = app.checked(id).unwrap_or(false);
                    let tip = commands::find(id).map(|c| match c.keys {
                        Some(k) => format!("{} ({})", c.label, k.label()),
                        None => c.label.to_string(),
                    });
                    let b = egui::Button::new(RichText::new(label).size(11.0))
                        .selected(on)
                        .min_size(vec2(0.0, 20.0));
                    let r = ui.add(b);
                    if r.clicked() {
                        app.queue(id);
                    }
                    if let Some(tip) = tip {
                        r.on_hover_text(tip);
                    }
                }
                separator(ui, &t);
                page_nav(app, ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    zoom_box(app, ui);
                    separator(ui, &t);
                    let scale = app.scale_readout();
                    ui.label(RichText::new(scale).size(11.0).color(t.text_muted));
                    if !app.status.is_empty() {
                        separator(ui, &t);
                        ui.label(RichText::new(&app.status).size(11.0).color(t.text_muted));
                    }
                });
            });
        });
}

fn page_nav(app: &mut AppState, ui: &mut egui::Ui) {
    let has = app.has_doc();
    let (current, count, label) = app
        .doc()
        .and_then(|d| {
            let r = d.render.as_ref()?;
            Some((
                d.view.current,
                r.page_count(),
                r.page(d.view.current).map(|g| g.label.clone()).unwrap_or_default(),
            ))
        })
        .unwrap_or((0, 0, String::new()));
    ui.add_enabled_ui(has, |ui| {
        for id in ["view.first_page", "view.prev_page"] {
            tool_button(app, ui, id);
        }
        let id = egui::Id::new("page-entry");
        let focused = ui.memory(|m| m.has_focus(id));
        if !focused {
            app.page_entry = if count == 0 {
                String::new()
            } else {
                format!("{}", current + 1)
            };
        }
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.page_entry)
                .id(id)
                .desired_width(36.0)
                .horizontal_align(Align::Center),
        );
        if r.lost_focus()
            && let Ok(n) = app.page_entry.trim().parse::<usize>()
            && let Some(d) = app.doc_mut()
        {
            d.view.go_to_page(n.saturating_sub(1), count);
        }
        let of = if label.is_empty() || label == format!("{}", current + 1) {
            format!("of {count}")
        } else {
            format!("({label}) of {count}")
        };
        ui.label(RichText::new(of).size(11.0));
        for id in ["view.next_page", "view.last_page"] {
            tool_button(app, ui, id);
        }
    });
}

fn zoom_box(app: &mut AppState, ui: &mut egui::Ui) {
    let zoom = app.doc().map(|d| d.view.zoom);
    let text = zoom.map_or_else(|| "-".to_string(), |z| format!("{:.0}%", z * 100.0));
    ui.add_enabled_ui(zoom.is_some(), |ui| {
        ui.menu_button(RichText::new(text).size(11.0), |ui| {
            for z in [0.25f32, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0] {
                if ui.button(format!("{:.0}%", z * 100.0)).clicked() {
                    app.set_zoom(z, ui.ctx());
                    ui.close();
                }
            }
            ui.separator();
            for id in ["view.fit_page", "view.fit_width", "view.actual_size"] {
                if let Some(c) = commands::find(id)
                    && ui
                        .add(egui::Button::new(c.label).shortcut_text(c.keys.map(|k| k.label()).unwrap_or_default()))
                        .clicked()
                {
                    app.queue(id);
                    ui.close();
                }
            }
        });
    });
}

/// The middle of the dock: document tabs over the canvas, or the start screen.
pub fn document_area(app: &mut AppState, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    if app.docs.is_empty() {
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, t.workspace);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.35);
                ui.label(RichText::new("MarkupCraft").size(26.0).color(egui::Color32::WHITE));
                ui.label(
                    RichText::new("Open a PDF to start (Ctrl+O), or drop one on the window.")
                        .color(egui::Color32::from_gray(235)),
                );
                ui.add_space(10.0);
                if ui.button("Open...").clicked() {
                    app.queue("file.open");
                }
            });
        });
        return;
    }
    // Document tabs.
    let mut close = None;
    egui::Frame::NONE
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for i in 0..app.docs.len() {
                    let Some(d) = app.docs.get(i) else { continue };
                    let name = if d.session.modified {
                        format!("{} *", d.name)
                    } else {
                        d.name.clone()
                    };
                    let active = i == app.active;
                    let r = ui.add(
                        egui::Button::new(RichText::new(name).size(12.0))
                            .selected(active)
                            .min_size(vec2(80.0, 22.0)),
                    );
                    if r.clicked() {
                        app.active = i;
                    }
                    if icons::button(ui, "x", 18.0, false, "Close").clicked() {
                        close = Some(i);
                    }
                    ui.add_space(6.0);
                }
            });
        });
    if let Some(i) = close {
        app.close_doc(i);
        return;
    }
    let tool = crate::tools::find(app.tool).unwrap_or(&crate::tools::select::TOOL);
    let cx = CanvasCx {
        tool,
        wheel_zooms: app.wheel_zooms,
        hide_markups: app.hide_markups,
        want_thumbs: app.thumbs_wanted_last,
        author: &app.author,
    };
    let Some(doc) = app.docs.get_mut(app.active) else {
        return;
    };
    let out = canvas::show(
        ui,
        &mut doc.session,
        doc.render.as_ref(),
        &mut doc.view,
        &mut doc.selection,
        &cx,
    );
    if let Some(s) = out.status {
        app.status = s;
    }
    if out.created {
        app.tool = "select";
    }
}

/// Help > Keyboard Shortcuts, Help > About, Document Properties.
pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    let mut open = app.show_shortcuts;
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .default_size([420.0, 480.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("keys").striped(true).num_columns(2).show(ui, |ui| {
                    for (k, id) in commands::bindings().iter().rev() {
                        let label = commands::describe(id).map_or_else(|| id.clone(), |d| d.0);
                        ui.label(label);
                        ui.label(RichText::new(k.label()).monospace());
                        ui.end_row();
                    }
                });
            });
        });
    app.show_shortcuts = open;

    let mut open = app.show_about;
    egui::Window::new("About MarkupCraft")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("MarkupCraft {}", env!("CARGO_PKG_VERSION"))).strong());
            ui.label("A clean-room, open-source PDF markup and takeoff application.");
            ui.label("MIT OR Apache-2.0. Icons: Lucide (ISC). Rendering: PdfCraft and hayro.");
        });
    app.show_about = open;

    let mut open = app.show_properties;
    let info = app.doc().map(|d| {
        (
            d.name.clone(),
            d.session.doc.path.clone(),
            d.render.as_ref().map_or(0, |r| r.page_count()),
            d.session.doc.markups.len(),
            d.render.as_ref().map_or(0, |r| r.hidden_count()),
        )
    });
    egui::Window::new("Document Properties")
        .open(&mut open)
        .collapsible(false)
        .show(ctx, |ui| match &info {
            Some((name, path, pages, markups, drawn)) => {
                egui::Grid::new("docprops").num_columns(2).show(ui, |ui| {
                    ui.label("File");
                    ui.label(name);
                    ui.end_row();
                    ui.label("Location");
                    ui.label(path);
                    ui.end_row();
                    ui.label("Pages");
                    ui.label(format!("{pages}"));
                    ui.end_row();
                    ui.label("Markups");
                    ui.label(format!("{markups} ({drawn} drawn live by MarkupCraft)"));
                    ui.end_row();
                });
            }
            None => {
                ui.label("No document open.");
            }
        });
    app.show_properties = open;
}

/// The default and current view mode for menu checkmarks.
pub fn mode_checked(app: &AppState, mode: PageMode) -> Option<bool> {
    Some(app.doc().is_some_and(|d| d.view.mode == mode))
}

pub fn fit_checked(app: &AppState, fit: Fit) -> Option<bool> {
    Some(app.doc().is_some_and(|d| d.view.fit == fit))
}
