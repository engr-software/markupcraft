//! Measurement tools (Revu's Measure menu, Shift+Alt shortcuts). Each click places a point and
//! the value under construction follows the cursor. Length takes two points (or a drag);
//! Polylength, Area and Perimeter take points until Enter, a double-click or a right-click;
//! Count adds one item per click until Enter or Esc. Calibrate takes two points and asks for
//! their real distance; Polygon Cutout cuts a hole in the Area it is drawn inside (Ellipse
//! Cutout: dragged). Volume is an Area with a depth; Diameter takes the two ends, Center Radius
//! the centre then a point on the circle, 3-Point Radius three points on it, Angle an arm end,
//! the vertex and the other arm end. Area (Rectangle) and Add Viewport are dragged boxes.

use egui::Key;
use markupcraft_model::Kind;

use super::{DragShape, Role, ToolDef, ToolKind};
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

pub static AREA_RECT: ToolDef = ToolDef {
    id: "area_rect",
    label: "Area (Rectangle)",
    icon: "square-dashed",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Drag {
        kind: Kind::Area,
        shape: DragShape::Rect,
        role: Role::Markup,
    },
};

pub static VOLUME: ToolDef = ToolDef {
    id: "volume",
    label: "Volume",
    icon: "box",
    menu: "Measure",
    keys: shift_alt(Key::V),
    kind: ToolKind::Points {
        kind: Kind::Volume,
        closed: true,
        finish_at: 0,
        role: Role::Markup,
    },
};

pub static DIAMETER: ToolDef = ToolDef {
    id: "diameter",
    label: "Diameter",
    icon: "diameter",
    menu: "Measure",
    keys: shift_alt(Key::D),
    kind: ToolKind::Points {
        kind: Kind::Diameter,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static RADIUS: ToolDef = ToolDef {
    id: "radius",
    label: "Center Radius",
    icon: "radius",
    menu: "Measure",
    keys: shift_alt(Key::U),
    kind: ToolKind::Points {
        kind: Kind::Radius,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static RADIUS3: ToolDef = ToolDef {
    id: "radius3",
    label: "3-Point Radius",
    icon: "circle-dot-dashed",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Points {
        kind: Kind::Radius,
        closed: false,
        finish_at: 3,
        role: Role::Radius3,
    },
};

pub static ANGLE: ToolDef = ToolDef {
    id: "angle",
    label: "Angle",
    icon: "triangle-right",
    menu: "Measure",
    keys: shift_alt(Key::G),
    kind: ToolKind::Points {
        kind: Kind::Angle,
        closed: false,
        finish_at: 3,
        role: Role::Markup,
    },
};

pub static ELLIPSE_CUTOUT: ToolDef = ToolDef {
    id: "ellipse_cutout",
    label: "Ellipse Cutout",
    icon: "circle-dashed",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Drag {
        kind: Kind::Area,
        shape: DragShape::Ellipse,
        role: Role::Cutout,
    },
};

pub static VIEWPORT: ToolDef = ToolDef {
    id: "viewport",
    label: "Add Viewport",
    icon: "frame",
    menu: "Measure",
    keys: None,
    kind: ToolKind::Drag {
        kind: Kind::Polygon,
        shape: DragShape::Rect,
        role: Role::Viewport,
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
