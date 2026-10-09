//! The tool registry: what the left mouse button does on the canvas. To add a drawing tool,
//! write `pub static TOOL: ToolDef` (or several) in `tools/<family>.rs` and add one line to
//! [`TOOLS`].
//!
//! A tool describes its gesture ([`ToolKind`]) and the markup kind it makes; the canvas
//! (`interact.rs`) runs the gesture and [`new_markup`] builds the markup with Revu's default
//! look. Only kinds the Revu writer knows are offered, so everything drawn saves faithfully.

pub mod draw;
pub mod measure;
pub mod pan;
pub mod select;
pub mod shapes;
pub mod text;
pub mod textmarkup;
pub mod zoom;

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Color, CountSymbol, Kind, Markup, SnapshotSource};

use crate::commands::Keys;

/// What a click-to-place-points tool does with its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// a markup of the tool's kind
    Markup,
    /// two points, then the Calibrate dialog
    Calibrate,
    /// a hole in the Area under the first point (or the selected Area)
    Cutout,
    /// a cloud, then a callout pointing at it
    CloudPlus,
    /// a dragged box that becomes a viewport (asks for its name and scale)
    Viewport,
    /// three points on a circle: a Radius measurement from its centre
    Radius3,
}

/// The outline a press-drag-release draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragShape {
    Rect,
    Ellipse,
}

/// Points of the outline dragged from `a` to `b` (a 4-vertex rectangle, or a 64-vertex
/// ellipse), `None` when smaller than 2 points.
pub fn drag_ring(shape: DragShape, a: Point, b: Point) -> Option<Vec<Point>> {
    let r = drag_rect(a, b)?;
    Some(match shape {
        DragShape::Rect => vec![
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y1),
        ],
        DragShape::Ellipse => markupcraft_model::measure_extras::ellipse_ring(
            Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0),
            r.width() / 2.0,
            r.height() / 2.0,
            64,
        ),
    })
}

/// The points a role's clicks make: a three-point radius becomes [centre, first point].
pub fn role_points(role: Role, pts: &[Point]) -> Vec<Point> {
    if role == Role::Radius3
        && let [a, t, b] = pts
        && let Some((c, _)) = markupcraft_model::measure_extras::circle_through(*a, *t, *b)
    {
        return vec![c, *a];
    }
    pts.to_vec()
}

#[derive(Clone, Copy)]
pub enum ToolKind {
    /// Click, box-select, move, handles, captions.
    Select,
    /// Drag to scroll the view.
    Pan,
    /// Press, drag, release: a box (Rectangle, Ellipse, Snapshot). Shift makes it square.
    Box(Kind),
    /// Click to place points; Enter, double-click or right-click finishes, Backspace removes
    /// the last point, Esc cancels. `finish_at` > 0 finishes by itself after that many points
    /// (two-point tools also take a press-drag-release).
    Points {
        kind: Kind,
        closed: bool,
        finish_at: usize,
        role: Role,
    },
    /// Freehand strokes (Pen, Highlight).
    Freehand(Kind),
    /// Click or drag a box, then type (Text Box, Typewriter); Callout: the tip first.
    Text(Kind),
    /// Click to place the chosen stamp, or drag its box.
    Stamp,
    /// Click to place a note, then type its comment.
    Note,
    /// Drag across page text (Highlight, Underline, Strikethrough).
    TextMarkup(Kind),
    /// Press, drag, release an outline that becomes a polygon of `kind` (Area by rectangle),
    /// a cutout (Ellipse Cutout) or a viewport.
    Drag { kind: Kind, shape: DragShape, role: Role },
}

pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Lucide icon name.
    pub icon: &'static str,
    /// Menu it is listed in ("Tools", "Markup", "Measure").
    pub menu: &'static str,
    pub keys: Option<Keys>,
    pub kind: ToolKind,
}

impl ToolDef {
    /// The markup kind this tool makes (`None` for Select, Pan, Calibrate, Cutout).
    pub fn creates(&self) -> Option<Kind> {
        match self.kind {
            ToolKind::Select | ToolKind::Pan => None,
            ToolKind::Points {
                role: Role::Calibrate | Role::Cutout | Role::Viewport,
                ..
            } => None,
            ToolKind::Drag {
                kind,
                role: Role::Markup,
                ..
            } => Some(kind),
            ToolKind::Drag { .. } => None,
            ToolKind::Points { kind, .. }
            | ToolKind::Box(kind)
            | ToolKind::Freehand(kind)
            | ToolKind::Text(kind)
            | ToolKind::TextMarkup(kind) => Some(kind),
            ToolKind::Stamp => Some(Kind::Stamp),
            ToolKind::Note => Some(Kind::Note),
        }
    }

    /// Drawing tools (everything but Select and Pan).
    pub fn draws(&self) -> bool {
        !matches!(self.kind, ToolKind::Select | ToolKind::Pan)
    }
}

/// Every tool, in menu and toolbar order. One line per tool.
pub static TOOLS: &[&ToolDef] = &[
    &select::TOOL,
    &pan::TOOL,
    &zoom::TOOL,
    &text::TEXT_BOX,
    &text::CALLOUT,
    &text::TYPEWRITER,
    &text::NOTE,
    &draw::PEN,
    &draw::HIGHLIGHT,
    &draw::STAMP,
    &draw::SNAPSHOT,
    &shapes::LINE,
    &shapes::ARROW,
    &shapes::POLYLINE,
    &shapes::POLYGON,
    &shapes::RECTANGLE,
    &shapes::ELLIPSE,
    &shapes::CLOUD,
    &shapes::CLOUD_PLUS,
    &textmarkup::HIGHLIGHT_TEXT,
    &textmarkup::UNDERLINE,
    &textmarkup::STRIKETHROUGH,
    &measure::CALIBRATE,
    &measure::LENGTH,
    &measure::POLYLENGTH,
    &measure::AREA,
    &measure::PERIMETER,
    &measure::COUNT,
    &measure::CUTOUT,
    &measure::AREA_RECT,
    &measure::VOLUME,
    &measure::DIAMETER,
    &measure::RADIUS,
    &measure::RADIUS3,
    &measure::ANGLE,
    &measure::ELLIPSE_CUTOUT,
    &measure::VIEWPORT,
];

pub fn find(id: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().copied().find(|t| t.id == id)
}

/// The tool that draws markups of `kind` (for the Tool Chest and Set as Default).
pub fn tool_for_kind(kind: Kind) -> Option<&'static ToolDef> {
    TOOLS.iter().copied().find(|t| t.creates() == Some(kind))
}

/// The rectangle spanned by two drag points, `None` if it is smaller than 2 points.
pub(crate) fn drag_rect(a: Point, b: Point) -> Option<Rect> {
    let r = Rect::new(a.x, a.y, b.x, b.y).normalized();
    (r.width() >= 2.0 && r.height() >= 2.0).then_some(r)
}

/// Shift while drawing: `to` moved onto the nearest 0, 45 or 90 degree line through `from`.
pub fn constrain(from: Point, to: Point) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return to;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let a = (dy.atan2(dx) / step).round() * step;
    Point::new(from.x + len * a.cos(), from.y + len * a.sin())
}

/// Shift on a box tool: the square with the drag's larger side.
pub fn square(from: Point, to: Point) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let s = dx.abs().max(dy.abs());
    Point::new(from.x + s.copysign(dx), from.y + s.copysign(dy))
}

/// The look Revu gives a new markup of `kind` (before the user's tool defaults).
pub fn default_look(kind: Kind) -> Markup {
    let mut m = Markup::new(kind, 0, Vec::new());
    m.color = Color::RED;
    m.text.color = Color::RED;
    match kind {
        Kind::Arrow => m.line_start = "OpenArrow".into(),
        Kind::Cloud => m.cloud = 2.0,
        Kind::Callout => {
            m.line_end = "OpenArrow".into();
            m.fill = Some(Color::WHITE);
        }
        Kind::Typewriter => m.line_width = 0.0,
        Kind::Text => m.fill = Some(Color::WHITE),
        Kind::Highlight => {
            m.color = Color::rgb(1.0, 0.9, 0.0);
            m.line_width = 12.0;
            m.multiply = true;
        }
        Kind::Ink => m.line_width = 1.5,
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly | Kind::Note => {
            m.color = markupcraft_revu::kinds::textmarkup::default_color(kind);
            m.multiply = kind == Kind::TextHighlight;
            if kind == Kind::Note {
                m.icon = "Comment".into();
            }
        }
        Kind::Area => {
            m.fill = Some(Color::RED);
            m.fill_opacity = 0.2;
        }
        Kind::Volume => {
            m.fill = Some(Color::RED);
            m.fill_opacity = 0.2;
            // one unit deep until the user types the depth
            m.depth = 1.0;
        }
        Kind::Count => m.count_symbol = CountSymbol::Circle,
        Kind::Snapshot => m.line_width = 0.0,
        Kind::Stamp => {
            m.line_width = 2.5;
            m.text.bold = true;
        }
        _ => {}
    }
    m
}

/// Copy the look and the list fields of `template` onto a freshly drawn markup (Properties
/// mode, Set as Default): everything except identity, geometry, kind and the measured scale.
/// Comments carry over except on text markups (their text is what was typed).
pub fn apply_look(template: &Markup, m: &mut Markup) {
    m.color = template.color;
    m.fill = template.fill;
    m.opacity = template.opacity;
    m.fill_opacity = template.fill_opacity;
    m.line_width = template.line_width;
    m.dash = template.dash.clone();
    m.line_start = template.line_start.clone();
    m.line_end = template.line_end.clone();
    m.cloud = template.cloud;
    m.multiply = template.multiply;
    m.hatch = template.hatch;
    m.hide_caption = template.hide_caption;
    m.text = template.text.clone();
    m.subject = template.subject.clone();
    m.label = template.label.clone();
    m.layer = template.layer.clone();
    m.count_symbol = template.count_symbol;
    m.symbol_scale = template.symbol_scale;
    m.symbol_paths = template.symbol_paths.clone();
    m.segment_values = template.segment_values;
    m.depth = template.depth;
    m.rise_drop = template.rise_drop;
    m.column_data = template.column_data.clone();
    if !template.stamp.is_empty() {
        m.stamp = template.stamp.clone();
    }
    if !template.icon.is_empty() {
        m.icon = template.icon.clone();
    }
    if !m.kind.is_text() && !m.kind.is_measurement() && m.kind != Kind::Stamp {
        m.contents = template.contents.clone();
    }
}

/// The markup a tool makes from its points (PDF user space, page `page`), with Revu's default
/// look; `None` when the points do not make one (too few, too small).
pub fn new_markup(kind: Kind, page: usize, pts: &[Point]) -> Option<Markup> {
    let mut m = default_look(kind);
    m.page = page;
    m.subject = markupcraft_engine::props::default_subject(kind);
    let (min, _) = markupcraft_engine::geometry::point_limits(kind);
    match kind {
        Kind::Rectangle | Kind::Ellipse | Kind::Snapshot | Kind::Text | Kind::Typewriter | Kind::Stamp => {
            let r = drag_rect(*pts.first()?, *pts.get(1)?)?;
            m.pts = r.corners().to_vec();
            m.rect = r;
            if kind == Kind::Snapshot {
                m.snapshot = Some(SnapshotSource {
                    annot: None,
                    page: Some(page),
                    region: r,
                });
            }
        }
        Kind::Note => {
            let at = *pts.first()?;
            let r = Rect::new(at.x, at.y - 24.0, at.x + 24.0, at.y);
            m.pts = r.corners().to_vec();
            m.rect = r;
        }
        Kind::Count => {
            if pts.is_empty() {
                return None;
            }
            m.pts = pts.to_vec();
        }
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly => {
            if pts.is_empty() || !pts.len().is_multiple_of(4) {
                return None;
            }
            m.pts = pts.to_vec();
        }
        Kind::Callout => {
            // The tip, then two corners of the text box; the knee sits just off the box side
            // that faces the tip.
            let tip = *pts.first()?;
            let r = drag_rect(*pts.get(1)?, *pts.get(2)?)?;
            let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
            let gap = 18.0;
            let knee = if tip.x < r.x0 {
                Point::new(r.x0 - gap, cy)
            } else if tip.x > r.x1 {
                Point::new(r.x1 + gap, cy)
            } else if tip.y > r.y1 {
                Point::new(cx, r.y1 + gap)
            } else {
                Point::new(cx, r.y0 - gap)
            };
            m.pts = r.corners().to_vec();
            m.pts.push(tip);
            m.pts.push(knee);
            m.rect = r;
        }
        _ => {
            let mut v: Vec<Point> = Vec::with_capacity(pts.len());
            for p in pts {
                // Double-clicks and clicks in place add the same point twice.
                if v.last().is_none_or(|l: &Point| l.dist(*p) > 0.01) {
                    v.push(*p);
                }
            }
            if matches!(
                kind,
                Kind::Area | Kind::Perimeter | Kind::Polygon | Kind::Cloud | Kind::Volume
            ) && v.len() > 3
                && v.first().zip(v.last()).is_some_and(|(a, b)| a.dist(*b) < 0.01)
            {
                v.pop();
            }
            let (_, max) = markupcraft_engine::geometry::point_limits(kind);
            v.truncate(max);
            if v.len() < min.max(2) {
                return None;
            }
            let b = bbox(&v)?;
            if b.width() < 1.0 && b.height() < 1.0 {
                return None;
            }
            m.pts = v;
        }
    }
    if let Some(b) = bbox(&m.pts)
        && m.rect == Rect::default()
    {
        m.rect = b.padded(m.line_width.max(1.0));
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for t in TOOLS {
            assert!(ids.insert(t.id), "duplicate tool {}", t.id);
            assert!(crate::icons::exists(t.icon), "icon {}", t.icon);
            assert!(crate::commands::MENUS.contains(&t.menu));
            if let Some(k) = t.creates() {
                assert!(markupcraft_engine::props::can_create(k), "{} makes {k:?}", t.id);
            }
        }
    }

    #[test]
    fn every_drawing_tool_makes_a_writable_markup() {
        let pts = [Point::new(10.0, 10.0), Point::new(80.0, 50.0), Point::new(30.0, 90.0)];
        for t in TOOLS {
            let Some(kind) = t.creates() else { continue };
            let pts: Vec<Point> = match t.kind {
                ToolKind::TextMarkup(_) => vec![
                    Point::new(10.0, 60.0),
                    Point::new(90.0, 60.0),
                    Point::new(10.0, 48.0),
                    Point::new(90.0, 48.0),
                ],
                _ => pts.to_vec(),
            };
            let m = new_markup(kind, 2, &pts).unwrap_or_else(|| panic!("{} made nothing", t.id));
            assert_eq!(m.page, 2);
            assert!(crate::actions::editable(&m), "{} must be writable", t.id);
            assert!(new_markup(kind, 0, &[]).is_none(), "{} from nothing", t.id);
        }
    }

    #[test]
    fn shift_constrains_to_45_degrees() {
        let o = Point::new(0.0, 0.0);
        let p = constrain(o, Point::new(100.0, 8.0));
        assert!((p.y).abs() < 1e-9 && (p.x - 100.32).abs() < 0.01, "{p:?}");
        let p = constrain(o, Point::new(50.0, 45.0));
        assert!((p.x - p.y).abs() < 1e-9);
        let p = constrain(o, Point::new(-3.0, -100.0));
        assert!(p.x.abs() < 1e-9 && p.y < -99.0);
        assert_eq!(square(o, Point::new(10.0, -4.0)), Point::new(10.0, -10.0));
    }

    #[test]
    fn duplicate_clicks_collapse() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(100.0, 0.0);
        let c = Point::new(100.0, 100.0);
        let m = new_markup(Kind::Polygon, 0, &[a, b, c, c, c]).unwrap();
        assert_eq!(m.pts.len(), 3);
        assert!(new_markup(Kind::Line, 0, &[a, a]).is_none());
        let n = new_markup(Kind::Note, 0, &[a]).unwrap();
        assert_eq!(n.rect, Rect::new(0.0, -24.0, 24.0, 0.0));
    }
}
