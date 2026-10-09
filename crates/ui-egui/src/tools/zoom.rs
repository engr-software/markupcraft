//! Zoom (Z): click to zoom in, Ctrl+click or right-click to zoom out, drag a box to fill the
//! view with it. Shift+Z toggles it on and off. The behaviour lives in `canvas.rs` (it is a
//! navigation tool, like Pan).

use egui::Key;

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static TOOL: ToolDef = ToolDef {
    id: "zoom",
    label: "Zoom",
    icon: "zoom-in",
    menu: "Tools",
    keys: key(Key::Z),
    kind: ToolKind::Pan,
};
