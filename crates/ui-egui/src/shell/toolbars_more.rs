//! Toolbars in toolstrips: each toolbar (Main, Markup, Measure, the named toolbars of
//! [`BUILTIN`] (File, Edit, Navigation, Zoom, Shapes, Text...) and the user's own) docks at
//! the top, in a second row, or down the left or right side. Drag a toolbar by its grip to an
//! edge (or use the grip's menu) to move it. Customize Toolbars makes new toolbars and edits
//! any toolbar's commands.

use std::collections::BTreeMap;

use egui::{Color32, Sense, Stroke};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::theme::Tokens;

/// Where a toolbar sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Dock {
    #[default]
    Top,
    /// A second row under the first.
    Row2,
    Left,
    Right,
}

impl Dock {
    pub const ALL: [(Dock, &'static str); 4] = [
        (Dock::Top, "Dock at the Top"),
        (Dock::Row2, "Dock in a Second Row"),
        (Dock::Left, "Dock on the Left"),
        (Dock::Right, "Dock on the Right"),
    ];
}

/// The named toolbars MarkupCraft ships besides Main, Markup and Measure: (name, buttons).
/// Each is hidden until Window > Toolbars shows it.
pub const BUILTIN: &[(&str, &[&str])] = &[
    (
        "File",
        &["file.new", "file.open", "file.save", "file.save_as", "file.print"],
    ),
    (
        "Edit",
        &[
            "edit.undo",
            "edit.redo",
            "|",
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "edit.delete",
        ],
    ),
    (
        "Navigation",
        &[
            "view.first_page",
            "view.prev_page",
            "view.next_page",
            "view.last_page",
            "|",
            "view.prev_view",
            "view.next_view",
        ],
    ),
    (
        "Zoom",
        &[
            "view.zoom_in",
            "view.zoom_out",
            "view.fit_page",
            "view.fit_width",
            "view.actual_size",
        ],
    ),
    (
        "Shapes",
        &[
            "tool.line",
            "tool.arrow",
            "tool.polyline",
            "tool.polygon",
            "tool.rectangle",
            "tool.ellipse",
            "tool.cloud",
            "tool.cloudplus",
        ],
    ),
    ("Text", &["tool.text", "tool.callout", "tool.typewriter", "tool.note"]),
    (
        "Text Markup",
        &[
            "tool.texthighlight",
            "tool.underline",
            "tool.strikethrough",
            "tool.squiggly",
        ],
    ),
    ("Sketch", &["tool.pen", "tool.highlight", "tool.eraser", "tool.lasso"]),
    (
        "Order",
        &[
            "arrange.bring_to_front",
            "arrange.bring_forward",
            "arrange.send_backward",
            "arrange.send_to_back",
        ],
    ),
    (
        "Alignment",
        &[
            "arrange.align_left",
            "arrange.align_center",
            "arrange.align_right",
            "|",
            "arrange.align_top",
            "arrange.align_middle",
            "arrange.align_bottom",
            "|",
            "arrange.distribute_horizontal",
            "arrange.distribute_vertical",
        ],
    ),
    (
        "Rotation",
        &[
            "arrange.flip_horizontal",
            "arrange.flip_vertical",
            "|",
            "view.rotate_view_ccw",
            "view.rotate_view_cw",
        ],
    ),
    (
        "Document",
        &[
            "document.insert_blank",
            "document.insert_pages",
            "document.extract_pages",
            "document.delete_pages",
            "|",
            "document.rotate_ccw",
            "document.rotate_cw",
            "document.crop_pages",
        ],
    ),
];

/// The buttons of a named toolbar of [`BUILTIN`].
pub fn builtin(name: &str) -> Option<&'static [&'static str]> {
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, items)| *items)
}

/// A toolbar the user made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomToolbar {
    pub name: String,
    pub items: Vec<String>,
    pub show: bool,
}

/// Toolbar placement and the user's toolbars (in `ToolbarPrefs::more`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ToolbarsMore {
    /// Toolbar name -> where it is (missing = the top row).
    pub docks: BTreeMap<String, Dock>,
    pub custom: Vec<CustomToolbar>,
    /// Named toolbars of [`BUILTIN`] that show.
    pub builtin_shown: Vec<String>,
}

impl ToolbarsMore {
    pub fn dock(&self, name: &str) -> Dock {
        self.docks.get(name).copied().unwrap_or_default()
    }

    pub fn sanitize(&mut self) {
        self.custom.truncate(32);
        for c in &mut self.custom {
            c.items
                .retain(|id| id == "|" || crate::commands::describe(id).is_some());
            c.items.truncate(128);
        }
        self.docks.retain(|k, _| k.chars().count() <= 100);
        self.builtin_shown.retain(|n| builtin(n).is_some());
        self.builtin_shown.dedup();
    }
}

/// The buttons of toolbar `name`.
pub fn items(app: &AppState, name: &str) -> Vec<String> {
    let tb = &app.shell.ui.toolbars;
    match name {
        "Main" => tb.main.clone(),
        "Markup" | "Measure" => crate::tools::TOOLS
            .iter()
            .filter(|t| t.menu == name && t.draws())
            .map(|t| format!("tool.{}", t.id))
            .collect(),
        other if builtin(other).is_some() => builtin(other)
            .unwrap_or_default()
            .iter()
            .filter(|id| **id == "|" || crate::commands::describe(id).is_some())
            .map(|s| s.to_string())
            .collect(),
        other => tb
            .more
            .custom
            .iter()
            .find(|c| c.name == other)
            .map(|c| c.items.clone())
            .unwrap_or_default(),
    }
}

/// The toolbars shown, in order.
pub fn shown(app: &AppState) -> Vec<String> {
    let tb = &app.shell.ui.toolbars;
    let mut v = Vec::new();
    for (n, on) in [
        ("Main", tb.show_main),
        ("Markup", tb.show_markup),
        ("Measure", tb.show_measure),
    ] {
        if on {
            v.push(n.to_string());
        }
    }
    v.extend(
        BUILTIN
            .iter()
            .filter(|(n, _)| tb.more.builtin_shown.iter().any(|s| s == n))
            .map(|(n, _)| n.to_string()),
    );
    v.extend(tb.more.custom.iter().filter(|c| c.show).map(|c| c.name.clone()));
    v
}

/// Show or hide named toolbar `name` of [`BUILTIN`].
pub fn toggle_builtin(app: &mut AppState, name: &str) {
    if builtin(name).is_none() {
        return;
    }
    let shown = &mut app.shell.ui.toolbars.more.builtin_shown;
    if let Some(i) = shown.iter().position(|n| n == name) {
        shown.remove(i);
    } else {
        shown.push(name.to_string());
    }
    app.shell.save_ui();
}

/// Window > Toolbars: a checkable row per named toolbar of [`BUILTIN`].
pub fn builtin_menu(app: &mut AppState, ui: &mut egui::Ui) {
    for (name, _) in BUILTIN {
        let mut on = app.shell.ui.toolbars.more.builtin_shown.iter().any(|n| n == name);
        if ui.checkbox(&mut on, *name).clicked() {
            toggle_builtin(app, name);
        }
    }
}

/// Toolbars at `dock`.
pub fn at(app: &AppState, dock: Dock) -> Vec<String> {
    shown(app)
        .into_iter()
        .filter(|n| app.shell.ui.toolbars.more.dock(n) == dock)
        .collect()
}

/// Move toolbar `name` to `dock`.
pub fn set_dock(app: &mut AppState, name: &str, dock: Dock) {
    app.shell.ui.toolbars.more.docks.insert(name.to_string(), dock);
    app.shell.save_ui();
}

/// A toolbar's grip: drag it to an edge of the window, or use its menu, to dock the toolbar.
pub fn grip(app: &mut AppState, ui: &mut egui::Ui, name: &str, vertical: bool) {
    let t = Tokens::get(ui.ctx());
    let size = if vertical {
        egui::vec2(28.0, 8.0)
    } else {
        egui::vec2(8.0, 28.0)
    };
    let (rect, r) = ui.allocate_exact_size(size, Sense::click_and_drag());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{name} toolbar")));
    let c = if r.hovered() { t.text } else { t.text_faint };
    for k in 0..3 {
        let f = k as f32 * 3.0 - 3.0;
        let (a, b) = if vertical {
            (rect.center() + egui::vec2(-6.0, f), rect.center() + egui::vec2(6.0, f))
        } else {
            (rect.center() + egui::vec2(f, -6.0), rect.center() + egui::vec2(f, 6.0))
        };
        ui.painter().line_segment([a, b], Stroke::new(1.0, c));
    }
    if r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if r.drag_stopped()
        && let Some(p) = ui.input(|i| i.pointer.latest_pos())
    {
        let screen = ui.ctx().content_rect();
        let dock = if p.x < screen.left() + 80.0 {
            Some(Dock::Left)
        } else if p.x > screen.right() - 80.0 {
            Some(Dock::Right)
        } else if p.y < screen.top() + 60.0 {
            Some(Dock::Top)
        } else if p.y < screen.top() + 140.0 {
            Some(Dock::Row2)
        } else {
            None
        };
        if let Some(d) = dock {
            set_dock(app, name, d);
        }
    }
    r.context_menu(|ui| {
        for (d, label) in Dock::ALL {
            if ui.button(label).clicked() {
                set_dock(app, name, d);
                ui.close();
            }
        }
    });
}

/// The second row and the side strips.
pub fn strips(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.chrome_visible() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .stroke(Stroke::new(1.0, t.divider));
    let row2 = at(app, Dock::Row2);
    if !row2.is_empty() {
        egui::Panel::top("toolbar-row2")
            .exact_size(36.0)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for name in &row2 {
                        strip(app, ui, name, false);
                    }
                });
            });
    }
    for (dock, id) in [(Dock::Left, "toolbar-left"), (Dock::Right, "toolbar-right")] {
        let list = at(app, dock);
        if list.is_empty() {
            continue;
        }
        let panel = if dock == Dock::Left {
            egui::Panel::left(id)
        } else {
            egui::Panel::right(id)
        };
        panel.exact_size(40.0).resizable(false).frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt(id).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for name in &list {
                        strip(app, ui, name, true);
                    }
                });
            });
        });
    }
}

/// One toolbar: its grip and buttons.
pub fn strip(app: &mut AppState, ui: &mut egui::Ui, name: &str, vertical: bool) {
    grip(app, ui, name, vertical);
    for id in items(app, name) {
        if id == "|" {
            let r = ui.max_rect();
            let at = ui.cursor().min;
            if vertical {
                ui.painter()
                    .hline(r.x_range().shrink(6.0), at.y + 2.0, Stroke::new(1.0, Color32::GRAY));
                ui.add_space(5.0);
            } else {
                ui.painter()
                    .vline(at.x + 2.0, r.y_range().shrink(6.0), Stroke::new(1.0, Color32::GRAY));
                ui.add_space(5.0);
            }
            continue;
        }
        if crate::commands::describe(&id).is_some_and(|d| d.1.is_empty()) {
            // A command without an icon shows its name.
            let label = crate::commands::describe(&id).map(|d| d.0).unwrap_or_default();
            let enabled = app.enabled(&id);
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(egui::RichText::new(&label).size(11.0)).small(),
                )
                .clicked()
            {
                app.queue(&id);
            }
            continue;
        }
        crate::chrome::tool_button_pub(app, ui, &id);
    }
    ui.add_space(6.0);
}

/// Customize Toolbars: New Toolbar, its visibility, delete.
pub fn customize_rows(ui: &mut egui::Ui, more: &mut ToolbarsMore, target: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Toolbar");
        egui::ComboBox::from_id_salt("tb-target")
            .selected_text(target.clone())
            .show_ui(ui, |ui| {
                ui.selectable_value(target, "Main".to_string(), "Main");
                for c in &more.custom {
                    ui.selectable_value(target, c.name.clone(), &c.name);
                }
            });
        if let Some(c) = more.custom.iter_mut().find(|c| c.name == *target) {
            changed |= ui.checkbox(&mut c.show, "Show").changed();
            if ui.button("Delete Toolbar").clicked() {
                let n = c.name.clone();
                more.custom.retain(|c| c.name != n);
                more.docks.remove(&n);
                *target = "Main".into();
                changed = true;
            }
        }
    });
    ui.horizontal(|ui| {
        let id = egui::Id::new("tb-new-name");
        let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("New toolbar name")
                .desired_width(160.0),
        );
        let ok = !name.trim().is_empty()
            && !["Main", "Markup", "Measure"].contains(&name.trim())
            && builtin(name.trim()).is_none()
            && !more.custom.iter().any(|c| c.name == name.trim())
            && more.custom.len() < 32;
        if ui.add_enabled(ok, egui::Button::new("New Toolbar")).clicked() {
            more.custom.push(CustomToolbar {
                name: name.trim().to_string(),
                items: Vec::new(),
                show: true,
            });
            *target = name.trim().to_string();
            name.clear();
            changed = true;
        }
        ui.data_mut(|d| d.insert_temp(id, name));
    });
    changed
}
