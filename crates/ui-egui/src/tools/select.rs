//! Select (V): click to select, Shift/Ctrl+click to add, drag a box to select several, drag a
//! selection to move it, drag a handle to reshape. The behaviour lives in `canvas.rs`.

use egui::Key;

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static TOOL: ToolDef = ToolDef {
    id: "select",
    label: "Select",
    icon: "mouse-pointer-2",
    menu: "Tools",
    keys: key(Key::V),
    kind: ToolKind::Select,
};
