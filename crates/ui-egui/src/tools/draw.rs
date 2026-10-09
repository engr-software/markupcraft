//! Freehand and placed markups: Pen and Highlight (freehand strokes), Stamp (click to place the
//! stamp chosen in the Tool Chest, or drag its box) and Snapshot (drag a region; it is copied
//! and Ctrl+V pastes it as a snapshot markup).

use egui::Key;
use markupcraft_model::Kind;

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static PEN: ToolDef = ToolDef {
    id: "pen",
    label: "Pen",
    icon: "pen-tool",
    menu: "Markup",
    keys: key(Key::P),
    kind: ToolKind::Freehand(Kind::Ink),
};

pub static HIGHLIGHT: ToolDef = ToolDef {
    id: "highlight",
    label: "Highlight",
    icon: "highlighter",
    menu: "Markup",
    keys: key(Key::H),
    kind: ToolKind::Freehand(Kind::Highlight),
};

pub static STAMP: ToolDef = ToolDef {
    id: "stamp",
    label: "Stamp",
    icon: "stamp",
    menu: "Markup",
    keys: key(Key::S),
    kind: ToolKind::Stamp,
};

pub static SNAPSHOT: ToolDef = ToolDef {
    id: "snapshot",
    label: "Snapshot",
    icon: "camera",
    menu: "Markup",
    keys: key(Key::G),
    kind: ToolKind::Box(Kind::Snapshot),
};
