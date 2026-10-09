//! Markup properties: the patch the edit commands apply, and the names tools use for kinds
//! and colours.

use std::collections::BTreeMap;

use markupcraft_model::{Color, CountSymbol, Kind, Markup, Point, Scale};
use markupcraft_revu::kinds;

use crate::{Result, invalid};

/// Longest text accepted for one property.
pub const MAX_TEXT: usize = 65_536;

/// Every markup kind, in the model's order.
pub const ALL_KINDS: &[Kind] = &[
    Kind::Length,
    Kind::Polylength,
    Kind::Area,
    Kind::Perimeter,
    Kind::Count,
    Kind::Rectangle,
    Kind::Ellipse,
    Kind::Polygon,
    Kind::Polyline,
    Kind::Line,
    Kind::Cloud,
    Kind::Text,
    Kind::Callout,
    Kind::Highlight,
    Kind::Stamp,
    Kind::Other,
    Kind::Arrow,
    Kind::Ink,
    Kind::Typewriter,
    Kind::TextHighlight,
    Kind::Underline,
    Kind::Strikeout,
    Kind::Squiggly,
    Kind::Caret,
    Kind::Note,
    Kind::Snapshot,
    Kind::Volume,
    Kind::Diameter,
    Kind::Angle,
    Kind::Radius,
    Kind::Hyperlink,
    Kind::Attachment,
    Kind::Dimension,
    Kind::Arc,
];

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// A kind by its Markups List name (`Text Box`, `Pen`) or its model name (`Text`, `Ink`),
/// ignoring case, spaces and underscores.
pub fn kind_from_name(name: &str) -> Option<Kind> {
    let want = squash(name);
    ALL_KINDS
        .iter()
        .copied()
        .find(|k| squash(k.name()) == want || squash(&format!("{k:?}")) == want)
}

/// Kinds the PDF writer can create today (a registered writer).
pub fn can_create(kind: Kind) -> bool {
    kind != Kind::Other && kinds::kind_for(kind).kind == kind
}

/// Names of the kinds [`can_create`] accepts.
pub fn creatable_kinds() -> Vec<&'static str> {
    ALL_KINDS
        .iter()
        .copied()
        .filter(|k| can_create(*k))
        .map(Kind::name)
        .collect()
}

/// The writer redraws this markup from its geometry (so its geometry can be edited).
pub fn geometry_editable(m: &Markup) -> bool {
    can_create(m.kind) && (!m.in_file() || kinds::app_draws(&m.subtype, &m.intent, !m.stamp.is_empty(), m.foreign_look))
}

/// Saving this markup after a property change keeps or redraws its look correctly.
pub fn properties_editable(m: &Markup) -> bool {
    if !m.in_file() {
        return can_create(m.kind);
    }
    can_create(m.kind) || !kinds::app_draws(&m.subtype, &m.intent, !m.stamp.is_empty(), m.foreign_look)
}

/// The subject Revu gives a new markup of `kind`.
pub fn default_subject(kind: Kind) -> String {
    match kind {
        Kind::Length
        | Kind::Polylength
        | Kind::Area
        | Kind::Perimeter
        | Kind::Count
        | Kind::Volume
        | Kind::Diameter
        | Kind::Angle
        | Kind::Radius => format!("{} Measurement", kind.name()),
        k => k.name().to_string(),
    }
}

/// `#RRGGBB`, `RRGGBB` or a basic colour name.
pub fn parse_color(s: &str) -> Option<Color> {
    let t = s.trim();
    let named = match t.to_ascii_lowercase().as_str() {
        "red" => Some((1.0, 0.0, 0.0)),
        "green" => Some((0.0, 0.5, 0.0)),
        "blue" => Some((0.0, 0.0, 1.0)),
        "yellow" => Some((1.0, 1.0, 0.0)),
        "orange" => Some((1.0, 0.5, 0.0)),
        "purple" => Some((0.5, 0.0, 0.5)),
        "magenta" => Some((1.0, 0.0, 1.0)),
        "cyan" => Some((0.0, 1.0, 1.0)),
        "black" => Some((0.0, 0.0, 0.0)),
        "white" => Some((1.0, 1.0, 1.0)),
        "gray" | "grey" => Some((0.5, 0.5, 0.5)),
        _ => None,
    };
    if let Some((r, g, b)) = named {
        return Some(Color::rgb(r, g, b));
    }
    let hex = t.strip_prefix('#').unwrap_or(t);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let ch = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .map(|v| f64::from(v) / 255.0)
    };
    Some(Color::rgb(ch(0)?, ch(2)?, ch(4)?))
}

/// Property changes for one or more markups. `None` leaves a property as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkupPatch {
    pub color: Option<Color>,
    /// `Some(None)` removes the fill
    pub fill: Option<Option<Color>>,
    pub opacity: Option<f64>,
    pub fill_opacity: Option<f64>,
    pub line_width: Option<f64>,
    /// empty = solid
    pub dash: Option<Vec<f64>>,
    pub subject: Option<String>,
    pub label: Option<String>,
    pub author: Option<String>,
    pub contents: Option<String>,
    pub layer: Option<String>,
    /// "" or "None" clears it
    pub status: Option<String>,
    pub checked: Option<bool>,
    pub locked: Option<bool>,
    pub line_start: Option<String>,
    pub line_end: Option<String>,
    pub cloud: Option<f64>,
    pub font: Option<String>,
    pub font_size: Option<f64>,
    pub text_color: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub depth: Option<f64>,
    pub rise_drop: Option<f64>,
    pub scale: Option<Scale>,
    /// custom column id -> value ("" removes the value)
    pub columns: BTreeMap<String, String>,
    pub underline: Option<bool>,
    /// 0 left, 1 center, 2 right
    pub align: Option<i32>,
    pub count_symbol: Option<CountSymbol>,
    pub symbol_scale: Option<f64>,
    /// Show Segment Values
    pub segment_values: Option<bool>,
    /// a moved measurement caption (`Some(None)` puts it back)
    pub caption_offset: Option<Option<Point>>,
    /// Note icon (`/Name`)
    pub icon: Option<String>,
    /// Blend mode: Multiply (`true`) or Normal
    pub multiply: Option<bool>,
    /// annotation flags (`/F`): Hidden, Print, NoView
    pub hidden: Option<bool>,
    pub print: Option<bool>,
    pub no_view: Option<bool>,
    /// Show Caption of a measurement
    pub show_caption: Option<bool>,
    /// a custom count symbol (outlines around the item's centre); sets the symbol to Custom
    pub symbol_paths: Option<Vec<Vec<Point>>>,
    /// rich text runs of a text markup (char offsets into its contents)
    pub rich: Option<Vec<markupcraft_model::rich::TextRun>>,
    /// Dimension: offset of the dimension line and the extension past it (points)
    pub leader: Option<f64>,
    pub leader_ext: Option<f64>,
    /// a measurement's slope: (`/SlopeType` 0 none, 1 pitch, 2 degrees, 3 grade; its value)
    pub slope: Option<(i64, f64)>,
    /// a measurement's caption contents (`{value}`, `{subject}` ...; "" = the value)
    pub caption_template: Option<String>,
    /// join a moved caption to its markup with a leader line
    pub caption_leader: Option<bool>,
    /// mark an area's centroid
    pub show_centroid: Option<bool>,
    /// a Count's item width and height (first `/D` unit)
    pub item_width: Option<f64>,
    pub item_height: Option<f64>,
    /// a text box's margin beyond its frame's (points) and its line spacing (x single)
    pub text_margin: Option<f64>,
    pub line_spacing: Option<f64>,
    /// a note's pop-up shown open
    pub popup_open: Option<bool>,
}

fn unit_range(name: &str, v: Option<f64>) -> Result<()> {
    match v {
        Some(x) if !(0.0..=1.0).contains(&x) => Err(invalid(format!("{name} must be 0 to 1 (got {x})"))),
        _ => Ok(()),
    }
}

fn text_len(name: &str, v: &Option<String>) -> Result<()> {
    match v {
        Some(s) if s.len() > MAX_TEXT => Err(invalid(format!("{name} is longer than {MAX_TEXT} bytes"))),
        _ => Ok(()),
    }
}

impl MarkupPatch {
    pub fn is_empty(&self) -> bool {
        *self == MarkupPatch::default()
    }

    /// Does this patch change anything other than the lock?
    fn changes_more_than_lock(&self) -> bool {
        let mut p = self.clone();
        p.locked = None;
        !p.is_empty()
    }

    pub fn validate(&self) -> Result<()> {
        unit_range("opacity", self.opacity)?;
        unit_range("fill_opacity", self.fill_opacity)?;
        if let Some(w) = self.line_width
            && !(0.0..=1000.0).contains(&w)
        {
            return Err(invalid(format!("line width must be 0 to 1000 points (got {w})")));
        }
        if let Some(d) = &self.dash
            && (d.len() > 16
                || d.iter().any(|v| !(0.0..=1000.0).contains(v))
                || (!d.is_empty() && d.iter().all(|v| *v == 0.0)))
        {
            return Err(invalid(
                "dash must be up to 16 lengths of 0 to 1000 points, not all zero ([] = solid)",
            ));
        }
        if let Some(c) = self.cloud
            && !(0.0..=2.0).contains(&c)
        {
            return Err(invalid(format!("cloud intensity must be 0 to 2 (got {c})")));
        }
        if let Some(s) = self.font_size
            && !(1.0..=1000.0).contains(&s)
        {
            return Err(invalid(format!("font size must be 1 to 1000 (got {s})")));
        }
        for (name, v) in [("depth", self.depth), ("rise_drop", self.rise_drop)] {
            if let Some(x) = v
                && !(x.is_finite() && x.abs() <= 1.0e9)
            {
                return Err(invalid(format!("{name} must be a finite number")));
            }
        }
        if let Some(s) = &self.scale
            && !s.valid()
        {
            return Err(invalid("the scale has no usable /X conversion"));
        }
        for (name, v) in [
            ("subject", &self.subject),
            ("label", &self.label),
            ("author", &self.author),
            ("contents", &self.contents),
            ("layer", &self.layer),
            ("status", &self.status),
            ("line_start", &self.line_start),
            ("line_end", &self.line_end),
            ("font", &self.font),
        ] {
            text_len(name, v)?;
        }
        if let Some(a) = self.align
            && !(0..=2).contains(&a)
        {
            return Err(invalid(format!(
                "align must be 0 (left), 1 (center) or 2 (right) (got {a})"
            )));
        }
        if let Some(v) = self.symbol_scale
            && !(0.05..=50.0).contains(&v)
        {
            return Err(invalid(format!("symbol scale must be 0.05 to 50 (got {v})")));
        }
        if let Some(Some(o)) = self.caption_offset
            && !(o.x.is_finite() && o.y.is_finite() && o.x.abs() <= 1.0e6 && o.y.abs() <= 1.0e6)
        {
            return Err(invalid("the caption offset must be finite numbers"));
        }
        text_len("icon", &self.icon)?;
        if let Some(r) = &self.rich
            && (r.len() > markupcraft_model::rich::MAX_RUNS || r.iter().any(|t| t.start > t.end))
        {
            return Err(invalid("rich text: at most 10000 runs, each start before its end"));
        }
        if let Some(paths) = &self.symbol_paths {
            let n: usize = paths.iter().map(Vec::len).sum();
            if paths.len() > 1000 || n > 20_000 || paths.iter().flatten().any(|p| !(p.x.is_finite() && p.y.is_finite()))
            {
                return Err(invalid(
                    "a count symbol is at most 1000 outlines of 20000 finite points",
                ));
            }
        }
        if let Some(v) = self.text_margin
            && !(v.is_finite() && (-4.0..=200.0).contains(&v))
        {
            return Err(invalid("the text margin must be -4 to 200 points"));
        }
        if let Some(v) = self.line_spacing
            && !(v.is_finite() && (0.5..=5.0).contains(&v))
        {
            return Err(invalid("line spacing must be 0.5 to 5 times single"));
        }
        for (name, v) in [("item_width", self.item_width), ("item_height", self.item_height)] {
            if let Some(x) = v
                && !(x.is_finite() && (0.0..=1.0e6).contains(&x))
            {
                return Err(invalid(format!("{name} must be a number from 0")));
            }
        }
        for (name, v) in [("leader", self.leader), ("leader_ext", self.leader_ext)] {
            if let Some(x) = v
                && !(x.is_finite() && x.abs() <= 10_000.0)
            {
                return Err(invalid(format!("{name} must be a number of points up to 10000")));
            }
        }
        if let Some((t, v)) = self.slope
            && (!(0..=3).contains(&t) || !v.is_finite() || v.abs() > 1.0e6 || (t == 2 && v.abs() >= 90.0))
        {
            return Err(invalid(
                "slope type must be 0 (none), 1 (pitch: rise in 12), 2 (degrees, under 90) or 3 (grade %)",
            ));
        }
        text_len("caption_template", &self.caption_template)?;
        for (k, v) in &self.columns {
            if k.is_empty() || k.len() > 256 || v.len() > MAX_TEXT {
                return Err(invalid(
                    "custom column ids must be 1 to 256 bytes and values short text",
                ));
            }
        }
        Ok(())
    }

    /// Apply to `m`. A locked markup takes only an unlock (with or without other changes).
    pub fn apply(&self, m: &mut Markup) -> Result<()> {
        if self.locked == Some(false) {
            m.set_locked(false);
        }
        if m.locked() && self.changes_more_than_lock() {
            return Err(crate::EngineError::Locked(m.id.clone()));
        }
        if self.changes_more_than_lock() && !properties_editable(m) {
            return Err(invalid(format!(
                "markup {} ({}) is a kind MarkupCraft cannot rewrite yet; its properties are read-only",
                m.id,
                m.kind.name()
            )));
        }
        if let Some(c) = self.color {
            m.color = c;
        }
        if let Some(f) = self.fill {
            m.fill = f;
        }
        if let Some(v) = self.opacity {
            m.opacity = v;
        }
        if let Some(v) = self.fill_opacity {
            m.fill_opacity = v;
        }
        if let Some(v) = self.line_width {
            m.line_width = v;
        }
        if let Some(d) = &self.dash {
            m.dash = d.clone();
        }
        let set = |dst: &mut String, v: &Option<String>| {
            if let Some(v) = v {
                *dst = v.clone();
            }
        };
        set(&mut m.subject, &self.subject);
        set(&mut m.label, &self.label);
        set(&mut m.author, &self.author);
        set(&mut m.contents, &self.contents);
        set(&mut m.layer, &self.layer);
        set(&mut m.line_start, &self.line_start);
        set(&mut m.line_end, &self.line_end);
        if let Some(s) = &self.status {
            m.status = if s.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                s.clone()
            };
        }
        if let Some(c) = self.checked {
            m.checked = c;
        }
        if let Some(v) = self.cloud {
            m.cloud = v;
        }
        if let Some(f) = &self.font {
            m.text.font = f.clone();
        }
        if let Some(v) = self.font_size {
            m.text.size = v;
        }
        if let Some(c) = self.text_color {
            m.text.color = c;
        }
        if let Some(v) = self.bold {
            m.text.bold = v;
        }
        if let Some(v) = self.italic {
            m.text.italic = v;
        }
        if let Some(v) = self.depth {
            m.depth = v;
        }
        if let Some(v) = self.rise_drop {
            m.rise_drop = v;
        }
        if let Some(s) = &self.scale {
            m.scale = Some(s.clone());
        }
        for (k, v) in &self.columns {
            if v.is_empty() {
                m.column_data.remove(k);
            } else {
                m.column_data.insert(k.clone(), v.clone());
            }
        }
        if let Some(v) = self.underline {
            m.text.underline = v;
        }
        if let Some(v) = self.align {
            m.text.align = v;
        }
        if let Some(v) = self.count_symbol {
            m.count_symbol = v;
        }
        if let Some(v) = self.symbol_scale {
            m.symbol_scale = v;
        }
        if let Some(v) = self.segment_values {
            m.segment_values = v;
        }
        if let Some(v) = self.caption_offset {
            m.caption_offset = v;
        }
        if let Some(v) = &self.icon {
            m.icon = v.clone();
        }
        if let Some(v) = self.multiply {
            m.multiply = v;
        }
        let mut flag = |bit: i64, on: Option<bool>| {
            if let Some(on) = on {
                m.flags = if on { m.flags | bit } else { m.flags & !bit };
            }
        };
        flag(markupcraft_model::flags::HIDDEN, self.hidden);
        flag(markupcraft_model::flags::PRINT, self.print);
        flag(markupcraft_model::flags::NO_VIEW, self.no_view);
        if let Some(v) = self.show_caption {
            m.hide_caption = !v;
        }
        if let Some(runs) = &self.rich {
            let n = self.contents.as_deref().unwrap_or(&m.contents).chars().count();
            m.rich = markupcraft_model::rich::normalize(&m.text, runs, n);
        }
        if let Some(paths) = &self.symbol_paths {
            m.symbol_paths = paths.clone();
            m.count_symbol = if paths.is_empty() {
                CountSymbol::Circle
            } else {
                CountSymbol::Custom
            };
        }
        if let Some(v) = self.leader {
            m.leader = v;
        }
        if let Some(v) = self.leader_ext {
            m.leader_ext = v.abs();
        }
        if let Some((t, v)) = self.slope {
            (m.slope_type, m.slope) = if t == 0 { (0, 0.0) } else { (t, v) };
        }
        if let Some(v) = &self.caption_template {
            m.caption_template = v.clone();
        }
        if let Some(v) = self.caption_leader {
            m.caption_leader = v;
        }
        if let Some(v) = self.show_centroid {
            m.show_centroid = v;
        }
        if let Some(v) = self.item_width {
            m.item_width = v;
        }
        if let Some(v) = self.text_margin {
            m.text.margin = v;
        }
        if let Some(v) = self.popup_open {
            m.popup_open = v;
        }
        if let Some(v) = self.line_spacing {
            m.text.line_spacing = v;
        }
        if let Some(v) = self.item_height {
            m.item_height = v;
        }
        if self.locked == Some(true) {
            m.set_locked(true);
        }
        m.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(kind_from_name("text box"), Some(Kind::Text));
        assert_eq!(kind_from_name("Text"), Some(Kind::Text));
        assert_eq!(kind_from_name("pen"), Some(Kind::Ink));
        assert_eq!(kind_from_name("POLY_LENGTH"), Some(Kind::Polylength));
        assert_eq!(kind_from_name("nope"), None);
        assert_eq!(parse_color("#FF8000").map(|c| c.hex()), Some("#FF8000".into()));
        assert_eq!(parse_color("blue"), Some(Color::rgb(0.0, 0.0, 1.0)));
        assert_eq!(parse_color("#GG0000"), None);
    }
}
