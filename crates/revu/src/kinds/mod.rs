//! How each markup [`Kind`] is written as a Revu-compatible PDF annotation.
//!
//! One [`AnnotKind`] per kind. To add a kind: write `pub static FOO: AnnotKind` in a file in
//! this folder and add one line to [`REGISTRY`]. Everything shared by all kinds (`/C`, `/BS`,
//! `/Measure`, `/Contents`, `/AP`, `/Rect` ...) is written by `write::write_annot`.

pub mod common;
pub mod draw;
pub mod measure;
pub mod measure_more;
pub mod more;
pub mod shapes;
pub mod snapshot;
pub mod text;
pub mod textmarkup;

use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Kind, Markup, code};
use pdfcraft_cos::{Dict, Document as CosDoc};

use crate::ap::Ap;

/// Keys written once, when the annotation is created here.
pub type CreateKeys = fn(&mut Dict, &Markup);
/// Draws the shape into the `/AP /N` stream (inside `q ... Q`, colours and width already set).
/// Anything drawn outside `m.pts` must be pushed onto `extent` so `/Rect` covers it.
pub type DrawShape = fn(&mut Ap, &Markup, &mut Vec<Point>);
/// Writes the geometry keys (`/Vertices`, `/L` ...).
pub type WriteGeometry = fn(&mut Dict, &Markup);
/// `/Rect` and the `/AP /BBox`.
pub type RectOf = fn(&Markup) -> Rect;
/// Reads this kind's own keys after the common ones.
pub type ReadKeys = fn(&CosDoc, &Dict, &mut Markup);
/// Called last, after `/AP` and `/Rect`: may replace the appearance or write objects of its own.
pub type Finish = fn(&mut CosDoc, &mut Dict, &mut Markup);

pub struct AnnotKind {
    pub kind: Kind,
    /// `/Subtype` written for a new annotation, without the slash
    pub subtype: &'static str,
    /// `/IT`, `None` = none
    pub intent: Option<&'static str>,
    /// `/MeasurementTypes`, 0 = none
    pub code: i64,
    /// closed outline: the path is closed and fill opacity applies
    pub closed: bool,
    pub create_keys: Option<CreateKeys>,
    /// `None` = a straight path through `m.pts` (closed per `closed`)
    pub draw_shape: Option<DrawShape>,
    /// `None` = by `/Subtype`: Line -> `/L`, Square/Circle -> none, else `/Vertices`
    pub write_geometry: Option<WriteGeometry>,
    /// `None` = the drawn extent padded by the line width + 1
    pub rect_of: Option<RectOf>,
    pub read_keys: Option<ReadKeys>,
    pub finish: Option<Finish>,
}

impl AnnotKind {
    pub const fn new(kind: Kind, subtype: &'static str, intent: Option<&'static str>, code: i64, closed: bool) -> Self {
        Self {
            kind,
            subtype,
            intent,
            code,
            closed,
            create_keys: None,
            draw_shape: None,
            write_geometry: None,
            rect_of: None,
            read_keys: None,
            finish: None,
        }
    }
}

/// Every kind MarkupCraft writes. Add new kinds at the END, one line.
pub static REGISTRY: &[&AnnotKind] = &[
    &measure::AREA,
    &measure::PERIMETER,
    &measure::POLYLENGTH,
    &measure::LENGTH,
    &measure::COUNT,
    &shapes::CLOUD,
    &shapes::POLYGON,
    &shapes::POLYLINE,
    &shapes::LINE,
    &shapes::RECTANGLE,
    &text::TEXT,
    &text::CALLOUT,
    &text::TYPEWRITER,
    &shapes::ARROW,
    &shapes::ELLIPSE,
    &draw::INK,
    &draw::HIGHLIGHT,
    &draw::STAMP,
    &textmarkup::TEXT_HIGHLIGHT,
    &textmarkup::UNDERLINE,
    &textmarkup::STRIKEOUT,
    &textmarkup::SQUIGGLY,
    &textmarkup::CARET,
    &textmarkup::NOTE,
    &snapshot::SNAPSHOT,
    &measure_more::VOLUME,
    &measure_more::DIAMETER,
    &measure_more::RADIUS,
    &measure_more::ANGLE,
    &more::DIMENSION,
    &more::ARC,
    &more::ATTACHMENT,
];

static FALLBACK: AnnotKind = AnnotKind::new(Kind::Other, "Polygon", None, 0, false);

/// The entry for `k`; kinds without one get a plain open `/Polygon`.
pub fn kind_for(k: Kind) -> &'static AnnotKind {
    REGISTRY.iter().copied().find(|a| a.kind == k).unwrap_or(&FALLBACK)
}

/// Which kind an annotation is, from its `/Subtype`, `/IT` and `/MeasurementTypes`.
pub fn classify(sub: &str, it: &str, code_: i64) -> Kind {
    if code_ == code::COUNT {
        return Kind::Count;
    }
    if code_ == code::VOLUME {
        return Kind::Volume;
    }
    if code_ == code::DIAMETER {
        return if it == measure_more::RADIUS_INTENT {
            Kind::Radius
        } else {
            Kind::Diameter
        };
    }
    if code_ == code::ANGLE {
        return Kind::Angle;
    }
    match sub {
        "Polygon" => match it {
            "PolygonDimension" if code_ == code::LENGTH => Kind::Perimeter,
            "PolygonDimension" => Kind::Area,
            "PolygonCloud" => Kind::Cloud,
            _ if code_ == code::AREA => Kind::Area,
            _ => Kind::Polygon,
        },
        "PolyLine" if it == more::ARC_INTENT => Kind::Arc,
        "PolyLine" => {
            if it == "PolyLineDimension" || code_ == code::LENGTH {
                Kind::Polylength
            } else {
                Kind::Polyline
            }
        }
        "Line" if it == "LineArrow" => Kind::Arrow,
        "Line" if it == more::DIMENSION_INTENT => Kind::Dimension,
        "Line" => {
            if it == "LineDimension" || code_ == code::LENGTH {
                Kind::Length
            } else {
                Kind::Line
            }
        }
        "Ink" => Kind::Ink,
        "FreeText" if it == "FreeTextTypewriter" => Kind::Typewriter,
        "FreeText" if it == "FreeTextCallout" => Kind::Callout,
        "FreeText" => Kind::Text,
        "Square" if code_ == code::AREA => Kind::Area,
        "Square" => Kind::Rectangle,
        "Circle" => Kind::Ellipse,
        "Stamp" if it == "StampSnapshot" => Kind::Snapshot,
        "Stamp" => Kind::Stamp,
        "Highlight" => Kind::TextHighlight,
        "Underline" => Kind::Underline,
        "StrikeOut" => Kind::Strikeout,
        "Squiggly" => Kind::Squiggly,
        "Caret" => Kind::Caret,
        "Text" => Kind::Note,
        "FileAttachment" => Kind::Attachment,
        _ => Kind::Other,
    }
}

/// Which annotations MarkupCraft draws and edits itself; everything else keeps its `/AP`.
pub fn app_draws(subtype: &str, intent: &str, our_stamp: bool, foreign_look: bool) -> bool {
    if foreign_look {
        return false;
    }
    match subtype {
        "Line" | "Polygon" | "PolyLine" | "Circle" | "Ink" | "FreeText" => true,
        "Square" => intent != "SquareImage",
        "Stamp" => our_stamp,
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" | "Caret" | "Text" | "FileAttachment" => true,
        _ => false,
    }
}
