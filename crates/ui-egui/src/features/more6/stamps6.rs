//! Interactive stamps (Markup > Interactive Stamp, Edit Stamp Fields), the stamp settings
//! (Markup > Stamp Settings: folder, default stamp, opacity, blend mode, lock) and Place Default
//! Stamp, and Add Selection to Tool Chest as Symbol.

use std::collections::BTreeMap;

use markupcraft_engine::extras6::StampSettings;
use markupcraft_engine::stamp_fields::{StampField, StampFieldKind, TEMPLATE_KEY};
use markupcraft_engine::stamps::{StampLibrary, StampPlace};
use markupcraft_geom::Point;
use markupcraft_model::Color;

use super::{Ask6, Pick6, doc_step};
use crate::AppState;
use crate::dialogs::Purpose;
use crate::features::{Ask, start_pick};

/// A starting template for a new interactive stamp.
pub const EXAMPLE: &str = "REVIEWED\r{check:No Exceptions Taken}  {check:Make Corrections Noted}\r{check:Revise and Resubmit}  {choice:Status=Open|Closed}\rBy {user}  {date}  {field:Initials}";

pub struct Stamps6State {
    pub place_open: bool,
    pub template: String,
    pub color: Color,
    /// Edit Stamp Fields: the stamp and its fields being edited.
    pub editing: Option<(String, Vec<StampField>)>,
    pub settings_open: bool,
    pub settings: StampSettings,
    /// Stamp ids for the default-stamp list.
    pub library_ids: Vec<(String, String)>,
    pub message: String,
}

impl Default for Stamps6State {
    fn default() -> Self {
        Self {
            place_open: false,
            template: EXAMPLE.into(),
            color: Color::rgb(0.05, 0.3, 0.8),
            editing: None,
            settings_open: false,
            settings: StampSettings::default(),
            library_ids: Vec::new(),
            message: String::new(),
        }
    }
}

/// The configuration folder stamp settings live in: the Tool Chest file's folder (none when the
/// Tool Chest is in memory only, as in tests: then the settings last for the session).
fn config(app: &AppState) -> Option<std::path::PathBuf> {
    app.toolchest
        .path
        .as_ref()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
}

/// The single selected markup when it is an interactive stamp.
pub fn selected_interactive(app: &AppState) -> Option<String> {
    let d = app.doc()?;
    let [id] = d.selection() else { return None };
    let m = d.session.doc().find(id)?;
    m.column_data.contains_key(TEMPLATE_KEY).then(|| id.clone())
}

/// Markup > Edit Stamp Fields.
pub fn edit_fields(app: &mut AppState) {
    let Some(id) = selected_interactive(app) else {
        app.status = "Select an interactive stamp first".into();
        return;
    };
    let Some(d) = app.doc() else { return };
    match d.session.stamp_fields(&id) {
        Ok(f) => app.features.more6.stamps.editing = Some((id, f)),
        Err(e) => app.status = e.to_string(),
    }
}

/// The interactive stamp's place was picked.
pub fn place_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(at) = place_of(pts) else { return };
    let st = &app.features.more6.stamps;
    let (text, color) = (st.template.clone(), st.color);
    let values = BTreeMap::new();
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    match d
        .session
        .place_interactive_stamp(page, at, &text, color, &BTreeMap::new(), &values)
    {
        Ok(id) => {
            d.rerender(threads);
            crate::actions::select(&mut d.session, vec![id]);
            app.status = "Interactive stamp placed: fill in its fields".into();
            edit_fields(app);
        }
        Err(e) => app.status = e.to_string(),
    }
}

fn place_of(pts: &[Point]) -> Option<StampPlace> {
    match crate::features::rect_of(pts) {
        Some(r) => Some(StampPlace::Rect(r)),
        None => pts.first().map(|p| StampPlace::Center(*p)),
    }
}

/// Markup > Stamp Settings.
pub fn open_settings(app: &mut AppState) {
    let cfg = config(app);
    let st = &mut app.features.more6.stamps;
    if let Some(c) = cfg.clone() {
        match StampSettings::load(&c) {
            Ok(s) => st.settings = s,
            Err(e) => st.message = e.to_string(),
        }
    }
    let lib = cfg.map(|c| st.settings.library(&c));
    let all = match &lib {
        Some(l) => l.all().unwrap_or_else(|_| markupcraft_engine::stamps::builtin_stamps()),
        None => markupcraft_engine::stamps::builtin_stamps(),
    };
    st.library_ids = all.into_iter().map(|e| (e.id, e.name)).collect();
    st.settings_open = true;
}

fn library(app: &AppState, st: &StampSettings) -> Option<StampLibrary> {
    match config(app) {
        Some(c) => Some(st.library(&c)),
        None => (!st.folder.trim().is_empty()).then(|| StampLibrary::new(st.folder.trim())),
    }
}

/// Place Default Stamp: the place was picked.
pub fn default_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(at) = place_of(pts) else { return };
    let settings = app.features.more6.stamps.settings.clone();
    let lib = library(app, &settings);
    doc_step(app, |s| {
        s.place_stamp_with_settings(page, at, None, lib.as_ref(), &settings)
            .map(|_| format!("Placed {}", settings.default_stamp))
    });
}

/// Markup > Add Selection to Tool Chest as Symbol: a Drawing-mode item in the Symbols tool set,
/// drawn at the page's scale (placed copies are resized to each page's scale).
pub fn symbol_from_selection(app: &mut AppState) {
    let Some(d) = app.doc() else { return };
    let [id] = d.selection() else {
        app.status = "Select one markup to save as a symbol (group several into one markup first)".into();
        return;
    };
    let Some(m) = d.session.doc().find(id).cloned() else {
        return;
    };
    let scale = d.session.doc().pages.get(m.page).and_then(|p| p.scale.clone());
    let chest = &mut app.toolchest;
    let set = match chest.sets.iter().find(|s| s.title == "Symbols") {
        Some(s) => s.id.clone(),
        None => {
            let id = chest.add_set("Symbols");
            chest.set_scale(&id, scale);
            id
        }
    };
    match chest.add_markup(&set, &m) {
        Some(item) => {
            chest.update_item(&set, &item, |i| i.mode = crate::chest::Mode::Drawing);
            app.status = "Saved to the Symbols tool set: click it in the Tool Chest, then click to place".into();
            app.show_panel("toolchest");
        }
        None => {
            app.status = chest
                .error
                .clone()
                .unwrap_or_else(|| "That markup cannot be a tool".into())
        }
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    place_window(app, ctx);
    fields_window(app, ctx);
    settings_window(app, ctx);
}

fn place_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.stamps.place_open {
        return;
    }
    let mut open = true;
    let mut go = false;
    let st = &mut app.features.more6.stamps;
    crate::features::window("Interactive Stamp").open(&mut open).show(ctx, |ui| {
        ui.label("Fields the reviewer fills in after placing: {check:Label}, {field:Label=Default}, {choice:Label=A|B|C}; plus {user}, {date} and the other dynamic fields.");
        ui.add(
            egui::TextEdit::multiline(&mut st.template)
                .id(egui::Id::new("interactive-template"))
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.label("Colour");
            crate::features::color_edit(ui, &mut st.color);
        });
        let n = markupcraft_engine::stamp_fields::interactive_fields(&st.template).len();
        ui.weak(format!("{} in this template", crate::actions::plural(n, "field")));
        go = ui.add_enabled(n > 0, egui::Button::new("Place...")).clicked();
    });
    app.features.more6.stamps.place_open = open;
    if go {
        app.features.more6.stamps.place_open = false;
        start_pick(
            app,
            Pick6::InteractiveStamp.into(),
            "Click to place the stamp, or drag its box",
        );
    }
}

fn fields_window(app: &mut AppState, ctx: &egui::Context) {
    let Some((id, mut fields)) = app.features.more6.stamps.editing.clone() else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    crate::features::window("Stamp Fields").open(&mut open).show(ctx, |ui| {
        egui::Grid::new("stamp-fields").num_columns(2).show(ui, |ui| {
            for f in &mut fields {
                match f.kind {
                    StampFieldKind::Check => {
                        let mut on = f.value == "on";
                        ui.label("");
                        if ui.checkbox(&mut on, &f.label).changed() {
                            f.value = if on { "on".into() } else { String::new() };
                        }
                    }
                    StampFieldKind::Text => {
                        ui.label(&f.label);
                        ui.text_edit_singleline(&mut f.value);
                    }
                    StampFieldKind::Choice => {
                        ui.label(&f.label);
                        egui::ComboBox::from_id_salt(("stamp-choice", f.label.clone()))
                            .selected_text(f.value.clone())
                            .show_ui(ui, |ui| {
                                for o in f.options.clone() {
                                    ui.selectable_value(&mut f.value, o.clone(), o);
                                }
                            });
                    }
                }
                ui.end_row();
            }
        });
        apply = ui.button("Apply").clicked();
    });
    if !open {
        app.features.more6.stamps.editing = None;
        return;
    }
    if let Some(e) = app.features.more6.stamps.editing.as_mut() {
        e.1 = fields.clone();
    }
    if apply {
        let values: BTreeMap<String, String> = fields.iter().map(|f| (f.label.clone(), f.value.clone())).collect();
        app.features.more6.stamps.editing = None;
        doc_step(app, |s| {
            s.set_stamp_fields(&id, &values).map(|_| "Stamp updated".to_string())
        });
    }
}

fn settings_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.stamps.settings_open {
        return;
    }
    let mut open = true;
    let (mut save, mut folder) = (false, false);
    let st = &mut app.features.more6.stamps;
    crate::features::window("Stamp Settings")
        .open(&mut open)
        .show(ctx, |ui| {
            let s = &mut st.settings;
            ui.horizontal(|ui| {
                ui.label("Stamp folder");
                ui.add(
                    egui::TextEdit::singleline(&mut s.folder)
                        .desired_width(240.0)
                        .hint_text("(default library)"),
                );
                folder = ui.button("Browse...").clicked();
            });
            ui.horizontal(|ui| {
                ui.label("Default stamp");
                let shown = st
                    .library_ids
                    .iter()
                    .find(|(i, _)| *i == s.default_stamp)
                    .map_or_else(|| s.default_stamp.clone(), |(_, n)| n.clone());
                egui::ComboBox::from_id_salt("default-stamp")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (i, n) in &st.library_ids {
                            ui.selectable_value(&mut s.default_stamp, i.clone(), n);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Opacity");
                ui.add(egui::Slider::new(&mut s.opacity, 0.05..=1.0));
            });
            ui.horizontal(|ui| {
                ui.label("Blend mode");
                ui.selectable_value(&mut s.blend, "normal".to_string(), "Normal");
                ui.selectable_value(&mut s.blend, "multiply".to_string(), "Multiply");
            });
            ui.checkbox(&mut s.lock, "Lock placed stamps");
            save = ui.button("Save").clicked();
            if !st.message.is_empty() {
                ui.label(&st.message);
            }
        });
    app.features.more6.stamps.settings_open = open;
    if folder {
        app.dialogs.folder(Purpose::Feature(Ask::More6(Ask6::StampFolder)));
    }
    if save {
        let cfg = config(app);
        // The Stamp Library uses the chosen folder too.
        let folder = app.features.more6.stamps.settings.folder.trim().to_string();
        app.features.stamps.dir = (!folder.is_empty()).then(|| std::path::PathBuf::from(folder));
        app.features.stamps.loaded = false;
        let st = &mut app.features.more6.stamps;
        st.message = match (cfg, st.settings.validate()) {
            (_, Err(e)) => e.to_string(),
            (Some(c), Ok(())) => match st.settings.save(&c) {
                Ok(()) => "Stamp settings saved".into(),
                Err(e) => e.to_string(),
            },
            (None, Ok(())) => "Stamp settings apply to this session".into(),
        };
    }
}
