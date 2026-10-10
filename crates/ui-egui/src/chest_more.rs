//! More of the Tool Chest and the user's library, kept in the Tool Chest file (`extras`):
//!
//! - Recent Tools options: how many to keep, the mode new recent tools take, Clear.
//! - Tool Chest options: comment persistence (never / except text boxes / always) and Update
//!   Tool Set Item on Reuse (a markup's later look changes write back to the item it came from).
//! - Tool sets pinned as floating toolbars, locked (read-only) sets, the flyout of a collapsed
//!   set, items updated from the selected markup, sequence items (a trailing number that
//!   counts up each time the item is placed).
//! - The user's scale presets (Measurements panel > Add Preset) and line styles (Line Styles
//!   window; exported and imported as JSON to share), and the custom columns saved to the
//!   profile (Manage Columns > Save to Profile / Load from Profile).

use markupcraft_model::{CustomColumn, Markup, Scale};
use serde::{Deserialize, Serialize};

use crate::chest::{Mode, ToolChest};
use crate::{AppState, actions};

/// A named user scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedScale {
    pub name: String,
    pub scale: Scale,
}

/// A named dash pattern (points; empty = solid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineStyle {
    pub name: String,
    pub dash: Vec<f64>,
}

/// Whether a tool's saved comment carries into new markups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CommentMode {
    Never,
    /// all but text boxes, callouts and typewriter text (whose text is what is typed)
    #[default]
    ExceptText,
    Always,
}

impl CommentMode {
    pub const ALL: [CommentMode; 3] = [CommentMode::Never, CommentMode::ExceptText, CommentMode::Always];
    pub fn name(self) -> &'static str {
        match self {
            CommentMode::Never => "No",
            CommentMode::ExceptText => "Except text boxes",
            CommentMode::Always => "All markups",
        }
    }
}

fn default_recent_max() -> usize {
    crate::chest::RECENT_MAX
}

/// Library and options saved with the Tool Chest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChestExtras {
    pub scale_presets: Vec<NamedScale>,
    pub line_styles: Vec<LineStyle>,
    /// custom columns saved to the profile
    pub columns: Vec<CustomColumn>,
    #[serde(default = "default_recent_max")]
    pub recent_max: usize,
    pub recent_mode: Mode,
    pub comments: CommentMode,
    pub update_on_reuse: bool,
    /// tool sets shown as floating toolbars (set ids)
    pub pinned: Vec<String>,
    /// read-only tool sets (set ids)
    pub locked: Vec<String>,
    /// the user's own review statuses (after None, Accepted, Rejected, Cancelled, Completed)
    pub statuses: Vec<String>,
    /// tool sets linked to files in a shared folder (`chest_shared`)
    pub shared: Vec<crate::chest_shared::SharedSet>,
    /// add the profile's columns to every document opened
    pub columns_on_open: bool,
}

impl Default for ChestExtras {
    fn default() -> Self {
        Self {
            scale_presets: Vec::new(),
            line_styles: Vec::new(),
            columns: Vec::new(),
            recent_max: default_recent_max(),
            recent_mode: Mode::Properties,
            comments: CommentMode::ExceptText,
            update_on_reuse: false,
            pinned: Vec::new(),
            locked: Vec::new(),
            statuses: Vec::new(),
            shared: Vec::new(),
            columns_on_open: false,
        }
    }
}

/// Most user scale presets, line styles and profile columns kept.
const MAX_LIBRARY: usize = 500;

impl ChestExtras {
    /// Keep a hand-edited file within bounds.
    pub fn sanitize(&mut self) {
        self.recent_max = self.recent_max.clamp(1, 50);
        self.scale_presets
            .retain(|p| p.scale.valid() && !p.name.trim().is_empty());
        self.scale_presets.truncate(MAX_LIBRARY);
        self.line_styles
            .retain(|s| valid_dash(&s.dash) && !s.name.trim().is_empty());
        self.line_styles.truncate(MAX_LIBRARY);
        self.columns.truncate(MAX_LIBRARY);
        self.statuses.retain(|s| !s.trim().is_empty() && s.len() <= 100);
        self.statuses.truncate(100);
    }
}

/// A dash pattern the writer takes: up to 16 lengths of 0 to 1000 points, not all zero.
pub fn valid_dash(d: &[f64]) -> bool {
    d.len() <= 16
        && d.iter().all(|v| v.is_finite() && (0.0..=1000.0).contains(v))
        && (d.is_empty() || d.iter().any(|v| *v > 0.0))
}

/// `6, 3, 1, 3` -> a dash pattern.
pub fn parse_dash(text: &str) -> Option<Vec<f64>> {
    let v: Option<Vec<f64>> = text
        .split([',', ' '])
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().parse::<f64>().ok())
        .collect();
    v.filter(|d| valid_dash(d))
}

/// The number at the end of `s` counted up by one (`A1` -> `A2`, `D-09` -> `D-10`); `None`
/// without a trailing number.
pub fn next_in_sequence(s: &str) -> Option<String> {
    let digits = s.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 18 {
        return None;
    }
    let cut = s.len() - digits;
    let (head, num) = (s.get(..cut)?, s.get(cut..)?);
    let n: u64 = num.parse().ok()?;
    let next = n.checked_add(1)?.to_string();
    let width = num.len().max(next.len());
    Some(format!("{head}{next:0>width$}"))
}

impl ToolChest {
    pub fn is_locked(&self, set: &str) -> bool {
        self.extras.locked.iter().any(|s| s == set)
    }

    pub fn set_locked(&mut self, set: &str, on: bool) {
        self.extras.locked.retain(|s| s != set);
        if on {
            self.extras.locked.push(set.to_string());
        }
        self.save();
    }

    pub fn set_pinned(&mut self, set: &str, on: bool) {
        self.extras.pinned.retain(|s| s != set);
        if on {
            self.extras.pinned.push(set.to_string());
        }
        self.save();
    }

    /// Keep a custom review status for the status menus.
    pub fn add_status(&mut self, status: &str) {
        let s = status.trim();
        if s.is_empty()
            || s.len() > 100
            || markupcraft_model::review_statuses()
                .iter()
                .any(|d| d.eq_ignore_ascii_case(s))
            || self.extras.statuses.iter().any(|d| d == s)
        {
            return;
        }
        self.extras.statuses.push(s.to_string());
        self.extras.statuses.truncate(100);
        self.save();
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// Save the page scale `scale` as preset `name` (the same name replaces it).
    pub fn add_scale_preset(&mut self, name: &str, scale: &Scale) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || !scale.valid() {
            return Err("a preset needs a name and a scale".into());
        }
        self.extras.scale_presets.retain(|p| p.name != name);
        self.extras.scale_presets.insert(
            0,
            NamedScale {
                name: name.into(),
                scale: scale.clone(),
            },
        );
        self.extras.scale_presets.truncate(MAX_LIBRARY);
        self.save();
        Ok(())
    }

    pub fn remove_scale_preset(&mut self, name: &str) {
        self.extras.scale_presets.retain(|p| p.name != name);
        self.save();
    }

    /// Add or replace line style `name`.
    pub fn add_line_style(&mut self, name: &str, dash: Vec<f64>) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || !valid_dash(&dash) {
            return Err("a line style needs a name and up to 16 dash lengths (not all zero)".into());
        }
        self.extras.line_styles.retain(|s| s.name != name);
        self.extras.line_styles.push(LineStyle {
            name: name.into(),
            dash,
        });
        self.extras.line_styles.truncate(MAX_LIBRARY);
        self.save();
        Ok(())
    }

    pub fn remove_line_style(&mut self, name: &str) {
        self.extras.line_styles.retain(|s| s.name != name);
        self.save();
    }

    /// The line styles as a shareable JSON set.
    pub fn export_line_styles(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&LineStyleSet {
            format: "markupcraft-linestyles".into(),
            styles: self.extras.line_styles.clone(),
        })
        .map_err(|e| e.to_string())
    }

    /// Add a shared line style set (styles of the same name are replaced). Returns how many.
    pub fn import_line_styles(&mut self, text: &str) -> Result<usize, String> {
        let set: LineStyleSet = serde_json::from_str(text).map_err(|e| format!("not a line style set: {e}"))?;
        if set.format != "markupcraft-linestyles" {
            return Err("not a MarkupCraft line style set".into());
        }
        let mut n = 0;
        for s in set.styles.into_iter().take(MAX_LIBRARY) {
            if valid_dash(&s.dash) && !s.name.trim().is_empty() {
                self.extras.line_styles.retain(|o| o.name != s.name);
                self.extras.line_styles.push(s);
                n += 1;
            }
        }
        self.extras.line_styles.truncate(MAX_LIBRARY);
        self.save();
        Ok(n)
    }

    /// Save custom columns to the profile (replacing those of the same name).
    pub fn save_profile_columns(&mut self, cols: &[CustomColumn]) {
        for c in cols {
            self.extras.columns.retain(|o| o.name != c.name);
            self.extras.columns.push(c.clone());
        }
        self.extras.columns.truncate(MAX_LIBRARY);
        self.save();
    }

    /// Item `item` of `set` takes `m`'s look (Update from Selection, Update on Reuse).
    pub fn update_item_look(&mut self, set: &str, item: &str, m: &Markup) -> bool {
        if self.is_locked(set) {
            return false;
        }
        let Some(it) = self.item(set, item) else { return false };
        if it.markup.kind != m.kind {
            return false;
        }
        let mut t = crate::chest::template_of(m);
        if it.mode == Mode::Properties {
            // Properties mode keeps the look only; the saved geometry stays.
            t.pts = it.markup.pts.clone();
            t.rect = it.markup.rect;
            t.page = it.markup.page;
        }
        self.update_item(set, item, |i| i.markup = t);
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LineStyleSet {
    format: String,
    styles: Vec<LineStyle>,
}

/// State of these features for the session (not saved).
#[derive(Debug, Clone, Default)]
pub struct ChestRuntime {
    /// markups drawn from a tool item: markup id -> (set, item)
    pub origins: std::collections::HashMap<String, (String, String)>,
    /// the Line Styles window is open; its entry fields
    pub styles_open: bool,
    pub style_name: String,
    pub style_dash: String,
}

/// A markup was drawn from the active Tool Chest item: remember where it came from, and count
/// a sequence item on.
pub fn created(app: &mut AppState, m: &Markup) {
    let Some((set, item)) = app.active_item.clone() else {
        return;
    };
    if set == "recent" {
        return;
    }
    app.edit
        .chest_rt
        .origins
        .insert(m.id.clone(), (set.clone(), item.clone()));
    if app.edit.chest_rt.origins.len() > 10_000 {
        app.edit.chest_rt.origins.clear();
    }
    let seq = app.toolchest.item(&set, &item).is_some_and(|i| i.sequence);
    if seq && !app.toolchest.is_locked(&set) {
        app.toolchest.update_item(&set, &item, |i| {
            if let Some(n) = next_in_sequence(&i.markup.label) {
                i.markup.label = n;
            }
            if i.mode == Mode::Drawing
                && let Some(n) = next_in_sequence(&i.markup.contents)
            {
                i.markup.contents = n;
            }
        });
    }
}

/// Once a frame: Update on Reuse writes the selected markup's look back to its item; pinned
/// tool sets float as toolbars; the Line Styles window.
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    app.edit.chest_comments = app.toolchest.extras.comments;
    if app.toolchest.extras.update_on_reuse
        && let Some(m) = app
            .doc()
            .and_then(|d| match d.selection() {
                [id] => d.session.doc().find(id),
                _ => None,
            })
            .cloned()
        && let Some((set, item)) = app.edit.chest_rt.origins.get(&m.id).cloned()
        && let Some(it) = app.toolchest.item(&set, &item)
        && actions::patch_from(&it.markup) != actions::patch_from(&m)
    {
        app.toolchest.update_item_look(&set, &item, &m);
    }
    pinned_toolbars(app, ctx);
    line_styles_window(app, ctx);
}

fn pinned_toolbars(app: &mut AppState, ctx: &egui::Context) {
    let mut use_item = None;
    let mut unpin = None;
    for sid in app.toolchest.extras.pinned.clone() {
        let Some(set) = app.toolchest.find_set(&sid).cloned() else {
            continue;
        };
        let mut open = true;
        egui::Window::new(&set.title)
            .id(egui::Id::new(("pinned-set", &sid)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_pos(egui::pos2(400.0, 120.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for it in &set.items {
                        let Some(tool) = crate::tools::find(&it.tool) else {
                            continue;
                        };
                        let active = app.active_item.as_ref().is_some_and(|(s, i)| *s == sid && *i == it.id);
                        if crate::icons::button(ui, tool.icon, 26.0, active, &it.name).clicked() {
                            use_item = Some((sid.clone(), it.id.clone()));
                        }
                    }
                    if set.items.is_empty() {
                        ui.label("(empty)");
                    }
                });
            });
        if !open {
            unpin = Some(sid);
        }
    }
    if let Some((s, i)) = use_item {
        app.edit.item_scale = app.toolchest.item_set_scale(&s).cloned();
        app.use_item(&s, &i);
    }
    if let Some(s) = unpin {
        app.toolchest.set_pinned(&s, false);
    }
}

fn line_styles_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.edit.chest_rt.styles_open {
        return;
    }
    let mut open = true;
    let mut status = None;
    let mut remove = None;
    let mut add = false;
    let mut io: Option<&'static str> = None;
    crate::i18n::window("Line Styles")
        .open(&mut open)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.label("Dash patterns in points: dash, gap, dash, gap ...");
            egui::Grid::new("line-styles").num_columns(3).show(ui, |ui| {
                for s in &app.toolchest.extras.line_styles {
                    ui.label(&s.name);
                    ui.label(s.dash.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(", "));
                    if crate::icons::button(ui, "trash-2", 20.0, false, "Delete line style").clicked() {
                        remove = Some(s.name.clone());
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.edit.chest_rt.style_name)
                        .hint_text("Name")
                        .desired_width(110.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut app.edit.chest_rt.style_dash)
                        .hint_text("8, 3, 2, 3")
                        .desired_width(110.0),
                );
                if ui.button("Add Style").clicked() {
                    add = true;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Export Set...").clicked() {
                    io = Some("export");
                }
                if ui.button("Import Set...").clicked() {
                    io = Some("import");
                }
            });
        });
    if add {
        let name = app.edit.chest_rt.style_name.clone();
        status = Some(match parse_dash(&app.edit.chest_rt.style_dash) {
            Some(d) => match app.toolchest.add_line_style(&name, d) {
                Ok(()) => {
                    app.edit.chest_rt.style_name.clear();
                    app.edit.chest_rt.style_dash.clear();
                    format!("Added line style {}", name.trim())
                }
                Err(e) => e,
            },
            None => "Type the dash lengths as numbers, like 8, 3, 2, 3".into(),
        });
    }
    if let Some(n) = remove {
        app.toolchest.remove_line_style(&n);
    }
    match io {
        Some("export") => app.dialogs.save(
            crate::dialogs::Purpose::Edit("export_linestyles", String::new()),
            STYLES,
            "Line Styles.json",
        ),
        Some(_) => app.dialogs.open(
            crate::dialogs::Purpose::Edit("import_linestyles", String::new()),
            STYLES,
            false,
        ),
        None => {}
    }
    app.edit.chest_rt.styles_open = open;
    if let Some(s) = status {
        app.status = s;
    }
}

/// Line style sets.
pub const STYLES: crate::dialogs::Filter = ("Line styles", &["json"]);

/// The file dialog tags answered here.
pub fn handles_dialog(tag: &str) -> bool {
    matches!(tag, "export_linestyles" | "import_linestyles" | "choice_csv")
}

pub fn dialog_answer(app: &mut AppState, tag: &str, arg: &str, path: &std::path::Path) -> String {
    match tag {
        "export_linestyles" => match app
            .toolchest
            .export_line_styles()
            .and_then(|t| crate::chest::write_atomic(path, t.as_bytes()))
        {
            Ok(()) => format!("Exported the line styles to {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        },
        "import_linestyles" => {
            let text = match std::fs::metadata(path) {
                Ok(m) if m.len() > 4 << 20 => return "The file is too large to be a line style set".into(),
                _ => std::fs::read_to_string(path),
            };
            match text
                .map_err(|e| e.to_string())
                .and_then(|t| app.toolchest.import_line_styles(&t))
            {
                Ok(n) => format!("Imported {}", actions::plural(n, "line style")),
                Err(e) => format!("Import failed: {e}"),
            }
        }
        "choice_csv" => crate::more::choice_csv(app, arg, path),
        _ => String::new(),
    }
}

/// Tool Chest options (the panel's Options section).
pub fn options_ui(ui: &mut egui::Ui, app: &mut AppState) {
    egui::CollapsingHeader::new("Options")
        .id_salt("chest-options")
        .default_open(false)
        .show(ui, |ui| {
            let x = &mut app.toolchest.extras;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Recent tools kept");
                changed |= ui.add(egui::DragValue::new(&mut x.recent_max).range(1..=50)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("New recent tools");
                for (m, name) in [(Mode::Properties, "Properties"), (Mode::Drawing, "Drawing")] {
                    changed |= ui.selectable_value(&mut x.recent_mode, m, name).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Keep comments");
                egui::ComboBox::from_id_salt("chest-comments")
                    .selected_text(x.comments.name())
                    .show_ui(ui, |ui| {
                        for c in CommentMode::ALL {
                            changed |= ui.selectable_value(&mut x.comments, c, c.name()).changed();
                        }
                    });
            });
            changed |= ui
                .checkbox(&mut x.update_on_reuse, "Update Tool Set Item on Reuse")
                .on_hover_text("Changing the look of a markup drawn from a tool updates that tool")
                .changed();
            if changed {
                let max = x.recent_max;
                app.toolchest.recent.truncate(max.max(1));
                app.toolchest.save();
            }
            ui.horizontal(|ui| {
                if ui.button("Clear Recent Tools").clicked() {
                    app.toolchest.clear_recent();
                }
                if ui.button("Line Styles...").clicked() {
                    app.edit.chest_rt.styles_open = true;
                }
            });
        });
}

/// More rows of a tool item's right-click menu. Returns a status message.
pub fn item_menu(ui: &mut egui::Ui, app: &AppState, set: &str, item: &str, out: &mut Vec<ChestAct>) {
    if set == "recent" {
        return;
    }
    let Some(it) = app.toolchest.item(set, item) else {
        return;
    };
    let locked = app.toolchest.is_locked(set);
    ui.separator();
    if ui
        .add_enabled(!locked, egui::Button::new("Update from Selection"))
        .on_hover_text("The item takes the selected markup's look")
        .clicked()
    {
        out.push(ChestAct::UpdateFromSelection(set.into(), item.into()));
        ui.close();
    }
    let mut seq = it.sequence;
    if ui
        .add_enabled(!locked, egui::Checkbox::new(&mut seq, "Sequence (count up each time)"))
        .clicked()
    {
        out.push(ChestAct::Sequence(set.into(), item.into(), seq));
        ui.close();
    }
}

/// More rows of a tool set's right-click menu.
pub fn set_menu(ui: &mut egui::Ui, app: &AppState, set: &str, out: &mut Vec<ChestAct>) {
    let pinned = app.toolchest.extras.pinned.iter().any(|s| s == set);
    if ui
        .button(if pinned { "Unpin from Toolbar" } else { "Pin to Toolbar" })
        .clicked()
    {
        out.push(ChestAct::Pin(set.into(), !pinned));
        ui.close();
    }
    let locked = app.toolchest.is_locked(set);
    if ui
        .button(if locked {
            "Unlock Tool Set (Check Out)"
        } else {
            "Lock Tool Set"
        })
        .clicked()
    {
        out.push(ChestAct::Lock(set.into(), !locked));
        ui.close();
    }
    crate::chest_shared::set_menu(ui, app, set, out);
}

/// The flyout of a collapsed tool set: its items in a menu next to the header.
pub fn flyout(ui: &mut egui::Ui, app: &AppState, set: &crate::chest::ToolSet, out: &mut Vec<ChestAct>) {
    ui.menu_button(format!("{} tools", set.items.len()), |ui| {
        for it in &set.items {
            let Some(tool) = crate::tools::find(&it.tool) else {
                continue;
            };
            let active = app
                .active_item
                .as_ref()
                .is_some_and(|(s, i)| *s == set.id && *i == it.id);
            if ui
                .add(egui::Button::new(format!("{}  ({})", it.name, tool.label)).selected(active))
                .clicked()
            {
                out.push(ChestAct::Use(set.id.clone(), it.id.clone()));
                ui.close();
            }
        }
    });
}

/// Markups being dragged off the canvas: (document, markup ids).
#[derive(Debug, Clone, PartialEq)]
pub struct ChestDrag(pub u64, pub Vec<String>);

/// The canvas is dragging these markups: they can be dropped on the Tool Chest.
pub fn offer_drag(ctx: &egui::Context, doc: u64, ids: &[String]) {
    egui::DragAndDrop::set_payload(ctx, ChestDrag(doc, ids.to_vec()));
}

/// The Tool Chest panel: markups dragged from the page and released over it are added to My
/// Tools. Returns how many were added.
pub fn drop_target(ui: &egui::Ui, app: &mut AppState) -> usize {
    let released = ui.input(|i| i.pointer.primary_released());
    let over = ui
        .input(|i| i.pointer.latest_pos())
        .is_some_and(|p| ui.max_rect().contains(p));
    if !(released && over) {
        return 0;
    }
    let Some(p) = egui::DragAndDrop::take_payload::<ChestDrag>(ui.ctx()) else {
        return 0;
    };
    let ChestDrag(uid, ids) = (*p).clone();
    let markups: Vec<Markup> = app
        .docs
        .iter()
        .find(|d| d.uid == uid)
        .map(|d| ids.iter().filter_map(|i| d.session.doc().find(i).cloned()).collect())
        .unwrap_or_default();
    let n = markups
        .iter()
        .filter(|m| app.toolchest.add_markup(crate::chest::MY_TOOLS, m).is_some())
        .count();
    if n > 0 {
        app.status = format!("Added {} to My Tools", actions::plural(n, "tool"));
    }
    n
}

/// What the rows here ask for (applied after the panel draws).
#[derive(Debug, Clone, PartialEq)]
pub enum ChestAct {
    UpdateFromSelection(String, String),
    Sequence(String, String, bool),
    Pin(String, bool),
    Lock(String, bool),
    Use(String, String),
    CheckOut(String),
    CheckIn(String),
    RefreshShared(String),
}

pub fn apply(app: &mut AppState, acts: Vec<ChestAct>) {
    for a in acts {
        match a {
            ChestAct::UpdateFromSelection(set, item) => {
                let m = app
                    .doc()
                    .and_then(|d| d.selection().first().and_then(|i| d.session.doc().find(i)).cloned());
                app.status = match m {
                    Some(m) if app.toolchest.update_item_look(&set, &item, &m) => {
                        "The tool takes the selected markup's look".into()
                    }
                    Some(_) => "Select a markup the tool makes (and unlock its set)".into(),
                    None => "Select the markup whose look the tool takes".into(),
                };
            }
            ChestAct::Sequence(set, item, on) => {
                if !app.toolchest.is_locked(&set) {
                    app.toolchest.update_item(&set, &item, |i| i.sequence = on);
                }
            }
            ChestAct::Pin(set, on) => app.toolchest.set_pinned(&set, on),
            ChestAct::Lock(set, on) => app.toolchest.set_locked(&set, on),
            ChestAct::Use(set, item) => {
                app.edit.item_scale = app.toolchest.item_set_scale(&set).cloned();
                app.use_item(&set, &item);
            }
            act @ (ChestAct::CheckOut(_) | ChestAct::CheckIn(_) | ChestAct::RefreshShared(_)) => {
                crate::chest_shared::apply(app, &act);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_and_dashes() {
        assert_eq!(next_in_sequence("A1").as_deref(), Some("A2"));
        assert_eq!(next_in_sequence("D-09").as_deref(), Some("D-10"));
        assert_eq!(next_in_sequence("99").as_deref(), Some("100"));
        assert_eq!(next_in_sequence("Door"), None);
        assert_eq!(next_in_sequence(""), None);
        assert_eq!(parse_dash("8, 3, 2,3"), Some(vec![8.0, 3.0, 2.0, 3.0]));
        assert_eq!(parse_dash("0 0"), None);
        assert_eq!(parse_dash("x"), None);
        assert_eq!(parse_dash(""), Some(Vec::new()));
        let mut c = ToolChest::default();
        assert!(c.add_line_style("Fence", vec![10.0, 4.0]).is_ok());
        assert!(c.add_line_style("", vec![1.0]).is_err());
        let text = c.export_line_styles().unwrap();
        let mut d = ToolChest::default();
        assert_eq!(d.import_line_styles(&text), Ok(1));
        assert_eq!(d.extras.line_styles[0].dash, vec![10.0, 4.0]);
        assert!(d.import_line_styles("{}").is_err());
    }
}
