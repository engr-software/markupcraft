//! Pan (Shift+V): the left button drags the view. Space-drag and middle-drag pan with any tool.

use egui::Key;

use super::{ToolDef, ToolKind};
use crate::commands::shift;

pub static TOOL: ToolDef = ToolDef {
    id: "pan",
    label: "Pan",
    icon: "hand",
    menu: "Tools",
    keys: shift(Key::V),
    kind: ToolKind::Pan,
};
