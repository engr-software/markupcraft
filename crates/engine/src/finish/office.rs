//! Office and CAD files to PDF pages without an authoring program: Word (`.docx`) text with
//! headings, lists and tables; Excel (`.xlsx`) sheets as ruled tables; AutoCAD DXF (the public
//! text exchange format) drawn as vector linework and text. Layout is our own: Word and Excel
//! formatting beyond this (fonts, images, charts, merged cells) is not reproduced, and binary
//! DWG files are not read (no public specification; save them as DXF first).

use std::fmt::Write as _;

use markupcraft_geom::text::text_width;

use super::zip;
use crate::{Result, invalid};

/// One generated page: its size (points) and content stream (font `/F1`, Helvetica).
#[derive(Debug, Clone, PartialEq)]
pub struct DocPage {
    pub w: f64,
    pub h: f64,
    pub content: String,
}

/// Most pages one file makes.
const MAX_PAGES: usize = 5_000;

/// Text safe for a WinAnsi Helvetica string (common punctuation mapped to ASCII).
pub fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{2022}' => '*',
            '\u{00A0}' | '\t' => ' ',
            c if (' '..='~').contains(&c) => c,
            _ => '?',
        })
        .collect()
}

fn esc(s: &str) -> String {
    ascii(s).replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

fn helv(size: f64) -> markupcraft_geom::text::Font {
    markupcraft_revu::kinds::common::font_of(&markupcraft_model::TextStyle {
        size,
        ..Default::default()
    })
}

/// Break `text` into lines no wider than `width` at `size` points.
fn wrap(text: &str, size: f64, width: f64) -> Vec<String> {
    let f = helv(size);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in ascii(text).split(' ') {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text_width(&candidate, &f) <= width || line.is_empty() {
            line = candidate;
            // a single word wider than the line is cut
            while text_width(&line, &f) > width && line.chars().count() > 1 {
                let keep = (line.chars().count() * 9 / 10).max(1);
                let head: String = line.chars().take(keep).collect();
                let tail: String = line.chars().skip(keep).collect();
                lines.push(head);
                line = tail;
            }
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    lines.push(line);
    lines
}

/// Flows paragraphs onto letter pages.
struct Flow {
    pages: Vec<DocPage>,
    content: String,
    y: f64,
}

const PAGE_W: f64 = 612.0;
const PAGE_H: f64 = 792.0;
const MARGIN: f64 = 54.0;

impl Flow {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            content: String::new(),
            y: PAGE_H - MARGIN,
        }
    }

    fn break_page(&mut self) {
        self.pages.push(DocPage {
            w: PAGE_W,
            h: PAGE_H,
            content: std::mem::take(&mut self.content),
        });
        self.y = PAGE_H - MARGIN;
    }

    fn para(&mut self, text: &str, size: f64, indent: f64, after: f64) {
        for l in wrap(text, size, PAGE_W - 2.0 * MARGIN - indent) {
            if self.pages.len() >= MAX_PAGES {
                return;
            }
            if self.y - size * 1.25 < MARGIN {
                self.break_page();
            }
            self.y -= size * 1.25;
            let _ = writeln!(
                self.content,
                "BT /F1 {size} Tf {} {} Td ({}) Tj ET",
                MARGIN + indent,
                self.y,
                esc(&l)
            );
        }
        self.y -= after;
    }

    fn finish(mut self) -> Vec<DocPage> {
        if !self.content.is_empty() || self.pages.is_empty() {
            self.break_page();
        }
        self.pages
    }
}

/// Text of a Word paragraph or cell (runs, tabs and breaks).
fn run_text(n: roxmltree::Node) -> String {
    let mut s = String::new();
    for d in n.descendants().filter(|d| d.is_element()) {
        match d.tag_name().name() {
            "t" => s.push_str(d.text().unwrap_or_default()),
            "tab" => s.push(' '),
            "br" | "cr" => s.push(' '),
            _ => {}
        }
    }
    s
}

/// A Word document's pages: headings larger, list items indented with a bullet, tables as
/// rows of cells separated by bars.
pub fn docx_pages(bytes: &[u8]) -> Result<Vec<DocPage>> {
    let entries = zip::read(bytes)?;
    let doc = zip::find(&entries, "word/document.xml")
        .ok_or_else(|| invalid("not a Word document (no word/document.xml)"))?;
    let xml = String::from_utf8_lossy(&doc.data);
    let d = roxmltree::Document::parse(&xml).map_err(|e| invalid(format!("word/document.xml: {e}")))?;
    let body = d
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
        .ok_or_else(|| invalid("the Word document has no body"))?;
    let mut flow = Flow::new();
    for block in body.children().filter(|n| n.is_element()) {
        match block.tag_name().name() {
            "p" => {
                let style = block
                    .descendants()
                    .find(|n| n.is_element() && n.tag_name().name() == "pStyle")
                    .and_then(|n| {
                        n.attributes()
                            .find(|a| a.name() == "val")
                            .map(|a| a.value().to_string())
                    })
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let list = block
                    .descendants()
                    .any(|n| n.is_element() && n.tag_name().name() == "numPr");
                let page_break = block.descendants().any(|n| {
                    n.is_element()
                        && n.tag_name().name() == "br"
                        && n.attributes().any(|a| a.name() == "type" && a.value() == "page")
                });
                let text = run_text(block);
                let (size, after) = match style.as_str() {
                    "title" => (22.0, 10.0),
                    s if s.starts_with("heading1") || s == "heading 1" => (18.0, 6.0),
                    s if s.starts_with("heading2") => (15.0, 5.0),
                    s if s.starts_with("heading") => (13.0, 4.0),
                    _ => (10.5, 5.0),
                };
                if list {
                    flow.para(&format!("* {text}"), size, 18.0, 2.0);
                } else if text.trim().is_empty() {
                    flow.y -= size;
                } else {
                    flow.para(&text, size, 0.0, after);
                }
                if page_break {
                    flow.break_page();
                }
            }
            "tbl" => {
                for row in block
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "tr")
                {
                    let cells: Vec<String> = row
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "tc")
                        .map(|c| run_text(c).trim().to_string())
                        .collect();
                    flow.para(&cells.join("  |  "), 9.5, 0.0, 2.0);
                }
                flow.y -= 6.0;
            }
            _ => {}
        }
    }
    Ok(flow.finish())
}

/// An Excel workbook's pages: each sheet a ruled table (several pages when long), its name
/// on top.
pub fn xlsx_pages(bytes: &[u8]) -> Result<Vec<DocPage>> {
    let wb = super::xlsx_edit::Workbook::open(bytes)?;
    let sheets = wb.sheets()?;
    let mut pages = Vec::new();
    for (name, _) in sheets.iter().take(200) {
        let cells = super::xlsx_edit::read_sheet(bytes, name)?;
        let rows = cells.iter().map(|c| c.0 + 1).max().unwrap_or(0).min(100_000);
        let cols = cells.iter().map(|c| c.1 + 1).max().unwrap_or(0).min(256);
        let mut grid = vec![vec![String::new(); cols]; rows];
        for (r, c, t) in &cells {
            if let Some(cell) = grid.get_mut(*r).and_then(|row| row.get_mut(*c)) {
                *cell = ascii(t);
            }
        }
        let size = 8.0;
        let f = helv(size);
        let widths: Vec<f64> = (0..cols)
            .map(|c| {
                grid.iter()
                    .filter_map(|r| r.get(c))
                    .map(|t| text_width(t, &f))
                    .fold(30.0_f64, f64::max)
                    .min(220.0)
                    + 8.0
            })
            .collect();
        let total: f64 = widths.iter().sum();
        let (pw, ph) = if total > PAGE_W - 2.0 * MARGIN {
            (PAGE_H, PAGE_W)
        } else {
            (PAGE_W, PAGE_H)
        };
        let k = ((pw - 2.0 * MARGIN) / total.max(1.0)).min(1.0);
        let rh = 13.0 * k.max(0.5);
        let per = (((ph - 2.0 * MARGIN - 24.0) / rh).floor() as usize).max(1);
        let chunks: Vec<&[Vec<String>]> = if grid.is_empty() {
            vec![&[][..]]
        } else {
            grid.chunks(per).collect()
        };
        for (pi, chunk) in chunks.iter().enumerate() {
            if pages.len() >= MAX_PAGES {
                break;
            }
            let mut c = String::new();
            let title = if chunks.len() > 1 {
                format!("{name} ({}/{})", pi + 1, chunks.len())
            } else {
                name.clone()
            };
            let _ = writeln!(c, "BT /F1 12 Tf {MARGIN} {} Td ({}) Tj ET", ph - MARGIN, esc(&title));
            let top = ph - MARGIN - 20.0;
            let _ = writeln!(c, "0.6 G 0.5 w");
            for (ri, row) in chunk.iter().enumerate() {
                let y = top - (ri as f64 + 1.0) * rh;
                let mut x = MARGIN;
                for (ci, w) in widths.iter().enumerate() {
                    let w = w * k;
                    let _ = writeln!(c, "{x:.2} {y:.2} {w:.2} {rh:.2} re S");
                    if let Some(t) = row.get(ci).filter(|t| !t.is_empty()) {
                        let _ = writeln!(
                            c,
                            "BT /F1 {:.2} Tf {:.2} {:.2} Td ({}) Tj ET",
                            size * k,
                            x + 3.0 * k,
                            y + rh * 0.3,
                            esc(t)
                        );
                    }
                    x += w;
                }
            }
            pages.push(DocPage {
                w: pw,
                h: ph,
                content: c,
            });
        }
    }
    if pages.is_empty() {
        pages.push(DocPage {
            w: PAGE_W,
            h: PAGE_H,
            content: String::new(),
        });
    }
    Ok(pages)
}

// ---- DXF ---------------------------------------------------------------------------------------

/// A DXF entity drawn.
#[derive(Debug, Clone, PartialEq)]
enum Shape {
    /// A polyline (closed or not) in drawing units, with its colour.
    Poly(Vec<(f64, f64)>, bool, [f64; 3]),
    /// Text at a point: height, rotation (degrees), colour.
    Text((f64, f64), f64, f64, String, [f64; 3]),
}

/// AutoCAD Color Index 1-9 (others black).
fn aci(i: i64) -> [f64; 3] {
    match i {
        1 => [1.0, 0.0, 0.0],
        2 => [0.8, 0.8, 0.0],
        3 => [0.0, 0.7, 0.0],
        4 => [0.0, 0.7, 0.7],
        5 => [0.0, 0.0, 1.0],
        6 => [0.8, 0.0, 0.8],
        8 => [0.5, 0.5, 0.5],
        9 => [0.75, 0.75, 0.75],
        _ => [0.0, 0.0, 0.0],
    }
}

/// Group-code pairs of an ASCII DXF.
fn pairs(text: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .chunks(2)
        .take(5_000_000)
        .filter_map(|c| Some((c.first()?.trim().parse::<i32>().ok()?, c.get(1)?.trim_end().to_string())))
        .collect()
}

/// An entity: its type and its group codes.
#[derive(Debug, Clone, Default)]
struct Ent {
    kind: String,
    codes: Vec<(i32, String)>,
}

impl Ent {
    fn num(&self, code: i32) -> Option<f64> {
        self.codes
            .iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, v)| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }
    fn nums(&self, code: i32) -> Vec<f64> {
        self.codes
            .iter()
            .filter(|(c, _)| *c == code)
            .filter_map(|(_, v)| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .collect()
    }
    fn text(&self, code: i32) -> String {
        self.codes
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

/// Entities of a section, split at group code 0.
fn entities(p: &[(i32, String)]) -> Vec<Ent> {
    let mut out: Vec<Ent> = Vec::new();
    for (c, v) in p {
        if *c == 0 {
            out.push(Ent {
                kind: v.trim().to_ascii_uppercase(),
                codes: Vec::new(),
            });
        } else if let Some(e) = out.last_mut() {
            e.codes.push((*c, v.clone()));
        }
    }
    out
}

/// The ENTITIES section and the blocks (name -> base point and entities).
type Blocks = std::collections::HashMap<String, ((f64, f64), Vec<Ent>)>;

fn sections(p: &[(i32, String)]) -> (Vec<Ent>, Blocks) {
    let mut ents = Vec::new();
    let mut blocks = Blocks::new();
    let mut i = 0;
    while i < p.len() {
        if p.get(i).is_some_and(|(c, v)| *c == 0 && v.trim() == "SECTION")
            && let Some((2, name)) = p.get(i + 1).cloned()
        {
            let start = i + 2;
            let end = (start..p.len())
                .find(|&j| p.get(j).is_some_and(|(c, v)| *c == 0 && v.trim() == "ENDSEC"))
                .unwrap_or(p.len());
            let body = entities(p.get(start..end).unwrap_or_default());
            match name.trim() {
                "ENTITIES" => ents = body,
                "BLOCKS" => {
                    let mut cur: Option<(String, (f64, f64), Vec<Ent>)> = None;
                    for e in body {
                        match e.kind.as_str() {
                            "BLOCK" => {
                                cur = Some((
                                    e.text(2),
                                    (e.num(10).unwrap_or(0.0), e.num(20).unwrap_or(0.0)),
                                    Vec::new(),
                                ));
                            }
                            "ENDBLK" => {
                                if let Some((n, base, list)) = cur.take() {
                                    blocks.insert(n, (base, list));
                                }
                            }
                            _ => {
                                if let Some((_, _, list)) = cur.as_mut() {
                                    list.push(e);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    (ents, blocks)
}

/// A 2-D affine transform (a, b, c, d, e, f): x' = a x + c y + e, y' = b x + d y + f.
#[derive(Debug, Clone, Copy)]
struct Xf([f64; 6]);

impl Xf {
    const ID: Xf = Xf([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn apply(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
    fn then(&self, o: &Xf) -> Xf {
        // self first, then o
        let (a, b) = (self.0, o.0);
        Xf([
            b[0] * a[0] + b[2] * a[1],
            b[1] * a[0] + b[3] * a[1],
            b[0] * a[2] + b[2] * a[3],
            b[1] * a[2] + b[3] * a[3],
            b[0] * a[4] + b[2] * a[5] + b[4],
            b[1] * a[4] + b[3] * a[5] + b[5],
        ])
    }
    fn scale(&self) -> f64 {
        (self.0[0] * self.0[3] - self.0[1] * self.0[2]).abs().sqrt()
    }
}

fn arc_points(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> Vec<(f64, f64)> {
    let mut sweep = a1 - a0;
    while sweep <= 0.0 {
        sweep += 360.0;
    }
    let n = ((sweep / 6.0).ceil() as usize).clamp(2, 120);
    (0..=n)
        .map(|i| {
            let t = (a0 + sweep * i as f64 / n as f64).to_radians();
            (cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

fn draw(e: &Ent, xf: &Xf, blocks: &Blocks, depth: usize, out: &mut Vec<Shape>) {
    if out.len() > 500_000 {
        return;
    }
    let color = aci(e.num(62).map_or(7, |v| v as i64));
    let pt = |x: i32, y: i32| (e.num(x).unwrap_or(0.0), e.num(y).unwrap_or(0.0));
    let poly = |out: &mut Vec<Shape>, pts: Vec<(f64, f64)>, closed: bool| {
        if pts.len() >= 2 {
            out.push(Shape::Poly(pts.iter().map(|p| xf.apply(*p)).collect(), closed, color));
        }
    };
    match e.kind.as_str() {
        "LINE" => poly(out, vec![pt(10, 20), pt(11, 21)], false),
        "LWPOLYLINE" => {
            let (xs, ys) = (e.nums(10), e.nums(20));
            let closed = e.num(70).is_some_and(|f| (f as i64) & 1 == 1);
            poly(out, xs.into_iter().zip(ys).collect(), closed);
        }
        "CIRCLE" => {
            let (c, r) = (pt(10, 20), e.num(40).unwrap_or(0.0));
            if r > 0.0 {
                poly(out, arc_points(c.0, c.1, r, 0.0, 360.0), true);
            }
        }
        "ARC" => {
            let (c, r) = (pt(10, 20), e.num(40).unwrap_or(0.0));
            if r > 0.0 {
                poly(
                    out,
                    arc_points(c.0, c.1, r, e.num(50).unwrap_or(0.0), e.num(51).unwrap_or(360.0)),
                    false,
                );
            }
        }
        "ELLIPSE" => {
            let c = pt(10, 20);
            let (mx, my) = pt(11, 21);
            let ratio = e.num(40).unwrap_or(1.0);
            let (t0, t1) = (e.num(41).unwrap_or(0.0), e.num(42).unwrap_or(std::f64::consts::TAU));
            let mut sweep = t1 - t0;
            if sweep <= 0.0 {
                sweep += std::f64::consts::TAU;
            }
            let n = 72;
            let pts = (0..=n)
                .map(|i| {
                    let t = t0 + sweep * f64::from(i) / f64::from(n);
                    let (ct, st) = (t.cos(), t.sin());
                    (c.0 + mx * ct - my * ratio * st, c.1 + my * ct + mx * ratio * st)
                })
                .collect();
            poly(out, pts, false);
        }
        "SPLINE" => {
            // fit points when given, else the control polygon
            let (fx, fy) = (e.nums(11), e.nums(21));
            let pts: Vec<(f64, f64)> = if fx.len() >= 2 {
                fx.into_iter().zip(fy).collect()
            } else {
                e.nums(10).into_iter().zip(e.nums(20)).collect()
            };
            poly(out, pts, false);
        }
        "SOLID" | "3DFACE" | "TRACE" => {
            let pts = vec![pt(10, 20), pt(11, 21), pt(13, 23), pt(12, 22)];
            poly(out, pts, true);
        }
        "POINT" => {
            let p = pt(10, 20);
            poly(out, vec![p, (p.0 + 1e-3, p.1)], false);
        }
        "TEXT" | "MTEXT" | "ATTRIB" => {
            let mut t = e.text(1);
            for (c, v) in &e.codes {
                if *c == 3 {
                    t = format!("{v}{t}");
                }
            }
            // MTEXT formatting codes: drop \P paragraph marks and {} groups
            let t = t.replace("\\P", " ").replace(['{', '}'], "");
            let h = e.num(40).unwrap_or(2.5) * xf.scale();
            let base = xf.apply(pt(10, 20));
            let rot = e.num(50).unwrap_or(0.0) + xf.0[1].atan2(xf.0[0]).to_degrees();
            if !t.trim().is_empty() {
                out.push(Shape::Text(base, h, rot, t, color));
            }
        }
        "INSERT" if depth < 8 => {
            let name = e.text(2);
            let Some((base, list)) = blocks.get(&name) else { return };
            let (sx, sy) = (e.num(41).unwrap_or(1.0), e.num(42).unwrap_or(1.0));
            let rot = e.num(50).unwrap_or(0.0).to_radians();
            let at = pt(10, 20);
            let (c, s) = (rot.cos(), rot.sin());
            let local = Xf([1.0, 0.0, 0.0, 1.0, -base.0, -base.1])
                .then(&Xf([sx, 0.0, 0.0, sy, 0.0, 0.0]))
                .then(&Xf([c, s, -s, c, at.0, at.1]));
            let inner = local.then(xf);
            for b in list.iter().take(100_000) {
                draw(b, &inner, blocks, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// Old-style POLYLINE ... VERTEX ... SEQEND runs folded into LWPOLYLINE-like entities.
fn fold_polylines(list: Vec<Ent>) -> Vec<Ent> {
    let mut out = Vec::new();
    let mut cur: Option<Ent> = None;
    for e in list {
        match e.kind.as_str() {
            "POLYLINE" => {
                let flags = e.text(70);
                cur = Some(Ent {
                    kind: "LWPOLYLINE".into(),
                    codes: vec![(70, flags), (62, e.text(62))],
                });
            }
            "VERTEX" => {
                if let Some(c) = cur.as_mut() {
                    c.codes.push((10, e.text(10)));
                    c.codes.push((20, e.text(20)));
                } else {
                    out.push(e);
                }
            }
            "SEQEND" => {
                if let Some(c) = cur.take() {
                    out.push(c);
                }
            }
            _ => out.push(e),
        }
    }
    out
}

/// A DXF drawing on one page: fitted to a sheet as large as ARCH D (36 x 24 in), linework in
/// its colours, text at its height and angle.
pub fn dxf_pages(bytes: &[u8]) -> Result<Vec<DocPage>> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(invalid("binary DXF is not read; save the drawing as ASCII DXF"));
    }
    if bytes.starts_with(b"AC10") {
        return Err(invalid(
            "DWG files are not read (no public specification); save the drawing as DXF",
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let p = pairs(&text);
    let (ents, mut blocks) = sections(&p);
    for (_, list) in blocks.values_mut() {
        *list = fold_polylines(std::mem::take(list));
    }
    let ents = fold_polylines(ents);
    if ents.is_empty() {
        return Err(invalid("no drawing entities in the DXF"));
    }
    let mut shapes = Vec::new();
    for e in &ents {
        draw(e, &Xf::ID, &blocks, 0, &mut shapes);
    }
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut grow = |x: f64, y: f64| {
        if x.is_finite() && y.is_finite() {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
    };
    for s in &shapes {
        match s {
            Shape::Poly(pts, _, _) => pts.iter().for_each(|p| grow(p.0, p.1)),
            Shape::Text(p, h, _, t, _) => {
                grow(p.0, p.1);
                grow(p.0 + h * 0.6 * t.chars().count() as f64, p.1 + h);
            }
        }
    }
    if b.0 > b.2 {
        return Err(invalid("the DXF has nothing to draw"));
    }
    let (dw, dh) = ((b.2 - b.0).max(1e-6), (b.3 - b.1).max(1e-6));
    let (pw, ph) = if dw >= dh { (2592.0, 1728.0) } else { (1728.0, 2592.0) };
    let margin = 36.0;
    let k = ((pw - 2.0 * margin) / dw).min((ph - 2.0 * margin) / dh);
    let (ox, oy) = (
        margin + ((pw - 2.0 * margin) - dw * k) / 2.0,
        margin + ((ph - 2.0 * margin) - dh * k) / 2.0,
    );
    let map = |p: (f64, f64)| (ox + (p.0 - b.0) * k, oy + (p.1 - b.1) * k);
    let mut c = String::from("1 J 1 j 0.5 w\n");
    for s in &shapes {
        match s {
            Shape::Poly(pts, closed, col) => {
                let _ = write!(c, "{} {} {} RG ", col[0], col[1], col[2]);
                for (i, p) in pts.iter().enumerate() {
                    let (x, y) = map(*p);
                    let _ = write!(c, "{x:.3} {y:.3} {} ", if i == 0 { "m" } else { "l" });
                }
                c.push_str(if *closed { "h S\n" } else { "S\n" });
            }
            Shape::Text(p, h, rot, t, col) => {
                let (x, y) = map(*p);
                let size = (h * k).clamp(0.5, 500.0);
                let (cs, sn) = (rot.to_radians().cos(), rot.to_radians().sin());
                let _ = writeln!(
                    c,
                    "BT {} {} {} rg /F1 {size:.3} Tf {cs:.5} {sn:.5} {:.5} {cs:.5} {x:.3} {y:.3} Tm ({}) Tj ET",
                    col[0],
                    col[1],
                    col[2],
                    -sn,
                    esc(t)
                );
            }
        }
    }
    Ok(vec![DocPage {
        w: pw,
        h: ph,
        content: c,
    }])
}

/// A PowerPoint presentation's slides, one landscape page each: every text paragraph of the
/// slide in order (the first, the title, larger). Pictures and drawing shapes are not drawn.
pub fn pptx_pages(bytes: &[u8]) -> Result<Vec<DocPage>> {
    let entries = zip::read(bytes)?;
    let mut slides: Vec<(u32, &zip::Entry)> = entries
        .iter()
        .filter_map(|e| {
            let n = e.name.replace('\\', "/").to_ascii_lowercase();
            let num = n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?;
            Some((num, e))
        })
        .collect();
    if slides.is_empty() {
        return Err(invalid("not a PowerPoint presentation (no ppt/slides)"));
    }
    slides.sort_by_key(|(n, _)| *n);
    let (w, h) = (792.0, 612.0);
    let mut out = Vec::new();
    for (_, e) in slides.into_iter().take(MAX_PAGES) {
        let xml = String::from_utf8_lossy(&e.data);
        let d = roxmltree::Document::parse(&xml).map_err(|err| invalid(format!("{}: {err}", e.name)))?;
        let mut c = String::new();
        let mut y = h - MARGIN;
        for (i, p) in d
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "p")
            .enumerate()
        {
            let text: String = p
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "t")
                .filter_map(|n| n.text())
                .collect();
            if text.trim().is_empty() {
                continue;
            }
            let size = if i == 0 { 28.0 } else { 16.0 };
            for l in wrap(&text, size, w - 2.0 * MARGIN) {
                if y - size * 1.3 < MARGIN {
                    break;
                }
                y -= size * 1.3;
                let _ = writeln!(c, "BT /F1 {size} Tf {MARGIN} {y} Td ({}) Tj ET", esc(&l));
            }
            y -= size * 0.4;
        }
        out.push(DocPage { w, h, content: c });
    }
    Ok(out)
}

/// The pages of an Office or CAD file by its extension (`None` = not one of these).
pub fn pages_for(ext: &str, bytes: &[u8]) -> Option<Result<Vec<DocPage>>> {
    match ext {
        "pptx" | "pptm" => Some(pptx_pages(bytes)),
        "docx" | "docm" => Some(docx_pages(bytes)),
        "xlsx" | "xlsm" => Some(xlsx_pages(bytes)),
        "dxf" => Some(dxf_pages(bytes)),
        "dwg" => Some(Err(invalid(
            "DWG files are not read (no public specification); save the drawing as DXF",
        ))),
        "doc" | "xls" | "ppt" => Some(Err(invalid(
            "old binary Office files are not read; save them as .docx or .xlsx",
        ))),
        _ => None,
    }
}

/// Every extension [`pages_for`] converts.
pub const OFFICE_EXTS: &[&str] = &["docx", "docm", "xlsx", "xlsm", "pptx", "pptm", "dxf"];

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::convert::TextTable;

    /// A minimal Word document of our own.
    pub(crate) fn docx(paras: &[(&str, &str)]) -> Vec<u8> {
        let mut body = String::new();
        for (style, text) in paras {
            let ps = if style.is_empty() {
                String::new()
            } else {
                format!("<w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>")
            };
            body.push_str(&format!("<w:p>{ps}<w:r><w:t>{text}</w:t></w:r></w:p>"));
        }
        body.push_str("<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Door</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>12</w:t></w:r></w:p></w:tc></w:tr></w:tbl>");
        let doc = format!(
            "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>"
        );
        zip::write(&[zip::Entry {
            name: "word/document.xml".into(),
            data: doc.into_bytes(),
        }])
    }

    pub(crate) const DXF: &str = "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nDOOR\n10\n0\n20\n0\n0\nLINE\n10\n0\n20\n0\n11\n3\n21\n0\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n100\n21\n0\n0\nLWPOLYLINE\n90\n4\n70\n1\n62\n1\n10\n10\n20\n10\n10\n50\n20\n10\n10\n50\n20\n40\n10\n10\n20\n40\n0\nCIRCLE\n10\n80\n20\n30\n40\n5\n0\nTEXT\n10\n10\n20\n50\n40\n4\n1\nROOM 101\n0\nINSERT\n2\nDOOR\n10\n60\n20\n5\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn word_excel_and_dxf_become_pages() {
        let d = docx(&[
            ("Title", "Project Notes"),
            ("", "The walls are \u{2018}type A\u{2019}."),
            ("Heading1", "Doors"),
        ]);
        let pages = docx_pages(&d).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].content.contains("(Project Notes)"));
        assert!(pages[0].content.contains("'type A'"));
        assert!(pages[0].content.contains("Door  |  12"));

        let mut t = TextTable::default();
        t.rows.push(vec!["Item".into(), "Qty".into()]);
        t.rows.push(vec!["Doors".into(), "12".into()]);
        let x = crate::convert::xlsx(&[("Bid".into(), t)]);
        let pages = xlsx_pages(&x).unwrap();
        assert!(pages[0].content.contains("(Bid)") && pages[0].content.contains("(Doors)"));

        let pages = dxf_pages(DXF.as_bytes()).unwrap();
        let c = &pages[0].content;
        assert!(c.contains("(ROOM 101)"));
        assert!(c.contains("1 0 0 RG"), "the red polyline");
        assert_eq!(
            c.matches(" S\n").count(),
            4,
            "line, polyline, circle and the block's line"
        );
        assert!(dxf_pages(b"AC1032 binary").is_err());
        let slide = |t: &str| {
            format!(
                "<p:sld xmlns:p=\"p\" xmlns:a=\"a\"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{t}</a:t></a:r></a:p><a:p><a:r><a:t>Bullet one</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
            )
        };
        let pptx = zip::write(&[
            zip::Entry {
                name: "ppt/slides/slide2.xml".into(),
                data: slide("Second").into_bytes(),
            },
            zip::Entry {
                name: "ppt/slides/slide1.xml".into(),
                data: slide("Kickoff").into_bytes(),
            },
        ]);
        let pages = pptx_pages(&pptx).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages[0].content.contains("(Kickoff)") && pages[1].content.contains("(Second)"));
        assert!(pptx_pages(&zip::write(&[])).is_err());
        assert!(pages_for("dwg", b"").is_some_and(|r| r.is_err()));
        assert!(pages_for("png", b"").is_none());
    }
}
