//! Markups List data that is not geometry: custom column definitions (document level),
//! review status and replies.
//!
//! Storage (read and written by `markupcraft-revu`):
//!   Catalog `/PCColumns [ << /Id /Name /Type /Decimals /Symbol /Default /Total /Formula /AllowCustom /Items >> ]`
//!   Annotation `/PCColumnData << /id (value) >>`
//!   Status: a `/Text` reply with `/IRT parent /StateModel /Review /State /Accepted` (ISO 32000-1 §12.5.6.4)
//!   Checkmark: a `/Text` reply with `/StateModel /Marked`
//!   Replies: `/Text` annotations with `/IRT parent` and `/Contents`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColumnType {
    #[default]
    Text,
    Number,
    Currency,
    Percent,
    Date,
    Choice,
    Formula,
    Checkmark,
}

impl ColumnType {
    pub fn name(self) -> &'static str {
        match self {
            ColumnType::Text => "Text",
            ColumnType::Number => "Number",
            ColumnType::Currency => "Currency",
            ColumnType::Percent => "Percent",
            ColumnType::Date => "Date",
            ColumnType::Choice => "Choice",
            ColumnType::Formula => "Formula",
            ColumnType::Checkmark => "Checkmark",
        }
    }
    pub fn from_name(s: &str) -> Option<ColumnType> {
        [
            ColumnType::Text,
            ColumnType::Number,
            ColumnType::Currency,
            ColumnType::Percent,
            ColumnType::Date,
            ColumnType::Choice,
            ColumnType::Formula,
            ColumnType::Checkmark,
        ]
        .into_iter()
        .find(|t| t.name().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChoiceItem {
    pub text: String,
    /// offered only for markups with this subject
    pub subject: String,
    /// numeric value (unit cost) used by formulas
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomColumn {
    /// stable key, letters / digits / '_'
    pub id: String,
    pub name: String,
    pub kind: ColumnType,
    pub decimals: i32,
    pub symbol: String,
    pub default_value: String,
    /// sum in group and grand totals (numeric types)
    pub total: bool,
    pub items: Vec<ChoiceItem>,
    pub allow_custom: bool,
    pub formula: String,
    /// a Formula column shows its number as Number, Currency or Percent
    pub display: ColumnType,
    /// a Date column's format: `yyyy-MM-dd`, `MM/dd/yyyy`, `dd/MM/yyyy`, `MMM d, yyyy`
    pub date_format: String,
}

impl Default for CustomColumn {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: ColumnType::Text,
            decimals: 2,
            symbol: "$".into(),
            default_value: String::new(),
            total: true,
            items: Vec::new(),
            allow_custom: false,
            formula: String::new(),
            display: ColumnType::Number,
            date_format: DATE_FORMATS[0].into(),
        }
    }
}

impl CustomColumn {
    pub fn numeric(&self) -> bool {
        matches!(
            self.kind,
            ColumnType::Number | ColumnType::Currency | ColumnType::Percent | ColumnType::Formula
        )
    }
}

/// The date formats a Date column offers.
pub const DATE_FORMATS: &[&str] = &["yyyy-MM-dd", "MM/dd/yyyy", "dd/MM/yyyy", "MMM d, yyyy"];

/// A Date column's default that means the markup's creation date.
pub const TODAY: &str = "{today}";

/// `(year, month, day)` of `yyyy-mm-dd`, `D:yyyymmdd...` or `mm/dd/yyyy` text.
pub fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let t = s.trim();
    let t = t.strip_prefix("D:").unwrap_or(t);
    let digits = |r: std::ops::Range<usize>| t.get(r).filter(|x| x.bytes().all(|b| b.is_ascii_digit()));
    let ok = |y: i32, m: u32, d: u32| ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d));
    if t.len() >= 8 && digits(0..8).is_some() {
        return ok(
            digits(0..4)?.parse().ok()?,
            digits(4..6)?.parse().ok()?,
            digits(6..8)?.parse().ok()?,
        );
    }
    let parts: Vec<&str> = t.split(['-', '/']).collect();
    match parts.as_slice() {
        [y, m, d] if y.len() == 4 => ok(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?),
        [m, d, y] if y.len() == 4 => ok(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?),
        _ => None,
    }
}

/// A date in a Date column's format.
pub fn format_date((y, m, d): (i32, u32, u32), fmt: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    match fmt {
        "MM/dd/yyyy" => format!("{m:02}/{d:02}/{y:04}"),
        "dd/MM/yyyy" => format!("{d:02}/{m:02}/{y:04}"),
        "MMM d, yyyy" => format!(
            "{} {d}, {y:04}",
            MONTHS.get((m as usize).saturating_sub(1)).copied().unwrap_or("")
        ),
        _ => format!("{y:04}-{m:02}-{d:02}"),
    }
}

/// A reply or state annotation attached to a markup (`/IRT`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reply {
    pub obj: (u32, u16),
    pub id: String,
    pub author: String,
    pub date: String,
    pub text: String,
    /// "", "Review", "Marked"
    pub state_model: String,
    pub state: String,
    pub dirty: bool,
}

/// The default status values. "" (None) = no status.
pub fn review_statuses() -> &'static [&'static str] {
    &["None", "Accepted", "Rejected", "Cancelled", "Completed"]
}

/// A unique, key-safe id for a new custom column named `name`.
pub fn make_column_id(name: &str, existing: &[CustomColumn]) -> String {
    let mut base: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
        base = format!("col{base}");
    }
    let used = |id: &str| existing.iter().any(|c| c.id == id);
    if !used(&base) {
        return base;
    }
    (2..=existing.len() + 2)
        .map(|n| format!("{base}_{n}"))
        .find(|id| !used(id))
        .unwrap_or(base)
}

/// How a Markups List cell is edited in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CellEdit {
    #[default]
    None,
    Text,
    Number,
    Date,
    Choice,
    Check,
}

/// One column of the Markups List: a standard Revu column or a custom one (`c:<id>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    /// stable id; custom columns: `c:<CustomColumn::id>`
    pub id: String,
    pub header: String,
    /// default width, pixels
    pub width: u32,
    pub right_align: bool,
    pub visible_by_default: bool,
    /// sorted as a number
    pub numeric: bool,
    /// summed in group and grand totals (per unit)
    pub total: bool,
    pub edit: CellEdit,
    pub custom: bool,
    /// the list draws the text in the markup's colour
    pub markup_color: bool,
}

/// One row of the standard column table: id, header, width, right, visible, numeric, total, edit.
type StdCol = (&'static str, &'static str, u32, bool, bool, bool, bool, CellEdit);

impl TableColumn {
    fn standard(c: &StdCol) -> Self {
        let (id, header, width, right_align, visible, numeric, total, edit) = *c;
        Self {
            id: id.into(),
            header: header.into(),
            width,
            right_align,
            visible_by_default: visible,
            numeric,
            total,
            edit,
            custom: false,
            markup_color: id == "subject",
        }
    }

    /// The column shown for custom column `c`.
    pub fn custom(c: &CustomColumn) -> Self {
        Self {
            id: format!("c:{}", c.id),
            header: c.name.clone(),
            width: 110,
            right_align: c.numeric(),
            visible_by_default: true,
            numeric: c.numeric() || c.kind == ColumnType::Checkmark,
            total: c.numeric() && c.total,
            edit: match c.kind {
                ColumnType::Text => CellEdit::Text,
                ColumnType::Number | ColumnType::Currency | ColumnType::Percent => CellEdit::Number,
                ColumnType::Date => CellEdit::Date,
                ColumnType::Choice => CellEdit::Choice,
                ColumnType::Formula => CellEdit::None,
                ColumnType::Checkmark => CellEdit::Check,
            },
            custom: true,
            markup_color: false,
        }
    }
}

const STANDARD: &[StdCol] = {
    use CellEdit as E;
    &[
        ("subject", "Subject", 200, false, true, false, false, E::Text),
        ("pagelabel", "Page Label", 90, false, true, false, false, E::None),
        ("label", "Label", 150, false, true, false, false, E::Text),
        ("measurement", "Measurement", 120, true, true, true, true, E::None),
        ("author", "Author", 110, false, true, false, false, E::None),
        ("date", "Date", 120, false, true, false, false, E::None),
        ("status", "Status", 90, false, true, false, false, E::Choice),
        ("checkmark", "Checkmark", 84, false, true, false, false, E::Check),
        ("color", "Color", 70, false, true, false, false, E::None),
        ("comments", "Comments", 180, false, true, false, false, E::Text),
        ("page", "Page Index", 70, true, false, true, false, E::None),
        ("created", "Creation Date", 120, false, false, false, false, E::None),
        ("layer", "Layer", 100, false, false, false, false, E::None),
        ("space", "Space", 100, false, false, false, false, E::None),
        ("lock", "Lock", 50, false, false, false, false, E::Check),
        ("capture", "Capture", 110, false, false, false, false, E::None),
        ("legend", "Legend", 60, false, false, false, false, E::None),
        ("view3d", "3D View", 90, false, false, false, false, E::None),
        ("type", "Type", 90, false, false, false, false, E::None),
        ("length", "Length", 100, true, false, true, true, E::None),
        ("area", "Area", 100, true, false, true, true, E::None),
        ("perimeter", "Perimeter", 100, true, false, true, true, E::None),
        ("volume", "Volume", 90, true, false, true, true, E::None),
        ("count", "Count", 60, true, false, true, true, E::None),
        ("depth", "Depth", 80, true, false, true, false, E::None),
        ("wallarea", "Wall Area", 90, true, false, true, true, E::None),
        ("width", "Width", 90, true, false, true, false, E::None),
        ("height", "Height", 90, true, false, true, false, E::None),
        ("risedrop", "Rise/Drop", 80, true, false, true, false, E::None),
        ("slope", "Slope", 70, true, false, true, false, E::None),
        ("unit", "Unit", 50, false, false, false, false, E::None),
        ("replies", "Replies", 60, true, false, true, false, E::None),
        ("id", "Markup ID", 140, false, false, false, false, E::None),
        ("sequence", "Sequence", 70, true, false, true, false, E::None),
        ("x", "X", 70, true, false, true, false, E::None),
        ("y", "Y", 70, true, false, true, false, E::None),
        ("xcenter", "X Center", 80, true, false, true, false, E::None),
        ("ycenter", "Y Center", 80, true, false, true, false, E::None),
        ("docwidth", "Document Width", 110, true, false, true, false, E::None),
        ("docheight", "Document Height", 110, true, false, true, false, E::None),
    ]
};

/// The standard (non-custom) Markups List columns in Revu's order: the identity and review
/// columns shown by default first, then the measurement and other optional columns.
pub fn standard_columns() -> Vec<TableColumn> {
    STANDARD.iter().map(TableColumn::standard).collect()
}

/// Ids of the columns shown by default, in order.
pub fn default_visible_columns() -> Vec<String> {
    STANDARD.iter().filter(|c| c.4).map(|c| c.0.to_string()).collect()
}
