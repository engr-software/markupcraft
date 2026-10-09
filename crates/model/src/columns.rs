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

/// The status values Revu offers by default. "" (None) = no status.
pub fn review_statuses() -> &'static [&'static str] {
    &["None", "Accepted", "Rejected", "Cancelled", "Completed"]
}
