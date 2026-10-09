//! Length units, display units, precision and standard scale presets.
//!
//! Port target: the C++ `core/Units.cpp` (custom/metric scales, display precision,
//! per-measurement display units, page ranges). Filled in by the measurement wave.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Inch,
    Foot,
    Yard,
    Mile,
    Millimeter,
    Centimeter,
    Meter,
    Kilometer,
    Point,
}

impl LengthUnit {
    /// 1 unit = this many meters.
    pub fn meters(self) -> f64 {
        match self {
            LengthUnit::Inch => 0.0254,
            LengthUnit::Foot => 0.3048,
            LengthUnit::Yard => 0.9144,
            LengthUnit::Mile => 1609.344,
            LengthUnit::Millimeter => 0.001,
            LengthUnit::Centimeter => 0.01,
            LengthUnit::Meter => 1.0,
            LengthUnit::Kilometer => 1000.0,
            LengthUnit::Point => 0.0254 / 72.0,
        }
    }

    /// The `/U` label MarkupCraft writes.
    pub fn label(self) -> &'static str {
        match self {
            LengthUnit::Inch => "in",
            LengthUnit::Foot => "ft",
            LengthUnit::Yard => "yd",
            LengthUnit::Mile => "mi",
            LengthUnit::Millimeter => "mm",
            LengthUnit::Centimeter => "cm",
            LengthUnit::Meter => "m",
            LengthUnit::Kilometer => "km",
            LengthUnit::Point => "pt",
        }
    }

    /// The unit of a `/U` label as Revu writes it (`'` ft, `"` in, `ft`, `m` ...).
    pub fn from_label(label: &str) -> Option<LengthUnit> {
        Some(match label.trim() {
            "'" | "ft" | "feet" => LengthUnit::Foot,
            "\"" | "in" | "inch" => LengthUnit::Inch,
            "yd" => LengthUnit::Yard,
            "mi" => LengthUnit::Mile,
            "mm" => LengthUnit::Millimeter,
            "cm" => LengthUnit::Centimeter,
            "m" => LengthUnit::Meter,
            "km" => LengthUnit::Kilometer,
            "pt" => LengthUnit::Point,
            _ => return None,
        })
    }

    pub fn is_metric(self) -> bool {
        matches!(
            self,
            LengthUnit::Millimeter | LengthUnit::Centimeter | LengthUnit::Meter | LengthUnit::Kilometer
        )
    }
}
