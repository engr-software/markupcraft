//! Length units, display units, precision, custom / metric scales, standard scale presets,
//! calibration and page ranges.
//!
//! Everything here produces or edits a [`Scale`] (the PDF `/Measure /RL` dictionary), so the
//! result is saved exactly like a Revu scale.

use std::collections::BTreeSet;

use markupcraft_geom::Point;
use serde::{Deserialize, Serialize};

use crate::{Fmt, NumberFormat, Scale, fmt_g};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The name the UI shows ("Inches", "Feet" ...).
    pub fn name(self) -> &'static str {
        match self {
            LengthUnit::Inch => "Inches",
            LengthUnit::Foot => "Feet",
            LengthUnit::Yard => "Yards",
            LengthUnit::Mile => "Miles",
            LengthUnit::Millimeter => "Millimeters",
            LengthUnit::Centimeter => "Centimeters",
            LengthUnit::Meter => "Meters",
            LengthUnit::Kilometer => "Kilometers",
            LengthUnit::Point => "Points",
        }
    }

    /// The unit of a `/U` label as Revu writes it (`'` ft, `"` in, `ft`, `sf`, `m` ...).
    /// Area and volume labels give their length unit.
    pub fn from_label(label: &str) -> Option<LengthUnit> {
        Some(match label.trim() {
            "'" | "ft" | "feet" | "sf" | "cu ft" => LengthUnit::Foot,
            "\"" | "in" | "inch" | "sq in" | "cu in" => LengthUnit::Inch,
            "yd" | "sy" | "cu yd" => LengthUnit::Yard,
            "mi" | "sq mi" | "cu mi" => LengthUnit::Mile,
            "mm" | "mm\u{b2}" | "mm\u{b3}" => LengthUnit::Millimeter,
            "cm" | "cm\u{b2}" | "cm\u{b3}" => LengthUnit::Centimeter,
            "m" | "m\u{b2}" | "m\u{b3}" => LengthUnit::Meter,
            "km" | "km\u{b2}" | "km\u{b3}" => LengthUnit::Kilometer,
            "pt" | "sq pt" | "cu pt" => LengthUnit::Point,
            _ => return None,
        })
    }

    pub fn is_metric(self) -> bool {
        matches!(
            self,
            LengthUnit::Millimeter | LengthUnit::Centimeter | LengthUnit::Meter | LengthUnit::Kilometer
        )
    }

    /// Area label (`sf`, `sq in`, `m²` ...).
    pub fn area_label(self) -> &'static str {
        match self {
            LengthUnit::Inch => "sq in",
            LengthUnit::Foot => "sf",
            LengthUnit::Yard => "sy",
            LengthUnit::Mile => "sq mi",
            LengthUnit::Millimeter => "mm\u{b2}",
            LengthUnit::Centimeter => "cm\u{b2}",
            LengthUnit::Meter => "m\u{b2}",
            LengthUnit::Kilometer => "km\u{b2}",
            LengthUnit::Point => "sq pt",
        }
    }

    /// Volume label (`cu ft`, `m³` ...).
    pub fn volume_label(self) -> &'static str {
        match self {
            LengthUnit::Inch => "cu in",
            LengthUnit::Foot => "cu ft",
            LengthUnit::Yard => "cu yd",
            LengthUnit::Mile => "cu mi",
            LengthUnit::Millimeter => "mm\u{b3}",
            LengthUnit::Centimeter => "cm\u{b3}",
            LengthUnit::Meter => "m\u{b3}",
            LengthUnit::Kilometer => "km\u{b3}",
            LengthUnit::Point => "cu pt",
        }
    }

    /// `v` of this unit in `to`.
    pub fn convert(self, v: f64, to: LengthUnit) -> f64 {
        v * self.meters() / to.meters()
    }
}

/// Units a drawing length can be typed in: in, mm, cm, pt.
pub fn paper_units() -> [LengthUnit; 4] {
    [
        LengthUnit::Inch,
        LengthUnit::Millimeter,
        LengthUnit::Centimeter,
        LengthUnit::Point,
    ]
}

/// Units a real length can be typed in: in, ft, yd, mi, mm, cm, m, km.
pub fn real_units() -> [LengthUnit; 8] {
    [
        LengthUnit::Inch,
        LengthUnit::Foot,
        LengthUnit::Yard,
        LengthUnit::Mile,
        LengthUnit::Millimeter,
        LengthUnit::Centimeter,
        LengthUnit::Meter,
        LengthUnit::Kilometer,
    ]
}

/// Display precision. Fraction: denominator 1, 2, 4 ... 64 (feet-inches / inches);
/// otherwise decimal places 0..4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Precision {
    pub fraction: bool,
    /// denominator if `fraction`, else decimal places
    pub value: i64,
}

impl Precision {
    pub const fn decimals(places: i64) -> Self {
        Self {
            fraction: false,
            value: places,
        }
    }
    pub const fn fraction(den: i64) -> Self {
        Self {
            fraction: true,
            value: den,
        }
    }

    /// `1/16` or `0.01`.
    pub fn name(&self) -> String {
        if self.fraction {
            return if self.value <= 1 {
                "1".into()
            } else {
                format!("1/{}", self.value)
            };
        }
        if self.value <= 0 {
            return "1".into();
        }
        format!("0.{}1", "0".repeat((self.value - 1).clamp(0, 12) as usize))
    }

    /// The `(/F, /D)` pair of a number format with this precision.
    fn fmt_den(&self) -> (Fmt, i64) {
        if self.fraction {
            (Fmt::Fraction, self.value.max(1))
        } else {
            (Fmt::Decimal, 10i64.pow(self.value.clamp(0, 6) as u32))
        }
    }
}

impl Default for Precision {
    fn default() -> Self {
        Precision::decimals(2)
    }
}

/// Precision choices in UI order: decimals 0..4, then fractions 1 .. 1/64.
pub fn precision_choices(allow_fractions: bool) -> Vec<Precision> {
    let mut out: Vec<Precision> = (0..=4).map(Precision::decimals).collect();
    if allow_fractions {
        out.extend([1, 2, 4, 8, 16, 32, 64].map(Precision::fraction));
    }
    out
}

/// How lengths are shown: a single unit, or Revu's feet-inches pair (`ft' in"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayUnit {
    pub unit: LengthUnit,
    /// only with `unit == Foot`
    pub feet_inches: bool,
}

impl DisplayUnit {
    pub const FEET_INCHES: DisplayUnit = DisplayUnit {
        unit: LengthUnit::Foot,
        feet_inches: true,
    };
    pub const fn single(unit: LengthUnit) -> Self {
        Self {
            unit,
            feet_inches: false,
        }
    }
    fn is_feet_inches(&self) -> bool {
        self.feet_inches && self.unit == LengthUnit::Foot
    }
    /// The unit `/D`'s first entry is in.
    fn base(&self) -> LengthUnit {
        if self.feet_inches { LengthUnit::Foot } else { self.unit }
    }
    /// `ft-in`, `ft`, `m` ...
    pub fn name(&self) -> String {
        if self.is_feet_inches() {
            "ft-in".into()
        } else {
            self.unit.label().into()
        }
    }
}

impl Default for DisplayUnit {
    fn default() -> Self {
        DisplayUnit::FEET_INCHES
    }
}

/// Feet-inches, then every real unit.
pub fn display_unit_choices() -> Vec<DisplayUnit> {
    let mut out = vec![DisplayUnit::FEET_INCHES];
    out.extend(real_units().map(DisplayUnit::single));
    out
}

fn nf(unit: &str, conv: f64, p: Precision, prefix: &str, suffix: &str) -> NumberFormat {
    let (f, den) = p.fmt_den();
    NumberFormat::new(unit, conv, f, den, prefix, suffix)
}

/// The real-world unit a scale's `/X` converts points to.
pub fn scale_real_unit(s: &Scale) -> Option<LengthUnit> {
    LengthUnit::from_label(&s.x.first()?.unit)
}

/// The display unit of a scale's `/D` (feet-inches when `/D` is `[ft, in]`).
pub fn scale_display_unit(s: &Scale) -> Option<DisplayUnit> {
    let u0 = LengthUnit::from_label(&s.dist.first()?.unit)?;
    let second = s.dist.get(1).and_then(|f| LengthUnit::from_label(&f.unit));
    if u0 == LengthUnit::Foot && second == Some(LengthUnit::Inch) {
        return Some(DisplayUnit::FEET_INCHES);
    }
    Some(DisplayUnit::single(u0))
}

fn precision_of(f: &NumberFormat) -> Precision {
    match f.fmt {
        Fmt::Fraction => Precision::fraction(f.den.max(1)),
        Fmt::Round | Fmt::Truncate => Precision::decimals(0),
        Fmt::Decimal => Precision::decimals(if f.den > 1 {
            (f.den as f64).log10().round() as i64
        } else {
            0
        }),
    }
}

/// The precision of `/D`'s last unit.
pub fn scale_precision(s: &Scale) -> Precision {
    s.dist.last().map_or_else(Precision::default, precision_of)
}

/// Change how lengths / areas are shown without changing the scale ratio: rebuilds `/D`, `/A`,
/// `/V` for `d`, keeping the current precision where it still applies. False when the scale's
/// `/X` unit is unknown.
pub fn set_display_unit(s: &mut Scale, d: DisplayUnit) -> bool {
    let Some(xu) = scale_real_unit(s) else {
        return false;
    };
    let mut p = if s.dist.is_empty() {
        if d.feet_inches {
            Precision::fraction(4)
        } else {
            Precision::decimals(2)
        }
    } else {
        scale_precision(s)
    };
    // Fractions only make sense for inches; switching to a decimal unit uses 2 places.
    let fractions_ok = d.unit == LengthUnit::Inch || d.is_feet_inches();
    if p.fraction && !fractions_ok {
        p = Precision::decimals(2);
    }
    // Back to feet-inches from a decimal unit: Revu's default 1/4".
    let before = scale_display_unit(s);
    let was_fractional = before.is_some_and(|b| b.feet_inches || b.unit == LengthUnit::Inch);
    if d.is_feet_inches() && before.is_some() && !was_fractional {
        p = Precision::fraction(4);
    }
    let area_p = match s.area.last() {
        Some(f) if f.fmt == Fmt::Decimal && f.den > 1 => Precision::decimals((f.den as f64).log10().round() as i64),
        Some(f) if f.den == 1 => Precision::decimals(0),
        _ => Precision::decimals(2),
    };

    let lu = d.base();
    let k = xu.meters() / lu.meters(); // x unit -> display unit
    s.dist = if d.is_feet_inches() {
        // Revu's own feet entry is /F /F /D 4 whatever the inch precision.
        vec![nf("'", k, Precision::fraction(4), "", "-"), nf("\"", 12.0, p, "", "")]
    } else {
        vec![nf(lu.label(), k, p, " ", "")]
    };
    s.area = vec![nf(lu.area_label(), k * k, area_p, " ", "")];
    s.volume = vec![nf(lu.volume_label(), k * k * k, Precision::decimals(2), " ", "")];
    true
}

/// Change the precision of `/D`'s last unit (and `/A`, `/V` when decimal).
pub fn set_precision(s: &mut Scale, p: Precision) {
    let (f, den) = p.fmt_den();
    if let Some(last) = s.dist.last_mut() {
        last.fmt = f;
        last.den = den;
    }
    if !p.fraction {
        for fa in [&mut s.area, &mut s.volume] {
            if let Some(last) = fa.last_mut() {
                last.fmt = Fmt::Decimal;
                last.den = den;
            }
        }
    }
}

/// A scale `paper_len paper_unit = real_len real_unit` (Custom scale, metric ratios,
/// architectural). Lengths display in `display` (default: feet-inches for ft, else the real
/// unit) with `precision`.
pub fn custom_scale(
    paper_len: f64,
    paper_unit: LengthUnit,
    real_len: f64,
    real_unit: LengthUnit,
    display: Option<DisplayUnit>,
    precision: Option<Precision>,
) -> Scale {
    let d = display.unwrap_or(DisplayUnit {
        unit: real_unit,
        feet_inches: real_unit == LengthUnit::Foot,
    });
    let du = d.base();
    let paper_pts = paper_unit.convert(paper_len, LengthUnit::Point);
    let real_in_display = real_unit.convert(real_len, du);
    let shown = if d.feet_inches { "ft' in\"" } else { du.label() };
    let conv = if paper_pts > 0.0 && paper_pts.is_finite() {
        real_in_display / paper_pts
    } else {
        0.0
    };
    let x = if d.feet_inches {
        NumberFormat::new("'", conv, Fmt::Fraction, 4, "", "")
    } else {
        NumberFormat::new(du.label(), conv, Fmt::Decimal, 100, " ", "")
    };
    let mut s = Scale {
        ratio: format!(
            "{} {} = {} {}",
            fmt_g(paper_len, 6),
            paper_unit.label(),
            fmt_g(real_in_display, 6),
            shown
        ),
        x: vec![x],
        ..Default::default()
    };
    set_display_unit(&mut s, d);
    if let Some(p) = precision {
        set_precision(&mut s, p);
    }
    s
}

/// Metric ratio 1:N with lengths shown in `display` (m by default), paper measured in mm.
pub fn metric_ratio(n: f64, display: LengthUnit) -> Scale {
    let places = if display == LengthUnit::Millimeter { 0 } else { 2 };
    custom_scale(
        1.0,
        LengthUnit::Millimeter,
        n,
        LengthUnit::Millimeter,
        Some(DisplayUnit::single(display)),
        Some(Precision::decimals(places)),
    )
}

/// Calibrate: the line `a`-`b` on the sheet (PDF points) is `known` `unit` long in the field.
/// The ratio is written per one real unit, with the paper side in inches (mm for metric units).
/// `None` for a zero-length line or a non-positive known length.
pub fn calibrate(
    a: Point,
    b: Point,
    known: f64,
    unit: LengthUnit,
    display: Option<DisplayUnit>,
    precision: Option<Precision>,
) -> Option<Scale> {
    let pts = a.dist(b);
    if !(pts > 0.0 && pts.is_finite() && known > 0.0 && known.is_finite()) {
        return None;
    }
    let paper_unit = if unit.is_metric() {
        LengthUnit::Millimeter
    } else {
        LengthUnit::Inch
    };
    let paper_per_unit = LengthUnit::Point.convert(pts, paper_unit) / known;
    Some(custom_scale(paper_per_unit, paper_unit, 1.0, unit, display, precision))
}

/// A standard scale the UI lists.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalePreset {
    /// `1/4" = 1'-0"`, `1" = 20'`, `1:100`
    pub name: String,
    pub scale: Scale,
}

/// Standard scales: architectural, engineering and metric, in the order the UI lists them.
pub fn scale_presets() -> Vec<ScalePreset> {
    let arch: [(&str, f64); 11] = [
        ("1/16\" = 1'-0\"", 1.0 / 16.0),
        ("3/32\" = 1'-0\"", 3.0 / 32.0),
        ("1/8\" = 1'-0\"", 1.0 / 8.0),
        ("3/16\" = 1'-0\"", 3.0 / 16.0),
        ("1/4\" = 1'-0\"", 1.0 / 4.0),
        ("3/8\" = 1'-0\"", 3.0 / 8.0),
        ("1/2\" = 1'-0\"", 1.0 / 2.0),
        ("3/4\" = 1'-0\"", 3.0 / 4.0),
        ("1\" = 1'-0\"", 1.0),
        ("1 1/2\" = 1'-0\"", 1.5),
        ("3\" = 1'-0\"", 3.0),
    ];
    let mut v: Vec<ScalePreset> = arch
        .iter()
        .map(|(name, inches)| ScalePreset {
            name: (*name).into(),
            scale: Scale::architectural(*inches, 1.0),
        })
        .collect();
    for ft in [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 100.0] {
        v.push(ScalePreset {
            name: format!("1\" = {}'", fmt_g(ft, 6)),
            scale: Scale::engineering(ft),
        });
    }
    for n in [
        1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 75.0, 100.0, 125.0, 150.0, 200.0, 250.0, 500.0, 1000.0, 1250.0, 2500.0,
    ] {
        let display = if n <= 20.0 {
            LengthUnit::Millimeter
        } else {
            LengthUnit::Meter
        };
        v.push(ScalePreset {
            name: format!("1:{}", fmt_g(n, 6)),
            scale: metric_ratio(n, display),
        });
    }
    v
}

/// The preset whose name is `name`.
pub fn preset(name: &str) -> Option<Scale> {
    scale_presets().into_iter().find(|p| p.name == name).map(|p| p.scale)
}

/// `"1-3, 5, 9"` to `[0, 1, 2, 4, 8]` (0-based, sorted, unique), pages limited to
/// `[1, page_count]`. `None` on a syntax error, a page out of range or an empty result.
pub fn parse_page_range(text: &str, page_count: usize) -> Option<Vec<usize>> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let skip_ws = |i: &mut usize| {
        while b.get(*i).is_some_and(u8::is_ascii_whitespace) {
            *i += 1;
        }
    };
    let number = |i: &mut usize| -> Option<usize> {
        skip_ws(i);
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        if *i == start || *i - start > 6 {
            return None;
        }
        text.get(start..*i)?.parse().ok()
    };
    let mut pages = BTreeSet::new();
    skip_ws(&mut i);
    if i >= b.len() {
        return None;
    }
    while i < b.len() {
        let a = number(&mut i)?;
        let mut e = a;
        skip_ws(&mut i);
        if b.get(i) == Some(&b'-') {
            i += 1;
            e = number(&mut i)?;
        }
        if a < 1 || e < a || e > page_count {
            return None;
        }
        pages.extend((a..=e).map(|p| p - 1));
        skip_ws(&mut i);
        if i < b.len() {
            if b.get(i) != Some(&b',') {
                return None;
            }
            i += 1;
            skip_ws(&mut i);
            if i >= b.len() {
                return None;
            }
        }
    }
    if pages.is_empty() {
        return None;
    }
    Some(pages.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format_value;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn line() -> Vec<Point> {
        // Revu: 6'-10 3/4" at 1/8" = 1'-0"
        vec![p(898.0167, 516.9689), p(960.1064, 516.9689)]
    }

    fn poly() -> Vec<Point> {
        // Revu: 1,360.53 sf at 1/8" = 1'-0"
        [
            (428.6077, 191.686),
            (428.6077, 169.811),
            (425.5742, 164.2861),
            (854.5376, 162.9998),
            (847.946, 193.3794),
            (998.7551, 186.5286),
            (996.8726, 475.3798),
            (726.2241, 475.1501),
            (726.2241, 493.158),
            (663.7778, 487.6912),
            (663.1968, 216.447),
            (560.1814, 220.1986),
            (563.4976, 185.8866),
        ]
        .iter()
        .map(|(x, y)| p(*x, *y))
        .collect()
    }

    #[test]
    fn custom_scale_equals_architectural() {
        let arch = Scale::architectural(0.125, 1.0);
        let custom = custom_scale(0.125, LengthUnit::Inch, 1.0, LengthUnit::Foot, None, None);
        assert!((custom.x_conv() - arch.x_conv()).abs() < 1e-12);
        assert_eq!(
            format_value(custom.length_of(&line(), false), &custom.dist),
            "6'-10 3/4\""
        );
        assert_eq!(custom.ratio, "0.125 in = 1 ft' in\"");
    }

    #[test]
    fn precision_fraction_and_decimal() {
        let arch = Scale::architectural(0.125, 1.0);
        let mut p16 = arch.clone();
        set_precision(&mut p16, Precision::fraction(16));
        assert_eq!(format_value(p16.length_of(&line(), false), &p16.dist), "6'-10 13/16\"");
        let mut p2 = arch.clone();
        set_precision(&mut p2, Precision::decimals(2));
        assert_eq!(format_value(p2.length_of(&line(), false), &p2.dist), "6'-10.79\"");
        assert_eq!(Precision::fraction(16).name(), "1/16");
        assert_eq!(Precision::decimals(3).name(), "0.001");
        assert_eq!(Precision::decimals(0).name(), "1");
        assert_eq!(scale_precision(&arch).name(), "1/4");
        assert_eq!(precision_choices(true).len(), 12);
        assert_eq!(precision_choices(false).len(), 5);
    }

    #[test]
    fn display_units_keep_the_ratio() {
        let arch = Scale::architectural(0.125, 1.0);
        let mut ft = arch.clone();
        assert!(set_display_unit(&mut ft, DisplayUnit::single(LengthUnit::Foot)));
        assert_eq!(format_value(ft.length_of(&line(), false), &ft.dist), "6.9 ft");
        let mut m = arch.clone();
        set_display_unit(&mut m, DisplayUnit::single(LengthUnit::Meter));
        assert_eq!(format_value(m.length_of(&line(), false), &m.dist), "2.1 m");
        assert_eq!(format_value(m.area_of(&poly()), &m.area), "126.4 m\u{b2}");
        let mut back = m.clone();
        set_display_unit(&mut back, DisplayUnit::FEET_INCHES);
        assert_eq!(
            format_value(back.length_of(&line(), false), &back.dist),
            format_value(arch.length_of(&line(), false), &arch.dist)
        );
        assert_eq!(format_value(back.area_of(&poly()), &back.area), "1,360.53 sf");
        assert_eq!(scale_display_unit(&arch), Some(DisplayUnit::FEET_INCHES));
        assert_eq!(scale_display_unit(&m).map(|d| d.name()), Some("m".to_string()));
        assert_eq!(display_unit_choices().len(), 9);
        let mut unknown = Scale::default();
        assert!(!set_display_unit(&mut unknown, DisplayUnit::FEET_INCHES));
    }

    #[test]
    fn metric_ratios() {
        let r100 = metric_ratio(100.0, LengthUnit::Meter);
        let l = [p(0.0, 0.0), p(72.0, 0.0)];
        assert!((r100.length_of(&l, false) - 2.54).abs() < 1e-9);
        assert_eq!(format_value(r100.length_of(&l, false), &r100.dist), "2.54 m");
        let sq = [p(0.0, 0.0), p(72.0, 0.0), p(72.0, 72.0), p(0.0, 72.0)];
        assert!((r100.area_of(&sq) - 2.54 * 2.54).abs() < 1e-9);
        let r50mm = metric_ratio(50.0, LengthUnit::Millimeter);
        assert_eq!(format_value(r50mm.length_of(&l, false), &r50mm.dist), "1,270 mm");
        // Mixed systems: 1 mm of paper = 1 ft real.
        let mixed = custom_scale(
            1.0,
            LengthUnit::Millimeter,
            1.0,
            LengthUnit::Foot,
            Some(DisplayUnit::single(LengthUnit::Foot)),
            None,
        );
        assert!((mixed.length_of(&l, false) - 25.4).abs() < 1e-9);
        assert!((LengthUnit::Foot.meters() / LengthUnit::Inch.meters() - 12.0).abs() < 1e-12);
    }

    #[test]
    fn presets_in_ui_order() {
        let ps = scale_presets();
        assert_eq!(ps.len(), 11 + 7 + 17);
        assert_eq!(ps.first().map(|p| p.name.as_str()), Some("1/16\" = 1'-0\""));
        assert_eq!(ps.get(11).map(|p| p.name.as_str()), Some("1\" = 10'"));
        let r100 = preset("1:100").unwrap_or_default();
        assert!((r100.length_of(&[p(0.0, 0.0), p(72.0, 0.0)], false) - 2.54).abs() < 1e-9);
        let eng = preset("1\" = 20'").unwrap_or_default();
        assert!((eng.length_of(&[p(0.0, 0.0), p(72.0, 0.0)], false) - 20.0).abs() < 1e-9);
        let q = preset("1/4\" = 1'-0\"").unwrap_or_default();
        assert!((q.length_of(&[p(0.0, 0.0), p(18.0, 0.0)], false) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn calibration_any_unit() {
        // 90 pt measured = 10 ft: 1/8" = 1'-0".
        let s = calibrate(p(0.0, 0.0), p(90.0, 0.0), 10.0, LengthUnit::Foot, None, None).unwrap_or_default();
        let arch = Scale::architectural(0.125, 1.0);
        assert!((s.x_conv() - arch.x_conv()).abs() < 1e-12);
        assert_eq!(s.ratio, "0.125 in = 1 ft' in\"");
        assert_eq!(format_value(s.length_of(&line(), false), &s.dist), "6'-10 3/4\"");
        // The same line typed in inches and meters gives the same real length.
        let si = calibrate(p(0.0, 0.0), p(0.0, 90.0), 120.0, LengthUnit::Inch, None, None).unwrap_or_default();
        assert!((si.length_of(&[p(0.0, 0.0), p(45.0, 0.0)], false) - 60.0).abs() < 1e-9);
        let sm = calibrate(p(10.0, 10.0), p(82.0, 10.0), 2.54, LengthUnit::Meter, None, None).unwrap_or_default();
        assert_eq!(sm.ratio, "10 mm = 1 m");
        assert_eq!(
            format_value(sm.length_of(&[p(0.0, 0.0), p(72.0, 0.0)], false), &sm.dist),
            "2.54 m"
        );
        assert!(calibrate(p(1.0, 1.0), p(1.0, 1.0), 10.0, LengthUnit::Foot, None, None).is_none());
        assert!(calibrate(p(0.0, 0.0), p(1.0, 1.0), 0.0, LengthUnit::Foot, None, None).is_none());
    }

    #[test]
    fn labels_and_units() {
        assert_eq!(LengthUnit::from_label("sf"), Some(LengthUnit::Foot));
        assert_eq!(LengthUnit::from_label("m\u{b2}"), Some(LengthUnit::Meter));
        assert_eq!(LengthUnit::from_label("cu yd"), Some(LengthUnit::Yard));
        assert_eq!(LengthUnit::from_label("ea"), None);
        assert_eq!(LengthUnit::Foot.area_label(), "sf");
        assert_eq!(LengthUnit::Meter.volume_label(), "m\u{b3}");
        assert_eq!(LengthUnit::Millimeter.name(), "Millimeters");
        assert_eq!(paper_units().len(), 4);
        assert!((LengthUnit::Yard.convert(1.0, LengthUnit::Inch) - 36.0).abs() < 1e-12);
    }

    #[test]
    fn page_ranges() {
        assert_eq!(parse_page_range("1-3, 5, 9", 10), Some(vec![0, 1, 2, 4, 8]));
        for bad in ["", "0", "3-1", "11", "2,", "a", "1-", "1--2", "   ", "1234567"] {
            assert_eq!(parse_page_range(bad, 10), None, "{bad:?} should be rejected");
        }
        assert_eq!(parse_page_range(" 2 ,2-3 ", 10), Some(vec![1, 2]));
    }
}
