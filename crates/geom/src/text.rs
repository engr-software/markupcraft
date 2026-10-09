//! Text box layout with the standard 14 fonts' advance widths: word wrap, alignment, Revu's
//! margins and autosize rule, and stamp lines.

use crate::Rect;

/// The base-14 family a CSS font name maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontFamily {
    #[default]
    Helvetica,
    Times,
    Courier,
}

impl FontFamily {
    /// Times for serif names, Courier for monospaced ones, else Helvetica.
    pub fn of(name: &str) -> FontFamily {
        let f = name.to_ascii_lowercase();
        if f.contains("times") || f.starts_with("serif") || f.contains("georgia") {
            FontFamily::Times
        } else if f.contains("courier") || f.contains("mono") {
            FontFamily::Courier
        } else {
            FontFamily::Helvetica
        }
    }
}

/// A font as the layout needs it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Font {
    pub family: FontFamily,
    pub bold: bool,
    pub italic: bool,
    /// points
    pub size: f64,
}

impl Font {
    /// The base-14 `/BaseFont` name.
    pub fn base_name(&self) -> &'static str {
        let (b, i) = (self.bold, self.italic);
        match self.family {
            FontFamily::Times => match (b, i) {
                (true, true) => "Times-BoldItalic",
                (true, false) => "Times-Bold",
                (false, true) => "Times-Italic",
                _ => "Times-Roman",
            },
            FontFamily::Courier => match (b, i) {
                (true, true) => "Courier-BoldOblique",
                (true, false) => "Courier-Bold",
                (false, true) => "Courier-Oblique",
                _ => "Courier",
            },
            FontFamily::Helvetica => match (b, i) {
                (true, true) => "Helvetica-BoldOblique",
                (true, false) => "Helvetica-Bold",
                (false, true) => "Helvetica-Oblique",
                _ => "Helvetica",
            },
        }
    }

    /// The font resource name used in `/DA` and appearance streams, without the slash:
    /// `Helv`, `TiRo`, `Cour`, `HeBo`, `TiIt`, `CoBI` ...
    pub fn res_name(&self) -> &'static str {
        match (self.family, self.bold, self.italic) {
            (FontFamily::Helvetica, false, false) => "Helv",
            (FontFamily::Times, false, false) => "TiRo",
            (FontFamily::Courier, false, false) => "Cour",
            (FontFamily::Helvetica, true, false) => "HeBo",
            (FontFamily::Helvetica, false, true) => "HeIt",
            (FontFamily::Helvetica, true, true) => "HeBI",
            (FontFamily::Times, true, false) => "TiBo",
            (FontFamily::Times, false, true) => "TiIt",
            (FontFamily::Times, true, true) => "TiBI",
            (FontFamily::Courier, true, false) => "CoBo",
            (FontFamily::Courier, false, true) => "CoIt",
            (FontFamily::Courier, true, true) => "CoBI",
        }
    }

    pub fn with_size(self, size: f64) -> Font {
        Font { size, ..self }
    }
}

// Advance widths (1/1000 em) of ASCII 32..126 from the standard 14 fonts' AFM metrics.
#[rustfmt::skip]
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778,
    722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278,
    278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584];
#[rustfmt::skip]
const HELV_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778,
    722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333,
    278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584];
#[rustfmt::skip]
const TIMES: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611, 556, 722,
    722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333,
    278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
    500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541];

fn char_width(c: char, f: &Font) -> u16 {
    let table = match f.family {
        FontFamily::Courier => return 600,
        FontFamily::Times => &TIMES,
        FontFamily::Helvetica if f.bold => &HELV_BOLD,
        FontFamily::Helvetica => &HELV,
    };
    let fallback = if f.family == FontFamily::Times { 500 } else { 556 };
    (c as usize)
        .checked_sub(32)
        .and_then(|i| table.get(i))
        .copied()
        .unwrap_or(fallback)
}

/// Width of `s` in points (approximate for characters outside ASCII).
pub fn text_width(s: &str, f: &Font) -> f64 {
    let units: f64 = s.chars().map(|c| f64::from(char_width(c, f))).sum();
    units * f.size / 1000.0
}

/// `s` in WinAnsiEncoding for a base-14 font's `Tj`: each char of the result is one byte
/// (U+0000..U+00FF); characters WinAnsi lacks become `?`.
pub fn to_win_ansi(s: &str) -> String {
    s.chars()
        .map(|c| match c as u32 {
            0..=0x7F | 0xA0..=0xFF => c,
            0x20AC => '\u{80}',
            0x2026 => '\u{85}',
            0x2018 => '\u{91}',
            0x2019 => '\u{92}',
            0x201C => '\u{93}',
            0x201D => '\u{94}',
            0x2022 => '\u{95}',
            0x2013 => '\u{96}',
            0x2014 => '\u{97}',
            0x2122 => '\u{99}',
            _ => '?',
        })
        .collect()
}

/// Paragraphs of `/Contents`: split on `\r\n`, `\r` or `\n`.
pub fn paragraphs(c: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut it = c.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\r' || ch == '\n' {
            if ch == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            out.push(String::new());
        } else if let Some(last) = out.last_mut() {
            last.push(ch);
        }
    }
    out
}

/// One placed line of text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLine {
    pub text: String,
    /// baseline start, PDF space
    pub x: f64,
    pub y: f64,
    pub width: f64,
}

/// Revu's line height: 1.15 x the font size.
pub fn line_height(size: f64) -> f64 {
    1.15 * size
}

/// Border plus margin, Revu's text inset: line width + 4.
pub fn text_inset(line_width: f64) -> f64 {
    line_width + 4.0
}

/// Most lines one box lays out (a guard against absurd input).
const MAX_LINES: usize = 10_000;

/// The byte index of the longest prefix of `s` (at a char boundary, at least one char) that
/// fits in `width`.
fn fit_prefix(s: &str, width: f64, f: &Font) -> usize {
    let mut w = 0.0;
    let mut end = 0;
    for (i, c) in s.char_indices() {
        w += f64::from(char_width(c, f)) * f.size / 1000.0;
        if w > width && i > 0 {
            return i;
        }
        end = i + c.len_utf8();
    }
    end
}

/// Greedy word wrap of one paragraph into lines no wider than `width`.
fn wrap(para: &str, width: f64, f: &Font) -> Vec<String> {
    let width = width + 0.02; // Revu autosizes boxes to exactly text + margins; allow the rounding
    if para.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut rest = para;
    while !rest.is_empty() && lines.len() < MAX_LINES {
        // next word with its leading spaces
        let spaces = rest.len() - rest.trim_start_matches(' ').len();
        let word_end = rest
            .get(spaces..)
            .and_then(|t| t.find(' '))
            .map_or(rest.len(), |k| spaces + k);
        let (word, tail) = rest.split_at(word_end);
        rest = tail;
        let trial = format!("{cur}{word}");
        if cur.is_empty() || text_width(&trial, f) <= width {
            cur = trial;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.trim_start_matches(' ').to_string();
        }
        // a single word wider than the box: break it by characters
        while text_width(&cur, f) > width && cur.chars().nth(1).is_some() && lines.len() < MAX_LINES {
            let cut = fit_prefix(&cur, width, f);
            let tail = cur.split_off(cut);
            lines.push(std::mem::replace(&mut cur, tail));
        }
    }
    lines.push(cur);
    lines
}

/// The wrapped lines of `contents` in a box `box_width` wide.
pub fn wrapped_lines(contents: &str, f: &Font, box_width: f64, inset: f64) -> Vec<String> {
    let avail = (box_width - 2.0 * inset).max(1.0);
    let mut out: Vec<String> = Vec::new();
    for p in paragraphs(contents) {
        out.extend(wrap(&p, avail, f));
        if out.len() >= MAX_LINES {
            break;
        }
    }
    // Revu keeps trailing empty paragraphs in /Contents but they take no space at the end.
    while out.len() > 1 && out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

/// Wrap `contents` into box `b` and place each line: `align` 0 left, 1 centre, 2 right;
/// first baseline at top - inset - 0.7762 x size (Revu's).
pub fn layout_text(b: Rect, contents: &str, f: &Font, align: i32, inset: f64) -> Vec<TextLine> {
    layout_text_spaced(b, contents, f, align, inset, 1.0)
}

/// [`layout_text`] with lines `spacing` times Revu's line height apart.
pub fn layout_text_spaced(b: Rect, contents: &str, f: &Font, align: i32, inset: f64, spacing: f64) -> Vec<TextLine> {
    let b = b.normalized();
    let lh = line_height(f.size) * spacing_of(spacing);
    let mut y = b.y1 - inset - 0.7762 * f.size;
    let mut out = Vec::new();
    for l in wrapped_lines(contents, f, b.x1 - b.x0, inset) {
        let width = text_width(&l, f);
        let x = match align {
            1 => (b.x0 + b.x1 - width) / 2.0,
            2 => b.x1 - inset - width,
            _ => b.x0 + inset,
        };
        out.push(TextLine { text: l, x, y, width });
        y -= lh;
    }
    out
}

/// Height a box `box_width` wide needs for its text (Revu's autosize rule:
/// 2 x inset + (lines - 1) x line height + one font size).
pub fn text_height_for(contents: &str, f: &Font, box_width: f64, inset: f64) -> f64 {
    text_height_spaced(contents, f, box_width, inset, 1.0)
}

/// [`text_height_for`] with lines `spacing` times Revu's line height apart.
pub fn text_height_spaced(contents: &str, f: &Font, box_width: f64, inset: f64, spacing: f64) -> f64 {
    let n = wrapped_lines(contents, f, box_width, inset).len().max(1);
    2.0 * inset + (n - 1) as f64 * line_height(f.size) * spacing_of(spacing) + f.size
}

/// A usable line spacing (0.5 to 5; anything else is single).
pub fn spacing_of(spacing: f64) -> f64 {
    if spacing.is_finite() && (0.5..=5.0).contains(&spacing) {
        spacing
    } else {
        1.0
    }
}

/// Width of the widest paragraph, unwrapped.
pub fn widest_paragraph(contents: &str, f: &Font) -> f64 {
    paragraphs(contents)
        .iter()
        .map(|p| text_width(p, f))
        .fold(0.0, f64::max)
}

/// Lines of a stamp in box `b`: the first paragraph of `contents` big, the rest small, all
/// centred. Returns each line with its font size.
pub fn stamp_lines(b: Rect, contents: &str, f: &Font, line_width: f64) -> Vec<(TextLine, f64)> {
    let b = b.normalized();
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    let pad = line_width + 6.0;
    let mut paras = paragraphs(contents);
    while paras.len() > 1 && paras.last().is_some_and(String::is_empty) {
        paras.pop();
    }
    paras.truncate(MAX_LINES);
    let unit = f.with_size(1.0);
    let n = paras.len();
    let mut main = h * if n > 1 { 0.42 } else { 0.55 };
    let first = paras.first().map_or(0.0, |p| text_width(p, &unit));
    if first > 0.0 {
        main = main.min((w - 2.0 * pad) / first);
    }
    let mut sub = main * 0.4;
    for p in paras.iter().skip(1) {
        let wi = text_width(p, &unit);
        if wi > 0.0 {
            sub = sub.min((w - 2.0 * pad) / wi);
        }
    }
    let main = main.max(1.0);
    let sub = sub.max(1.0);
    let total = 0.72 * main + n.saturating_sub(1) as f64 * 1.25 * sub;
    let mut y = (b.y0 + b.y1) / 2.0 + total / 2.0 - 0.72 * main;
    let mut out = Vec::new();
    for (i, p) in paras.into_iter().enumerate() {
        let size = if i == 0 { main } else { sub };
        let width = text_width(&p, &unit) * size;
        if i > 0 {
            y -= 1.25 * sub;
        }
        out.push((
            TextLine {
                text: p,
                x: (b.x0 + b.x1 - width) / 2.0,
                y,
                width,
            },
            size,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn helv(size: f64) -> Font {
        Font {
            size,
            ..Default::default()
        }
    }

    #[test]
    fn wraps_like_revu() {
        // a Revu box 85.8 wide, border 2: Revu breaks after 3'-0"
        let b = Rect::new(1038.114, 534.3577, 1123.914, 572.1577);
        let l = layout_text(b, "10'-0\" by 3'-0\" intake louver", &helv(12.0), 0, text_inset(2.0));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "10'-0\" by 3'-0\"");
        assert!((l[0].x - 1044.114).abs() < 1e-3);
        assert!((l[0].y - 556.8431).abs() < 2e-3);
        let h = text_height_for("10'-0\" by 3'-0\" intake louver", &helv(12.0), 85.8, text_inset(2.0));
        assert!((h - 37.8).abs() < 1e-6);
    }

    #[test]
    fn exact_width_stays_on_one_line() {
        let b = Rect::new(463.9644, 525.0919, 585.3604, 560.8919);
        let l = layout_text(b, "SPF Fan Suspended\r17,000 cfm", &helv(12.0), 0, text_inset(1.0));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "SPF Fan Suspended");
        assert!((text_width("SPF Fan Suspended", &helv(12.0)) - 111.396).abs() < 1e-3);
    }

    #[test]
    fn long_word_breaks_by_characters() {
        let l = wrapped_lines("ABCDEFGHIJKLMNOPQRSTUVWXYZ", &helv(12.0), 60.0, 5.0);
        assert!(l.len() > 2);
        assert_eq!(l.concat(), "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        assert!(l.iter().all(|s| !s.is_empty()));
        let l = wrapped_lines("ééééééééééé", &helv(40.0), 1.0, 5.0);
        assert_eq!(l.concat(), "ééééééééééé");
    }

    #[test]
    fn alignment_and_paragraphs() {
        assert_eq!(paragraphs("a\r\nb\rc\nd"), vec!["a", "b", "c", "d"]);
        let b = Rect::new(0.0, 0.0, 200.0, 100.0);
        let c = layout_text(b, "hi", &helv(10.0), 1, 5.0);
        let r = layout_text(b, "hi", &helv(10.0), 2, 5.0);
        assert!((c[0].x + c[0].width / 2.0 - 100.0).abs() < 1e-9);
        assert!((r[0].x + r[0].width - 195.0).abs() < 1e-9);
        assert_eq!(wrapped_lines("a\r\r", &helv(10.0), 100.0, 5.0).len(), 1);
    }

    #[test]
    fn fonts() {
        let f = Font {
            family: FontFamily::of("Times New Roman"),
            bold: true,
            ..Default::default()
        };
        assert_eq!(f.base_name(), "Times-Bold");
        assert_eq!(f.res_name(), "TiBo");
        assert_eq!(FontFamily::of("Consolas Mono"), FontFamily::Courier);
        assert_eq!(to_win_ansi("caf\u{e9} \u{2013} x\u{4e00}"), "caf\u{e9} \u{96} x?");
    }

    #[test]
    fn stamp_lines_are_centred() {
        let l = stamp_lines(Rect::new(0.0, 0.0, 170.0, 60.0), "APPROVED\rby me", &helv(1.0), 2.5);
        assert_eq!(l.len(), 2);
        assert!(l[0].1 > l[1].1);
        assert!((l[0].0.x + l[0].0.width / 2.0 - 85.0).abs() < 1e-9);
    }
}
