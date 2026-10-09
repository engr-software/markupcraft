//! Measurement tools (Revu's Measure menu, Shift+Alt shortcuts). Each click places a point and
//! the value under construction follows the cursor. Length takes two points (or a drag);
//! Polylength, Area and Perimeter take points until Enter, a double-click or a right-click;
//! Count adds one item per click until Enter or Esc. Calibrate takes two points and asks for
//! their real distance; Polygon Cutout cuts a hole in the Area it is drawn inside.

use egui::Key;
use markupcraft_model::Kind;

use super::{Role, ToolDef, ToolKind};
use crate::commands::shift_alt;

pub static CALIBRATE: ToolDef = ToolDef {
    id: "calibrate",
    label: "Calibrate",
    icon: "compass",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Points {
        kind: Kind::Length,
        closed: false,
        finish_at: 2,
        role: Role::Calibrate,
    },
};

pub static LENGTH: ToolDef = ToolDef {
    id: "length",
    label: "Length",
    icon: "ruler",
    menu: "Measure",
    keys: shift_alt(Key::L),
    kind: ToolKind::Points {
        kind: Kind::Length,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static POLYLENGTH: ToolDef = ToolDef {
    id: "polylength",
    label: "Polylength",
    icon: "spline",
    menu: "Measure",
    keys: shift_alt(Key::Q),
    kind: ToolKind::Points {
        kind: Kind::Polylength,
        closed: false,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static AREA: ToolDef = ToolDef {
    id: "area",
    label: "Area",
    icon: "square-dashed",
    menu: "Measure",
    keys: shift_alt(Key::A),
    kind: ToolKind::Points {
        kind: Kind::Area,
        closed: true,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static PERIMETER: ToolDef = ToolDef {
    id: "perimeter",
    label: "Perimeter",
    icon: "circle-dashed",
    menu: "Measure",
    keys: shift_alt(Key::P),
    kind: ToolKind::Points {
        kind: Kind::Perimeter,
        closed: true,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static COUNT: ToolDef = ToolDef {
    id: "count",
    label: "Count",
    icon: "hash",
    menu: "Measure",
    keys: shift_alt(Key::C),
    kind: ToolKind::Points {
        kind: Kind::Count,
        closed: false,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static CUTOUT: ToolDef = ToolDef {
    id: "cutout",
    label: "Polygon Cutout",
    icon: "squares-subtract",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Points {
        kind: Kind::Area,
        closed: true,
        finish_at: 0,
        role: Role::Cutout,
    },
};
