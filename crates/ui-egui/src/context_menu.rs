//! The right-click menu on the canvas (Revu's markup context menu): edit text, clipboard,
//! delete, properties, status and checkmark, lock, group, arrange, vertices and cutouts,
//! segment values and caption, Set as Default and Add to Tool Chest. On empty paper: paste and
//! selection commands.

use markupcraft_engine::MarkupPatch;
use markupcraft_model::{Kind, measure_extras, review_statuses};

use crate::DocTab;
use crate::actions::{self, editable};
use crate::canvas::{CanvasAction, CanvasOut};

fn command(ui: &mut egui::Ui, out: &mut CanvasOut, label: &str, id: &str, enabled: bool) {
    let keys = crate::commands::find(id).and_then(|c| c.keys).map(|k| k.label());
    let mut b = egui::Button::new(label);
    if let Some(k) = keys {
        b = b.shortcut_text(k);
    }
    if ui.add_enabled(enabled, b).clicked() {
        out.commands.push(id.to_string());
        ui.close();
    }
}

/// Paste where the menu was opened.
fn paste_here(ui: &mut egui::Ui, doc: &mut DocTab, out: &mut CanvasOut, target: &crate::interact::ContextTarget) {
    let keys = crate::commands::find("edit.paste")
        .and_then(|c| c.keys)
        .map(|k| k.label());
    let mut b = egui::Button::new("Paste");
    if let Some(k) = keys {
        b = b.shortcut_text(k);
    }
    if ui.add_enabled(!doc.session.clipboard().is_empty(), b).clicked() {
        let r = doc.session.paste(Some(target.page), Some(target.at));
        out.status = Some(actions::report(r, |v| {
            format!("Pasted {}", actions::plural(v.len(), "markup"))
        }));
        ui.close();
    }
}

/// The menu for the target recorded in `doc.view.context`.
pub fn show(ui: &mut egui::Ui, doc: &mut DocTab, out: &mut CanvasOut) {
    ui.set_min_width(210.0);
    let Some(target) = doc.view.context.clone() else {
        ui.close();
        return;
    };
    let ids = doc.session.selection().to_vec();
    let first = ids.first().and_then(|id| doc.session.doc().find(id)).cloned();
    let Some(m) = first.filter(|_| target.markup.is_some()) else {
        paste_here(ui, doc, out, &target);
        command(ui, out, "Select All", "edit.select_all", true);
        if !ids.is_empty() {
            command(ui, out, "Deselect All", "edit.deselect", true);
        }
        return;
    };
    let writable = editable(&m);
    let single = ids.len() == 1;
    let set = |doc: &mut DocTab, out: &mut CanvasOut, patch: MarkupPatch, what: &str| {
        let r = doc.session.set_properties(&ids, &patch);
        out.status = Some(actions::report(r, |n| {
            format!("{what}: {}", actions::plural(n, "markup"))
        }));
    };

    if single && (m.kind.is_text() || m.kind == Kind::Note) && writable && !m.locked() {
        if ui.button("Edit Text").clicked() {
            crate::interact::edit_existing(doc, &m.id);
            ui.close();
        }
        ui.separator();
    }
    command(ui, out, "Cut", "edit.cut", writable);
    command(ui, out, "Copy", "edit.copy", writable);
    paste_here(ui, doc, out, &target);
    command(ui, out, "Duplicate", "edit.duplicate", writable);
    command(ui, out, "Delete", "edit.delete", true);
    ui.separator();
    if ui.button("Properties").clicked() {
        out.actions.push(CanvasAction::ShowPanel("properties"));
        ui.close();
    }
    ui.menu_button("Status", |ui| {
        for st in review_statuses() {
            let on = if *st == "None" {
                m.status.is_empty()
            } else {
                m.status == *st
            };
            if ui.add(egui::Button::new(*st).selected(on)).clicked() {
                let patch = MarkupPatch {
                    status: Some((*st).to_string()),
                    ..Default::default()
                };
                set(doc, out, patch, &format!("Status {st}"));
                ui.close();
            }
        }
    });
    let mut checked = m.checked;
    if ui.checkbox(&mut checked, "Checkmark").clicked() {
        let patch = MarkupPatch {
            checked: Some(checked),
            ..Default::default()
        };
        set(doc, out, patch, if checked { "Checked" } else { "Unchecked" });
        ui.close();
    }
    if m.locked() {
        command(ui, out, "Unlock", "markup.unlock", true);
    } else {
        command(ui, out, "Lock", "markup.lock", true);
    }
    ui.separator();
    if ids.len() > 1 {
        command(ui, out, "Group", "markup.group", true);
    }
    if !m.group.is_empty() {
        command(ui, out, "Ungroup", "markup.ungroup", true);
        command(ui, out, "Remove From Group", "markup.remove_from_group", true);
    }
    ui.menu_button("Arrange", |ui| {
        command(ui, out, "Bring to Front", "arrange.bring_to_front", true);
        command(ui, out, "Bring Forward", "arrange.bring_forward", true);
        command(ui, out, "Send Backward", "arrange.send_backward", true);
        command(ui, out, "Send to Back", "arrange.send_to_back", true);
        ui.separator();
        command(ui, out, "Flip Horizontal", "arrange.flip_horizontal", writable);
        command(ui, out, "Flip Vertical", "arrange.flip_vertical", writable);
    });
    if ids.len() > 1 {
        ui.menu_button("Align", |ui| {
            for (label, id) in [
                ("Align Left", "arrange.align_left"),
                ("Align Center", "arrange.align_center"),
                ("Align Right", "arrange.align_right"),
                ("Align Top", "arrange.align_top"),
                ("Align Middle", "arrange.align_middle"),
                ("Align Bottom", "arrange.align_bottom"),
            ] {
                command(ui, out, label, id, true);
            }
            ui.separator();
            command(
                ui,
                out,
                "Distribute Horizontally",
                "arrange.distribute_horizontal",
                ids.len() > 2,
            );
            command(
                ui,
                out,
                "Distribute Vertically",
                "arrange.distribute_vertical",
                ids.len() > 2,
            );
        });
    }
    command(ui, out, "Apply to All Pages", "markup.apply_to_all_pages", writable);
    command(ui, out, "Format Painter", "markup.format_painter", single && writable);

    // Measurement and vertex items.
    if single && writable && !m.locked() {
        let mut extra = false;
        if matches!(m.kind, Kind::Polylength | Kind::Perimeter | Kind::Area | Kind::Volume) {
            let mut on = m.segment_values;
            if ui.checkbox(&mut on, "Show Segment Values").clicked() {
                let patch = MarkupPatch {
                    segment_values: Some(on),
                    ..Default::default()
                };
                set(doc, out, patch, "Segment values");
                ui.close();
            }
            extra = true;
        }
        if m.caption_offset.is_some() && ui.button("Reset Caption Position").clicked() {
            let patch = MarkupPatch {
                caption_offset: Some(None),
                ..Default::default()
            };
            set(doc, out, patch, "Caption reset");
            ui.close();
        }
        if let Some((after, p)) = target.segment
            && ui.button("Add Vertex").clicked()
        {
            let r = doc.session.insert_vertex(&m.id, after + 1, p);
            out.status = Some(actions::report(r, |_| "Vertex added".into()));
            ui.close();
        }
        if let Some(v) = target.vertex
            && measure_extras::can_add_vertices(m.kind)
            && ui.button("Delete Vertex").clicked()
        {
            let r = doc.session.delete_vertex(&m.id, v);
            out.status = Some(actions::report(r, |_| "Vertex deleted".into()));
            ui.close();
        }
        if let Some(h) = target.hole
            && ui.button("Delete Cutout").clicked()
        {
            let r = doc.session.remove_cutout(&m.id, h);
            out.status = Some(actions::report(r, |_| "Cutout deleted".into()));
            ui.close();
        }
        if extra || target.segment.is_some() || target.vertex.is_some() {
            ui.separator();
        }
    }
    ui.separator();
    if ui
        .add_enabled(single && writable, egui::Button::new("Set as Default"))
        .clicked()
    {
        out.actions.push(CanvasAction::SetDefault(m.clone()));
        ui.close();
    }
    if ui
        .add_enabled(single && writable, egui::Button::new("Add to Tool Chest"))
        .clicked()
    {
        out.actions.push(CanvasAction::AddToToolChest(m.clone()));
        ui.close();
    }
}
