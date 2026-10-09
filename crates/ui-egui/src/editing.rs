//! Markup editing commands that need the interface's state: nudging with the arrow keys, the
//! Format Painter, Save All, and the selection rules of the arrange commands (align,
//! distribute, flip, apply to all pages, remove from group run as engine commands).
//!
//! [`EditState`] is the editing state the canvas reads each frame (the Format Painter's
//! source, Highlight Viewports, the Sketch to Scale entry).

use markupcraft_model::Markup;

use crate::canvas::CanvasOut;
use crate::{AppState, actions};

/// Points one arrow-key nudge moves (Shift: ten times as far).
pub const NUDGE: f64 = 1.0;

/// Editing state shared by the canvas, the panels and the commands.
#[derive(Debug, Clone, Default)]
pub struct EditState {
    /// Format Painter: the markup whose look a click copies onto other markups.
    pub painter: Option<Markup>,
    /// Draw every viewport's outline and name over the pages.
    pub highlight_viewports: bool,
    /// Sketch to Scale: the typed length (and angle) of the next segment.
    pub sketch: crate::sketch::SketchState,
    /// The Add Viewport dialog.
    pub viewport: Option<crate::viewports::NewViewport>,
    /// The Measurements panel's viewport list.
    pub viewports_panel: crate::viewports::PanelState,
    /// The scale of the tool set the active Tool Chest item comes from.
    pub item_scale: Option<markupcraft_model::Scale>,
    /// Measure tool, Resume Count, Review Text (`more`).
    pub more: crate::more::MoreState,
    /// Tool Chest runtime state (item origins, the Line Styles window).
    pub chest_rt: crate::chest_more::ChestRuntime,
    /// Tool Chest > Options > Keep comments.
    pub chest_comments: crate::chest_more::CommentMode,
}

/// A file dialog answered for an editing feature.
pub fn dialog_answer(app: &mut AppState, tag: &str, arg: &str, path: &std::path::Path) -> String {
    match tag {
        "export_toolset" => match app
            .toolchest
            .export_set(arg)
            .and_then(|t| crate::chest::write_atomic(path, t.as_bytes()))
        {
            Ok(()) => format!("Exported the tool set to {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        },
        "import_toolset" => {
            let text = match std::fs::metadata(path) {
                Ok(m) if m.len() > crate::chest_sets::MAX_SET_FILE => {
                    return "The file is too large to be a tool set".into();
                }
                _ => std::fs::read_to_string(path),
            };
            match text
                .map_err(|e| e.to_string())
                .and_then(|t| app.toolchest.import_set(&t))
            {
                Ok(id) => {
                    app.show_panel("toolchest");
                    let title = app.toolchest.find_set(&id).map(|s| s.title.clone()).unwrap_or_default();
                    format!("Imported the tool set {title}")
                }
                Err(e) => format!("Import failed: {e}"),
            }
        }
        "export_keys" | "import_keys" => crate::keyprefs::dialog_answer(app, tag, path),
        t if crate::more::handles_dialog(t) => crate::more::dialog_answer(app, t, arg, path),
        t if crate::chest_more::handles_dialog(t) => crate::chest_more::dialog_answer(app, t, arg, path),
        _ => String::new(),
    }
}

/// Commands handled here.
pub fn handles(id: &str) -> bool {
    crate::more::handles(id)
        || id.starts_with("edit.nudge_")
        || matches!(
            id,
            "markup.format_painter" | "file.save_all" | "view.highlight_viewports" | "tools.customize_keys"
        )
}

/// Commands that act on the selection (disabled without one).
pub fn needs_selection(id: &str) -> bool {
    id.starts_with("edit.nudge_")
        || matches!(
            id,
            "markup.format_painter" | "markup.apply_to_all_pages" | "markup.remove_from_group"
        )
}

/// The nudge a command id asks for, in PDF points (y up).
pub fn nudge_of(id: &str) -> Option<(f64, f64)> {
    let rest = id.strip_prefix("edit.nudge_")?;
    let (dir, far) = match rest.strip_suffix("_far") {
        Some(d) => (d, true),
        None => (rest, false),
    };
    let step = if far { NUDGE * 10.0 } else { NUDGE };
    Some(match dir {
        "left" => (-step, 0.0),
        "right" => (step, 0.0),
        "up" => (0.0, step),
        "down" => (0.0, -step),
        _ => return None,
    })
}

pub fn run(app: &mut AppState, id: &str) {
    if crate::more::handles(id) {
        crate::more::run(app, id);
        return;
    }
    if let Some((dx, dy)) = nudge_of(id) {
        if let Some(d) = app.doc_mut() {
            if d.view.draft.is_some() || d.view.editor.is_some() {
                return;
            }
            let ids: Vec<String> = d
                .selection()
                .iter()
                .filter(|i| d.session.doc().find(i).is_some_and(|m| !m.locked()))
                .cloned()
                .collect();
            if ids.is_empty() {
                return;
            }
            d.session.set_merge_key(Some("nudge"));
            let r = d.session.move_markups(&ids, dx, dy);
            d.session.set_merge_key(None);
            app.status = actions::report(r, |n| format!("Nudged {}", actions::plural(n, "markup")));
        }
        return;
    }
    match id {
        "markup.format_painter" => {
            let m = app
                .doc()
                .and_then(|d| d.selection().first().and_then(|i| d.session.doc().find(i)).cloned());
            match m {
                Some(m) => {
                    app.status = format!(
                        "Format Painter: click markups to give them this {}'s look (Esc ends)",
                        m.kind.name()
                    );
                    app.edit.painter = Some(m);
                    app.set_tool("select");
                }
                None => app.status = "Format Painter: select the markup to copy from first".into(),
            }
        }
        "file.save_all" => {
            let dirty: Vec<u64> = app
                .docs
                .iter()
                .filter(|d| d.session.is_dirty())
                .map(|d| d.uid)
                .collect();
            for uid in &dirty {
                app.save_doc(*uid, false, false);
            }
            if dirty.is_empty() {
                app.status = "Nothing to save".into();
            }
        }
        "view.highlight_viewports" => app.edit.highlight_viewports = !app.edit.highlight_viewports,
        "tools.customize_keys" => app.keys.show = true,
        _ => {}
    }
}

/// Copy the Format Painter's look onto markup `id`.
pub fn paint_format(doc: &mut crate::DocTab, template: &Markup, id: &str, out: &mut CanvasOut) {
    let Some(target) = doc.session.doc().find(id).cloned() else {
        return;
    };
    if target.id == template.id {
        return;
    }
    let mut patch = actions::patch_from(template);
    // The look only: what the markup is (subject, label, layer) stays.
    patch.subject = None;
    patch.label = None;
    patch.layer = None;
    if !(target.kind.is_text() || target.kind.is_measurement() || target.kind == markupcraft_model::Kind::Stamp) {
        patch.font = None;
        patch.font_size = None;
        patch.text_color = None;
        patch.bold = None;
        patch.italic = None;
        patch.underline = None;
        patch.align = None;
    }
    if !markupcraft_revu::kinds::kind_for(target.kind).closed && !target.kind.is_text() {
        patch.fill = None;
        patch.fill_opacity = None;
    }
    let r = doc.session.set_properties(&[id.to_string()], &patch);
    if r.is_ok() && template.hatch != target.hatch && markupcraft_revu::kinds::kind_for(target.kind).closed {
        let _ = doc.session.set_hatch(&[id.to_string()], template.hatch);
    }
    out.status = Some(actions::report(r, |_| {
        format!("Format painted onto the {}", target.kind.name())
    }));
}

/// Esc: the Format Painter ends with the rest of the tool.
pub fn escape(app: &mut AppState) {
    app.edit.painter = None;
    crate::more::escape(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudges_are_one_point_or_ten() {
        assert_eq!(nudge_of("edit.nudge_left"), Some((-1.0, 0.0)));
        assert_eq!(nudge_of("edit.nudge_up_far"), Some((0.0, 10.0)));
        assert_eq!(nudge_of("edit.nudge_sideways"), None);
        assert!(handles("file.save_all") && needs_selection("edit.nudge_down"));
    }
}
