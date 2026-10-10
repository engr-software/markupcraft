//! Window chrome: the menu bar, the optional toolbars, the document area (tabs over the canvas),
//! the bottom bar (Markups List toggle, snap toggles, page layout, navigation tools, page box,
//! theme, page size and scale) and the small windows (shortcuts, about, properties). The
//! default arrangement is described in `docs/UI_LAYOUT.md`.

use egui::{Align, Layout, RichText, Stroke, vec2};

use crate::canvas::{Fit, PageMode};
use crate::commands::{self, MENU_BAR, TOOLS_SUBMENUS};
use crate::panels::PANELS;
use crate::theme::Tokens;
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
                for menu in MENU_BAR {
                    let r = ui.menu_button(crate::i18n::tr(menu), |ui| menu_body(app, ui, menu));
                    crate::shell::extra::menu_drawn(app, menu, &r.response);
                }
            });
        });
}

/// One menu's items. Long menus (View, Markup, Document) scroll instead of running off the
/// bottom of the window, where their last items could not be reached. Tools starts with the
/// Markup and Measure submenus.
fn menu_body(app: &mut AppState, ui: &mut egui::Ui, menu: &'static str) {
    ui.set_min_width(220.0);
    let max = (ui.ctx().content_rect().height() - 60.0).max(160.0);
    egui::ScrollArea::vertical()
        .id_salt(("menu-scroll", menu))
        .max_height(max)
        .show(ui, |ui| {
            if menu == "Tools" {
                for sub in TOOLS_SUBMENUS {
                    ui.menu_button(crate::i18n::tr(sub), |ui| menu_body(app, ui, sub));
                }
                ui.separator();
            }
            menu_items(app, ui, menu);
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
    let mut b = egui::Button::new(crate::i18n::tr(label)).selected(checked == Some(true));
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
    if crate::shell::extra::app_menu(app, ui, menu) {
        return;
    }
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
        ui.label(RichText::new(crate::i18n::tr("Panels")).size(11.0).weak());
        for p in PANELS {
            let on = app.open_panels.contains(&p.id);
            item(app, ui, &format!("panel.{}", p.id), p.title, p.keys, true, Some(on));
        }
        ui.menu_button(crate::i18n::tr("Toolbars"), |ui| {
            ui.set_min_width(200.0);
            for id in [
                "window.toolbar_main",
                "window.toolbar_markup",
                "window.toolbar_measure",
                "+",
                "|",
                "window.customize_toolbars",
                "window.lock_toolbars",
            ] {
                if id == "+" {
                    crate::shell::toolbars_more::builtin_menu(app, ui);
                } else if id == "|" {
                    ui.separator();
                } else if let Some(c) = commands::find(id) {
                    item(app, ui, id, c.label, c.keys, app.enabled(id), app.checked(id));
                }
            }
        });
    }
    let mut group = None;
    let mut first = tools.is_empty() && menu != "Window";
    for c in commands::all().filter(|c| c.menu == menu) {
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
            format!("{} ({})", crate::i18n::tr(c.label), crate::i18n::tr("not yet"))
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
    let top = crate::shell::toolbars_more::at(app, crate::shell::toolbars_more::Dock::Top);
    if !app.shell.chrome_visible() || top.is_empty() {
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
                for (k, name) in top.iter().enumerate() {
                    if k > 0 {
                        separator(ui, &t);
                    }
                    crate::shell::toolbars_more::strip(app, ui, name, false);
                }
            });
        });
}

/// A toolbar button for command `id` (for every toolbar strip).
pub fn tool_button_pub(app: &mut AppState, ui: &mut egui::Ui, id: &str) {
    tool_button(app, ui, id);
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

/// The bottom bar: one row under everything (see `docs/UI_LAYOUT.md`).
///
/// - left: the Markups List toggle and the thumbnail size slider;
/// - the status group (F8, Window > Status Bar): snap toggles, Reuse, security, jobs, view sync;
/// - the navigation group (F4, Window > Navigation Bar): page layout modes, rotate view, split,
///   dimmer, then pan / select / select text / zoom, first / previous page, the page box
///   ("label (n of N)"), next / last page, previous / next view and the zoom box;
/// - right: messages and the pointer position (status group), the light / dark switch, the page
///   size and the page scale (click to calibrate).
pub fn bottom_bar(app: &mut AppState, ui: &mut egui::Ui) {
    let (status, nav) = (app.shell.ui.show_status_bar, app.shell.ui.show_nav_bar);
    if !app.shell.chrome_visible() || !(status || nav) {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("bottom_bar")
        .exact_size(30.0)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(6, 0))
                .stroke(Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                bar_left(app, ui);
                if status {
                    separator(ui, &t);
                    status_group(app, ui);
                }
                if nav {
                    separator(ui, &t);
                    for (id, label) in [
                        ("view.single_page", "1"),
                        ("view.continuous", "C"),
                        ("view.side_by_side", "2"),
                        ("view.continuous_side", "C2"),
                    ] {
                        toggle(app, ui, id, label);
                    }
                    for id in ["view.rotate_view_ccw", "view.rotate_view_cw"] {
                        small_button(app, ui, id);
                    }
                    toggle(app, ui, "view.split_vertical", "Split |");
                    toggle(app, ui, "view.split_horizontal", "Split -");
                    toggle(app, ui, "view.dimmer", "Dimmer");
                    separator(ui, &t);
                    for id in ["tool.pan", "tool.select", "tool.selecttext", "tool.zoom"] {
                        small_button(app, ui, id);
                    }
                    separator(ui, &t);
                    page_nav(app, ui);
                    separator(ui, &t);
                    for id in ["view.prev_view", "view.next_view"] {
                        nav_button(app, ui, id);
                    }
                    zoom_box(app, ui);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    bar_right(app, ui, &t, status);
                });
            });
        });
}

/// The Markups List toggle and the thumbnail size.
fn bar_left(app: &mut AppState, ui: &mut egui::Ui) {
    small_button(app, ui, "panel.markups");
    let mut size = app.shell.thumbs.size;
    let r = ui
        .add_sized(
            [72.0, 18.0],
            egui::Slider::new(&mut size, 60.0..=320.0).show_value(false),
        )
        .on_hover_text("Thumbnail size");
    if r.changed() {
        app.shell.thumbs.size = size;
    }
}

/// Snap toggles, Reuse, the security icon, background jobs and the split views' sync.
fn status_group(app: &mut AppState, ui: &mut egui::Ui) {
    toggle(app, ui, "snap.grid", "Grid");
    toggle(app, ui, "snap.content", "Content");
    toggle(app, ui, "snap.markup", "Markup");
    toggle(app, ui, "window.reuse_tools", "Reuse");
    crate::features::partials_more3::security_icon(app, ui);
    crate::features::jobs::indicator(app, ui);
    if let Some(sync) = app.shell.split.as_ref().map(|s| s.sync) {
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
}

/// Right to left: the page scale, the page size, the theme switch, then (room permitting) the
/// pointer position and the last message.
fn bar_right(app: &mut AppState, ui: &mut egui::Ui, t: &Tokens, status: bool) {
    let scale = app.scale_readout();
    let r = ui
        .add(egui::Button::new(RichText::new(scale).size(11.0).color(t.text_muted)).frame(false))
        .on_hover_text("Click to calibrate the page scale");
    if r.clicked() && app.has_doc() {
        app.queue("tool.calibrate");
    }
    if let Some(s) = crate::shell::overlay::page_size_readout(app) {
        ui.add(egui::Separator::default().vertical().spacing(10.0));
        ui.label(RichText::new(s).size(11.0).color(t.text_muted));
    }
    ui.add(egui::Separator::default().vertical().spacing(10.0));
    let dark = app.shell.applied_theme == Some(true);
    let tip = if dark {
        "Switch to the light theme"
    } else {
        "Switch to the dark theme"
    };
    if icons::button(ui, "contrast", 22.0, false, tip).clicked() {
        app.shell.prefs.theme = if dark { "light" } else { "dark" }.to_string();
        app.shell.save_prefs();
    }
    if !status {
        return;
    }
    if let Some(p) = crate::shell::overlay::pointer_readout(app)
        && ui.available_width() > 150.0
    {
        ui.add(egui::Separator::default().vertical().spacing(10.0));
        ui.label(RichText::new(p).size(11.0).color(t.text_muted).monospace());
    }
    if !app.status.is_empty() && ui.available_width() > 60.0 {
        ui.add(egui::Separator::default().vertical().spacing(10.0));
        ui.add(egui::Label::new(RichText::new(&app.status).size(11.0).color(t.text_muted)).truncate());
    }
}

/// A 24-point icon button for a command, tool or panel.
fn small_button(app: &mut AppState, ui: &mut egui::Ui, id: &str) {
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
        .add_enabled_ui(enabled, |ui| icons::button(ui, icon, 24.0, selected, &tip))
        .inner;
    if r.clicked() {
        app.queue(id);
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
        // The page box shows the page label (the number when the page has none) and takes
        // either a number or a label.
        if !focused {
            app.page_entry = if count == 0 {
                String::new()
            } else if label.is_empty() {
                format!("{}", current + 1)
            } else {
                label.clone()
            };
        }
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.page_entry)
                .id(id)
                .desired_width(56.0)
                .horizontal_align(Align::Center),
        );
        if r.lost_focus()
            && let Some(p) = crate::shell::extra::page_from_entry(app, &app.page_entry.clone())
            && let Some(d) = app.doc_mut()
        {
            d.view.go_to_page(p, count);
        }
        let of = if count == 0 {
            String::new()
        } else {
            format!("({} of {count})", current + 1)
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
    // The main window shows its own tab when the active document is in a detached window.
    let area = ui.max_rect();
    let restore = crate::shell::detach::enter_main(app, ui);
    if restore == crate::shell::detach::MainView::Empty {
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, t.workspace);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.4);
                ui.label(
                    RichText::new("Every open document is in a detached window.").color(egui::Color32::from_gray(235)),
                );
                if ui.button("Reattach All Windows").clicked() {
                    app.shell.extra.detached.clear();
                }
            });
        });
        return;
    }
    // Document tabs (not in presentation), then one canvas or the split panes.
    if app.shell.screen != crate::shell::Screen::Presentation {
        if crate::shell::panelbars::tabs_visible(app, ui) {
            crate::shell::tabs::tab_bar(app, ui);
        }
        if app.docs.is_empty() {
            return;
        }
    }
    crate::shell::split::show(app, ui);
    crate::shell::detach::leave_main(app, ui, area, restore);
}

/// Help > Keyboard Shortcuts, Help > About, Document Properties.
pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    let mut open = app.show_shortcuts;
    crate::i18n::window("Keyboard Shortcuts")
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
    crate::i18n::window("About MarkupCraft")
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
    let tags = crate::features::partials_more3::page_tags(app);
    let mut std_act = None;
    let info = app.doc().map(|d| {
        (
            d.name.clone(),
            d.session.path().display().to_string(),
            d.render.as_ref().map_or(0, |r| r.page_count()),
            d.session.doc().markups.len(),
            d.render.as_ref().map_or(0, |r| r.hidden_count()),
            d.session.standards(),
        )
    });
    crate::i18n::window("Document Properties")
        .open(&mut open)
        .collapsible(false)
        .show(ctx, |ui| match &info {
            Some((name, path, pages, markups, drawn, st)) => {
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
                    ui.label("Standards");
                    ui.label(match &st.pdfa {
                        Some(p) => format!("PDF/A-{p} (page edits locked)"),
                        None => "None claimed".to_string(),
                    });
                    ui.end_row();
                    ui.label("Signatures");
                    ui.label(if st.certified {
                        format!("{} (certified)", st.signatures)
                    } else {
                        st.signatures.to_string()
                    });
                    ui.end_row();
                    std_act = crate::features::partials_more3::properties_rows(
                        ui,
                        &app.features.partials.more3,
                        st.pdfa.is_some(),
                        &tags,
                    );
                });
            }
            None => {
                ui.label("No document open.");
            }
        });
    app.show_properties = open;
    if let Some(a) = std_act {
        crate::features::partials_more3::properties_act(app, a);
    }
}

/// The default and current view mode for menu checkmarks.
pub fn mode_checked(app: &AppState, mode: PageMode) -> Option<bool> {
    Some(app.doc().is_some_and(|d| d.view.mode == mode))
}

pub fn fit_checked(app: &AppState, fit: Fit) -> Option<bool> {
    Some(app.doc().is_some_and(|d| d.view.fit == fit))
}
