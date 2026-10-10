//! Preferences (Ctrl+K): one dialog with a page list on the left, Revu's arrangement. Changes
//! apply at once and persist to the active profile: the engine's preferences
//! (`markupcraft_engine::prefs`: author, units, snapping, colours, save mode, recent files,
//! theme) and MarkupCraft's interface preferences (`shell::UiPrefs`). The Admin page backs up
//! and restores the settings and switches, creates and deletes profiles.

use egui::RichText;
use markupcraft_engine::prefs::Preferences;

use crate::AppState;
use crate::dialogs::Purpose;
use crate::shell::{RulerUnit, UiPrefs};

/// A profile shared as one file.
pub const PROFILE_FILE: crate::dialogs::Filter = ("MarkupCraft profile", &["mcprofile"]);

/// The pages, in order.
pub const PAGES: &[&str] = &[
    "General",
    "Document",
    "Navigation",
    "Grid & Snap",
    "Interface",
    "Tools",
    "Window",
    "Advanced",
    "Admin",
];

/// Apply preferences to the running app (author, snapping, tool reuse...).
pub fn apply_live(app: &mut AppState) {
    let p = app.shell.prefs.clone();
    if !p.author.is_empty() && p.author != app.author {
        app.author = p.author.clone();
        for d in &mut app.docs {
            d.session.set_author(&p.author);
        }
    }
    app.snaps.content = p.snapping.content;
    app.snaps.markup = p.snapping.markup;
    app.snaps.grid = p.snapping.grid;
    crate::snapping::set_grid_spacing(p.snapping.grid_spacing);
    crate::snapping::set_reach(p.snapping.sensitivity_px as f32);
    app.tool_locked = app.shell.ui.reuse_tools;
    app.shell.applied_theme = None;
    crate::features::more6::prefs::apply(app);
}

/// Load everything from the store into the app (startup and profile switches).
pub fn load_from_store(app: &mut AppState) {
    let Some(store) = app.shell.store.clone() else { return };
    match store.load() {
        Ok(p) => app.shell.prefs = p,
        Err(e) => app.shell.prefs_error = e.to_string(),
    }
    app.shell.ui = crate::shell::load_ui(&store);
    // Keyboard shortcuts belong to the profile: `<config>/keys/<profile>.json`. A profile
    // without its own file starts from the keys in use and saves them there.
    let keys = store.dir.join("keys").join(format!("{}.json", store.active()));
    if keys.is_file() {
        app.keys = crate::keyprefs::KeyPrefs::load(&keys);
    } else {
        app.keys.path = Some(keys);
    }
    if let Some(l) = app.shell.ui.layout.clone() {
        app.shell.apply_layout = Some(l);
    }
    apply_live(app);
}

/// A preferences file chosen in a dialog (restore / back up).
pub fn answer(app: &mut AppState, tag: &str, path: &std::path::Path) {
    if tag == "prefs-default-viewer" {
        crate::shell::admin_prefs::write_default_viewer(app, path);
        return;
    }
    let Some(store) = app.shell.store.clone() else {
        // In memory: back up and restore still work on the file.
        match tag {
            "prefs-backup" => {
                let r = markupcraft_engine::prefs::write_file(path, &app.shell.prefs);
                app.status = actions_report(r, "Settings backed up");
            }
            "prefs-restore" => match markupcraft_engine::prefs::read_file(path) {
                Ok(p) => {
                    app.shell.prefs = p;
                    apply_live(app);
                    app.status = "Settings restored".into();
                }
                Err(e) => app.shell.prefs_error = e.to_string(),
            },
            _ => {}
        }
        return;
    };
    let active = store.active();
    match tag {
        "prefs-profile-export" => {
            app.shell.save_ui();
            let deps = app.shell.extra2.export_dependencies;
            let r = store
                .save(&app.shell.prefs)
                .and_then(|_| store.export_bundle(&active, path, deps));
            app.status = match r {
                Ok(n) => format!("Profile {active} exported ({n} shared settings files)"),
                Err(e) => e.to_string(),
            };
        }
        "prefs-profile-import" => match store.import_bundle(path, None) {
            Ok(got) => {
                app.shell.prefs_error.clear();
                profile(app, "switch", &got.name);
                app.status = format!("Profile {} imported", got.name);
            }
            Err(e) => app.shell.prefs_error = e.to_string(),
        },
        "prefs-backup" => {
            let r = store.save(&app.shell.prefs).and_then(|_| store.export(&active, path));
            app.status = actions_report(r, "Settings backed up");
        }
        "prefs-restore" => match store.import(path, &active) {
            Ok(p) => {
                app.shell.prefs = p;
                apply_live(app);
                app.status = "Settings restored".into();
            }
            Err(e) => app.shell.prefs_error = e.to_string(),
        },
        _ => {}
    }
}

fn actions_report(r: markupcraft_engine::Result<()>, ok: &str) -> String {
    match r {
        Ok(()) => ok.to_string(),
        Err(e) => e.to_string(),
    }
}

/// The dialog.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.shell.show_prefs {
        return;
    }
    let mut open = true;
    let before = (app.shell.prefs.clone(), app.shell.ui.clone());
    let mut prefs = app.shell.prefs.clone();
    let mut ui_prefs = app.shell.ui.clone();
    let mut page = app.shell.prefs_page;
    let mut action: Option<&'static str> = None;
    let mut profile_cmd: Option<(&'static str, String)> = None;
    let mut open_url: Option<String> = None;
    crate::i18n::window("Preferences")
        .open(&mut open)
        .default_size([640.0, 440.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(130.0);
                    for p in PAGES {
                        if ui.selectable_label(page == *p, crate::i18n::tr(p)).clicked() {
                            page = p;
                        }
                        // The pages of wave 6A, under their parent page.
                        for (sub, _) in crate::features::more6::prefs::PAGES
                            .iter()
                            .filter(|(_, parent)| parent == p)
                        {
                            if ui
                                .selectable_label(page == *sub, format!("    {}", crate::i18n::tr(sub)))
                                .clicked()
                            {
                                page = sub;
                            }
                        }
                    }
                    for (sub, _) in crate::features::more6::prefs::PAGES
                        .iter()
                        .filter(|(_, parent)| parent.is_empty())
                    {
                        if ui.selectable_label(page == *sub, crate::i18n::tr(sub)).clicked() {
                            page = sub;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(420.0);
                    ui.heading(crate::i18n::tr(page));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        match page {
                            "General" => general(ui, &mut prefs, &mut ui_prefs),
                            "Document" => document(ui, &mut prefs, &mut ui_prefs),
                            "Navigation" => navigation(ui, &mut ui_prefs),
                            "Grid & Snap" => grid_snap(ui, &mut prefs, &mut ui_prefs),
                            "Interface" => {
                                if interface(ui, &mut prefs, &mut ui_prefs) {
                                    action = Some("clear-recent");
                                }
                            }
                            "Tools" => tools(ui, &mut prefs, &mut ui_prefs),
                            "Window" => window_page(ui, &mut ui_prefs),
                            "Advanced" => {
                                advanced(ui, &mut ui_prefs);
                                crate::shell::render_prefs::section(ui, &mut ui_prefs.render);
                            }
                            "Admin" => {
                                if let Some(a) = admin(ui, app, &mut profile_cmd) {
                                    action = Some(a);
                                }
                                if let Some(a) = crate::shell::admin_prefs::section(ui, &mut ui_prefs.admin) {
                                    action = Some(a);
                                }
                            }
                            _ => {}
                        }
                        crate::shell::extra2::section(ui, page, &mut ui_prefs);
                        if let Some(a) = crate::shell::prefs_more::section(ui, page, &mut ui_prefs) {
                            action = Some(a);
                        }
                        if let Some(u) = crate::features::more6::prefs::section(ui, page, &mut prefs) {
                            open_url = Some(u);
                        }
                    });
                    if !app.shell.prefs_error.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(0xB0, 0x20, 0x20), &app.shell.prefs_error);
                    }
                });
            });
        });
    app.shell.prefs_page = page;
    app.shell.show_prefs = open;
    ui_prefs.sanitize();
    if prefs != before.0 {
        match prefs.validate() {
            Ok(()) => {
                app.shell.prefs = prefs;
                app.shell.prefs_error.clear();
                app.shell.save_prefs();
                apply_live(app);
            }
            Err(e) => app.shell.prefs_error = e.to_string(),
        }
    }
    if ui_prefs != before.1 {
        let reuse_changed = ui_prefs.reuse_tools != before.1.reuse_tools;
        app.shell.ui = ui_prefs;
        app.shell.save_ui();
        if reuse_changed {
            app.tool_locked = app.shell.ui.reuse_tools;
        }
    }
    match action {
        Some("clear-recent") => app.queue("file.clear_recent"),
        Some("backup") => app.dialogs.save(
            Purpose::Shell {
                tag: "prefs-backup".into(),
            },
            ("Settings", &["json"]),
            "MarkupCraft settings.json",
        ),
        Some("profile-export") => {
            let name = app.shell.store.as_ref().map(|s| s.active()).unwrap_or_default();
            app.dialogs.save(
                Purpose::Shell {
                    tag: "prefs-profile-export".into(),
                },
                PROFILE_FILE,
                &format!("{name}.mcprofile"),
            )
        }
        Some("profile-import") => app.dialogs.open(
            Purpose::Shell {
                tag: "prefs-profile-import".into(),
            },
            PROFILE_FILE,
            false,
        ),
        Some("restore") => app.dialogs.open(
            Purpose::Shell {
                tag: "prefs-restore".into(),
            },
            ("Settings", &["json"]),
            false,
        ),
        Some("startup-file") => app.dialogs.open(
            Purpose::Shell {
                tag: "x-startup-file".into(),
            },
            crate::dialogs::PDF,
            false,
        ),
        Some("default-viewer") => app.dialogs.folder(Purpose::Shell {
            tag: "prefs-default-viewer".into(),
        }),
        Some("copy-mcp") => {
            ctx.copy_text(crate::shell::prefs_more::MCP_CONFIG.to_string());
            app.status = "MCP configuration copied: paste it into your AI assistant's settings".into();
        }
        Some("reset") => {
            app.shell.prefs = Preferences::default();
            app.shell.ui = UiPrefs::default();
            app.shell.save_prefs();
            app.shell.save_ui();
            apply_live(app);
            app.shell.apply_layout = Some(serde_json::Value::Null);
        }
        _ => {}
    }
    if let Some((cmd, name)) = profile_cmd {
        match cmd {
            "deps" => app.shell.extra2.export_dependencies = !name.is_empty(),
            "rename" => {
                let (from, to) = name.split_once('\n').unwrap_or((name.as_str(), ""));
                app.shell.extra2.rename_to = to.to_string();
                let from = from.to_string();
                profile(app, "rename", &from);
            }
            _ => profile(app, cmd, &name),
        }
    }
    if let Some(u) = open_url {
        crate::features::more6::web::open_in_browser(app, ctx, &u);
    }
}

/// Switch to, create or delete a profile.
pub fn profile(app: &mut AppState, cmd: &str, name: &str) {
    let Some(store) = app.shell.store.clone() else {
        app.shell.prefs_error = "Profiles need a settings folder".into();
        return;
    };
    let r = match cmd {
        "switch" | "new" => {
            // The current interface goes with the profile we leave.
            app.shell.save_ui();
            store.switch(name, cmd == "new").map(|_| ())
        }
        "delete" => store.delete_profile(name),
        "rename" => {
            let to = app.shell.extra2.rename_to.trim().to_string();
            app.shell.save_ui();
            let r = store.rename_profile(name, &to);
            if r.is_ok() {
                app.shell.extra2.rename_to.clear();
                app.status = format!("Profile {name} renamed {to}");
            }
            r
        }
        _ => Ok(()),
    };
    match r {
        Ok(()) => {
            app.shell.prefs_error.clear();
            if cmd != "delete" && cmd != "rename" {
                load_from_store(app);
                app.status = format!("Profile: {name}");
            }
        }
        Err(e) => app.shell.prefs_error = e.to_string(),
    }
}

fn general(ui: &mut egui::Ui, p: &mut Preferences, u: &mut UiPrefs) {
    use crate::i18n::tr;
    ui.label(RichText::new(tr("Options")).strong());
    ui.horizontal(|ui| {
        ui.label(tr("User name (author of new markups)"));
        ui.add(
            egui::TextEdit::singleline(&mut p.author)
                .hint_text("Your name")
                .desired_width(200.0)
                .background_color(ui.visuals().faint_bg_color),
        );
    });
    ui.horizontal(|ui| {
        ui.label(tr("Theme"));
        for (v, l) in [("system", "System"), ("light", "Light"), ("dark", "Dark")] {
            ui.radio_value(&mut p.theme, v.to_string(), tr(l));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Language"));
        for (code, name) in crate::i18n::LANGUAGES {
            ui.radio_value(&mut p.language, (*code).to_string(), *name);
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Startup").strong());
    ui.checkbox(&mut u.reopen_last_session, "Reopen the files that were open last time");
    ui.checkbox(&mut u.show_recents_on_start, "Show recent files on the start page");
    ui.add_space(6.0);
    ui.label(RichText::new("Document tabs").strong());
    ui.horizontal(|ui| {
        ui.label("Longest tab name");
        ui.add(
            egui::DragValue::new(&mut u.tab_max_chars)
                .range(8..=200)
                .suffix(" characters"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Shorten long names at the");
        ui.radio_value(&mut u.tab_truncate_start, false, "end");
        ui.radio_value(&mut u.tab_truncate_start, true, "start");
    });
}

fn document(ui: &mut egui::Ui, p: &mut Preferences, u: &mut UiPrefs) {
    ui.label(RichText::new("Recovery and saving").strong());
    ui.horizontal(|ui| {
        ui.label("Save recovery data every");
        ui.add(
            egui::DragValue::new(&mut p.autosave_minutes)
                .range(0..=1440)
                .suffix(" min"),
        );
        ui.label("(0 = off)");
    });
    ui.horizontal(|ui| {
        ui.label("Save mode");
        ui.radio_value(
            &mut p.save_mode,
            "incremental".to_string(),
            "Keep revisions (incremental)",
        );
        ui.radio_value(&mut p.save_mode, "full".to_string(), "Publish (full rewrite)");
        ui.radio_value(&mut p.save_mode, "compressed".to_string(), "Publish compressed");
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Opening documents").strong());
    ui.horizontal(|ui| {
        ui.label("Page layout");
        egui::ComboBox::from_id_salt("pref-mode")
            .selected_text(mode_label(&u.default_mode))
            .show_ui(ui, |ui| {
                for m in ["single", "continuous", "side", "continuous-side", "auto"] {
                    ui.selectable_value(&mut u.default_mode, m.to_string(), mode_label(m));
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Fit");
        ui.radio_value(&mut u.default_fit, "page".to_string(), "Fit Page");
        ui.radio_value(&mut u.default_fit, "width".to_string(), "Fit Width");
    });
    ui.horizontal(|ui| {
        ui.label("Maximum zoom");
        ui.add(
            egui::DragValue::new(&mut u.max_zoom_pct)
                .range(100.0..=6400.0)
                .suffix(" %"),
        );
    });
    ui.checkbox(
        &mut u.remember_last_page,
        "Reopen each file at its last page, zoom and layout",
    );
}

fn mode_label(m: &str) -> &'static str {
    match m {
        "single" => "Single Page",
        "side" => "Side by Side",
        "continuous-side" => "Continuous Side by Side",
        "auto" => "By page size (drawings one page at a time)",
        _ => "Continuous",
    }
}

fn navigation(ui: &mut egui::Ui, u: &mut UiPrefs) {
    ui.label(RichText::new("Mouse wheel (hold Ctrl for the other)").strong());
    ui.horizontal(|ui| {
        ui.label("Single Page / Side by Side:");
        ui.radio_value(&mut u.wheel_zooms_single, true, "Zoom");
        ui.radio_value(&mut u.wheel_zooms_single, false, "Scroll");
    });
    ui.horizontal(|ui| {
        ui.label("Continuous modes:");
        ui.radio_value(&mut u.wheel_zooms_continuous, true, "Zoom");
        ui.radio_value(&mut u.wheel_zooms_continuous, false, "Scroll");
    });
    ui.checkbox(&mut u.reverse_wheel, "Reverse zoom direction");
    ui.horizontal(|ui| {
        ui.label("Zoom sensitivity");
        ui.add(egui::Slider::new(&mut u.wheel_sensitivity, 0.25..=4.0));
    });
    ui.checkbox(&mut u.lock_fit_width, "Lock panning to up and down in Fit Width");
}

fn grid_snap(ui: &mut egui::Ui, p: &mut Preferences, u: &mut UiPrefs) {
    ui.horizontal(|ui| {
        ui.label("Units (rulers)");
        egui::ComboBox::from_id_salt("pref-ruler")
            .selected_text(u.ruler_unit.label())
            .show_ui(ui, |ui| {
                for r in RulerUnit::ALL {
                    ui.selectable_value(&mut u.ruler_unit, r, r.label());
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Grid spacing");
        ui.add(
            egui::DragValue::new(&mut p.snapping.grid_spacing)
                .range(1.0..=1000.0)
                .suffix(" pt"),
        );
        ui.label(format!("({:.3} in)", p.snapping.grid_spacing / 72.0));
    });
    ui.checkbox(&mut p.snapping.grid, "Snap to grid");
    ui.checkbox(&mut p.snapping.content, "Snap to content");
    ui.checkbox(&mut p.snapping.markup, "Snap to markup");
    ui.horizontal(|ui| {
        ui.label("Snap sensitivity");
        ui.add(egui::Slider::new(&mut p.snapping.sensitivity_px, 1.0..=40.0).suffix(" px"));
    });
}

fn interface(ui: &mut egui::Ui, p: &mut Preferences, u: &mut UiPrefs) -> bool {
    ui.label(RichText::new("File Access").strong());
    ui.horizontal(|ui| {
        ui.label("Recent files listed");
        ui.add(egui::DragValue::new(&mut p.recent_files).range(0..=200));
    });
    ui.horizontal(|ui| {
        ui.label("Forget files not opened for");
        ui.add(
            egui::DragValue::new(&mut u.recents_days)
                .range(0..=3650)
                .suffix(" days"),
        );
        ui.label("(0 = never)");
    });
    ui.checkbox(&mut u.recents_preview, "Preview recent files (first page on hover)");
    ui.button("Clear Recent Files").clicked()
}

fn tools(ui: &mut egui::Ui, p: &mut Preferences, u: &mut UiPrefs) {
    ui.label(RichText::new("Markup").strong());
    ui.checkbox(
        &mut u.reuse_tools,
        "Reuse markup tools (stay in the tool after placing a markup)",
    );
    ui.horizontal(|ui| {
        ui.label("Measurement units");
        egui::ComboBox::from_id_salt("pref-units")
            .selected_text(p.units.clone())
            .show_ui(ui, |ui| {
                for v in ["ft-in", "ft", "in", "yd", "m", "cm", "mm", "pt"] {
                    ui.selectable_value(&mut p.units, v.to_string(), v);
                }
            });
        ui.label("Precision");
        ui.add(egui::DragValue::new(&mut p.precision).range(0..=8));
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Colours of new markups").strong());
    for (label, c) in [
        ("Markups", &mut p.colors.markup),
        ("Measurements", &mut p.colors.measurement),
        ("Highlights", &mut p.colors.highlight),
        ("Text", &mut p.colors.text),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut rgb = hex_rgb(c).unwrap_or([255, 0, 0]);
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                *c = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
            }
        });
    }
    crate::markup_prefs::section(ui, &mut u.markup);
}

fn hex_rgb(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    Some([v(0)?, v(2)?, v(4)?])
}

fn window_page(ui: &mut egui::Ui, u: &mut UiPrefs) {
    ui.label(RichText::new("Presentation").strong());
    ui.checkbox(&mut u.presentation_loop, "Loop back to the first page after the last");
    ui.horizontal(|ui| {
        ui.label("Advance every");
        ui.add(
            egui::DragValue::new(&mut u.presentation_advance_secs)
                .range(0.0..=3600.0)
                .suffix(" s"),
        );
        ui.label("(0 = by hand)");
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Bars").strong());
    ui.checkbox(&mut u.show_menu, "Menu bar (F9)");
    ui.checkbox(&mut u.show_nav_bar, "Navigation bar (F4)");
    ui.checkbox(&mut u.show_status_bar, "Status bar (F8)");
    ui.checkbox(&mut u.toolbars.show_main, "Main toolbar");
    ui.checkbox(&mut u.toolbars.show_markup, "Markup tools");
    ui.checkbox(&mut u.toolbars.show_measure, "Measure tools");
}

fn advanced(ui: &mut egui::Ui, u: &mut UiPrefs) {
    ui.label(RichText::new("2D rendering").strong());
    ui.horizontal(|ui| {
        ui.label("Dimmer amount");
        ui.add(egui::Slider::new(&mut u.dimmer_pct, 5.0..=95.0).suffix(" %"));
    });
    ui.checkbox(&mut u.crosshair, "Full-screen crosshair");
}

fn admin(ui: &mut egui::Ui, app: &AppState, cmd: &mut Option<(&'static str, String)>) -> Option<&'static str> {
    let mut out = None;
    ui.label(RichText::new("Settings").strong());
    ui.horizontal(|ui| {
        if ui.button("Back Up Settings...").clicked() {
            out = Some("backup");
        }
        if ui.button("Restore Settings...").clicked() {
            out = Some("restore");
        }
        if ui.button("Reset All Settings").clicked() {
            out = Some("reset");
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Profiles").strong());
    match &app.shell.store {
        None => {
            ui.label(RichText::new("Settings are not saved in this session.").weak());
        }
        Some(store) => {
            let active = store.active();
            for name in store.profiles() {
                ui.horizontal(|ui| {
                    let on = name == active;
                    if ui.selectable_label(on, &name).clicked() && !on {
                        *cmd = Some(("switch", name.clone()));
                    }
                    if !on && ui.small_button("Delete").clicked() {
                        *cmd = Some(("delete", name.clone()));
                    }
                });
            }
            ui.horizontal(|ui| {
                let id = egui::Id::new("rename-profile");
                let mut to: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut to)
                        .hint_text("New name")
                        .desired_width(140.0),
                );
                if ui.button("Rename Profile").clicked() && !to.trim().is_empty() {
                    *cmd = Some(("rename", format!("{active}\n{}", to.trim())));
                    to.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, to));
            });
            ui.horizontal(|ui| {
                if ui.button("Export Profile...").clicked() {
                    out = Some("profile-export");
                }
                if ui.button("Import Profile...").clicked() {
                    out = Some("profile-import");
                }
                let mut deps = app.shell.extra2.export_dependencies;
                if ui
                    .checkbox(&mut deps, "Include dependencies")
                    .on_hover_text("Tool sets, line styles, presets and the other shared settings files")
                    .changed()
                {
                    *cmd = Some(("deps", if deps { "1".into() } else { String::new() }));
                }
            });
            ui.label(RichText::new("MarkupCraft ships Takeoff, Construction, Design Review and Simple.").weak());
            ui.horizontal(|ui| {
                let id = egui::Id::new("new-profile");
                let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                ui.text_edit_singleline(&mut name);
                if ui.button("New Profile").clicked() && !name.trim().is_empty() {
                    *cmd = Some(("new", name.trim().to_string()));
                    name.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, name));
            });
        }
    }
    out
}
