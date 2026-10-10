//! Scales and number formatting, following the PDF `/Measure /RL` dictionary (ISO 32000-1 §12.9).
//!
//! Revu stores every measurement's scale this way, so the same structure serves reading,
//! computing and writing. Formatting matches what Revu writes into `/Contents`
//! (checked on 410 Revu-made measurements: fractions always reduced, trailing zeros stripped).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod units;

use markupcraft_geom::{Point, polygon_area, polyline_length};
use serde::{Deserialize, Serialize};

/// `/F` of a number format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fmt {
    /// `/D` decimal
    Decimal,
    /// `/F` fraction
    Fraction,
    /// `/R` round to integer
    Round,
    /// `/T` truncate
    Truncate,
}

impl Fmt {
    pub fn from_name(n: &str) -> Fmt {
        match n.as_bytes().first() {
            Some(b'F') => Fmt::Fraction,
            Some(b'R') => Fmt::Round,
            Some(b'T') => Fmt::Truncate,
            _ => Fmt::Decimal,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Fmt::Decimal => "D",
            Fmt::Fraction => "F",
            Fmt::Round => "R",
            Fmt::Truncate => "T",
        }
    }
}

/// One element of a number-format array (a `/NumberFormat` dictionary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumberFormat {
    /// `/U` label, e.g. `'` `"` `sf`
    pub unit: String,
    /// `/C` multiply the previous unit by this to get this unit
    pub conv: f64,
    /// `/F`
    pub fmt: Fmt,
    /// `/D` decimal: 10^n precision; fraction: denominator
    pub den: i64,
    /// `/FD` keep the fraction denominator as is
    pub no_reduce: bool,
    /// `/RT`
    pub thousands: String,
    /// `/RD`
    pub decimal: String,
    /// `/PS` text before the label
    pub prefix: String,
    /// `/SS` text after the label
    pub suffix: String,
    /// `/O /P`: label before the number
    pub label_first: bool,
}

impl Default for NumberFormat {
    fn default() -> Self {
        Self {
            unit: String::new(),
            conv: 1.0,
            fmt: Fmt::Decimal,
            den: 100,
            no_reduce: false,
            thousands: ",".into(),
            decimal: ".".into(),
            prefix: " ".into(),
            suffix: " ".into(),
            label_first: false,
        }
    }
}

impl NumberFormat {
    /// The shape MarkupCraft writes for new scales (`/FD true`, explicit `/PS` `/SS`).
    pub fn new(unit: &str, conv: f64, fmt: Fmt, den: i64, prefix: &str, suffix: &str) -> Self {
        Self {
            unit: unit.into(),
            conv,
            fmt,
            den,
            no_reduce: true,
            prefix: prefix.into(),
            suffix: suffix.into(),
            ..Default::default()
        }
    }
}

pub type FormatArray = Vec<NumberFormat>;

/// A `/Measure /RL` dictionary.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Scale {
    /// `/R` human text, e.g. `0.125 in = 1 ft' in"`
    pub ratio: String,
    /// `/X` user-space units to x units
    pub x: FormatArray,
    /// `/Y` optional, when y scales differently
    pub y: FormatArray,
    /// `/D` distance
    pub dist: FormatArray,
    /// `/A` area
    pub area: FormatArray,
    /// `/V` volume (Revu)
    pub volume: FormatArray,
    /// Revu `/TargetUnitConversion`
    pub target_unit_conversion: Option<f64>,
}

impl Scale {
    pub fn valid(&self) -> bool {
        self.x.first().is_some_and(|f| f.conv != 0.0 && f.conv.is_finite())
    }
    pub fn x_conv(&self) -> f64 {
        self.x.first().map_or(1.0, |f| f.conv)
    }
    pub fn y_conv(&self) -> f64 {
        self.y.first().map_or_else(|| self.x_conv(), |f| f.conv)
    }

    fn scaled(&self, pts: &[Point]) -> Vec<Point> {
        let (sx, sy) = (self.x_conv(), self.y_conv());
        pts.iter().map(|p| Point::new(p.x * sx, p.y * sy)).collect()
    }

    /// Raw geometry (PDF points) to a value in the first unit of `/D`.
    pub fn length_of(&self, pts: &[Point], closed: bool) -> f64 {
        let d = self.dist.first().map_or(1.0, |f| f.conv);
        polyline_length(&self.scaled(pts), closed) * d
    }

    /// Raw geometry (PDF points) to a value in the first unit of `/A`.
    pub fn area_of(&self, pts: &[Point]) -> f64 {
        let a = self.area.first().map_or(1.0, |f| f.conv);
        polygon_area(&self.scaled(pts)) * a
    }

    /// Revu's own layout for feet-inches drawings (seen in real Revu files).
    pub fn feet_inches(pt_to_feet: f64, ratio: &str) -> Scale {
        Scale {
            ratio: ratio.into(),
            x: vec![NumberFormat::new("'", pt_to_feet, Fmt::Fraction, 4, "", "")],
            dist: vec![
                NumberFormat::new("'", 1.0, Fmt::Fraction, 4, "", "-"),
                NumberFormat::new("\"", 12.0, Fmt::Fraction, 4, "", ""),
            ],
            area: vec![NumberFormat::new("sf", 1.0, Fmt::Decimal, 100, " ", "")],
            volume: vec![NumberFormat::new("cu ft", 1.0, Fmt::Decimal, 100, " ", "")],
            ..Default::default()
        }
    }

    /// `paper_inches` of drawing = `real_feet` in the field.
    pub fn architectural(paper_inches: f64, real_feet: f64) -> Scale {
        let pt_to_feet = real_feet / (paper_inches * 72.0);
        Scale::feet_inches(
            pt_to_feet,
            &format!("{} in = {} ft' in\"", fmt_g(paper_inches, 6), fmt_g(real_feet, 6)),
        )
    }

    /// From a calibration: a line of `points` length on the sheet is `real_feet` long.
    pub fn calibrated(points: f64, real_feet: f64) -> Scale {
        let pt_to_feet = real_feet / points;
        Scale::feet_inches(
            pt_to_feet,
            &format!("{} in = 1 ft' in\"", fmt_g(1.0 / (pt_to_feet * 72.0), 4)),
        )
    }

    /// One drawing inch = `real_feet_per_inch`; decimal feet display.
    pub fn engineering(real_feet_per_inch: f64) -> Scale {
        Scale {
            ratio: format!("1 in = {} ft", fmt_g(real_feet_per_inch, 6)),
            x: vec![NumberFormat::new(
                "ft",
                real_feet_per_inch / 72.0,
                Fmt::Decimal,
                100,
                " ",
                "",
            )],
            dist: vec![NumberFormat::new("ft", 1.0, Fmt::Decimal, 100, " ", "")],
            area: vec![NumberFormat::new("sf", 1.0, Fmt::Decimal, 100, " ", "")],
            volume: vec![NumberFormat::new("cu ft", 1.0, Fmt::Decimal, 100, " ", "")],
            ..Default::default()
        }
    }
}

/// C's `%.{prec}g`.
pub fn fmt_g(v: f64, prec: usize) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    if v == 0.0 {
        return "0".into();
    }
    let prec = prec.max(1);
    let exp = v.abs().log10().floor() as i32;
    if exp < -4 || exp >= prec as i32 {
        let s = format!("{:.*e}", prec - 1, v);
        // Rust writes 1.5e3; C writes 1.5e+03. Trim mantissa zeros, then format the exponent.
        match s.split_once('e') {
            Some((m, e)) => {
                let m = trim_zeros(m);
                let e: i32 = e.parse().unwrap_or(0);
                format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
            }
            None => s,
        }
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{v:.decimals$}")).to_string()
    }
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

fn group_thousands(v: i64, sep: &str) -> String {
    let digits = v.unsigned_abs().to_string();
    let sign = if v < 0 { "-" } else { "" };
    if sep.is_empty() {
        return format!("{sign}{digits}");
    }
    let n = digits.len();
    let mut out = String::from(sign);
    for (i, c) in digits.chars().enumerate() {
        out.push(c);
        let left = n - i - 1;
        if left > 0 && left.is_multiple_of(3) {
            out.push_str(sep);
        }
    }
    out
}

fn with_label(num: &str, f: &NumberFormat) -> String {
    if f.unit.is_empty() {
        return num.to_string();
    }
    if f.label_first {
        format!("{}{}{}{}", f.prefix, f.unit, f.suffix, num)
    } else {
        format!("{}{}{}{}", num, f.prefix, f.unit, f.suffix)
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// Values from this magnitude up have no fractional digits left in an `f64`, and the whole
/// part no longer fits the integer arithmetic below: they are shown as whole numbers, capped.
const MAX_FORMATTED: f64 = 1e15;

/// `v` is already rounded to this unit's precision and non-negative.
fn format_last(v: f64, f: &NumberFormat) -> String {
    // A damaged /Measure (a huge conversion factor or precision) can make any value huge.
    if !v.is_finite() || v.abs() >= MAX_FORMATTED {
        let w = if v.is_nan() {
            0
        } else {
            v.clamp(-MAX_FORMATTED, MAX_FORMATTED) as i64
        };
        return group_thousands(w, &f.thousands);
    }
    match f.fmt {
        Fmt::Fraction => {
            let den = f.den.max(1);
            let mut whole = (v + 1e-9).floor() as i64;
            let mut num = ((v - whole as f64) * den as f64).round() as i64;
            if num >= den {
                whole += 1;
                num -= den;
            }
            let mut s = String::new();
            if whole != 0 || num == 0 {
                s = group_thousands(whole, &f.thousands);
            }
            if num != 0 {
                // Revu always reduces (2/4 -> 1/2) even with /FD true; match Revu.
                let g = gcd(num, den);
                if !s.is_empty() {
                    s.push(' ');
                }
                s += &format!("{}/{}", num / g, den / g);
            }
            s
        }
        Fmt::Round | Fmt::Truncate => group_thousands(v as i64, &f.thousands),
        Fmt::Decimal => {
            let digits = if f.den > 1 {
                (f.den as f64).log10().round().clamp(0.0, 12.0) as u32
            } else {
                0
            };
            let mut whole = v.floor() as i64;
            let frac = v - whole as f64;
            let scale = 10i64.pow(digits);
            let mut fi = (frac * scale as f64).round() as i64;
            if digits > 0 && fi >= scale {
                whole += 1;
                fi -= scale;
            }
            let mut s = group_thousands(whole, &f.thousands);
            if digits > 0 {
                let fs = format!("{:0width$}", fi, width = digits as usize);
                // Revu strips trailing zeros (247.1 sf) even with /FD true.
                let fs = fs.trim_end_matches('0');
                if !fs.is_empty() {
                    s += &f.decimal;
                    s += fs;
                }
            }
            s
        }
    }
}

/// Format a value expressed in `fa[0]`'s unit, per the spec's multi-unit rules
/// (6.8988 ft with `[ft, in 1/4]` gives `6'-10 3/4"`).
pub fn format_value(v: f64, fa: &[NumberFormat]) -> String {
    let Some(last) = fa.last() else {
        return format!("{v:.2}");
    };
    if !v.is_finite() {
        return String::new();
    }
    let n = fa.len();
    let neg = v < 0.0;
    let v = v.abs();

    // Work in the smallest unit, rounded to its precision, then split upward.
    let mult: f64 = fa.iter().skip(1).map(|f| f.conv).product();
    let mut total = v * mult;
    let step = match last.fmt {
        Fmt::Fraction | Fmt::Decimal => 1.0 / last.den.max(1) as f64,
        _ => 1.0,
    };
    if last.fmt == Fmt::Truncate {
        total = total.floor();
    } else {
        // Half-way values (71.875 to 0.01) must round the same on every platform: nudge before
        // rounding so 7187.4999999 from binary floating point still rounds up.
        total = (total / step + 0.5 + 1e-7).floor() * step;
    }

    let mut out = String::new();
    let mut rem = total;
    for (i, f) in fa.iter().enumerate().take(n.saturating_sub(1)) {
        let below: f64 = fa.iter().skip(i + 1).map(|f| f.conv).product();
        if below == 0.0 || !below.is_finite() {
            break;
        }
        let whole = (rem / below + 1e-9).floor() as i64;
        rem -= whole as f64 * below;
        if rem < 0.0 {
            rem = 0.0;
        }
        out += &with_label(&group_thousands(whole, &f.thousands), f);
    }
    out += &with_label(&format_last(rem, last), last);
    let out = out.trim_end_matches(' ').to_string();
    if neg { format!("-{out}") } else { out }
}

/// Parse a Revu label back to a number in the first unit (`1,360.53 sf`, `6'-10 3/4"`, `12 ea`).
/// Used only to compare against our own computation.
pub fn parse_label(s: &str) -> Option<f64> {
    if let Some(v) = parse_feet_inches(s) {
        return Some(v);
    }
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let digit_at = |k: usize| b.get(k).is_some_and(u8::is_ascii_digit);
        let starts = c.is_ascii_digit()
            || (c == b'.' && digit_at(i + 1))
            || ((c == b'-' || c == b'+') && (digit_at(i + 1) || (b.get(i + 1) == Some(&b'.') && digit_at(i + 2))));
        if starts {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b',' || b[j] == b'.') {
                j += 1;
            }
            let t: String = s.get(i..j)?.chars().filter(|c| *c != ',').collect();
            return t.trim_end_matches('.').parse().ok();
        }
        i += 1;
    }
    None
}

/// `12'-6 1/2"`, `12'`, `6 1/2"`, `1,234'-3"` to feet.
fn parse_feet_inches(s: &str) -> Option<f64> {
    let t = s.trim();
    if !t.contains('\'') && !t.contains('"') {
        return None;
    }
    let mut feet = 0.0;
    let mut rest = t;
    if let Some((f, r)) = t.split_once('\'') {
        let f = f.trim().replace(',', "");
        if f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        feet = f.parse::<f64>().ok()?;
        rest = r;
    }
    let rest = rest.trim().trim_start_matches(['-', ' ']).trim();
    if rest.is_empty() {
        return Some(feet);
    }
    let inner = rest.strip_suffix('"')?.trim();
    let mut inches = 0.0;
    for part in inner.split_whitespace() {
        if let Some((n, d)) = part.split_once('/') {
            let n: f64 = n.parse().ok()?;
            let d: f64 = d.parse().ok()?;
            if d == 0.0 {
                return None;
            }
            inches += n / d;
        } else {
            if !part.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            inches += part.parse::<f64>().ok()?;
        }
    }
    Some(feet + inches / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ft_in() -> FormatArray {
        Scale::architectural(0.125, 1.0).dist
    }

    #[test]
    fn feet_inches_quarter() {
        assert_eq!(format_value(6.8988, &ft_in()), "6'-10 3/4\"");
        assert_eq!(format_value(10.0, &ft_in()), "10'-0\"");
        assert_eq!(format_value(0.5, &ft_in()), "0'-6\"");
        // halves reduce even with /FD true; zero whole inches are omitted before a fraction
        assert_eq!(format_value(1.0 + 0.5 / 12.0, &ft_in()), "1'-1/2\"");
    }

    /// fuzz: a damaged /Measure made a quantity so large that formatting it overflowed an
    /// integer (a panic in debug builds, nonsense digits in release).
    #[test]
    fn huge_values_format_without_overflow() {
        let a = Scale::architectural(0.125, 1.0).area;
        for v in [9.3e18, 1e300, f64::MAX, -1e300] {
            let s = format_value(v, &a);
            assert!(s.ends_with(" sf") && s.contains("000,000,000"), "{v}: {s}");
        }
        assert_eq!(format_value(f64::INFINITY, &a), "");
        for v in [9.3e18, 1e300, f64::MAX] {
            assert!(!format_value(v, &ft_in()).is_empty());
        }
        let mut huge = ft_in();
        if let Some(f) = huge.get_mut(1) {
            f.conv = 1e308;
            f.den = i64::MAX;
        }
        let _ = format_value(10.0, &huge);
        let _ = format_value(1e300, &huge);
    }

    #[test]
    fn decimal_area_strips_zeros_and_groups() {
        let a = Scale::architectural(0.125, 1.0).area;
        assert_eq!(format_value(247.1, &a), "247.1 sf");
        assert_eq!(format_value(1360.5312, &a), "1,360.53 sf");
        assert_eq!(format_value(100.0, &a), "100 sf");
        assert_eq!(format_value(71.875, &a), "71.88 sf");
        assert_eq!(format_value(1234567.0, &a), "1,234,567 sf");
    }

    #[test]
    fn labels_parse_back() {
        assert_eq!(parse_label("1,360.53 sf"), Some(1360.53));
        assert_eq!(parse_label("6'-10 3/4\""), Some(6.0 + 10.75 / 12.0));
        assert_eq!(parse_label("12 ea"), Some(12.0));
        assert_eq!(parse_label("3/4\""), Some(0.75 / 12.0));
    }

    #[test]
    fn printf_g() {
        assert_eq!(fmt_g(0.125, 6), "0.125");
        assert_eq!(fmt_g(1.0, 6), "1");
        assert_eq!(fmt_g(0.1875, 4), "0.1875");
        assert_eq!(fmt_g(1234567.0, 6), "1.23457e+06");
    }

    #[test]
    fn scale_quantities() {
        let s = Scale::architectural(0.125, 1.0);
        // 90 pt at 1/8" = 1'-0" is 10 ft
        let line = [Point::new(0.0, 0.0), Point::new(90.0, 0.0)];
        assert!((s.length_of(&line, false) - 10.0).abs() < 1e-9);
        let sq = markupcraft_geom::Rect::new(0.0, 0.0, 90.0, 90.0).corners();
        assert!((s.area_of(&sq) - 100.0).abs() < 1e-9);
    }
}
