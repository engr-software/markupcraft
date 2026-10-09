//! Window chrome: the menu bar, the toolbar, the document tabs, the status bar (snap toggles,
//! page navigation, zoom, scale) and the small windows (shortcuts, about, properties).

use egui::{Align, Layout, RichText, Stroke, vec2};

use crate::canvas::{Fit, PageMode};
use crate::commands::{self, COMMANDS, MENUS};
use crate::panels::PANELS;
use crate::theme::Tokens;
use crate::tools::ToolDef;
use crate::tools::{TOOLS, ToolKind};
use crate::{AppState, icons};

/// The menu bar, from the command, tool and panel tables.
pub fn menu_bar(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.menu_visible() {
        return;
    }
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
        let keys = app.keys.keys_for(&id);
        item(
            app,
            ui,
            &id,
            tool.label,
            keys,
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
        ui.menu_button("Toolbars", |ui| {
            ui.set_min_width(200.0);
            for id in [
                "window.toolbar_main",
                "window.toolbar_markup",
                "window.toolbar_measure",
                "|",
                "window.customize_toolbars",
                "window.lock_toolbars",
            ] {
                if id == "|" {
                    ui.separator();
                } else if let Some(c) = commands::find(id) {
                    item(app, ui, id, c.label, c.keys, app.enabled(id), app.checked(id));
                }
            }
        });
    }
    let mut group = None;
    let mut first = tools.is_empty() && menu != "Window";
    for c in COMMANDS
        .iter()
        .chain(crate::features::COMMANDS)
        .filter(|c| c.menu == menu)
    {
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
        let keys = app.keys.keys_for(c.id);
        item(app, ui, c.id, &label, keys, enabled, app.checked(c.id));
        if c.id == "file.open" {
            crate::shell::recent::menu(app, ui);
        }
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
    let tb = app.shell.ui.toolbars.clone();
    if !app.shell.chrome_visible() || !(tb.show_main || tb.show_markup || tb.show_measure) {
        return;
    }
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
                let mut first = true;
                if tb.show_main {
                    for id in &tb.main {
                        if id == "|" {
                            separator(ui, &t);
                            continue;
                        }
                        tool_button(app, ui, id);
                    }
                    first = false;
                }
                for (group, on) in [("Markup", tb.show_markup), ("Measure", tb.show_measure)] {
                    if !on {
                        continue;
                    }
                    if !first {
                        separator(ui, &t);
                    }
                    first = false;
                    let tools: Vec<&&ToolDef> = TOOLS.iter().filter(|tl| tl.menu == group && tl.draws()).collect();
                    for tool in tools {
                        tool_button(app, ui, &format!("tool.{}", tool.id));
                    }
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

fn toggle(app: &mut AppState, ui: &mut egui::Ui, id: &str, label: &str) {
    let on = app.checked(id).unwrap_or(false);
    let tip = commands::describe(id).map(|(l, _, k)| match k {
        Some(k) => format!("{l} ({})", k.label()),
        None => l,
    });
    let b = egui::Button::new(RichText::new(label).size(11.0))
        .selected(on)
        .min_size(vec2(0.0, 20.0));
    let r = ui.add_enabled(app.enabled(id), b);
    if r.clicked() {
        app.queue(id);
    }
    if let Some(tip) = tip {
        r.on_hover_text(tip);
    }
}

/// The status bar (F8): snap toggles, reuse tool, view sync, pointer position, page size,
/// messages. Under it (F4) the navigation bar: page navigation, previous / next view, layout
/// modes, rotate view, split, dimmer, scale (click to calibrate) and zoom.
pub fn status_bar(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let bar = |id: &'static str| {
        egui::Panel::bottom(id).exact_size(28.0).frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(8, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
    };
    if app.shell.ui.show_status_bar {
        bar("status_bar").show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                toggle(app, ui, "snap.grid", "Grid");
                toggle(app, ui, "snap.content", "Content");
                toggle(app, ui, "snap.markup", "Markup");
                separator(ui, &t);
                toggle(app, ui, "window.reuse_tools", "Reuse");
                if app.shell.split.is_some() {
                    let sync = app
                        .shell
                        .split
                        .as_ref()
                        .map_or(crate::shell::split::Sync::Off, |s| s.sync);
                    let (next, label) = match sync {
                        crate::shell::split::Sync::Off => ("view.sync_document", "Sync: Off"),
                        crate::shell::split::Sync::Document => ("view.sync_page", "Sync: Document"),
                        crate::shell::split::Sync::Page => ("view.sync_off", "Sync: Page"),
                    };
                    if ui
                        .add(egui::Button::new(RichText::new(label).size(11.0)).min_size(vec2(0.0, 20.0)))
                        .on_hover_text("Synchronize the split views (click to change)")
                        .clicked()
                    {
                        app.queue(next);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(s) = crate::shell::overlay::page_size_readout(app) {
                        ui.label(RichText::new(s).size(11.0).color(t.text_muted));
                    }
                    if let Some(p) = crate::shell::overlay::pointer_readout(app) {
                        separator(ui, &t);
                        ui.label(RichText::new(p).size(11.0).color(t.text_muted).monospace());
                    }
                    if !app.status.is_empty() {
                        separator(ui, &t);
                        ui.label(RichText::new(&app.status).size(11.0).color(t.text_muted));
                    }
                });
            });
        });
    }
}

/// The navigation bar (F4) under the document area (Revu puts it between the workspace and
/// the bottom panel): page navigation, previous / next view, layout modes, rotate view, split,
/// dimmer, scale (click to calibrate) and zoom.
pub fn nav_bar(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() || !app.shell.ui.show_nav_bar {
        return;
    }
    let t = Tokens::get(ui.ctx());
    {
        egui::Panel::bottom("nav_bar")
            .exact_size(28.0)
            .frame(
                egui::Frame::NONE
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(8, 0))
                    .stroke(Stroke::new(1.0, t.divider)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    page_nav(app, ui);
                    separator(ui, &t);
                    for id in ["view.prev_view", "view.next_view"] {
                        nav_button(app, ui, id);
                    }
                    separator(ui, &t);
                    for (id, label) in [
                        ("view.single_page", "1"),
                        ("view.continuous", "C"),
                        ("view.side_by_side", "2"),
                        ("view.continuous_side", "C2"),
                    ] {
                        toggle(app, ui, id, label);
                    }
                    separator(ui, &t);
                    for id in ["view.rotate_view_ccw", "view.rotate_view_cw"] {
                        nav_button(app, ui, id);
                    }
                    separator(ui, &t);
                    toggle(app, ui, "view.split_vertical", "Split |");
                    toggle(app, ui, "view.split_horizontal", "Split -");
                    toggle(app, ui, "view.dimmer", "Dimmer");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        zoom_box(app, ui);
                        separator(ui, &t);
                        let scale = app.scale_readout();
                        let r = ui
                            .add(egui::Button::new(RichText::new(scale).size(11.0).color(t.text_muted)).frame(false))
                            .on_hover_text("Click to calibrate the page scale");
                        if r.clicked() && app.has_doc() {
                            app.queue("tool.calibrate");
                        }
                    });
                });
            });
    }
}

/// A navigation bar button: the command's icon (or its short label when it has none).
fn nav_button(app: &mut AppState, ui: &mut egui::Ui, id: &str) {
    let has_icon = commands::describe(id).is_some_and(|(_, i, _)| !i.is_empty());
    if has_icon {
        tool_button(app, ui, id);
        return;
    }
    let label = match id {
        "view.prev_view" => "<",
        "view.next_view" => ">",
        _ => "?",
    };
    toggle(app, ui, id, label);
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
                if app.shell.ui.show_recents_on_start && !app.shell.recent.files.is_empty() {
                    ui.add_space(14.0);
                    ui.label(RichText::new("Recent files").color(egui::Color32::from_gray(235)));
                    let files = app.shell.recent.sorted(crate::shell::recent::Sort::Date);
                    for f in files.iter().take(8) {
                        let name = f
                            .path
                            .file_name()
                            .map_or_else(|| f.path.display().to_string(), |n| n.to_string_lossy().into_owned());
                        if ui.button(name).on_hover_text(f.path.display().to_string()).clicked() {
                            app.open_path(&f.path);
                        }
                    }
                }
            });
        });
        return;
    }
    nav_bar(app, ui);
    // Document tabs (not in presentation), then one canvas or the split panes.
    if app.shell.screen != crate::shell::Screen::Presentation {
        crate::shell::tabs::tab_bar(app, ui);
        if app.docs.is_empty() {
            return;
        }
    }
    crate::shell::split::show(app, ui);
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
            d.session.path().display().to_string(),
            d.render.as_ref().map_or(0, |r| r.page_count()),
            d.session.doc().markups.len(),
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
