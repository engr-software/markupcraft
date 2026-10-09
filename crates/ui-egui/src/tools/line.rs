//! Line (L): drag from one end to the other; written as a `/Line` annotation.

use egui::Key;
use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Kind, Markup};

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static TOOL: ToolDef = ToolDef {
    id: "line",
    label: "Line",
    icon: "pen-line",
    menu: "Markup",
    keys: key(Key::L),
    kind: ToolKind::Drag(make),
};

fn make(page: usize, a: Point, b: Point) -> Option<Markup> {
    if a.dist(b) < 2.0 {
        return None;
    }
    let mut m = Markup::new(Kind::Line, page, vec![a, b]);
    m.rect = Rect::new(a.x, a.y, b.x, b.y).normalized();
    m.subtype = "Line".into();
    m.subject = "Line".into();
    Some(m)
}
