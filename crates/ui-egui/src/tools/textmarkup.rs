//! Text markups over the page's own text: drag across the words (or click one) to highlight,
//! underline or strike them through. The marks follow the text lines the page's text layer
//! reports; on a page without text the dragged rectangle is marked instead.

use markupcraft_model::Kind;

use egui::Key;

use super::{ToolDef, ToolKind};
use crate::commands::key;

pub static HIGHLIGHT_TEXT: ToolDef = ToolDef {
    id: "texthighlight",
    label: "Highlight Text",
    icon: "text-select",
    menu: "Markup",
    keys: None,
    kind: ToolKind::TextMarkup(Kind::TextHighlight),
};

pub static UNDERLINE: ToolDef = ToolDef {
    id: "underline",
    label: "Underline",
    icon: "underline",
    menu: "Markup",
    keys: key(Key::U),
    kind: ToolKind::TextMarkup(Kind::Underline),
};

pub static STRIKETHROUGH: ToolDef = ToolDef {
    id: "strikethrough",
    label: "Strikethrough",
    icon: "strikethrough",
    menu: "Markup",
    keys: key(Key::D),
    kind: ToolKind::TextMarkup(Kind::Strikeout),
};
