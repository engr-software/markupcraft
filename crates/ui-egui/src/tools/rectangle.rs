//! Rectangle (R): drag a box; written as a `/Square` annotation.

use egui::Key;
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};

use super::{ToolDef, ToolKind, drag_rect};
use crate::commands::key;

pub static TOOL: ToolDef = ToolDef {
    id: "rectangle",
    label: "Rectangle",
    icon: "square",
    menu: "Markup",
    keys: key(Key::R),
    kind: ToolKind::Drag(make),
};

fn make(page: usize, a: Point, b: Point) -> Option<Markup> {
    let r = drag_rect(a, b)?;
    let mut m = Markup::new(Kind::Rectangle, page, r.corners().to_vec());
    m.rect = r;
    m.subtype = "Square".into();
    m.subject = "Rectangle".into();
    Some(m)
}
