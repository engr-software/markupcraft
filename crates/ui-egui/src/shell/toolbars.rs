//! Toolbars: the main toolbar (customisable: add, remove and reorder its commands), the Markup
//! and Measure tool strips, each shown or hidden from Window > Toolbars, and Lock Toolbars
//! (no customising while locked).

use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::commands::{self, COMMANDS, MAIN_TOOLBAR};

/// Most buttons on a toolbar.
const MAX_ITEMS: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolbarPrefs {
    pub show_main: bool,
    pub show_markup: bool,
    pub show_measure: bool,
    pub locked: bool,
    /// The main toolbar's command ids in order ("|" = separator).
    pub main: Vec<String>,
    /// Where each toolbar docks, and the user's own toolbars (`toolbars_more`).
    pub more: super::toolbars_more::ToolbarsMore,
}

impl Default for ToolbarPrefs {
    fn default() -> Self {
        let mut main: Vec<String> = MAIN_TOOLBAR.iter().map(|s| s.to_string()).collect();
        // Navigation tools sit with Select and Pan.
        if let Some(i) = main.iter().position(|s| s == "tool.pan") {
            main.insert(i + 1, "tool.zoom".into());
        }
        Self {
            show_main: true,
            show_markup: true,
            show_measure: true,
            locked: false,
            main,
            more: Default::default(),
        }
    }
}

impl ToolbarPrefs {
    /// Drop ids that are not commands (a settings file may hold anything).
    pub fn sanitize(&mut self) {
        self.main.retain(|id| id == "|" || commands::describe(id).is_some());
        self.main.truncate(MAX_ITEMS);
        self.more.sanitize();
    }
}

/// Every command a toolbar may hold (commands with an icon, tools, panels).
pub fn candidates() -> Vec<String> {
    let mut out: Vec<String> = COMMANDS
        .iter()
        .filter(|c| c.built && !c.icon.is_empty())
        .map(|c| c.id.to_string())
        .collect();
    out.extend(crate::tools::TOOLS.iter().map(|t| format!("tool.{}", t.id)));
    out.extend(crate::panels::PANELS.iter().map(|p| format!("panel.{}", p.id)));
    out
}

/// Window > Toolbars > Customize.
pub fn customize_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.shell.show_customize {
        return;
    }
    let mut open = true;
    let mut changed = false;
    let locked = app.shell.ui.toolbars.locked;
    egui::Window::new("Customize Toolbars")
        .open(&mut open)
        .default_size([420.0, 420.0])
        .collapsible(false)
        .show(ctx, |ui| {
            if locked {
                ui.label("Toolbars are locked. Unlock them (Window > Toolbars > Lock Toolbars) to customize.");
                return;
            }
            let target_id = egui::Id::new("tb-target-name");
            let mut target: String = ui.data(|d| d.get_temp(target_id)).unwrap_or_else(|| "Main".into());
            changed |= super::toolbars_more::customize_rows(ui, &mut app.shell.ui.toolbars.more, &mut target);
            ui.data_mut(|d| d.insert_temp(target_id, target.clone()));
            ui.label(format!("{target} toolbar, in order:"));
            let tbs = &mut app.shell.ui.toolbars;
            let items = match tbs.more.custom.iter_mut().find(|c| c.name == target) {
                Some(c) => &mut c.items,
                None => &mut tbs.main,
            };
            let mut action: Option<(usize, i32)> = None;
            egui::ScrollArea::vertical()
                .id_salt("tb-items")
                .max_height(220.0)
                .show(ui, |ui| {
                    for (i, id) in items.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let label = if id == "|" {
                                "---- separator ----".to_string()
                            } else {
                                commands::describe(id).map_or_else(|| id.clone(), |d| d.0)
                            };
                            ui.label(label);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Remove").clicked() {
                                    action = Some((i, 0));
                                }
                                if ui.small_button("Down").clicked() {
                                    action = Some((i, 1));
                                }
                                if ui.small_button("Up").clicked() {
                                    action = Some((i, -1));
                                }
                            });
                        });
                    }
                });
            if let Some((i, a)) = action {
                changed = true;
                match a {
                    0 => {
                        if i < items.len() {
                            items.remove(i);
                        }
                    }
                    -1 if i > 0 && i < items.len() => items.swap(i, i - 1),
                    1 if i + 1 < items.len() => items.swap(i, i + 1),
                    _ => {}
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let pick_id = egui::Id::new("tb-pick");
                let mut pick: String = ui.data(|d| d.get_temp(pick_id)).unwrap_or_default();
                let text = commands::describe(&pick).map_or_else(|| "Choose a command".to_string(), |d| d.0);
                egui::ComboBox::from_id_salt("tb-add")
                    .selected_text(text)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for c in candidates() {
                            let label = commands::describe(&c).map_or_else(|| c.clone(), |d| d.0);
                            ui.selectable_value(&mut pick, c, label);
                        }
                    });
                if ui.button("Add").clicked() && !pick.is_empty() && items.len() < MAX_ITEMS {
                    items.push(pick.clone());
                    changed = true;
                }
                if ui.button("Add Separator").clicked() && items.len() < MAX_ITEMS {
                    items.push("|".into());
                    changed = true;
                }
                ui.data_mut(|d| d.insert_temp(pick_id, pick));
            });
            if ui.button("Reset").clicked() {
                *items = if target == "Main" {
                    ToolbarPrefs::default().main
                } else {
                    Vec::new()
                };
                changed = true;
            }
        });
    if changed {
        app.shell.save_ui();
    }
    app.shell.show_customize = open;
}

/// Add `id` to the main toolbar (automation and tests; also the Customize dialog's Add).
pub fn add(app: &mut AppState, id: &str) -> bool {
    let tb = &mut app.shell.ui.toolbars;
    if tb.locked || commands::describe(id).is_none() || tb.main.len() >= MAX_ITEMS {
        return false;
    }
    tb.main.push(id.to_string());
    app.shell.save_ui();
    true
}

/// Remove `id` from the main toolbar.
pub fn remove(app: &mut AppState, id: &str) -> bool {
    let tb = &mut app.shell.ui.toolbars;
    if tb.locked {
        return false;
    }
    let before = tb.main.len();
    tb.main.retain(|i| i != id);
    let done = tb.main.len() != before;
    if done {
        app.shell.save_ui();
    }
    done
}
