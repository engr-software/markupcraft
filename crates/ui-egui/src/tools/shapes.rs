//! Lines and shapes (Revu's Shapes group). Line and Arrow take two clicks or a drag; Polyline,
//! Polygon, Cloud and Cloud+ take clicks until Enter, a double-click or a right-click;
//! Rectangle and Ellipse are dragged. Shift constrains to 0/45/90 degrees (squares for boxes).

use egui::Key;
use markupcraft_model::Kind;

use super::{Role, ToolDef, ToolKind};
use crate::commands::{key, shift};

pub static LINE: ToolDef = ToolDef {
    id: "line",
    label: "Line",
    icon: "minus",
    menu: "Markup",
    keys: key(Key::L),
    kind: ToolKind::Points {
        kind: Kind::Line,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static ARROW: ToolDef = ToolDef {
    id: "arrow",
    label: "Arrow",
    icon: "arrow-up-right",
    menu: "Markup",
    keys: key(Key::A),
    kind: ToolKind::Points {
        kind: Kind::Arrow,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static POLYLINE: ToolDef = ToolDef {
    id: "polyline",
    label: "Polyline",
    icon: "pen-line",
    menu: "Markup",
    keys: shift(Key::N),
    kind: ToolKind::Points {
        kind: Kind::Polyline,
        closed: false,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static POLYGON: ToolDef = ToolDef {
    id: "polygon",
    label: "Polygon",
    icon: "pentagon",
    menu: "Markup",
    keys: shift(Key::P),
    kind: ToolKind::Points {
        kind: Kind::Polygon,
        closed: true,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static RECTANGLE: ToolDef = ToolDef {
    id: "rectangle",
    label: "Rectangle",
    icon: "square",
    menu: "Markup",
    keys: key(Key::R),
    kind: ToolKind::Box(Kind::Rectangle),
};

pub static ELLIPSE: ToolDef = ToolDef {
    id: "ellipse",
    label: "Ellipse",
    icon: "circle",
    menu: "Markup",
    keys: key(Key::E),
    kind: ToolKind::Box(Kind::Ellipse),
};

pub static CLOUD: ToolDef = ToolDef {
    id: "cloud",
    label: "Cloud",
    icon: "cloud",
    menu: "Markup",
    keys: key(Key::C),
    kind: ToolKind::Points {
        kind: Kind::Cloud,
        closed: true,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static CLOUD_PLUS: ToolDef = ToolDef {
    id: "cloudplus",
    label: "Cloud+",
    icon: "message-square-plus",
    menu: "Markup",
    keys: key(Key::K),
    kind: ToolKind::Points {
        kind: Kind::Cloud,
        closed: true,
        finish_at: 0,
        role: Role::CloudPlus,
    },
};
