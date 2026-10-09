//! The tool registry: what the left mouse button does on the canvas. To add a drawing tool,
//! write `pub static TOOL: ToolDef` in `tools/<name>.rs` and add one line to [`TOOLS`].
//!
//! Select and Pan are built into the canvas; drawing tools describe the markup they make.
//! Only kinds the Revu writer knows are offered, so everything drawn saves faithfully.

pub mod ellipse;
pub mod line;
pub mod pan;
pub mod rectangle;
pub mod select;

use markupcraft_geom::Point;
use markupcraft_model::Markup;

use crate::commands::Keys;

/// Builds the markup for a drag from `a` to `b` on `page` (user space); `None` if too small.
pub type DragCreate = fn(page: usize, a: Point, b: Point) -> Option<Markup>;

#[derive(Clone, Copy)]
pub enum ToolKind {
    /// Click, box-select, move, handles.
    Select,
    /// Drag to scroll the view.
    Pan,
    /// Press, drag, release to create a markup.
    Drag(DragCreate),
}

pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Lucide icon name.
    pub icon: &'static str,
    /// Menu it is listed in ("Tools", "Markup", "Measure").
    pub menu: &'static str,
    pub keys: Option<Keys>,
    pub kind: ToolKind,
}

/// Every tool, in menu and toolbar order. One line per tool.
pub static TOOLS: &[&ToolDef] = &[&select::TOOL, &pan::TOOL, &rectangle::TOOL, &ellipse::TOOL, &line::TOOL];

pub fn find(id: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().copied().find(|t| t.id == id)
}

/// The rectangle spanned by two drag points, `None` if it is smaller than 2 points.
pub(crate) fn drag_rect(a: Point, b: Point) -> Option<markupcraft_geom::Rect> {
    let r = markupcraft_geom::Rect::new(a.x, a.y, b.x, b.y).normalized();
    (r.width() >= 2.0 && r.height() >= 2.0).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for t in TOOLS {
            assert!(ids.insert(t.id), "duplicate tool {}", t.id);
            assert!(crate::icons::exists(t.icon), "icon {}", t.icon);
            assert!(crate::commands::MENUS.contains(&t.menu));
        }
    }

    #[test]
    fn drag_tools_make_writable_markups() {
        for t in TOOLS {
            if let ToolKind::Drag(make) = t.kind {
                assert!(
                    make(0, Point::new(0.0, 0.0), Point::new(1.0, 1.0)).is_none(),
                    "{} too small",
                    t.id
                );
                let m = make(2, Point::new(10.0, 10.0), Point::new(80.0, 50.0)).unwrap();
                assert_eq!(m.page, 2);
                assert!(crate::actions::editable(&m), "{} must be writable", t.id);
            }
        }
    }
}
