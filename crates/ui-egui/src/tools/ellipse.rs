//! Ellipse (E): drag a box; written as a `/Circle` annotation.

use egui::Key;
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};

use super::{ToolDef, ToolKind, drag_rect};
use crate::commands::key;

pub static TOOL: ToolDef = ToolDef {
    id: "ellipse",
    label: "Ellipse",
    icon: "circle",
    menu: "Markup",
    keys: key(Key::E),
    kind: ToolKind::Drag(make),
};

fn make(page: usize, a: Point, b: Point) -> Option<Markup> {
    let r = drag_rect(a, b)?;
    let mut m = Markup::new(Kind::Ellipse, page, r.corners().to_vec());
    m.rect = r;
    m.subtype = "Circle".into();
    m.subject = "Ellipse".into();
    Some(m)
}
