//! More completions of partial rows: Profile Columns (custom columns saved to the profile,
//! added to every document opened, removed from the profile), the tool-set legend (a legend
//! listing a Tool Chest set's subjects), and the Takeoff profile.

use std::collections::HashSet;

use egui::RichText;
use markupcraft_engine::legend::LegendOptions;
use markupcraft_model::columns::CustomColumn;

use crate::{AppState, actions};

#[derive(Default)]
pub struct MoreState {
    pub columns_open: bool,
    /// Documents (uid) the profile's columns were offered to.
    pub columns_seen: HashSet<u64>,
}

/// Run one of these commands; false when `id` is not one of them.
pub fn run(app: &mut AppState, id: &str) -> bool {
    match id {
        "markup.profile_columns" => app.features.partials.more.columns_open = true,
        _ => return super::partials_more2::run(app, id),
    }
    true
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    auto_columns(app);
    columns_window(app, ctx);
    super::partials_more2::window(app, ctx);
}

/// A column id made from `name`, unique among `cols`.
fn column_id(name: &str, cols: &[CustomColumn]) -> String {
    let base: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(40)
        .collect::<String>()
        .to_ascii_lowercase();
    let base = if base.is_empty() { "col".to_string() } else { base };
    let mut id = base.clone();
    let mut k = 2;
    while cols.iter().any(|c| c.id == id) && k < 10_000 {
        id = format!("{base}{k}");
        k += 1;
    }
    id
}

/// Add the profile's columns missing from document `i`; how many were added.
pub fn add_profile_columns(app: &mut AppState, i: usize) -> markupcraft_engine::Result<usize> {
    let profile = app.toolchest.extras.columns.clone();
    let Some(d) = app.docs.get_mut(i) else { return Ok(0) };
    let mut cols = d.session.doc().columns.clone();
    let mut added = 0;
    for c in profile {
        if !cols.iter().any(|o| o.name == c.name) {
            let id = column_id(&c.name, &cols);
            cols.push(CustomColumn { id, ..c });
            added += 1;
        }
    }
    if added > 0 {
        d.session.set_custom_columns(cols)?;
    }
    Ok(added)
}

/// With "every document" on, each document opened gets the profile's columns once.
fn auto_columns(app: &mut AppState) {
    if !app.toolchest.extras.columns_on_open || app.toolchest.extras.columns.is_empty() {
        return;
    }
    let uids: Vec<(usize, u64)> = app.docs.iter().enumerate().map(|(i, d)| (i, d.uid)).collect();
    for (i, uid) in uids {
        if app.features.partials.more.columns_seen.insert(uid)
            && let Ok(n) = add_profile_columns(app, i)
            && n > 0
        {
            app.status = format!("Added {} from the profile", actions::plural(n, "column"));
        }
    }
}

fn columns_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.partials.more.columns_open {
        return;
    }
    let (mut open, mut remove, mut add_now) = (true, None, false);
    let mut every = app.toolchest.extras.columns_on_open;
    super::window("Profile Columns").open(&mut open).show(ctx, |ui| {
        ui.label("Custom columns saved to the profile (Manage Columns > Save to Profile).");
        if app.toolchest.extras.columns.is_empty() {
            ui.label(RichText::new("None saved yet.").weak());
        }
        for c in &app.toolchest.extras.columns {
            ui.horizontal(|ui| {
                ui.label(format!("{}  ({})", c.name, c.kind.name()));
                if ui.small_button("Delete from Profile").clicked() {
                    remove = Some(c.name.clone());
                }
            });
        }
        ui.separator();
        ui.checkbox(&mut every, "Add them to every document opened");
        if ui.button("Add to This Document").clicked() {
            add_now = true;
        }
    });
    app.features.partials.more.columns_open = open;
    if every != app.toolchest.extras.columns_on_open {
        app.toolchest.extras.columns_on_open = every;
        app.features.partials.more.columns_seen.clear();
        app.toolchest.save();
    }
    if let Some(name) = remove {
        app.toolchest.extras.columns.retain(|c| c.name != name);
        app.toolchest.save();
        app.status = format!("Deleted {name} from the profile");
    }
    if add_now {
        let i = app.active;
        let r = add_profile_columns(app, i);
        app.status = actions::report(r, |n| {
            format!("Added {} from the profile", actions::plural(n, "column"))
        });
    }
}

/// Window > Takeoff Workspace: switch to the Takeoff profile (made from the current one the
/// first time), with the Measurements panel, the Markups List and the Tool Chest shown and the
/// measure tool active. The profile keeps that arrangement and its own preferences.
pub fn takeoff_profile(app: &mut AppState) {
    let store = app.shell.store.clone();
    let mut saved = false;
    if let Some(store) = store {
        if store.active() != "Takeoff" {
            let cmd = if store.profiles().iter().any(|p| p == "Takeoff") {
                "switch"
            } else {
                "new"
            };
            crate::prefs_ui::profile(app, cmd, "Takeoff");
        }
        saved = app.shell.store.as_ref().is_some_and(|s| s.active() == "Takeoff");
    }
    // The Tool Chest in the left panel area, the Markups List under the canvas and the
    // Measurements panel docked on the right, so all three show at once.
    for p in ["markups", "toolchest"] {
        app.show_panel(p);
    }
    app.shell.extra.dock_ops.push(crate::shell::extra::DockOp::Attach(
        "measurements",
        crate::panels::Slot::Right,
    ));
    let t = app.edit.more.measure_tool;
    app.set_tool(t);
    app.shell.save_ui();
    app.status = if saved {
        "Takeoff profile: Measurements, Markups List and Tool Chest".into()
    } else {
        "Takeoff workspace: Measurements, Markups List and Tool Chest (no settings folder for a profile)".into()
    };
}

/// The subjects a Tool Chest set's tools make (their subjects, else their names).
pub fn set_subjects(set: &crate::chest::ToolSet) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for it in &set.items {
        let s = if it.markup.subject.trim().is_empty() {
            it.name.trim().to_string()
        } else {
            it.markup.subject.trim().to_string()
        };
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The Legend dialog's "From Tool Chest set" picker: the legend lists every subject of the set
/// (even with none drawn yet) and picks up new markups of those subjects as they are drawn.
pub fn toolset_legend_ui(ui: &mut egui::Ui, chest: &crate::chest::ToolChest, l: &mut LegendOptions) {
    ui.horizontal(|ui| {
        ui.label("From Tool Chest set");
        egui::ComboBox::from_id_salt("legend-toolset")
            .selected_text("Choose a set")
            .show_ui(ui, |ui| {
                for s in &chest.sets {
                    if ui.selectable_label(false, &s.title).clicked() {
                        l.subjects = set_subjects(s);
                        l.show_empty = true;
                        if l.title.trim().is_empty() || l.title == "Legend" {
                            l.title = s.title.clone();
                        }
                    }
                }
            });
    });
}
