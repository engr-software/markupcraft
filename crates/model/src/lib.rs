//! The markup model. A markup is one PDF annotation; while the app runs it lives here, and on
//! save it is written back as a Revu-compatible annotation (`markupcraft-revu`).
//!
//! The file IS the project: there is no side database. Everything the user can set on a markup
//! has a field here and a key in the PDF.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod caption;
pub mod columns;

use std::collections::BTreeMap;

pub use columns::{ChoiceItem, ColumnType, CustomColumn, Reply, review_statuses};
pub use markupcraft_geom::{Point, Rect};
pub use markupcraft_measure::{FormatArray, NumberFormat, Scale, format_value};
use serde::{Deserialize, Serialize};

/// Revu `/MeasurementTypes` codes (seen in Revu files).
pub mod code {
    pub const COUNT: i64 = 128;
    pub const AREA: i64 = 129;
    pub const LENGTH: i64 = 130;
    pub const VOLUME: i64 = 132;
    pub const DIAMETER: i64 = 384;
    pub const ANGLE: i64 = 1152;
}

/// What a markup is. One variant per Revu tool family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    /// `/Line` + `/LineDimension`
    Length,
    /// `/PolyLine` + `/PolyLineDimension`
    Polylength,
    /// `/Polygon` + `/PolygonDimension`, code 129
    Area,
    /// closed perimeter, code 130
    Perimeter,
    /// counted symbols, code 128
    Count,
    /// `/Square`
    Rectangle,
    /// `/Circle`
    Ellipse,
    /// `/Polygon`
    Polygon,
    /// `/PolyLine`
    Polyline,
    /// `/Line`
    Line,
    /// `/Polygon` + `/PolygonCloud`
    Cloud,
    /// `/FreeText`
    Text,
    /// `/FreeText` + `/FreeTextCallout`
    Callout,
    /// `/Ink` + `/BM /Multiply`
    Highlight,
    /// `/Stamp`
    Stamp,
    /// anything else (kept, never rewritten)
    Other,
    /// `/Line` + `/LineArrow`
    Arrow,
    /// `/Ink`, the Pen tool
    Ink,
    /// `/FreeText` + `/FreeTextTypewriter`
    Typewriter,
    /// `/Highlight` over page text
    TextHighlight,
    /// `/Underline`
    Underline,
    /// `/StrikeOut`
    Strikeout,
    /// `/Squiggly`
    Squiggly,
    /// `/Caret`
    Caret,
    /// `/Text` sticky note with a `/Popup`
    Note,
    /// `/Stamp` + `/StampSnapshot`
    Snapshot,
    /// Volume, code 132
    Volume,
    /// Diameter, code 384
    Diameter,
    /// Angle, code 1152
    Angle,
    /// Radius
    Radius,
    /// `/Link` hyperlink markup
    Hyperlink,
    /// `/FileAttachment`
    Attachment,
}

impl Kind {
    /// The name the Markups List shows in its Type column.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Length => "Length",
            Kind::Polylength => "Polylength",
            Kind::Area => "Area",
            Kind::Perimeter => "Perimeter",
            Kind::Count => "Count",
            Kind::Rectangle => "Rectangle",
            Kind::Ellipse => "Ellipse",
            Kind::Polygon => "Polygon",
            Kind::Polyline => "Polyline",
            Kind::Line => "Line",
            Kind::Cloud => "Cloud",
            Kind::Text => "Text Box",
            Kind::Callout => "Callout",
            Kind::Highlight => "Highlight",
            Kind::Stamp => "Stamp",
            Kind::Other => "Other",
            Kind::Arrow => "Arrow",
            Kind::Ink => "Pen",
            Kind::Typewriter => "Typewriter",
            Kind::TextHighlight => "Text Highlight",
            Kind::Underline => "Underline",
            Kind::Strikeout => "Strikethrough",
            Kind::Squiggly => "Squiggly",
            Kind::Caret => "Caret",
            Kind::Note => "Note",
            Kind::Snapshot => "Snapshot",
            Kind::Volume => "Volume",
            Kind::Diameter => "Diameter",
            Kind::Angle => "Angle",
            Kind::Radius => "Radius",
            Kind::Hyperlink => "Hyperlink",
            Kind::Attachment => "File Attachment",
        }
    }

    pub fn is_measurement(self) -> bool {
        matches!(
            self,
            Kind::Length
                | Kind::Polylength
                | Kind::Area
                | Kind::Perimeter
                | Kind::Count
                | Kind::Volume
                | Kind::Diameter
                | Kind::Angle
                | Kind::Radius
        )
    }

    /// Kinds whose text is the markup (text box, callout, typewriter).
    pub fn is_text(self) -> bool {
        matches!(self, Kind::Text | Kind::Callout | Kind::Typewriter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Color {
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    /// `#RRGGBB`
    pub fn hex(&self) -> String {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02X}{:02X}{:02X}", c(self.r), c(self.g), c(self.b))
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::RED
    }
}

/// Font of a text-bearing markup (FreeText `/DA` `/DS` `/RC`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// family: Helvetica, Times, Courier (CSS name as Revu writes it)
    pub font: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 0 left, 1 center, 2 right
    pub align: i32,
    pub color: Color,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: "Helvetica".into(),
            size: 12.0,
            bold: false,
            italic: false,
            underline: false,
            align: 0,
            color: Color::RED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CountSymbol {
    #[default]
    Circle,
    Square,
    Check,
    Cross,
    /// `symbol_paths`
    Custom,
}

/// A PDF object id `(number, generation)`; `(0, 0)` = not in the file yet.
pub type ObjId = (u32, u16);

/// Where a Snapshot's content comes from (all in the markup's own file).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SnapshotSource {
    /// a snapshot annotation whose `/AP /N` is the content (read from the file, or copied)
    pub annot: Option<ObjId>,
    /// a page (0-based) whose content, cut to `region`, is captured on save
    pub page: Option<usize>,
    /// the captured region, source page user space
    pub region: Rect,
}

/// Annotation flag bits (ISO 32000-1 §12.5.3).
pub mod flags {
    pub const PRINT: i64 = 4;
    pub const LOCKED: i64 = 128;
    pub const LOCKED_CONTENTS: i64 = 512;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Markup {
    // identity
    /// `/NM`
    pub id: String,
    /// 0-based
    pub page: usize,
    /// source object, `(0, 0)` if created here
    pub obj: ObjId,
    /// index in its page's `/Annots` when loaded
    pub annot_index: Option<usize>,

    // what it is
    pub kind: Kind,
    /// `/Subtype` without slash
    pub subtype: String,
    /// `/IT` without slash
    pub intent: String,
    /// `/MeasurementTypes`
    pub measure_code: i64,

    // Markups List columns
    pub subject: String,
    pub label: String,
    pub author: String,
    /// `/Contents` (Revu puts the shown quantity here)
    pub contents: String,
    /// optional content group name
    pub layer: String,
    pub created: String,
    pub modified: String,

    // look
    pub color: Color,
    pub fill: Option<Color>,
    /// `/CA`
    pub opacity: f64,
    /// `/FillOpacity` (Revu)
    pub fill_opacity: f64,
    /// `/BS /W`
    pub line_width: f64,
    /// `/BS /D`; empty = solid
    pub dash: Vec<f64>,
    /// `/LE` names
    pub line_start: String,
    pub line_end: String,
    /// `/BE /I` intensity; 0 = straight edges
    pub cloud: f64,
    /// `/BM /Multiply`
    pub multiply: bool,
    pub text: TextStyle,

    // geometry, PDF user space (bottom-left origin)
    /// `/Vertices`, `/L`, box corners, `/QuadPoints` or ink points. Box kinds (Rectangle,
    /// Ellipse, Text Box, Typewriter, Stamp, Caret, Note, Snapshot): the four corners; a Callout
    /// adds `pts[4]` = leader tip and `pts[5]` = knee.
    pub pts: Vec<Point>,
    pub rect: Rect,
    /// Ink: index into pts where each stroke after the first starts
    pub strokes: Vec<usize>,

    /// `/Measure`
    pub scale: Option<Scale>,

    // takeoff
    /// Area cutouts (holes), PDF user space
    pub holes: Vec<Vec<Point>>,
    pub count_symbol: CountSymbol,
    pub symbol_scale: f64,
    pub symbol_paths: Vec<Vec<Point>>,
    pub segment_values: bool,
    /// Polylength Rise/Drop in the first `/D` unit
    pub rise_drop: f64,
    /// caption moved by the user: offset from its default anchor (PDF units)
    pub caption_offset: Option<Point>,
    /// arcs: (index of the arc's start vertex in pts, number of interior points)
    pub arcs: Vec<(usize, usize)>,
    /// depth for volume (first `/D` unit)
    pub depth: f64,

    // state
    /// `/F`
    pub flags: i64,
    /// changed since load: rewritten on save
    pub dirty: bool,
    /// loaded with its own `/AP`: shown from it until edited
    pub stored_look: bool,
    /// a look we cannot redraw (hatch pattern, rich text spans): its `/AP` is kept
    pub foreign_look: bool,

    // text markups, notes, stamps
    /// our text stamp's design id; empty = not ours
    pub stamp: String,
    /// Note `/Name`
    pub icon: String,
    pub popup_open: bool,
    pub popup: Option<Rect>,
    /// Snapshot: its content
    pub snapshot: Option<SnapshotSource>,

    // Markups List
    /// "" = None, else Accepted / Rejected / ...
    pub status: String,
    pub checked: bool,
    /// custom column id -> value
    pub column_data: BTreeMap<String, String>,
    pub replies: Vec<Reply>,
    pub state_replies: Vec<Reply>,
    /// the group's id (its leader's `/NM`); "" = not grouped
    pub group: String,
    /// while loading: this annotation replies to that object
    pub irt: Option<ObjId>,
    /// keys we do not model, kept as raw PDF source text for round trips (informational)
    pub extra: BTreeMap<String, String>,
}

impl Default for Markup {
    fn default() -> Self {
        Self {
            id: String::new(),
            page: 0,
            obj: (0, 0),
            annot_index: None,
            kind: Kind::Other,
            subtype: String::new(),
            intent: String::new(),
            measure_code: 0,
            subject: String::new(),
            label: String::new(),
            author: String::new(),
            contents: String::new(),
            layer: String::new(),
            created: String::new(),
            modified: String::new(),
            color: Color::RED,
            fill: None,
            opacity: 1.0,
            fill_opacity: 1.0,
            line_width: 1.0,
            dash: Vec::new(),
            line_start: "None".into(),
            line_end: "None".into(),
            cloud: 0.0,
            multiply: false,
            text: TextStyle::default(),
            pts: Vec::new(),
            rect: Rect::default(),
            strokes: Vec::new(),
            scale: None,
            holes: Vec::new(),
            count_symbol: CountSymbol::Circle,
            symbol_scale: 1.0,
            symbol_paths: Vec::new(),
            segment_values: false,
            rise_drop: 0.0,
            caption_offset: None,
            arcs: Vec::new(),
            depth: 0.0,
            flags: flags::PRINT,
            dirty: false,
            stored_look: false,
            foreign_look: false,
            stamp: String::new(),
            icon: String::new(),
            popup_open: false,
            popup: None,
            snapshot: None,
            status: String::new(),
            checked: false,
            column_data: BTreeMap::new(),
            replies: Vec::new(),
            state_replies: Vec::new(),
            group: String::new(),
            irt: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Markup {
    pub fn new(kind: Kind, page: usize, pts: Vec<Point>) -> Self {
        Self {
            kind,
            page,
            pts,
            dirty: true,
            ..Default::default()
        }
    }

    pub fn locked(&self) -> bool {
        self.flags & (flags::LOCKED | flags::LOCKED_CONTENTS) != 0
    }

    pub fn set_locked(&mut self, on: bool) {
        self.flags = if on {
            self.flags | flags::LOCKED
        } else {
            self.flags & !(flags::LOCKED | flags::LOCKED_CONTENTS)
        };
    }

    pub fn in_file(&self) -> bool {
        self.obj.0 != 0
    }

    /// Area without cutouts (0 without a scale).
    pub fn gross_area(&self) -> f64 {
        match &self.scale {
            Some(s) if s.valid() => s.area_of(&self.pts),
            _ => 0.0,
        }
    }

    /// Quantity computed from geometry and scale, in the first unit of `/D` or `/A`.
    pub fn quantity(&self) -> Option<f64> {
        if self.kind == Kind::Count {
            return Some(if self.pts.is_empty() {
                1.0
            } else {
                self.pts.len() as f64
            });
        }
        let s = self.scale.as_ref().filter(|s| s.valid())?;
        match self.kind {
            Kind::Area => {
                let mut a = s.area_of(&self.pts);
                for h in &self.holes {
                    a -= s.area_of(h);
                }
                Some(a.max(0.0))
            }
            Kind::Length => Some(s.length_of(&self.pts, false)),
            Kind::Polylength => Some(s.length_of(&self.pts, false) + self.rise_drop.abs()),
            Kind::Perimeter => Some(s.length_of(&self.pts, true)),
            Kind::Volume => {
                let mut a = s.area_of(&self.pts);
                for h in &self.holes {
                    a -= s.area_of(h);
                }
                Some(a.max(0.0) * self.depth)
            }
            _ => None,
        }
    }

    /// The format array the quantity is shown with.
    pub fn quantity_formats(&self) -> Option<&FormatArray> {
        let s = self.scale.as_ref()?;
        Some(match self.kind {
            Kind::Area => &s.area,
            Kind::Volume => &s.volume,
            _ => &s.dist,
        })
    }

    /// Unit shown in the Markups List (`ft`, `in`, `sf`, `ea` ...).
    pub fn unit(&self) -> String {
        if self.kind == Kind::Count {
            return "ea".into();
        }
        let Some(fa) = self.quantity_formats() else {
            return String::new();
        };
        match fa.first().map(|f| f.unit.as_str()) {
            Some("'") => "ft".into(),
            Some("\"") => "in".into(),
            Some(u) => u.into(),
            None => String::new(),
        }
    }

    /// The quantity formatted like Revu writes it into `/Contents`.
    pub fn quantity_text(&self) -> String {
        let Some(q) = self.quantity() else { return String::new() };
        if self.kind == Kind::Count {
            return format!("{}", q as i64);
        }
        match self.quantity_formats() {
            Some(fa) => format_value(q, fa),
            None => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Viewport {
    /// user space (the file stores it relative to the media box corner)
    pub bbox: Rect,
    pub name: String,
    pub id: String,
    pub scale: Scale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    pub media: Rect,
    pub crop: Rect,
    pub rotate: i32,
    /// `/VP`
    pub viewports: Vec<Viewport>,
    /// page-level scale calibrated here (written as a full-page `/VP`)
    pub scale: Option<Scale>,
    /// calibrated here, not yet saved
    pub scale_changed: bool,
    /// `/PageLabels` label ("" = none)
    pub label: String,
}

impl Default for PageInfo {
    fn default() -> Self {
        let letter = Rect::new(0.0, 0.0, 612.0, 792.0);
        Self {
            media: letter,
            crop: letter,
            rotate: 0,
            viewports: Vec::new(),
            scale: None,
            scale_changed: false,
            label: String::new(),
        }
    }
}

impl PageInfo {
    /// The scale in effect at a point: a viewport containing it, else the page scale.
    pub fn scale_at(&self, p: Point) -> Option<&Scale> {
        self.viewports
            .iter()
            .find(|vp| vp.bbox.contains(p) && vp.scale.valid())
            .map(|vp| &vp.scale)
            .or(self.scale.as_ref().filter(|s| s.valid()))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub pages: Vec<PageInfo>,
    pub markups: Vec<Markup>,
    /// source objects removed by the user
    pub deleted: Vec<ObjId>,
    /// Markups List custom columns
    pub columns: Vec<CustomColumn>,
    pub columns_changed: bool,
    /// z-order edited, not yet saved
    pub order_changed: bool,
}

impl Document {
    pub fn markups_on(&self, page: usize) -> impl Iterator<Item = &Markup> {
        self.markups.iter().filter(move |m| m.page == page)
    }
    pub fn find(&self, id: &str) -> Option<&Markup> {
        self.markups.iter().find(|m| m.id == id)
    }
    pub fn find_mut(&mut self, id: &str) -> Option<&mut Markup> {
        self.markups.iter_mut().find(|m| m.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_with_cutout() {
        let mut m = Markup::new(Kind::Area, 0, Rect::new(0.0, 0.0, 90.0, 90.0).corners().to_vec());
        m.scale = Some(Scale::architectural(0.125, 1.0));
        assert_eq!(m.quantity_text(), "100 sf");
        m.holes.push(Rect::new(0.0, 0.0, 45.0, 45.0).corners().to_vec());
        assert_eq!(m.quantity_text(), "75 sf");
        assert_eq!(m.unit(), "sf");
    }

    #[test]
    fn viewport_wins_over_page_scale() {
        let mut p = PageInfo {
            scale: Some(Scale::architectural(0.25, 1.0)),
            ..Default::default()
        };
        p.viewports.push(Viewport {
            bbox: Rect::new(0.0, 0.0, 100.0, 100.0),
            scale: Scale::architectural(0.125, 1.0),
            ..Default::default()
        });
        assert_eq!(
            p.scale_at(Point::new(50.0, 50.0)).map(|s| s.ratio.as_str()),
            Some("0.125 in = 1 ft' in\"")
        );
        assert_eq!(
            p.scale_at(Point::new(500.0, 500.0)).map(|s| s.ratio.as_str()),
            Some("0.25 in = 1 ft' in\"")
        );
    }
}
