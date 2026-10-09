//! Canvas picks of the shell's dialogs, as tools (listed under Document): Deskew's Get Line (two
//! points that should be level) and Page Labels from Region (a box over the title block).
//! Their results come back through `extra::canvas_out`.

use markupcraft_model::Kind;

use crate::tools::{DragShape, Role, ToolDef, ToolKind};

pub static TOOL: ToolDef = ToolDef {
    id: "deskew_line",
    label: "Deskew Line",
    icon: "ruler",
    menu: "Document",
    keys: None,
    kind: ToolKind::Points {
        kind: Kind::Line,
        closed: false,
        finish_at: 2,
        role: Role::Calibrate,
    },
};

pub static REGION_TOOL: ToolDef = ToolDef {
    id: "label_region",
    label: "Page Label Region",
    icon: "frame",
    menu: "Document",
    keys: None,
    kind: ToolKind::Drag {
        kind: Kind::Polygon,
        shape: DragShape::Rect,
        role: Role::Viewport,
    },
};
