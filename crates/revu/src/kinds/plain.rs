//! Plain shapes with the default straight-path appearance. The markup-tools wave replaces these
//! with full ports (line endings, clouds, ellipses, ink strokes, rich text).

use markupcraft_model::Kind;

use super::AnnotKind;

pub static POLYGON: AnnotKind = AnnotKind::new(Kind::Polygon, "Polygon", None, 0, true);
pub static POLYLINE: AnnotKind = AnnotKind::new(Kind::Polyline, "PolyLine", None, 0, false);
pub static LINE: AnnotKind = AnnotKind::new(Kind::Line, "Line", None, 0, false);
pub static RECTANGLE: AnnotKind = AnnotKind::new(Kind::Rectangle, "Square", None, 0, true);
pub static ELLIPSE: AnnotKind = AnnotKind::new(Kind::Ellipse, "Circle", None, 0, true);
pub static INK: AnnotKind = AnnotKind::new(Kind::Ink, "Ink", None, 0, false);
