//! Text tools: Text Box (drag a box or click, then type), Callout (click the arrow tip, then
//! where the box goes, then type), Typewriter (click, then type; no border, the box fits the
//! text) and Note (click to drop the icon, then type its comment). Typing happens in an editor
//! over the page; clicking elsewhere or Esc ends it.

use egui::Key;
use markupcraft_model::Kind;

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static TEXT_BOX: ToolDef = ToolDef {
    id: "text",
    label: "Text Box",
    icon: "type",
    menu: "Markup",
    keys: key(Key::T),
    kind: ToolKind::Text(Kind::Text),
};

pub static CALLOUT: ToolDef = ToolDef {
    id: "callout",
    label: "Callout",
    icon: "message-square-quote",
    menu: "Markup",
    keys: key(Key::Q),
    kind: ToolKind::Text(Kind::Callout),
};

pub static TYPEWRITER: ToolDef = ToolDef {
    id: "typewriter",
    label: "Typewriter",
    icon: "text-cursor-input",
    menu: "Markup",
    keys: key(Key::W),
    kind: ToolKind::Text(Kind::Typewriter),
};

pub static NOTE: ToolDef = ToolDef {
    id: "note",
    label: "Note",
    icon: "sticky-note",
    menu: "Markup",
    keys: key(Key::N),
    kind: ToolKind::Note,
};
