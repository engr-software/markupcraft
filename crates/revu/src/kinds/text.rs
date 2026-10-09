//! Text markups: Text Box (`/FreeText`), Callout (`/FreeText /FreeTextCallout` + `/CL` leader)
//! and Typewriter (`/FreeText /FreeTextTypewriter`). Key layout as Revu 21 writes it:
//!
//! - `/C` the fill colour (`[]` = no fill); `/DA` `"1 0 0 rg /Helv 12 Tf"` (text colour, font)
//! - `/DS` `"font: Helvetica 12pt; text-align:left; margin:3pt; line-height:13.8pt; color:#FF0000"`
//! - `/RC` an XHTML body, one `<p>` per line; `/Contents` the text, lines joined by `\r`
//! - `/Rect` the box's outer edge; Callout: box + leader, `/RD` = `/Rect` minus the box,
//!   `/CL` `[tip knee attach]`, `/LE` one name
//!
//! Revu draws the border in the text colour; when ours differs `"R G B RG"` leads `/DA`.

use markupcraft_geom::text::{line_height, text_inset, to_win_ansi, widest_paragraph};
use markupcraft_geom::{Point, Rect};
use markupcraft_model::rich::{self, CharStyle, TextRun};
use markupcraft_model::{Color, Kind, Markup, TextStyle};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use super::AnnotKind;
use super::common::{box_from_rd, box_of, draw_ending, font_of, write_rd};
use crate::ap::Ap;
use crate::pdf::{self, color_arr, n, points_arr, s};

/// A Callout with its tip and knee in `pts[4]`, `pts[5]`.
pub fn has_leader(m: &Markup) -> bool {
    m.kind == Kind::Callout && m.pts.len() >= 6
}

/// The point on the box edge the leader attaches to: the middle of the side facing the knee.
pub fn callout_attach(m: &Markup) -> Point {
    let b = box_of(m);
    let knee = m.pts.get(5).copied().unwrap_or(Point::new(b.x1, b.y0));
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    if knee.x < b.x0 {
        Point::new(b.x0, cy)
    } else if knee.x > b.x1 {
        Point::new(b.x1, cy)
    } else if knee.y > cy {
        Point::new(cx, b.y1)
    } else {
        Point::new(cx, b.y0)
    }
}

/// `/Rect`: the box, plus the leader and its ending for a Callout.
pub fn text_rect(m: &Markup) -> Rect {
    let mut r = box_of(m);
    if !has_leader(m) {
        return r;
    }
    let reach = markupcraft_geom::shapes::line_ending_reach(m.line_width.max(1.0)) + 1.0;
    for p in m.pts.iter().skip(4).take(2) {
        r.x0 = r.x0.min(p.x - reach);
        r.y0 = r.y0.min(p.y - reach);
        r.x1 = r.x1.max(p.x + reach);
        r.y1 = r.y1.max(p.y + reach);
    }
    r
}

// ------------------------------------------------------------------------------- CSS ---

/// Decimal text with up to four decimals, trailing zeros removed (`12`, `13.8`).
fn num4(v: f64) -> String {
    let t = format!("{:.4}", if v.is_finite() { v } else { 0.0 });
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".into()
    } else {
        t.into()
    }
}

/// Revu's `/DS`: `"font: Helvetica 12pt; text-align:left; margin:3pt; line-height:13.8pt; ..."`.
pub fn text_css(s: &TextStyle) -> String {
    let align = match s.align {
        1 => "center",
        2 => "right",
        _ => "left",
    };
    let mut css = format!(
        "font: {} {}pt; text-align:{align}; margin:{}pt; line-height:{}pt; color:{}",
        s.font,
        num4(s.size),
        num4(3.0 + if s.margin.is_finite() { s.margin } else { 0.0 }),
        num4(line_height(s.size) * markupcraft_geom::text::spacing_of(s.line_spacing)),
        s.color.hex()
    );
    if s.bold {
        css += "; font-weight:bold";
    }
    if s.italic {
        css += "; font-style:italic";
    }
    match (s.underline, s.strike) {
        (true, true) => css += "; text-decoration:underline line-through",
        (true, false) => css += "; text-decoration:underline",
        (false, true) => css += "; text-decoration:line-through",
        (false, false) => {}
    }
    match s.script {
        1 => css += "; vertical-align:super",
        -1 => css += "; vertical-align:sub",
        _ => {}
    }
    css
}

fn unquote(v: &str) -> &str {
    for q in ['\'', '"'] {
        if let Some(inner) = v.strip_prefix(q).and_then(|t| t.strip_suffix(q)) {
            return inner;
        }
    }
    v
}

fn leading_number(v: &str) -> f64 {
    let end = v
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+'))))
        .map_or(v.len(), |(i, _)| i);
    v.get(..end).and_then(|t| t.parse().ok()).unwrap_or(0.0)
}

/// Parse Revu's `/DS` (or the `/RC` body style) into `s`. Unknown properties are ignored.
pub fn parse_text_css(css: &str, s: &mut TextStyle) {
    for decl in css.split(';') {
        let Some((key, val)) = decl.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let val = val.trim();
        let lval = val.to_ascii_lowercase();
        match key.as_str() {
            "font" => {
                // "[italic] [bold] Family Name 12pt" (Revu puts the size last)
                let mut family: Vec<&str> = Vec::new();
                for w in val.split(' ').filter(|w| !w.is_empty()) {
                    let lw = w.to_ascii_lowercase();
                    let starts_num = lw.starts_with(|c: char| c.is_ascii_digit() || c == '.');
                    match lw.as_str() {
                        "bold" => s.bold = true,
                        "italic" | "oblique" => s.italic = true,
                        "normal" => {}
                        _ if lw.len() > 2 && lw.ends_with("pt") && starts_num => s.size = leading_number(&lw),
                        _ => family.push(w),
                    }
                }
                if !family.is_empty() {
                    s.font = unquote(&family.join(" ")).to_string();
                }
            }
            "font-family" => s.font = unquote(val).to_string(),
            "font-size" => s.size = leading_number(val),
            "font-weight" => s.bold = lval == "bold" || leading_number(val) >= 600.0,
            "font-style" => s.italic = lval == "italic" || lval == "oblique",
            "text-decoration" => {
                s.underline = lval.contains("underline");
                s.strike = lval.contains("line-through");
            }
            "vertical-align" => {
                s.script = match lval.as_str() {
                    "super" | "superscript" => 1,
                    "sub" | "subscript" => -1,
                    _ => 0,
                }
            }
            "text-align" => {
                s.align = match lval.as_str() {
                    "center" => 1,
                    "right" => 2,
                    _ => 0,
                }
            }
            "color" if val.len() == 7 && val.starts_with('#') => {
                if let Some(v) = val.get(1..).and_then(|h| u32::from_str_radix(h, 16).ok()) {
                    let ch = |sh: u32| f64::from((v >> sh) & 255) / 255.0;
                    s.color = Color::rgb(ch(16), ch(8), ch(0));
                }
            }
            _ => {}
        }
    }
    if !(s.size > 0.0 && s.size.is_finite()) {
        s.size = 12.0;
    }
}

/// C's `%.4g`: four significant digits, trailing zeros removed.
fn g4(v: f64) -> String {
    if !v.is_finite() || v == 0.0 {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    let decimals = (3 - exp).clamp(0, 12) as usize;
    let t = format!("{v:.decimals$}");
    let t = if t.contains('.') {
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        t
    };
    if t == "-0" { "0".into() } else { t }
}

/// `/DA`: `"[R G B RG ]r g b rg /Font size Tf"`.
fn default_appearance(m: &Markup) -> String {
    let t = &m.text;
    let c = &m.color;
    let border_differs = (c.r - t.color.r).abs() + (c.g - t.color.g).abs() + (c.b - t.color.b).abs() > 1e-3;
    let border = if border_differs {
        format!("{} {} {} RG ", g4(c.r), g4(c.g), g4(c.b))
    } else {
        String::new()
    };
    format!(
        "{border}{} {} {} rg /{} {} Tf",
        g4(t.color.r),
        g4(t.color.g),
        g4(t.color.b),
        font_of(t).res_name(),
        g4(t.size)
    )
}

/// Read `/DA` (`"r g b rg /Font size Tf"`, optionally `"R G B RG"`); returns whether it set a
/// border colour.
fn parse_da(da: &str, m: &mut Markup) -> bool {
    let tok: Vec<&str> = da.split_whitespace().collect();
    let num = |i: Option<usize>| -> f64 {
        i.and_then(|i| tok.get(i))
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    let mut border = false;
    for (k, t) in tok.iter().enumerate() {
        let rgb = || Color::rgb(num(k.checked_sub(3)), num(k.checked_sub(2)), num(k.checked_sub(1)));
        match *t {
            "rg" if k >= 3 => m.text.color = rgb(),
            "g" if k >= 1 => {
                let g = num(k.checked_sub(1));
                m.text.color = Color::rgb(g, g, g);
            }
            "RG" if k >= 3 => {
                m.color = rgb();
                border = true;
            }
            "Tf" if k >= 2 => {
                m.text.size = num(k.checked_sub(1));
                let f = k.checked_sub(2).and_then(|i| tok.get(i)).copied().unwrap_or("");
                if f.starts_with("/Ti") {
                    m.text.font = "Times".into();
                } else if f.starts_with("/Co") {
                    m.text.font = "Courier".into();
                }
                if matches!(f, "/HeBo" | "/TiBo" | "/CoBo") || (f.len() > 3 && f.ends_with("BI")) {
                    m.text.bold = true;
                }
            }
            _ => {}
        }
    }
    if m.text.size <= 0.0 {
        m.text.size = 12.0;
    }
    border
}

// ------------------------------------------------------------------------- rich text ---

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o += "&amp;",
            '<' => o += "&lt;",
            '>' => o += "&gt;",
            '"' => o += "&quot;",
            _ => o.push(c),
        }
    }
    o
}

/// `/RC`: the XHTML body Revu writes, one `<p>` per paragraph.
pub fn rich_text(m: &Markup) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\"?><body xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\" \
         xfa:contentType=\"text/html\" xfa:APIVersion=\"MarkupCraft:0.1\" xfa:spec=\"2.2.0\" style=\"{}\" \
         xmlns=\"http://www.w3.org/1999/xhtml\">",
        xml_escape(&text_css(&m.text))
    );
    let mut at = 0;
    for p in markupcraft_geom::text::paragraphs(&m.contents) {
        let n = p.chars().count();
        if p.is_empty() {
            out += "<p />";
        } else if m.rich.is_empty() {
            out += &format!("<p>{}</p>", xml_escape(&p));
        } else {
            out += "<p>";
            let chars: Vec<char> = p.chars().collect();
            let base = CharStyle::of(&m.text);
            for (s, e, st) in rich::segments(&m.text, &m.rich, at, at + n) {
                let t: String = chars.get(s - at..e - at).unwrap_or_default().iter().collect();
                if st == base {
                    out += &xml_escape(&t);
                } else {
                    out += &format!("<span style=\"{}\">{}</span>", span_css(&st), xml_escape(&t));
                }
            }
            out += "</p>";
        }
        at += n + 1;
        // "\r\n" counts as one separator in paragraphs() but two chars in the contents
        if m.contents.chars().nth(at - 1) == Some('\r') && m.contents.chars().nth(at) == Some('\n') {
            at += 1;
        }
    }
    out + "</body>"
}

/// A span's style: every property, so it reads the same over any base.
fn span_css(s: &CharStyle) -> String {
    format!(
        "font-weight:{};font-style:{};text-decoration:{};color:{}",
        if s.bold { "bold" } else { "normal" },
        if s.italic { "italic" } else { "normal" },
        if s.underline { "underline" } else { "none" },
        s.color.hex()
    )
}

/// The runs of an `/RC` body MarkupCraft wrote (`<span style>` inside `<p>`), against `base`.
/// `None` when the body is not ours or has markup we do not read.
pub fn read_rich_runs(rc: &str, base: &TextStyle) -> Option<Vec<TextRun>> {
    if !rc.contains("MarkupCraft:") {
        return None;
    }
    let body = rc.split_once("<body")?.1.split_once('>')?.1;
    let body = body.rsplit_once("</body>").map_or(body, |(b, _)| b);
    let mut runs = Vec::new();
    let mut at = 0usize;
    let mut first = true;
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<p />") {
            if !first {
                at += 1;
            }
            first = false;
            rest = r;
            continue;
        }
        let r = rest.strip_prefix("<p>")?;
        if !first {
            at += 1;
        }
        first = false;
        let (inner, after) = r.split_once("</p>")?;
        rest = after;
        let mut s = inner;
        while !s.is_empty() {
            if let Some(span) = s.strip_prefix("<span style=\"") {
                let (css, tail) = span.split_once("\">")?;
                let (text, tail) = tail.split_once("</span>")?;
                let n = xml_unescape(text).chars().count();
                let mut st = base.clone();
                parse_text_css(css, &mut st);
                for decl in css.split(';') {
                    match decl.split_once(':').map(|(k, v)| (k.trim(), v.trim())) {
                        Some(("font-weight", "normal")) => st.bold = false,
                        Some(("font-style", "normal")) => st.italic = false,
                        Some(("text-decoration", "none")) => st.underline = false,
                        _ => {}
                    }
                }
                runs.push(TextRun {
                    start: at,
                    end: at + n,
                    bold: st.bold,
                    italic: st.italic,
                    underline: st.underline,
                    color: st.color,
                });
                at += n;
                s = tail;
            } else {
                let end = s.find('<').unwrap_or(s.len());
                if end == 0 {
                    return None;
                }
                at += xml_unescape(s.get(..end)?).chars().count();
                s = s.get(end..)?;
            }
        }
        if runs.len() > rich::MAX_RUNS {
            return None;
        }
    }
    Some(rich::normalize(base, &runs, at))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// ---------------------------------------------------------------------------- layout ---

/// Grow (never shrink) the box downwards so the text fits; a Typewriter also fits its width.
/// The inset of a text markup's text: the frame's (line width + 4) plus its own margin.
pub fn markup_inset(m: &Markup) -> f64 {
    let extra = if m.text.margin.is_finite() {
        m.text.margin.clamp(-4.0, 200.0)
    } else {
        0.0
    };
    (text_inset(m.line_width) + extra).max(0.0)
}

/// A text markup's lines laid out in box `b` (its margin and line spacing applied).
pub fn markup_lines(m: &Markup, b: Rect) -> Vec<markupcraft_geom::text::TextLine> {
    let f = font_of(&m.text);
    markupcraft_geom::text::layout_text_spaced(b, &m.contents, &f, m.text.align, markup_inset(m), m.text.line_spacing)
}

fn height_for(m: &Markup, f: &markupcraft_geom::text::Font, width: f64, inset: f64) -> f64 {
    markupcraft_geom::text::text_height_spaced(&m.contents, f, width, inset, m.text.line_spacing)
}

pub fn autosize_text_box(m: &mut Markup) {
    if m.pts.len() < 4 {
        return;
    }
    let mut b = box_of(m);
    let f = font_of(&m.text);
    let inset = markup_inset(m);
    if m.kind == Kind::Typewriter {
        b.x1 = b.x0 + (widest_paragraph(&m.contents, &f) + 2.0 * inset + 1.0).max(2.0 * inset + f.size);
    }
    let need = height_for(m, &f, b.x1 - b.x0, inset);
    if m.kind == Kind::Typewriter || need > b.y1 - b.y0 {
        b.y0 = b.y1 - need;
    }
    let rest: Vec<Point> = m.pts.iter().skip(4).copied().collect();
    m.pts = b.corners().to_vec();
    m.pts.extend(rest);
}

/// Autosize Text Box (Alt+Z): the frame shrinks or grows to fit its text exactly (the widest
/// line and every line), keeping its top-left corner. Empty text keeps the frame.
pub fn fit_text_box(m: &mut Markup) {
    if m.pts.len() < 4 || m.contents.trim().is_empty() {
        return;
    }
    let mut b = box_of(m);
    let f = font_of(&m.text);
    let inset = markup_inset(m);
    let w = (widest_paragraph(&m.contents, &f) + 2.0 * inset + 1.0).max(2.0 * inset + f.size);
    b.x1 = b.x0 + w;
    b.y0 = b.y1 - height_for(m, &f, w, inset);
    let rest: Vec<Point> = m.pts.iter().skip(4).copied().collect();
    m.pts = b.corners().to_vec();
    m.pts.extend(rest);
}

// ----------------------------------------------------------------------------- write ---

fn draw_text(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let w = m.line_width;
    let b = box_of(m);
    *extent = vec![Point::new(b.x0, b.y0), Point::new(b.x1, b.y1)];
    // Leader first, so the box covers its end.
    if has_leader(m)
        && let (Some(tip), Some(knee)) = (m.pts.get(4).copied(), m.pts.get(5).copied())
    {
        let at = callout_attach(m);
        let lw = if w > 0.0 { w } else { 1.0 };
        ap.op("q ").nums(&[lw], "w").stroke_rgb(&m.color).newline();
        ap.move_to(at).line_to(knee).line_to(tip).op("S\n");
        let from = if (knee.x - tip.x).abs() + (knee.y - tip.y).abs() > 1e-6 {
            knee
        } else {
            at
        };
        let style = if m.line_end.is_empty() {
            "None"
        } else {
            m.line_end.as_str()
        };
        draw_ending(ap, m, from, tip, style, lw, extent);
        ap.op("Q\n");
    }
    // Box: fill and border (border inset by half its width so /Rect is its outer edge).
    let border = w > 0.0;
    let filled = m.fill.is_some();
    if border || filled {
        ap.nums(&[b.x0 + w / 2.0, b.y0 + w / 2.0, b.width() - w, b.height() - w], "re");
        ap.op(match (border, filled) {
            (true, true) => "B\n",
            (true, false) => "S\n",
            _ => "f\n",
        });
    }
    // Text.
    let st = &m.text;
    let f = font_of(st);
    let lines = markup_lines(m, b);
    if !m.rich.is_empty() {
        draw_rich_lines(ap, m, &lines);
        return;
    }
    ap.op("[] 0 d BT /").op(f.res_name()).op(" ").nums(&[st.size], "Tf");
    ap.fill_rgb(&st.color).newline();
    for l in lines.iter().filter(|l| !l.text.is_empty()) {
        ap.op("1 0 0 1 ").nums(&[l.x, l.y], "Tm");
        ap.op(&format!("({}) Tj\n", Ap::text_literal(&to_win_ansi(&l.text))));
    }
    ap.op("ET\n");
    if st.underline {
        ap.op("q ")
            .nums(&[(st.size / 16.0).max(0.5)], "w")
            .stroke_rgb(&st.color);
        for l in lines.iter().filter(|l| !l.text.is_empty()) {
            let y = l.y - st.size * 0.12;
            ap.move_to(Point::new(l.x, y))
                .line_to(Point::new(l.x + l.width, y))
                .op("S ");
        }
        ap.op("Q\n");
    }
}

/// Char offsets of each laid-out line in the contents (lines drop the spaces they wrap at).
pub fn line_starts(contents: &str, lines: &[markupcraft_geom::text::TextLine]) -> Vec<usize> {
    let chars: Vec<char> = contents.chars().collect();
    let mut at = 0usize;
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        let want: Vec<char> = l.text.chars().collect();
        let mut i = at;
        while i < chars.len() {
            if chars.get(i..i + want.len()) == Some(want.as_slice()) {
                break;
            }
            i += 1;
        }
        let i = i.min(chars.len());
        out.push(i);
        at = i + want.len();
    }
    out
}

/// The lines of a markup with rich runs: each line in segments of one font and colour.
fn draw_rich_lines(ap: &mut Ap, m: &Markup, lines: &[markupcraft_geom::text::TextLine]) {
    let st = &m.text;
    let starts = line_starts(&m.contents, lines);
    let mut unders: Vec<(Point, Point, Color)> = Vec::new();
    ap.op("[] 0 d BT\n");
    for (l, s0) in lines.iter().zip(&starts) {
        let n = l.text.chars().count();
        let chars: Vec<char> = l.text.chars().collect();
        let mut x = l.x;
        for (s, e, cs) in rich::segments(st, &m.rich, *s0, s0 + n) {
            let t: String = chars.get(s - s0..e - s0).unwrap_or_default().iter().collect();
            let f = font_of(&TextStyle {
                bold: cs.bold,
                italic: cs.italic,
                ..st.clone()
            });
            ap.op("/").op(f.res_name()).op(" ").nums(&[st.size], "Tf");
            ap.fill_rgb(&cs.color).op(" 1 0 0 1 ").nums(&[x, l.y], "Tm");
            ap.op(&format!("({}) Tj\n", Ap::text_literal(&to_win_ansi(&t))));
            let w = markupcraft_geom::text::text_width(&t, &f);
            if cs.underline {
                let y = l.y - st.size * 0.12;
                unders.push((Point::new(x, y), Point::new(x + w, y), cs.color));
            }
            x += w;
        }
    }
    ap.op("ET\n");
    for (a, b, c) in unders {
        ap.op("q ").nums(&[(st.size / 16.0).max(0.5)], "w").stroke_rgb(&c);
        ap.move_to(a).line_to(b).op("S Q\n");
    }
}

/// Every font a text markup's appearance uses (its own, plus the rich runs' variants).
pub fn fonts_used(m: &Markup) -> Vec<markupcraft_geom::text::Font> {
    let mut out = vec![font_of(&m.text)];
    for r in &m.rich {
        let f = font_of(&TextStyle {
            bold: r.bold,
            italic: r.italic,
            ..m.text.clone()
        });
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

fn write_text(a: &mut Dict, m: &Markup) {
    pdf::set(a, "C", m.fill.as_ref().map_or(Object::Array(Vec::new()), color_arr));
    pdf::remove(a, "IC");
    pdf::remove(a, "FillOpacity");
    pdf::set(a, "DA", s(&default_appearance(m)));
    pdf::set(a, "DS", s(&text_css(&m.text)));
    pdf::set(a, "RC", s(&rich_text(m)));
    super::common::set_or_remove(a, "PCTextMargin", m.text.margin != 0.0, crate::pdf::real(m.text.margin));
    super::common::set_or_remove(
        a,
        "PCLineSpacing",
        markupcraft_geom::text::spacing_of(m.text.line_spacing) != 1.0,
        crate::pdf::real(m.text.line_spacing),
    );
    if has_leader(m)
        && let (Some(tip), Some(knee)) = (m.pts.get(4), m.pts.get(5))
    {
        write_rd(a, text_rect(m), box_of(m));
        pdf::set(a, "CL", points_arr(&[*tip, *knee, callout_attach(m)]));
        pdf::set(a, "LE", n(if m.line_end.is_empty() { "None" } else { &m.line_end }));
    } else {
        pdf::remove(a, "RD");
    }
}

// ------------------------------------------------------------------------------ read ---

fn read_text(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    box_from_rd(a, m);
    m.fill = pdf::color(a.get(b"C"));
    let mut border = false;
    if let Some(Object::String(da)) = a.get(b"DA").map(|o| cos.resolve(o)).as_deref() {
        border = parse_da(&da.to_text(), m);
    }
    if let Some(Object::String(ds)) = a.get(b"DS").map(|o| cos.resolve(o)).as_deref() {
        parse_text_css(&ds.to_text(), &mut m.text);
    }
    m.text.margin = pdf::num(a.get(b"PCTextMargin"))
        .filter(|v| v.is_finite())
        .unwrap_or(0.0);
    m.text.line_spacing = pdf::num(a.get(b"PCLineSpacing"))
        .map(markupcraft_geom::text::spacing_of)
        .unwrap_or(1.0);
    if !border {
        m.color = m.text.color;
    }
    // Not Revu's (no /DS or /RC): a /C equal to the text colour would hide the text, so other
    // writers' /C is taken as the border colour, as their viewers draw it.
    if !a.contains(b"DS") && !a.contains(b"RC") && m.fill == Some(m.text.color) {
        m.fill = None;
        m.color = m.text.color;
    }
    if m.kind == Kind::Callout {
        let cl = pdf::points(Some(&cos.resolve(a.get(b"CL").unwrap_or(&Object::Null))));
        if let (Some(tip), Some(knee)) = (cl.first(), cl.get(1)) {
            m.pts.push(*tip);
            m.pts.push(*knee);
        }
    }
}

// ----------------------------------------------------------------------------- table ---

pub static TEXT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_text),
    write_geometry: Some(write_text),
    rect_of: Some(text_rect),
    read_keys: Some(read_text),
    ..AnnotKind::new(Kind::Text, "FreeText", None, 0, false)
};
pub static CALLOUT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_text),
    write_geometry: Some(write_text),
    rect_of: Some(text_rect),
    read_keys: Some(read_text),
    ..AnnotKind::new(Kind::Callout, "FreeText", Some("FreeTextCallout"), 0, false)
};
pub static TYPEWRITER: AnnotKind = AnnotKind {
    draw_shape: Some(draw_text),
    write_geometry: Some(write_text),
    rect_of: Some(text_rect),
    read_keys: Some(read_text),
    ..AnnotKind::new(Kind::Typewriter, "FreeText", Some("FreeTextTypewriter"), 0, false)
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_round_trip() {
        let mut s = TextStyle::default();
        parse_text_css(
            "font: Helvetica 26pt; text-align:center; margin:3pt; line-height:29.9pt; color:#0000FF; font-weight:bold",
            &mut s,
        );
        assert_eq!((s.size, s.align, s.bold), (26.0, 1, true));
        assert_eq!(s.color, Color::rgb(0.0, 0.0, 1.0));
        let mut t = TextStyle::default();
        parse_text_css(&text_css(&s), &mut t);
        assert_eq!(t, s);
        let mut q = TextStyle::default();
        parse_text_css(
            "font: italic 'Times New Roman' 10.5pt; text-decoration: underline",
            &mut q,
        );
        assert_eq!(q.font, "Times New Roman");
        assert!(q.italic && q.underline);
        assert_eq!(q.size, 10.5);
        let mut bad = TextStyle::default();
        parse_text_css("font-size: x; color:#GGGGGG; ;:;", &mut bad);
        assert_eq!(bad.size, 12.0);
    }

    #[test]
    fn da_round_trip() {
        let mut m = Markup::new(Kind::Text, 0, Vec::new());
        m.text.color = Color::rgb(0.0, 0.0, 0.8);
        m.text.font = "Times".into();
        m.text.bold = true;
        m.text.size = 14.0;
        m.color = Color::rgb(1.0, 0.0, 0.0);
        let da = default_appearance(&m);
        assert_eq!(da, "1 0 0 RG 0 0 0.8 rg /TiBo 14 Tf");
        let mut back = Markup::default();
        assert!(parse_da(&da, &mut back));
        assert_eq!(back.text.color, m.text.color);
        assert_eq!(back.color, m.color);
        assert_eq!(back.text.font, "Times");
        assert!(back.text.bold);
        assert_eq!(back.text.size, 14.0);
        assert!(!parse_da("rg Tf 0 g", &mut back));
        assert_eq!(g4(0.123456), "0.1235");
        assert_eq!(g4(12.0), "12");
    }

    #[test]
    fn autosize_grows_the_box() {
        let mut m = Markup::new(Kind::Text, 0, Rect::new(0.0, 90.0, 180.0, 100.0).corners().to_vec());
        m.line_width = 2.0;
        m.contents = "one\rtwo\rthree".into();
        autosize_text_box(&mut m);
        let b = box_of(&m);
        assert!((b.height() - (12.0 + 2.0 * 13.8 + 12.0)).abs() < 1e-9);
        assert_eq!(b.y1, 100.0);
        m.kind = Kind::Typewriter;
        autosize_text_box(&mut m);
        assert!(box_of(&m).width() < 180.0);
    }
}
