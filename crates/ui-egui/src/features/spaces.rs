//! Spaces: named areas of a page (rooms, zones). The Spaces panel lists them with what each
//! contains; new spaces are drawn as an outline or filled from the page linework; they are
//! highlighted on the pages.

use std::path::Path;

use egui::Color32;
use markupcraft_engine::spaces::SpaceMatch;
use markupcraft_geom::Point;

use super::{Ask, Mark, Pick};
use crate::{AppState, DocTab, actions};

pub struct SpacesState {
    pub highlight: bool,
    pub all_pages: bool,
    pub selected: Option<String>,
    pub new_name: String,
    pub rename: String,
    pub message: String,
}

impl Default for SpacesState {
    fn default() -> Self {
        Self {
            highlight: true,
            all_pages: false,
            selected: None,
            new_name: String::new(),
            rename: String::new(),
            message: String::new(),
        }
    }
}

impl SpacesState {
    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        for (page, s) in d.session.spaces(None) {
            let sel = self.selected.as_deref() == Some(s.id.as_str());
            if !self.highlight && !sel {
                continue;
            }
            let c = crate::theme::color32(&s.color, 1.0);
            let a = if sel { 0.45 } else { s.opacity.clamp(0.05, 0.6) };
            let fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8);
            out.push(Mark {
                page,
                pts: s.pts.clone(),
                fill,
                stroke: if sel { super::canvas::CURRENT_STROKE } else { c },
            });
        }
    }
}

/// Draw a new space's outline.
pub fn draw(app: &mut AppState) {
    super::start_pick(
        app,
        Pick::Space,
        "Click the space's corners; double-click or Enter to close it",
    );
}

/// Make a space from the region around a click.
pub fn fill(app: &mut AppState) {
    app.features.fill.space_name = app.features.spaces.new_name.clone();
    super::start_pick(app, Pick::SpaceFill, "Click inside the room to make it a space");
}

pub fn outline_picked(app: &mut AppState, page: usize, pts: Vec<Point>) {
    let n = app.doc().map_or(0, |d| d.session.spaces(Some(page)).len()) + 1;
    let name = match app.features.spaces.new_name.trim() {
        "" => format!("Space {n}"),
        s => s.to_string(),
    };
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.add_space(page, &name, pts, None, None);
    app.status = actions::report(r, |id| {
        app.features.spaces.selected = Some(id);
        format!("Added space {name}")
    });
    app.show_panel("spaces");
}

pub fn file(app: &mut AppState, ask: &Ask, path: &Path) {
    let Some(d) = app.doc_mut() else { return };
    let msg = match ask {
        Ask::SpacesExport => actions::report(d.session.export_spaces(path), |n| {
            format!("Exported {} to {}", actions::plural(n, "space"), path.display())
        }),
        Ask::SpacesImport => actions::report(d.session.import_spaces(path, SpaceMatch::Label), |n| {
            format!("Imported {}", actions::plural(n, "space"))
        }),
        _ => return,
    };
    app.status = msg.clone();
    app.features.spaces.message = msg;
}

pub fn window(_app: &mut AppState, _ctx: &egui::Context) {}
