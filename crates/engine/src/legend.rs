//! Legends: a table markup listing the markups on its page (or in the whole document) by
//! subject, with a symbol in each row's colours, how many there are and their total quantity.
//!
//! A legend is a FreeText annotation (subject "Legend") with our key `/PCLegend` holding its
//! options and an appearance MarkupCraft draws (a table, our own design). Its `/Contents` holds
//! the same table as text, so other viewers can read the numbers. [`Session::update_legends`]
//! recomputes every legend from the markups as they are now (Revu's legends follow edits; call
//! it after changes, before saving).
//!
//! ```text
//! /PCLegend << /Title (Legend) /Scope /Page|/Document /Cols [/Symbol /Subject /Count /Total]
//!              /Size 9 /Subjects [(Duct) ...] /MeasOnly false >>
//! ```

use std::collections::{BTreeMap, HashSet};

use markupcraft_geom::text::{Font, FontFamily, text_width, to_win_ansi};
use markupcraft_model::{Color, Document, Kind, Markup, ObjId, Point, Rect, format_value};
use markupcraft_revu::ap::Ap;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, Stream};

use crate::docutil::{annots_of, page_objs, set_annots};
use crate::{EngineError, Result, Session, invalid};

/// Most rows one legend shows.
pub const MAX_ROWS: usize = 500;
pub const SUBJECT: &str = "Legend";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendColumn {
    Symbol,
    Subject,
    Type,
    Count,
    Total,
}

impl LegendColumn {
    pub const ALL: [LegendColumn; 5] = [
        LegendColumn::Symbol,
        LegendColumn::Subject,
        LegendColumn::Type,
        LegendColumn::Count,
        LegendColumn::Total,
    ];
    pub fn name(self) -> &'static str {
        match self {
            LegendColumn::Symbol => "Symbol",
            LegendColumn::Subject => "Subject",
            LegendColumn::Type => "Type",
            LegendColumn::Count => "Count",
            LegendColumn::Total => "Total",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name().eq_ignore_ascii_case(s.trim()))
    }
    fn header(self) -> &'static str {
        match self {
            LegendColumn::Symbol => "",
            LegendColumn::Subject => "Description",
            other => other.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegendOptions {
    pub title: String,
    /// every page's markups (else only the legend's page)
    pub document: bool,
    pub columns: Vec<LegendColumn>,
    pub font_size: f64,
    /// only these subjects (empty = all)
    pub subjects: Vec<String>,
    pub measurements_only: bool,
}

impl Default for LegendOptions {
    fn default() -> Self {
        Self {
            title: "Legend".into(),
            document: false,
            columns: vec![
                LegendColumn::Symbol,
                LegendColumn::Subject,
                LegendColumn::Count,
                LegendColumn::Total,
            ],
            font_size: 9.0,
            subjects: Vec::new(),
            measurements_only: false,
        }
    }
}

impl LegendOptions {
    fn validate(&self) -> Result<()> {
        if !(self.font_size.is_finite() && (4.0..=72.0).contains(&self.font_size)) {
            return Err(invalid("the legend font size must be 4 to 72 points"));
        }
        if self.columns.is_empty() {
            return Err(invalid("a legend needs at least one column"));
        }
        if self.title.chars().count() > 200 {
            return Err(invalid("the legend title is too long (200 characters at most)"));
        }
        Ok(())
    }

    fn object(&self) -> Object {
        let mut d = Dict::new();
        d.set(b"Title".to_vec(), Object::String(PdfString::text(&self.title)));
        d.set(
            b"Scope".to_vec(),
            Object::name(if self.document { "Document" } else { "Page" }),
        );
        d.set(
            b"Cols".to_vec(),
            Object::Array(self.columns.iter().map(|c| Object::name(c.name())).collect()),
        );
        d.set(b"Size".to_vec(), Object::Real(self.font_size));
        d.set(
            b"Subjects".to_vec(),
            Object::Array(
                self.subjects
                    .iter()
                    .map(|s| Object::String(PdfString::text(s)))
                    .collect(),
            ),
        );
        d.set(b"MeasOnly".to_vec(), Object::Bool(self.measurements_only));
        Object::Dict(d)
    }

    fn read(cos: &CosDoc, d: &Dict) -> Self {
        let mut o = LegendOptions {
            title: markupcraft_revu::pdf::text(d.get(b"Title").map(|v| cos.resolve(v)).as_deref()),
            document: d.name(b"Scope") == Some(b"Document"),
            ..Default::default()
        };
        let cols: Vec<LegendColumn> = d
            .get(b"Cols")
            .map(|v| cos.resolve(v))
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|c| {
                c.as_name()
                    .and_then(|n| LegendColumn::from_name(&String::from_utf8_lossy(n)))
            })
            .collect();
        if !cols.is_empty() {
            o.columns = cols;
        }
        o.font_size = d
            .get(b"Size")
            .and_then(Object::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(9.0)
            .clamp(4.0, 72.0);
        o.subjects = d
            .get(b"Subjects")
            .map(|v| cos.resolve(v))
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|s| markupcraft_revu::pdf::text(Some(s)))
            .filter(|s| !s.is_empty())
            .collect();
        o.measurements_only = matches!(d.get(b"MeasOnly"), Some(Object::Bool(true)));
        o
    }
}

/// One row of a legend.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendRow {
    pub subject: String,
    pub kind: Kind,
    pub color: Color,
    pub fill: Option<Color>,
    pub markups: usize,
    /// counted items (Count measurements count their points), else the markup count
    pub count: usize,
    pub total: Option<f64>,
    pub total_text: String,
}

/// A legend in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendInfo {
    pub id: String,
    pub page: usize,
    pub options: LegendOptions,
    pub rows: Vec<LegendRow>,
}

/// Objects of the legends among `doc`'s markups.
fn legend_objs(cos: &CosDoc, doc: &Document) -> HashSet<ObjId> {
    doc.markups
        .iter()
        .filter(|m| m.in_file() && m.subtype == "FreeText")
        .filter(|m| {
            cos.get(ObjRef::new(m.obj.0, m.obj.1))
                .as_dict()
                .is_some_and(|d| d.contains(b"PCLegend"))
        })
        .map(|m| m.obj)
        .collect()
}

/// The rows a legend on `page` shows now.
pub fn compute_rows(doc: &Document, legends: &HashSet<ObjId>, page: usize, o: &LegendOptions) -> Vec<LegendRow> {
    struct Acc<'a> {
        first: &'a Markup,
        markups: usize,
        count: usize,
        total: f64,
        units: HashSet<String>,
        measured: bool,
    }
    let mut groups: BTreeMap<String, Acc> = BTreeMap::new();
    for m in &doc.markups {
        if legends.contains(&m.obj) && m.in_file() {
            continue;
        }
        if matches!(m.kind, Kind::Hyperlink | Kind::Attachment | Kind::Other) {
            continue;
        }
        if !o.document && m.page != page {
            continue;
        }
        if o.measurements_only && !m.kind.is_measurement() {
            continue;
        }
        let subject = if m.subject.is_empty() {
            m.kind.name().to_string()
        } else {
            m.subject.clone()
        };
        if !o.subjects.is_empty() && !o.subjects.iter().any(|s| s.eq_ignore_ascii_case(&subject)) {
            continue;
        }
        let key = format!("{}\u{0}{}", subject.to_lowercase(), subject);
        let a = groups.entry(key).or_insert(Acc {
            first: m,
            markups: 0,
            count: 0,
            total: 0.0,
            units: HashSet::new(),
            measured: false,
        });
        a.markups += 1;
        let q = m.quantity();
        if m.kind == Kind::Count {
            a.count += q.map_or(1, |v| v.max(0.0) as usize);
        } else {
            a.count += 1;
        }
        if m.kind.is_measurement()
            && let Some(v) = q
        {
            a.total += v;
            a.units.insert(m.unit());
            a.measured = true;
        }
    }
    groups
        .into_iter()
        .take(MAX_ROWS)
        .map(|(key, a)| {
            let subject = key.split('\u{0}').nth(1).unwrap_or_default().to_string();
            let m = a.first;
            let (total, total_text) = if !a.measured || a.units.len() > 1 {
                (
                    None,
                    if a.units.len() > 1 {
                        "mixed units".to_string()
                    } else {
                        String::new()
                    },
                )
            } else if m.kind == Kind::Count {
                (Some(a.count as f64), format!("{} ea", a.count))
            } else {
                let text = m
                    .quantity_formats()
                    .map(|fa| format_value(a.total, fa))
                    .unwrap_or_else(|| format!("{:.2}", a.total));
                (Some(a.total), text)
            };
            LegendRow {
                subject,
                kind: m.kind,
                color: m.color,
                fill: m.fill,
                markups: a.markups,
                count: a.count,
                total,
                total_text,
            }
        })
        .collect()
}

fn cell_text(r: &LegendRow, c: LegendColumn) -> String {
    match c {
        LegendColumn::Symbol => String::new(),
        LegendColumn::Subject => r.subject.clone(),
        LegendColumn::Type => r.kind.name().to_string(),
        LegendColumn::Count => r.count.to_string(),
        LegendColumn::Total => r.total_text.clone(),
    }
}

/// The table as text (for `/Contents`).
pub fn plain_text(o: &LegendOptions, rows: &[LegendRow]) -> String {
    let cols: Vec<LegendColumn> = o
        .columns
        .iter()
        .copied()
        .filter(|c| *c != LegendColumn::Symbol)
        .collect();
    let mut out = o.title.clone();
    out.push('\r');
    out.push_str(&cols.iter().map(|c| c.header()).collect::<Vec<_>>().join("\t"));
    for r in rows {
        out.push('\r');
        out.push_str(&cols.iter().map(|c| cell_text(r, *c)).collect::<Vec<_>>().join("\t"));
    }
    out
}

fn helv(size: f64, bold: bool) -> Font {
    Font {
        family: FontFamily::Helvetica,
        bold,
        italic: false,
        size,
    }
}

fn symbol(ap: &mut Ap, r: &LegendRow, x: f64, y: f64, s: f64) {
    let (x0, y0, x1, y1) = (x + 2.0, y + 2.0, x + s - 2.0, y + s - 2.0);
    ap.op("q ").stroke_rgb(&r.color).nums(&[1.0], "w");
    match r.kind {
        Kind::Length | Kind::Polylength | Kind::Line | Kind::Arrow | Kind::Polyline | Kind::Ink | Kind::Highlight => {
            ap.nums(&[2.0], "w")
                .nums(&[x0, (y0 + y1) / 2.0], "m")
                .nums(&[x1, (y0 + y1) / 2.0], "l")
                .op("S ");
        }
        Kind::Count | Kind::Ellipse | Kind::Diameter | Kind::Radius => {
            let (cx, cy, rr) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0);
            let k = 0.5523 * rr;
            let fill = if r.kind == Kind::Count { Some(r.color) } else { r.fill };
            if let Some(f) = fill {
                ap.fill_rgb(&f);
            }
            ap.nums(&[cx + rr, cy], "m")
                .nums(&[cx + rr, cy + k, cx + k, cy + rr, cx, cy + rr], "c")
                .nums(&[cx - k, cy + rr, cx - rr, cy + k, cx - rr, cy], "c")
                .nums(&[cx - rr, cy - k, cx - k, cy - rr, cx, cy - rr], "c")
                .nums(&[cx + k, cy - rr, cx + rr, cy - k, cx + rr, cy], "c")
                .op(if fill.is_some() { "b " } else { "s " });
        }
        _ => {
            if let Some(f) = r.fill {
                ap.fill_rgb(&f).nums(&[x0, y0, x1 - x0, y1 - y0], "re").op("B ");
            } else {
                ap.nums(&[x0, y0, x1 - x0, y1 - y0], "re").op("S ");
            }
        }
    }
    ap.op("Q\n");
}

fn text(ap: &mut Ap, t: &str, x: f64, y: f64, size: f64, bold: bool) {
    let res = if bold { "HeBo" } else { "Helv" };
    ap.op("BT /")
        .op(res)
        .op(" ")
        .nums(&[size], "Tf")
        .op("0 g ")
        .nums(&[x, y], "Td");
    ap.op(&format!("({}) Tj ET\n", Ap::text_literal(&to_win_ansi(t))));
}

/// The appearance of a legend whose top-left corner is `at`: the content and its box.
pub fn draw(o: &LegendOptions, rows: &[LegendRow], at: Point) -> (Vec<u8>, Rect) {
    let size = o.font_size;
    let pad = size * 0.5;
    let row_h = size * 1.7;
    let title_h = if o.title.is_empty() { 0.0 } else { size * 2.0 };
    let body = helv(size, false);
    let bold = helv(size, true);
    let widths: Vec<f64> = o
        .columns
        .iter()
        .map(|c| match c {
            LegendColumn::Symbol => row_h,
            _ => {
                let w = rows
                    .iter()
                    .map(|r| text_width(&cell_text(r, *c), &body))
                    .fold(text_width(c.header(), &bold), f64::max);
                w + 2.0 * pad
            }
        })
        .collect();
    let table_w: f64 = widths.iter().sum();
    let w = table_w.max(text_width(&o.title, &helv(size * 1.2, true)) + 2.0 * pad);
    let n_rows = if rows.is_empty() { 1 } else { rows.len() };
    let h = title_h + row_h * (1 + n_rows) as f64;
    let r = Rect::new(at.x, at.y - h, at.x + w, at.y);
    let mut ap = Ap::new();
    ap.op("q 1 1 1 rg ").nums(&[r.x0, r.y0, w, h], "re").op("f\n");
    ap.op("0.2 0.2 0.2 RG 0.75 w ")
        .nums(&[r.x0, r.y0, w, h], "re")
        .op("S\n");
    let mut y = r.y1;
    if !o.title.is_empty() {
        text(
            &mut ap,
            &o.title,
            r.x0 + pad,
            y - title_h + size * 0.6,
            size * 1.2,
            true,
        );
        y -= title_h;
        ap.op("0.2 0.2 0.2 RG 0.75 w ")
            .nums(&[r.x0, y], "m")
            .nums(&[r.x1, y], "l")
            .op("S\n");
    }
    // header
    let mut x = r.x0;
    for (c, cw) in o.columns.iter().zip(&widths) {
        text(&mut ap, c.header(), x + pad, y - row_h + size * 0.55, size, true);
        x += cw;
    }
    y -= row_h;
    ap.op("0.2 0.2 0.2 RG 0.5 w ")
        .nums(&[r.x0, y], "m")
        .nums(&[r.x1, y], "l")
        .op("S\n");
    if rows.is_empty() {
        text(
            &mut ap,
            "(no markups)",
            r.x0 + pad,
            y - row_h + size * 0.55,
            size,
            false,
        );
    }
    for (i, row) in rows.iter().enumerate() {
        let mut x = r.x0;
        for (c, cw) in o.columns.iter().zip(&widths) {
            if *c == LegendColumn::Symbol {
                symbol(&mut ap, row, x, y - row_h, row_h);
            } else {
                text(
                    &mut ap,
                    &cell_text(row, *c),
                    x + pad,
                    y - row_h + size * 0.55,
                    size,
                    false,
                );
            }
            x += cw;
        }
        y -= row_h;
        if i + 1 < rows.len() {
            ap.op("0.75 0.75 0.75 RG 0.25 w ")
                .nums(&[r.x0, y], "m")
                .nums(&[r.x1, y], "l")
                .op("S\n");
        }
    }
    ap.op("Q\n");
    (ap.bytes(), r)
}

fn font(base: &str) -> Object {
    let mut f = Dict::new();
    f.set(b"Type".to_vec(), Object::name("Font"));
    f.set(b"Subtype".to_vec(), Object::name("Type1"));
    f.set(b"BaseFont".to_vec(), Object::name(base));
    f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    Object::Dict(f)
}

fn rect_obj(r: &Rect) -> Object {
    Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect())
}

/// Write a legend's appearance, rectangle and contents onto annotation `a`.
fn apply_look(cos: &mut CosDoc, a: &mut Dict, o: &LegendOptions, rows: &[LegendRow], at: Point) {
    let (bytes, r) = draw(o, rows, at);
    let mut fonts = Dict::new();
    fonts.set(b"Helv".to_vec(), font("Helvetica"));
    fonts.set(b"HeBo".to_vec(), font("Helvetica-Bold"));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    let mut sd = Dict::new();
    sd.set(b"Type".to_vec(), Object::name("XObject"));
    sd.set(b"Subtype".to_vec(), Object::name("Form"));
    sd.set(b"BBox".to_vec(), rect_obj(&r));
    sd.set(b"Resources".to_vec(), Object::Dict(res));
    let ap_ref = cos.add(Object::Stream(Stream::flate(sd, &bytes)));
    let mut apd = Dict::new();
    apd.set(b"N".to_vec(), Object::Ref(ap_ref));
    a.set(b"AP".to_vec(), Object::Dict(apd));
    a.set(b"Rect".to_vec(), rect_obj(&r));
    a.set(
        b"Contents".to_vec(),
        Object::String(PdfString::text(&plain_text(o, rows))),
    );
    a.set(b"PCLegend".to_vec(), o.object());
}

impl Session {
    /// Add a legend on `page` with its top-left corner at `at`. Returns its id. Undoable.
    pub fn add_legend(&mut self, page: usize, at: Point, o: &LegendOptions) -> Result<String> {
        self.page(page)?;
        o.validate()?;
        if !(at.x.is_finite() && at.y.is_finite()) {
            return Err(invalid("the legend position must be finite"));
        }
        let id = self.new_id();
        let author = self.author.clone();
        let o = o.clone();
        let nm = id.clone();
        self.graph_edit("Add Legend", move |cos, doc| {
            let pages = page_objs(cos)?;
            let pref = *pages.get(page).ok_or(EngineError::NoPage {
                page: page + 1,
                count: pages.len(),
            })?;
            let legends = legend_objs(cos, doc);
            let rows = compute_rows(doc, &legends, page, &o);
            let mut a = Dict::new();
            a.set(b"Type".to_vec(), Object::name("Annot"));
            a.set(b"Subtype".to_vec(), Object::name("FreeText"));
            a.set(b"NM".to_vec(), Object::String(PdfString::text(&nm)));
            a.set(b"Subj".to_vec(), Object::String(PdfString::text(SUBJECT)));
            a.set(b"T".to_vec(), Object::String(PdfString::text(&author)));
            a.set(b"F".to_vec(), Object::Int(4));
            a.set(b"P".to_vec(), Object::Ref(pref));
            a.set(b"DA".to_vec(), Object::String(PdfString::text("/Helv 9 Tf 0 g")));
            let now = markupcraft_revu::pdf_date_now();
            a.set(b"CreationDate".to_vec(), Object::String(PdfString::text(&now)));
            a.set(b"M".to_vec(), Object::String(PdfString::text(&now)));
            apply_look(cos, &mut a, &o, &rows, at);
            let r = cos.add(Object::Dict(a));
            let mut list = annots_of(cos, pref);
            list.push(Object::Ref(r));
            set_annots(cos, pref, list)
        })?;
        self.selection = vec![id.clone()];
        Ok(id)
    }

    /// Every legend, with the rows it shows now.
    pub fn legends(&self) -> Vec<LegendInfo> {
        let cos = &self.file.cos;
        let set = legend_objs(cos, &self.doc);
        self.doc
            .markups
            .iter()
            .filter(|m| set.contains(&m.obj))
            .filter_map(|m| {
                let a = cos.get(ObjRef::new(m.obj.0, m.obj.1));
                let d = a.as_dict()?.get(b"PCLegend").and_then(|o| cos.dict(o))?;
                let options = LegendOptions::read(cos, &d);
                let rows = compute_rows(&self.doc, &set, m.page, &options);
                Some(LegendInfo {
                    id: m.id.clone(),
                    page: m.page,
                    options,
                    rows,
                })
            })
            .collect()
    }

    /// Change a legend's options and redraw it. Undoable.
    pub fn set_legend_options(&mut self, id: &str, o: &LegendOptions) -> Result<()> {
        o.validate()?;
        if !self.legends().iter().any(|l| l.id == id) {
            return Err(invalid(format!("{id:?} is not a legend (legend_list shows them)")));
        }
        self.refresh(Some((id.to_string(), o.clone())))?;
        Ok(())
    }

    /// Recompute every legend from the markups as they are now (rows, size, contents).
    /// Returns how many legends there are. Undoable (one step; nothing when there are none).
    pub fn update_legends(&mut self) -> Result<usize> {
        if self.legends().is_empty() {
            return Ok(0);
        }
        self.refresh(None)
    }

    fn refresh(&mut self, change: Option<(String, LegendOptions)>) -> Result<usize> {
        self.graph_edit("Update Legends", move |cos, doc| {
            let set = legend_objs(cos, doc);
            let mut n = 0;
            for m in doc.markups.iter().filter(|m| set.contains(&m.obj)) {
                let r = ObjRef::new(m.obj.0, m.obj.1);
                let Some(mut a) = cos.get(r).as_dict().cloned() else {
                    continue;
                };
                let Some(d) = a.get(b"PCLegend").and_then(|o| cos.dict(o)) else {
                    continue;
                };
                let mut o = LegendOptions::read(cos, &d);
                if let Some((id, new)) = &change {
                    if *id != m.id {
                        continue;
                    }
                    o = new.clone();
                }
                let rect = markupcraft_revu::pdf::rect(a.get(b"Rect"))
                    .unwrap_or(m.rect)
                    .normalized();
                let rows = compute_rows(doc, &set, m.page, &o);
                apply_look(cos, &mut a, &o, &rows, Point::new(rect.x0, rect.y1));
                cos.set(r, Object::Dict(a));
                n += 1;
            }
            Ok(n)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::Scale;

    #[test]
    fn legend_lists_subjects_counts_totals_and_follows_edits() {
        let dir = std::env::temp_dir().join(format!("mc-legend-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legend.pdf");
        let mut s = Session::new_blank(&path, &[(612.0, 792.0), (612.0, 792.0)]).unwrap();
        let mut a = Markup::new(Kind::Area, 0, Rect::new(0.0, 0.0, 90.0, 90.0).corners().to_vec());
        a.subject = "Floor".into();
        a.scale = Some(Scale::architectural(0.125, 1.0));
        s.add_markup(a.clone()).unwrap();
        s.add_markup(a.clone()).unwrap();
        let mut c = Markup::new(Kind::Count, 0, vec![Point::new(1.0, 1.0), Point::new(2.0, 2.0)]);
        c.subject = "Outlet".into();
        s.add_markup(c).unwrap();
        let mut other = a.clone();
        other.page = 1;
        s.add_markup(other).unwrap();
        let id = s
            .add_legend(0, Point::new(300.0, 700.0), &LegendOptions::default())
            .unwrap();
        let l = s.legends();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].id, id);
        let rows = &l[0].rows;
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!((rows[0].subject.as_str(), rows[0].count), ("Floor", 2));
        assert_eq!(rows[0].total_text, "200 sf");
        assert_eq!(
            (rows[1].subject.as_str(), rows[1].total_text.as_str()),
            ("Outlet", "2 ea")
        );
        let m = s.markup(&id).unwrap();
        assert!(m.contents.contains("Floor\t2\t200 sf"), "{}", m.contents);
        assert!((m.rect.y1 - 700.0).abs() < 1e-6);

        // a new markup, then update: the legend follows
        let mut b = a.clone();
        b.subject = "Base".into();
        s.add_markup(b).unwrap();
        assert_eq!(s.update_legends().unwrap(), 1);
        assert!(s.markup(&id).unwrap().contents.contains("Base"));
        // whole document scope
        let o = LegendOptions {
            document: true,
            title: "All".into(),
            ..Default::default()
        };
        s.set_legend_options(&id, &o).unwrap();
        let l = s.legends();
        assert!(l[0].options.document);
        assert_eq!(l[0].rows[1].count, 3);
        s.save(true).unwrap();
        let s2 = Session::open(&path).unwrap();
        assert_eq!(s2.legends().len(), 1);
        assert_eq!(s2.legends()[0].options.title, "All");
        std::fs::remove_dir_all(&dir).ok();
    }
}
