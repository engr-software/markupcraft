//! More of Revu's markup and selection tools:
//!
//! - Squiggly (Shift+U): a wavy underline over page text, dragged like Underline.
//! - Insert / Replace Text: a click places an insertion caret and asks for the text to insert;
//!   a drag across words strikes them through and adds the caret after them (a Replace, the two
//!   grouped).
//! - Arc (Shift+C): click the start, the end, then a point the arc passes through.
//! - Dimension (Shift+L): two clicks or a drag; arrowheads at both ends, the scaled length (when
//!   the page has a scale) as its text, an offset and extension lines in Properties.
//! - Eraser (Shift+E): drag across Pen and Highlight strokes to rub them out.
//! - Flag (Shift+F): a click places a flag marker.
//! - Lasso (Shift+O): drag a loop; the markups wholly inside it are selected (Shift adds).
//! - Select Text (Shift+T): drag across page text to copy it.
//!
//! The gestures of the special tools run in `crate::gestures`.

use egui::Key;
use markupcraft_model::Kind;

use super::{Role, ToolDef, ToolKind};
use crate::commands::{key, shift};

/// Tools whose gesture is their own (`crate::gestures`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    Eraser,
    Lasso,
    SelectText,
    InsertText,
    Flag,
    /// File Attachment: a click asks for the file
    Attach,
}

impl Special {
    /// The markup kind the tool makes.
    pub fn creates(self) -> Option<Kind> {
        match self {
            Special::InsertText => Some(Kind::Caret),
            Special::Flag => Some(Kind::Note),
            Special::Attach => Some(Kind::Attachment),
            Special::Eraser | Special::Lasso | Special::SelectText => None,
        }
    }
}

pub static SQUIGGLY: ToolDef = ToolDef {
    id: "squiggly",
    label: "Squiggly",
    icon: "spline",
    menu: "Markup",
    keys: shift(Key::U),
    kind: ToolKind::TextMarkup(Kind::Squiggly),
};

pub static INSERT_TEXT: ToolDef = ToolDef {
    id: "inserttext",
    label: "Insert / Replace Text",
    icon: "text-cursor-input",
    menu: "Markup",
    keys: None,
    kind: ToolKind::Special(Special::InsertText),
};

pub static ARC: ToolDef = ToolDef {
    id: "arc",
    label: "Arc",
    icon: "circle-dashed",
    menu: "Markup",
    keys: shift(Key::C),
    kind: ToolKind::Points {
        kind: Kind::Arc,
        closed: false,
        finish_at: 3,
        role: Role::Arc3,
    },
};

pub static DIMENSION: ToolDef = ToolDef {
    id: "dimension",
    label: "Dimension",
    icon: "arrow-left-right",
    menu: "Markup",
    keys: shift(Key::L),
    kind: ToolKind::Points {
        kind: Kind::Dimension,
        closed: false,
        finish_at: 2,
        role: Role::Markup,
    },
};

pub static ERASER: ToolDef = ToolDef {
    id: "eraser",
    label: "Eraser",
    icon: "eraser",
    menu: "Markup",
    keys: shift(Key::E),
    kind: ToolKind::Special(Special::Eraser),
};

pub static FLAG: ToolDef = ToolDef {
    id: "flag",
    label: "Flag",
    icon: "flag",
    menu: "Markup",
    keys: shift(Key::F),
    kind: ToolKind::Special(Special::Flag),
};

pub static FILE_ATTACHMENT: ToolDef = ToolDef {
    id: "attachment",
    label: "File Attachment",
    icon: "file-plus",
    menu: "Markup",
    keys: key(Key::F),
    kind: ToolKind::Special(Special::Attach),
};

pub static LASSO: ToolDef = ToolDef {
    id: "lasso",
    label: "Lasso",
    icon: "square-dashed-mouse-pointer",
    menu: "Tools",
    keys: shift(Key::O),
    kind: ToolKind::Special(Special::Lasso),
};

pub static SELECT_TEXT: ToolDef = ToolDef {
    id: "selecttext",
    label: "Select Text",
    icon: "text-select",
    menu: "Tools",
    keys: shift(Key::T),
    kind: ToolKind::Special(Special::SelectText),
};
