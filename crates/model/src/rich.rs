//! Rich text in text markups: runs of characters that differ from the markup's base text style
//! (bold, italic, underline, colour). Written as `<span style=...>` in `/RC`; char offsets
//! index `Markup::contents`.

use serde::{Deserialize, Serialize};

use crate::{Color, TextStyle};

/// Most runs one markup keeps.
pub const MAX_RUNS: usize = 10_000;

/// A run of characters `start..end` (char offsets into the contents) with its own style.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Color,
}

/// One character's style: the base, overridden by the run covering it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Color,
}

impl CharStyle {
    pub fn of(base: &TextStyle) -> Self {
        Self {
            bold: base.bold,
            italic: base.italic,
            underline: base.underline,
            color: base.color,
        }
    }
}

/// The style of char `i`.
pub fn style_at(base: &TextStyle, runs: &[TextRun], i: usize) -> CharStyle {
    runs.iter()
        .rev()
        .find(|r| r.start <= i && i < r.end)
        .map_or(CharStyle::of(base), |r| CharStyle {
            bold: r.bold,
            italic: r.italic,
            underline: r.underline,
            color: r.color,
        })
}

/// Segments of chars `from..to` with one style each: (start, end, style).
pub fn segments(base: &TextStyle, runs: &[TextRun], from: usize, to: usize) -> Vec<(usize, usize, CharStyle)> {
    let mut out: Vec<(usize, usize, CharStyle)> = Vec::new();
    if runs.is_empty() {
        if from < to {
            out.push((from, to, CharStyle::of(base)));
        }
        return out;
    }
    for i in from..to {
        let s = style_at(base, runs, i);
        match out.last_mut() {
            Some(last) if last.2 == s && last.1 == i => last.1 = i + 1,
            _ => out.push((i, i + 1, s)),
        }
    }
    out
}

/// Normalised runs: one per stretch of chars whose style differs from the base, in order.
pub fn normalize(base: &TextStyle, runs: &[TextRun], len: usize) -> Vec<TextRun> {
    let b = CharStyle::of(base);
    segments(base, runs, 0, len)
        .into_iter()
        .filter(|(_, _, s)| *s != b)
        .take(MAX_RUNS)
        .map(|(start, end, s)| TextRun {
            start,
            end,
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            color: s.color,
        })
        .collect()
}

/// What a style command changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StyleChange {
    Bold,
    Italic,
    Underline,
    Color(Color),
}

/// Apply `change` to chars `from..to`: Bold, Italic and Underline toggle (on unless every char
/// already has it), Color sets.
pub fn apply(
    base: &TextStyle,
    runs: &[TextRun],
    len: usize,
    from: usize,
    to: usize,
    change: StyleChange,
) -> Vec<TextRun> {
    let (from, to) = (from.min(len), to.min(len));
    if from >= to {
        return normalize(base, runs, len);
    }
    let all_on = |f: fn(&CharStyle) -> bool| (from..to).all(|i| f(&style_at(base, runs, i)));
    let target = match change {
        StyleChange::Bold => !all_on(|s| s.bold),
        StyleChange::Italic => !all_on(|s| s.italic),
        StyleChange::Underline => !all_on(|s| s.underline),
        StyleChange::Color(_) => true,
    };
    let mut per: Vec<CharStyle> = (0..len).map(|i| style_at(base, runs, i)).collect();
    for s in per.iter_mut().take(to).skip(from) {
        match change {
            StyleChange::Bold => s.bold = target,
            StyleChange::Italic => s.italic = target,
            StyleChange::Underline => s.underline = target,
            StyleChange::Color(c) => s.color = c,
        }
    }
    let one: Vec<TextRun> = per
        .iter()
        .enumerate()
        .map(|(i, s)| TextRun {
            start: i,
            end: i + 1,
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            color: s.color,
        })
        .collect();
    normalize(base, &one, len)
}

/// Keep runs on the same characters after the text changed from `old` to `new` (the edit is
/// the middle part between their common prefix and suffix; inserted chars take the style of
/// the char before them).
pub fn rebase(runs: &[TextRun], old: &str, new: &str) -> Vec<TextRun> {
    if runs.is_empty() || old == new {
        return runs.to_vec();
    }
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suf = a.len().min(b.len()) - pre;
    let suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suf)
        .take_while(|(x, y)| x == y)
        .count();
    let (old_mid_end, new_mid_end) = (a.len() - suf, b.len() - suf);
    let map = |i: usize, is_end: bool| -> usize {
        if i < pre || (i == pre && !is_end) {
            i
        } else if i >= old_mid_end {
            i - old_mid_end + new_mid_end
        } else if is_end {
            // inside the replaced part: an end stretches over the inserted chars
            new_mid_end
        } else {
            pre
        }
    };
    runs.iter()
        .filter_map(|r| {
            let (s, mut e) = (map(r.start, false), map(r.end, true));
            // typing right after a run continues it
            if r.end == pre && new_mid_end > pre && a.len() - suf == pre {
                e = new_mid_end;
            }
            (e > s).then_some(TextRun { start: s, end: e, ..*r })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> TextStyle {
        TextStyle::default()
    }

    #[test]
    fn toggle_and_merge() {
        let b = base();
        let r = apply(&b, &[], 10, 2, 5, StyleChange::Bold);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].start, r[0].end, r[0].bold), (2, 5, true));
        let r = apply(&b, &r, 10, 5, 7, StyleChange::Bold);
        assert_eq!((r[0].start, r[0].end), (2, 7), "adjacent bold merges");
        let r = apply(&b, &r, 10, 2, 7, StyleChange::Bold);
        assert!(r.is_empty(), "all bold toggles off");
        let r = apply(&b, &[], 4, 0, 4, StyleChange::Color(Color::BLACK));
        assert_eq!(r.len(), 1);
        assert_eq!(segments(&b, &r, 0, 4).len(), 1);
    }

    #[test]
    fn runs_follow_edits() {
        let b = base();
        let r = apply(&b, &[], 11, 6, 11, StyleChange::Italic); // "hello world": world italic
        let r2 = rebase(&r, "hello world", "oh hello world");
        assert_eq!((r2[0].start, r2[0].end), (9, 14));
        let r3 = rebase(&r, "hello world", "hello worlds");
        assert_eq!(
            (r3[0].start, r3[0].end),
            (6, 12),
            "typing at the end of a run extends it"
        );
        let r4 = rebase(&r, "hello world", "hello ");
        assert!(r4.is_empty());
    }
}
