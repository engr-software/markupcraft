//! The Markups List as data: every column (the standard set plus the document's custom
//! columns), each markup's cell values, and a [`View`] (scope, quick search, column filters,
//! sort, group-by) turned into grouped rows with per-unit subtotals and a grand total.
//!
//! Headless: the app's Markups List panel, the CSV / XML summaries, the CLI and the tests all
//! use this. Adding a standard column: one row in `columns::STANDARD` plus a case in
//! `standard_cell`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::columns::{ColumnType, CustomColumn, TableColumn, review_statuses, standard_columns};
use crate::formula::{Formula, formula_key};
use crate::{Document, FormatArray, Kind, Markup, format_value};

/// How deep formula columns may refer to other formula columns.
const MAX_FORMULA_DEPTH: usize = 16;

/// One cell of the list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    /// what the list shows
    pub text: String,
    /// numeric value (sorting, totals, formulas)
    pub num: Option<f64>,
    /// unit of `num` for totals (`ft`, `sf`, `ea`, `$`)
    pub unit: String,
    /// a formula error (the text says why)
    pub error: bool,
}

impl Cell {
    fn text(t: impl Into<String>) -> Self {
        Self {
            text: t.into(),
            ..Default::default()
        }
    }
    fn num(v: f64, text: impl Into<String>, unit: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            num: Some(v),
            unit: unit.into(),
            error: false,
        }
    }
    fn error(why: &str) -> Self {
        Self {
            text: format!("#ERR {why}"),
            error: true,
            ..Default::default()
        }
    }
}

/// Totals of one group (or of the whole list).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    /// markups
    pub count: usize,
    /// column index -> unit -> sum
    pub sums: BTreeMap<usize, BTreeMap<String, f64>>,
}

impl Totals {
    fn add(&mut self, t: &MarkupTable<'_>, row: usize) {
        self.count += 1;
        for (c, col) in t.cols.iter().enumerate() {
            if !col.total {
                continue;
            }
            let cell = t.cell(row, c);
            if let (Some(v), false) = (cell.num, cell.error) {
                *self.sums.entry(c).or_default().entry(cell.unit.clone()).or_default() += v;
            }
        }
    }

    fn merge(&mut self, other: &Totals) {
        self.count += other.count;
        for (c, units) in &other.sums {
            for (u, v) in units {
                *self.sums.entry(*c).or_default().entry(u.clone()).or_default() += v;
            }
        }
    }
}

/// A group of rows (the root when `column` is `None`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// grouped by this column index
    pub column: Option<usize>,
    /// the group's value ("" = blank)
    pub key: String,
    /// 0 = top-level group
    pub level: usize,
    /// markup indices (leaf groups, or the root without grouping)
    pub rows: Vec<usize>,
    pub children: Vec<Group>,
    pub totals: Totals,
}

impl Group {
    /// Every row of this group and its children, in display order.
    pub fn all_rows(&self) -> Vec<usize> {
        let mut out = self.rows.clone();
        for c in &self.children {
            out.extend(c.all_rows());
        }
        out
    }
}

/// Which markups the list covers.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Scope {
    #[default]
    AllPages,
    /// 0-based page
    CurrentPage(usize),
    /// markup indices
    Selected(BTreeSet<usize>),
}

/// What the list shows: columns, filters, search, sort and grouping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    /// column ids shown (exports, quick search)
    pub visible: Vec<String>,
    /// "" = document order
    pub sort_column: String,
    pub sort_descending: bool,
    /// column ids, outermost first
    pub group_by: Vec<String>,
    /// column id -> allowed cell texts
    pub filters: BTreeMap<String, BTreeSet<String>>,
    /// quick filter over the visible columns, case-insensitive
    pub search: String,
    pub scope: Scope,
    pub measurements_only: bool,
}

/// The Markups List of a document: columns and every cell, computed once.
pub struct MarkupTable<'a> {
    doc: &'a Document,
    cols: Vec<TableColumn>,
    /// [markup][column]
    cells: Vec<Vec<Cell>>,
}

impl<'a> MarkupTable<'a> {
    pub fn new(doc: &'a Document) -> Self {
        let mut cols = standard_columns();
        cols.extend(doc.columns.iter().map(TableColumn::custom));
        let mut t = MarkupTable {
            doc,
            cols,
            cells: Vec::new(),
        };
        t.cells = doc
            .markups
            .iter()
            .map(|m| t.cols.iter().map(|c| t.plain_cell(m, c)).collect())
            .collect();
        // Formula columns read the plain cells (and other formulas, depth-limited).
        let formulas: Vec<(usize, Formula)> = t
            .cols
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let cc = t.custom_of(c)?;
                (cc.kind == ColumnType::Formula).then(|| (i, Formula::parse(&cc.formula)))
            })
            .collect();
        if !formulas.is_empty() {
            let mut computed = Vec::new();
            for mi in 0..doc.markups.len() {
                for (ci, f) in &formulas {
                    computed.push((mi, *ci, t.formula_cell(mi, *ci, f, &formulas, 0)));
                }
            }
            for (mi, ci, cell) in computed {
                if let Some(slot) = t.cells.get_mut(mi).and_then(|r| r.get_mut(ci)) {
                    *slot = cell;
                }
            }
        }
        t
    }

    pub fn document(&self) -> &Document {
        self.doc
    }

    pub fn columns(&self) -> &[TableColumn] {
        &self.cols
    }

    /// A column by id, header, custom id or formula name (`Unit Cost`, `unitcost`, `c:unitcost`).
    pub fn column_index(&self, id: &str) -> Option<usize> {
        if let Some(i) = self.cols.iter().position(|c| c.id == id) {
            return Some(i);
        }
        let key = formula_key(id);
        self.cols
            .iter()
            .position(|c| formula_key(&c.header) == key)
            .or_else(|| {
                self.cols
                    .iter()
                    .position(|c| c.custom && formula_key(custom_id(&c.id)) == key)
            })
            .or_else(|| self.cols.iter().position(|c| formula_key(&c.id) == key))
    }

    /// The cell of markup `row` in column `col` (empty when out of range).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        static EMPTY: Cell = Cell {
            text: String::new(),
            num: None,
            unit: String::new(),
            error: false,
        };
        self.cells.get(row).and_then(|r| r.get(col)).unwrap_or(&EMPTY)
    }

    /// The cell by column id.
    pub fn cell_by_id(&self, row: usize, id: &str) -> &Cell {
        self.cell(row, self.column_index(id).unwrap_or(usize::MAX))
    }

    fn custom_of(&self, c: &TableColumn) -> Option<&'a CustomColumn> {
        if !c.custom {
            return None;
        }
        let id = custom_id(&c.id);
        self.doc.columns.iter().find(|cc| cc.id == id)
    }

    /// Choice values offered when editing (status values, a Choice column's items).
    pub fn choices(&self, row: usize, col: usize) -> Vec<String> {
        let Some(c) = self.cols.get(col) else { return Vec::new() };
        if c.id == "status" {
            return review_statuses().iter().map(|s| s.to_string()).collect();
        }
        let Some(cc) = self.custom_of(c).filter(|cc| cc.kind == ColumnType::Choice) else {
            return Vec::new();
        };
        let subject = self.doc.markups.get(row).map_or("", |m| m.subject.as_str());
        let mut out = vec![String::new()];
        for it in &cc.items {
            if (it.subject.is_empty() || it.subject == subject) && !out.contains(&it.text) {
                out.push(it.text.clone());
            }
        }
        out
    }

    // ---- cell values -----------------------------------------------------------------

    fn plain_cell(&self, m: &Markup, c: &TableColumn) -> Cell {
        if c.custom {
            return match self.custom_of(c) {
                Some(cc) if cc.kind != ColumnType::Formula => custom_cell(cc, m),
                _ => Cell::default(),
            };
        }
        self.standard_cell(m, &c.id)
    }

    fn formula_cell(&self, mi: usize, ci: usize, f: &Formula, formulas: &[(usize, Formula)], depth: usize) -> Cell {
        if depth > MAX_FORMULA_DEPTH {
            return Cell::error("circular");
        }
        let Some(cc) = self.cols.get(ci).and_then(|c| self.custom_of(c)) else {
            return Cell::default();
        };
        let mut any_input = false;
        let mut resolve = |name: &str| -> Option<f64> {
            let k = self.column_index(name)?;
            if k == ci {
                return None;
            }
            let other = match formulas.iter().find(|(i, _)| *i == k) {
                Some((_, g)) => self.formula_cell(mi, k, g, formulas, depth + 1),
                None => self.cell(mi, k).clone(),
            };
            if other.error {
                return None;
            }
            any_input |= other.num.is_some();
            // blank counts as 0, like a spreadsheet
            Some(other.num.unwrap_or(0.0))
        };
        let v = match f.eval(&mut resolve) {
            Ok(v) => v,
            Err(e) => return Cell::error(&e),
        };
        // Every input blank (a cost formula on a text box): the result is blank too.
        if !any_input && !f.names().is_empty() {
            return Cell::default();
        }
        number_cell(cc, v)
    }

    fn standard_cell(&self, m: &Markup, id: &str) -> Cell {
        let q = m.quantity();
        let dist = m.scale.as_ref().filter(|s| s.valid()).map(|s| &s.dist);
        match id {
            "subject" => Cell::text(&m.subject),
            "label" => Cell::text(&m.label),
            "pagelabel" => Cell::text(page_label(self.doc, m.page)),
            "page" => Cell::num((m.page + 1) as f64, (m.page + 1).to_string(), ""),
            "author" => Cell::text(&m.author),
            "date" => Cell::text(revu_date(if m.modified.is_empty() { &m.created } else { &m.modified })),
            "created" => Cell::text(revu_date(&m.created)),
            "status" => Cell::text(&m.status),
            "checkmark" => check_cell(m.checked, "Checked"),
            "lock" => check_cell(m.locked(), "Locked"),
            // the captured file of a File Attachment (Capture)
            "capture" => Cell::text(if m.kind == Kind::Attachment {
                if m.attachment_name.is_empty() {
                    "Attachment"
                } else {
                    m.attachment_name.as_str()
                }
            } else {
                ""
            }),
            // a legend (a text box titled by its subject "Legend")
            "legend" => check_cell(m.kind == Kind::Text && m.subject == "Legend", "Legend"),
            // the 3D view a markup was placed in: MarkupCraft has no 3D views, so it is empty
            "view3d" => Cell::text(""),
            "color" => Cell::text(m.color.hex()),
            "layer" => Cell::text(&m.layer),
            "space" => Cell::text(crate::spaces::space_path(self.doc, m)),
            "comments" => Cell::text(comments(m)),
            "type" => Cell::text(m.kind.name()),
            "id" => Cell::text(&m.id),
            "replies" => {
                let n = m.replies.len();
                Cell::num(n as f64, if n == 0 { String::new() } else { n.to_string() }, "")
            }
            "unit" => Cell::text(if q.is_some() { m.unit() } else { String::new() }),
            "measurement" => match q {
                Some(v) if m.kind == Kind::Count => Cell::num(v, format!("{} ea", v as i64), "ea"),
                Some(v) => Cell::num(v, m.quantity_text(), m.unit()),
                None => Cell::default(),
            },
            "length" => match q {
                Some(v) if matches!(m.kind, Kind::Length | Kind::Polylength) => {
                    Cell::num(v, m.quantity_text(), m.unit())
                }
                _ => Cell::default(),
            },
            "area" => match q {
                Some(v) if m.kind == Kind::Area => Cell::num(v, m.quantity_text(), m.unit()),
                _ => Cell::default(),
            },
            "volume" => match q {
                Some(v) if m.kind == Kind::Volume => Cell::num(v, m.quantity_text(), m.unit()),
                _ => Cell::default(),
            },
            "count" => match q {
                Some(v) if m.kind == Kind::Count => Cell::num(v, (v as i64).to_string(), "ea"),
                _ => Cell::default(),
            },
            "perimeter" => match (&m.scale, dist) {
                (Some(s), Some(fa)) if matches!(m.kind, Kind::Area | Kind::Perimeter | Kind::Volume) => {
                    measured(s.length_of(&m.pts, true), fa)
                }
                _ => Cell::default(),
            },
            "depth" if m.kind != Kind::Count => match dist {
                Some(fa) if m.kind == Kind::Volume || m.depth > 0.0 => measured(m.depth, fa),
                _ => Cell::default(),
            },
            "risedrop" => match dist {
                Some(fa) if m.kind == Kind::Polylength && m.rise_drop != 0.0 => measured(m.rise_drop, fa),
                _ => Cell::default(),
            },
            "width" | "height" | "depth" if m.kind == Kind::Count => {
                // a count's item dimensions, in the unit of the scale where it sits
                let v = match id {
                    "width" => m.item_width,
                    "height" => m.item_height,
                    _ => m.depth,
                };
                let page_scale = m.pts.first().and_then(|p| self.doc.pages.get(m.page)?.scale_at(*p));
                match m.scale.as_ref().filter(|s| s.valid()).or(page_scale) {
                    Some(s) if v > 0.0 => measured(v, &s.dist),
                    _ => Cell::default(),
                }
            }
            "width" | "height" => match (&m.scale, dist) {
                (Some(s), Some(fa))
                    if !m.pts.is_empty()
                        && (matches!(m.kind, Kind::Rectangle | Kind::Ellipse)
                            || (m.kind == Kind::Area && m.subtype == "Square")) =>
                {
                    let Some(b) = markupcraft_geom::bbox(&m.pts) else {
                        return Cell::default();
                    };
                    let (a, z) = if id == "width" {
                        (crate::Point::new(b.x0, 0.0), crate::Point::new(b.x1, 0.0))
                    } else {
                        (crate::Point::new(0.0, b.y0), crate::Point::new(0.0, b.y1))
                    };
                    measured(s.length_of(&[a, z], false), fa)
                }
                _ => Cell::default(),
            },
            "wallarea" => match (crate::measure_extras::wall_area(m), m.scale.as_ref()) {
                (Some(v), Some(s)) => measured(v, &s.area),
                _ => Cell::default(),
            },
            "slope" => match m.slope_of() {
                crate::measure_extras::Slope::None => Cell::default(),
                sl => Cell::num(sl.value(), sl.label(), ""),
            },
            "x" | "y" | "xcenter" | "ycenter" => {
                // the markup's extent, inches from the page's lower-left corner
                let b = markupcraft_geom::bbox(&m.pts).unwrap_or_else(|| m.rect.normalized());
                let origin = self.doc.pages.get(m.page).map(|p| p.crop.normalized());
                let (ox, oy) = origin.map_or((0.0, 0.0), |c| (c.x0, c.y0));
                let v = match id {
                    "x" => b.x0 - ox,
                    "y" => b.y0 - oy,
                    "xcenter" => (b.x0 + b.x1) / 2.0 - ox,
                    _ => (b.y0 + b.y1) / 2.0 - oy,
                } / 72.0;
                Cell::num(v, format!("{v:.2} in"), "in")
            }
            "sequence" => {
                // the number a sequence tool gave it: the label's trailing number
                let digits: String = m
                    .label
                    .chars()
                    .rev()
                    .take_while(char::is_ascii_digit)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                match digits.parse::<u64>() {
                    Ok(n) if digits.len() <= 15 => Cell::num(n as f64, digits, ""),
                    _ => Cell::default(),
                }
            }
            "docwidth" | "docheight" => match self.doc.pages.get(m.page).map(|p| p.crop.normalized()) {
                Some(c) => {
                    let v = if id == "docwidth" { c.width() } else { c.height() } / 72.0;
                    Cell::num(v, format!("{v:.2} in"), "in")
                }
                None => Cell::default(),
            },
            _ => Cell::default(),
        }
    }

    // ---- building --------------------------------------------------------------------

    fn in_scope(&self, row: usize, v: &View) -> bool {
        let Some(m) = self.doc.markups.get(row) else {
            return false;
        };
        if v.measurements_only && !m.kind.is_measurement() {
            return false;
        }
        match &v.scope {
            Scope::AllPages => true,
            Scope::CurrentPage(p) => m.page == *p,
            Scope::Selected(sel) => sel.contains(&row),
        }
    }

    /// Distinct texts of a column over the markups in scope (a filter's choices), sorted.
    pub fn distinct_values(&self, col: usize, view: &View) -> Vec<String> {
        let set: BTreeSet<&str> = (0..self.cells.len())
            .filter(|&i| self.in_scope(i, view))
            .map(|i| self.cell(i, col).text.as_str())
            .collect();
        let mut out: Vec<String> = set.into_iter().map(str::to_string).collect();
        out.sort_by(|a, b| natural_cmp(a, b));
        out
    }

    /// Rows matching `view`, grouped and sorted, with totals.
    pub fn build(&self, view: &View) -> Group {
        let visible: Vec<usize> = view.visible.iter().filter_map(|id| self.column_index(id)).collect();
        let needle = view.search.to_lowercase();
        let filters: Vec<(usize, &BTreeSet<String>)> = view
            .filters
            .iter()
            .filter_map(|(id, allowed)| Some((self.column_index(id)?, allowed)))
            .collect();
        let mut rows: Vec<usize> = (0..self.cells.len())
            .filter(|&i| self.in_scope(i, view))
            .filter(|&i| {
                filters
                    .iter()
                    .all(|(k, allowed)| allowed.contains(&self.cell(i, *k).text))
            })
            .filter(|&i| {
                needle.is_empty()
                    || visible
                        .iter()
                        .any(|&k| self.cell(i, k).text.to_lowercase().contains(&needle))
            })
            .collect();

        // Sort: by the sort column (numbers as numbers), then creation date, then file order.
        let sc = if view.sort_column.is_empty() {
            None
        } else {
            self.column_index(&view.sort_column)
        };
        if let Some(sc) = sc {
            let numeric = self.cols.get(sc).is_some_and(|c| c.numeric);
            rows.sort_by(|&a, &b| {
                let (x, y) = (self.cell(a, sc), self.cell(b, sc));
                let mut r = if numeric && (x.num.is_some() || y.num.is_some()) {
                    match (x.num, y.num) {
                        (None, _) => Ordering::Less,
                        (_, None) => Ordering::Greater,
                        (Some(p), Some(q)) => p.total_cmp(&q),
                    }
                } else {
                    natural_cmp(&x.text, &y.text)
                };
                if r == Ordering::Equal {
                    let ca = self.doc.markups.get(a).map_or("", |m| m.created.as_str());
                    let cb = self.doc.markups.get(b).map_or("", |m| m.created.as_str());
                    r = ca.cmp(cb);
                    if r == Ordering::Equal {
                        return a.cmp(&b);
                    }
                }
                if view.sort_descending { r.reverse() } else { r }
            });
        }

        let group_cols: Vec<usize> = view.group_by.iter().filter_map(|id| self.column_index(id)).collect();
        let mut root = Group::default();
        self.fill(&mut root, rows, &group_cols, 0, sc, view.sort_descending);
        root
    }

    fn fill(&self, g: &mut Group, rows: Vec<usize>, group_cols: &[usize], level: usize, sc: Option<usize>, desc: bool) {
        let Some(&gc) = group_cols.get(level) else {
            for &r in &rows {
                g.totals.add(self, r);
            }
            g.rows = rows;
            return;
        };
        let mut keys: Vec<String> = Vec::new();
        let mut parts: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for r in rows {
            let k = &self.cell(r, gc).text;
            if !parts.contains_key(k) {
                keys.push(k.clone());
            }
            parts.entry(k.clone()).or_default().push(r);
        }
        let numeric = self.cols.get(gc).is_some_and(|c| c.numeric);
        let first_num = |k: &String| parts.get(k).and_then(|v| v.first()).and_then(|&r| self.cell(r, gc).num);
        keys.sort_by(|a, b| {
            let mut r = Ordering::Equal;
            if numeric && let (Some(x), Some(y)) = (first_num(a), first_num(b)) {
                r = x.total_cmp(&y);
            }
            if r == Ordering::Equal {
                r = natural_cmp(a, b);
            }
            if sc == Some(gc) && desc { r.reverse() } else { r }
        });
        for k in keys {
            let part = parts.remove(&k).unwrap_or_default();
            let mut child = Group {
                column: Some(gc),
                key: k,
                level,
                ..Default::default()
            };
            self.fill(&mut child, part, group_cols, level + 1, sc, desc);
            g.totals.merge(&child.totals);
            g.children.push(child);
        }
    }

    /// `"1,234.50 sf; 12 ea"` for a totals entry of column `col`.
    pub fn total_text(&self, col: usize, sums: &BTreeMap<String, f64>) -> String {
        let Some(c) = self.cols.get(col) else {
            return String::new();
        };
        let cc = self.custom_of(c);
        let parts: Vec<String> = sums
            .iter()
            .map(|(unit, &v)| {
                if let Some(cc) = cc {
                    return number_cell(cc, v).text;
                }
                let with_unit = |s: String| if unit.is_empty() { s } else { format!("{s} {unit}") };
                if unit == "ea" || c.id == "count" {
                    with_unit(format_number(v, 0, true))
                } else {
                    with_unit(format_number(v, 2, true))
                }
            })
            .collect();
        parts.join("; ")
    }

    /// The totals of column `col` in `totals` as text ("" when the column has none).
    pub fn totals_text(&self, col: usize, totals: &Totals) -> String {
        totals
            .sums
            .get(&col)
            .map(|s| self.total_text(col, s))
            .unwrap_or_default()
    }

    // ---- editing ---------------------------------------------------------------------

    /// Edit a cell from text. Returns false when the column is read-only, unknown, or the text
    /// is not accepted; on success the markup is changed and marked dirty.
    pub fn set_cell(doc: &mut Document, row: usize, column_id: &str, text: &str) -> bool {
        let cid = custom_id(column_id);
        if !cid.is_empty() {
            let Some(cc) = doc.columns.iter().find(|c| c.id == cid).cloned() else {
                return false;
            };
            let Some(m) = doc.markups.get_mut(row) else {
                return false;
            };
            if cc.kind == ColumnType::Formula {
                return false;
            }
            let mut v = text.to_string();
            if cc.numeric() {
                if !v.trim().is_empty() {
                    let Some(n) = parse_number(&v) else { return false };
                    v = trim_number(n);
                } else {
                    v.clear();
                }
            } else if cc.kind == ColumnType::Checkmark {
                v = if truthy(text) { "1".into() } else { String::new() };
            } else if cc.kind == ColumnType::Choice
                && !cc.allow_custom
                && !v.is_empty()
                && !cc.items.iter().any(|it| it.text == v)
            {
                return false;
            }
            if v.is_empty() {
                m.column_data.remove(cid);
            } else {
                m.column_data.insert(cid.to_string(), v);
            }
            m.dirty = true;
            return true;
        }
        let Some(m) = doc.markups.get_mut(row) else {
            return false;
        };
        match column_id {
            "subject" => m.subject = text.into(),
            "label" => m.label = text.into(),
            "comments" => {
                // A measurement's /Contents is its value; text markups are edited on the canvas.
                if m.kind.is_measurement() || m.kind.is_text() {
                    return false;
                }
                m.contents = text.into();
            }
            "status" => {
                let t = text.trim();
                m.status = if t.is_empty() || t.eq_ignore_ascii_case("none") {
                    String::new()
                } else {
                    t.into()
                };
            }
            "checkmark" => m.checked = truthy(text),
            "lock" => m.set_locked(truthy(text)),
            _ => return false,
        }
        m.dirty = true;
        true
    }
}

// ---- helpers ---------------------------------------------------------------------------

/// `c:x` -> `x`, else "".
pub fn custom_id(column_id: &str) -> &str {
    column_id.strip_prefix("c:").unwrap_or("")
}

/// The page's label, or its 1-based number when it has none.
pub fn page_label(doc: &Document, page: usize) -> String {
    match doc.pages.get(page) {
        Some(p) if !p.label.is_empty() => p.label.clone(),
        _ => (page + 1).to_string(),
    }
}

fn check_cell(on: bool, word: &str) -> Cell {
    Cell::num(if on { 1.0 } else { 0.0 }, if on { word } else { "" }, "")
}

fn unit_label(fa: &FormatArray) -> String {
    match fa.first().map(|f| f.unit.as_str()) {
        Some("'") => "ft".into(),
        Some("\"") => "in".into(),
        Some(u) => u.into(),
        None => String::new(),
    }
}

fn measured(v: f64, fa: &FormatArray) -> Cell {
    Cell::num(v, format_value(v, fa), unit_label(fa))
}

fn comments(m: &Markup) -> String {
    let mut t = m.contents.clone();
    // Revu keeps a measurement's shown value in /Contents; that is not a comment.
    if m.kind.is_measurement() {
        let shown = m.quantity_text();
        if t.is_empty() || t == shown || t == format!("{shown} ea") || m.kind == Kind::Count && t.ends_with(" ea") {
            t.clear();
        }
    }
    if m.kind.is_text() {
        t = t.replace("\r\n", " ").replace(['\r', '\n'], " ");
    }
    t
}

fn custom_cell(cc: &CustomColumn, m: &Markup) -> Cell {
    let raw = m.column_data.get(&cc.id).unwrap_or(&cc.default_value);
    match cc.kind {
        ColumnType::Date => {
            // the default "current date" is the markup's creation date
            let src = if raw == crate::columns::TODAY {
                m.created.as_str()
            } else {
                raw.as_str()
            };
            match crate::columns::parse_date(src) {
                Some(d) => Cell::text(crate::columns::format_date(d, &cc.date_format)),
                None if raw == crate::columns::TODAY => Cell::default(),
                None => Cell::text(raw),
            }
        }
        ColumnType::Text => Cell::text(raw),
        ColumnType::Checkmark => check_cell(truthy(raw), "Checked"),
        ColumnType::Choice => Cell {
            text: raw.clone(),
            num: choice_value(cc, raw, &m.subject),
            ..Default::default()
        },
        ColumnType::Number | ColumnType::Currency | ColumnType::Percent => match parse_number(raw) {
            Some(v) => number_cell(cc, v),
            None => Cell::default(),
        },
        ColumnType::Formula => Cell::default(),
    }
}

/// A number shown in a numeric custom column's format.
fn number_cell(cc: &CustomColumn, v: f64) -> Cell {
    let kind = if cc.kind == ColumnType::Formula {
        cc.display
    } else {
        cc.kind
    };
    match kind {
        ColumnType::Currency => {
            let sign = if v < 0.0 { "-" } else { "" };
            Cell::num(
                v,
                format!("{sign}{}{}", cc.symbol, format_number(v.abs(), cc.decimals, true)),
                cc.symbol.clone(),
            )
        }
        ColumnType::Percent => Cell::num(v, format!("{}%", format_number(v, cc.decimals, true)), "%"),
        _ => Cell::num(v, format_number(v, cc.decimals, true), ""),
    }
}

/// A Choice item's value: the item for this subject first, else one for any subject.
fn choice_value(cc: &CustomColumn, text: &str, subject: &str) -> Option<f64> {
    let mut any = None;
    for it in cc.items.iter().filter(|it| it.text == text) {
        if it.subject == subject {
            return it.value;
        }
        if it.subject.is_empty() || any.is_none() {
            any = it.value;
        }
    }
    any
}

/// `D:YYYYMMDDHHmmSS...` -> `YYYY-MM-DD HH:mm`; anything else unchanged.
pub fn revu_date(d: &str) -> String {
    let Some(s) = d.strip_prefix("D:") else {
        return d.to_string();
    };
    let b = s.as_bytes();
    if b.len() < 12 || !b.iter().take(12).all(u8::is_ascii_digit) {
        return d.to_string();
    }
    let p = |a: usize, z: usize| s.get(a..z).unwrap_or("");
    format!("{}-{}-{} {}:{}", p(0, 4), p(4, 6), p(6, 8), p(8, 10), p(10, 12))
}

/// 1234.5 -> `1,234.50` (decimals clamped to 0..=10; no `-0`).
pub fn format_number(v: f64, decimals: i32, thousands: bool) -> String {
    let d = decimals.clamp(0, 10) as usize;
    let mut s = format!("{v:.d$}");
    if s.starts_with('-') && s.chars().skip(1).all(|c| c == '0' || c == '.') {
        s.remove(0);
    }
    if !thousands {
        return s;
    }
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r.to_string()),
        None => ("", s.clone()),
    };
    let (int, frac) = match rest.find('.') {
        Some(i) => (rest.get(..i).unwrap_or(""), rest.get(i..).unwrap_or("")),
        None => (rest.as_str(), ""),
    };
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{sign}{grouped}{frac}")
}

/// A number from user text: `$1,234.5` -> 1234.5. Keeps digits, `.`, `-`, `+`, `e`.
pub fn parse_number(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Shortest text for a stored number (`12.5`, not `12.5000000000`).
fn trim_number(v: f64) -> String {
    let s = format_number(v, 10, false);
    if !s.contains('.') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Checkbox text: `1`, `true`, `yes`, `checked`, `x`, `on`, `locked`.
pub fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "checked" | "x" | "on" | "locked"
    )
}

/// Case-insensitive compare with digit runs compared as numbers (`A2` < `A10`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while let (Some(&ca), Some(&cb)) = (a.get(i), b.get(j)) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let i2 = i + a
                .get(i..)
                .map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count());
            let j2 = j + b
                .get(j..)
                .map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count());
            let strip = |v: &[char]| -> String {
                let s: String = v.iter().collect();
                s.trim_start_matches('0').to_string()
            };
            let na = strip(a.get(i..i2).unwrap_or_default());
            let nb = strip(b.get(j..j2).unwrap_or_default());
            let r = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if r != Ordering::Equal {
                return r;
            }
            i = i2;
            j = j2;
            continue;
        }
        let (la, lb) = (ca.to_lowercase().next(), cb.to_lowercase().next());
        if la != lb {
            return la.cmp(&lb);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i.min(a.len())).cmp(&(b.len() - j.min(b.len())))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::columns::ChoiceItem;
    use crate::{Color, PageInfo, Point, Scale};

    fn area(page: usize, side_ft: f64, subject: &str) -> Markup {
        let s = side_ft * 9.0; // 1/8" = 1'-0": 9 pt per foot
        let mut m = Markup::new(
            Kind::Area,
            page,
            vec![
                Point::new(100.0, 100.0),
                Point::new(100.0 + s, 100.0),
                Point::new(100.0 + s, 100.0 + s),
                Point::new(100.0, 100.0 + s),
            ],
        );
        m.subject = subject.into();
        m.scale = Some(Scale::architectural(0.125, 1.0));
        m.color = Color::rgb(0.0, 0.0, 1.0);
        m
    }

    fn length(page: usize, feet: f64, subject: &str) -> Markup {
        let mut m = Markup::new(
            Kind::Length,
            page,
            vec![Point::new(50.0, 50.0), Point::new(50.0 + feet * 9.0, 50.0)],
        );
        m.subject = subject.into();
        m.scale = Some(Scale::architectural(0.125, 1.0));
        m
    }

    fn near(got: Option<f64>, want: f64, what: &str) {
        assert!(
            got.is_some_and(|g| (g - want).abs() < 1e-6),
            "{what}: got {got:?} want {want}"
        );
    }

    pub(crate) fn sample() -> Document {
        let mut doc = Document {
            pages: vec![
                PageInfo {
                    label: "A1.01".into(),
                    ..Default::default()
                },
                PageInfo {
                    label: "A1.02".into(),
                    ..Default::default()
                },
            ],
            markups: vec![
                area(0, 10.0, "Floor"),
                area(1, 20.0, "Floor"),
                length(0, 30.0, "Duct"),
                length(1, 5.0, "Duct"),
            ],
            ..Default::default()
        };
        let cost = CustomColumn {
            id: "unitcost".into(),
            name: "Unit Cost".into(),
            kind: ColumnType::Choice,
            items: vec![
                ChoiceItem {
                    text: "Cheap".into(),
                    subject: String::new(),
                    value: Some(2.0),
                },
                ChoiceItem {
                    text: "Dear".into(),
                    subject: String::new(),
                    value: Some(10.0),
                },
                ChoiceItem {
                    text: "Dear".into(),
                    subject: "Duct".into(),
                    value: Some(7.0),
                },
            ],
            ..Default::default()
        };
        let total = CustomColumn {
            id: "total".into(),
            name: "Total".into(),
            kind: ColumnType::Formula,
            formula: "Measurement * [Unit Cost]".into(),
            ..Default::default()
        };
        let price = CustomColumn {
            id: "price".into(),
            name: "Price".into(),
            kind: ColumnType::Currency,
            ..Default::default()
        };
        doc.columns = vec![cost, total, price];
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:unitcost", "Cheap"));
        assert!(MarkupTable::set_cell(&mut doc, 1, "c:unitcost", "Dear"));
        assert!(MarkupTable::set_cell(&mut doc, 2, "c:unitcost", "Dear"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:price", "$1,234.5"));
        assert!(MarkupTable::set_cell(&mut doc, 2, "status", "Accepted"));
        assert!(MarkupTable::set_cell(&mut doc, 1, "checkmark", "1"));
        doc
    }

    #[test]
    fn list_default_columns_in_revu_order() {
        assert_eq!(
            crate::columns::default_visible_columns(),
            [
                "subject",
                "pagelabel",
                "label",
                "measurement",
                "author",
                "date",
                "status",
                "checkmark",
                "color",
                "comments"
            ]
        );
        let doc = sample();
        let t = MarkupTable::new(&doc);
        let n = standard_columns().len();
        assert_eq!(t.columns().len(), n + 3);
        assert_eq!(t.columns().get(n).map(|c| c.id.as_str()), Some("c:unitcost"));
    }

    #[test]
    fn list_formula_display_and_date_formats() {
        let mut doc = sample();
        for c in &mut doc.columns {
            if c.kind == ColumnType::Formula {
                c.display = ColumnType::Currency;
            }
        }
        doc.columns.push(CustomColumn {
            id: "due".into(),
            name: "Due".into(),
            kind: ColumnType::Date,
            date_format: "MMM d, yyyy".into(),
            default_value: crate::columns::TODAY.into(),
            ..Default::default()
        });
        doc.markups[0].created = "D:20261009120000".into();
        doc.markups[1].column_data.insert("due".into(), "2026-12-25".into());
        let t = MarkupTable::new(&doc);
        assert_eq!(
            t.cell_by_id(0, "c:total").text,
            "$200.00",
            "a formula shown as currency"
        );
        assert_eq!(t.cell_by_id(0, "c:due").text, "Oct 9, 2026", "the current-date default");
        assert_eq!(t.cell_by_id(1, "c:due").text, "Dec 25, 2026");
        assert_eq!(t.cell_by_id(2, "c:due").text, "", "no creation date: blank");
        assert_eq!(crate::columns::format_date((2026, 1, 2), "dd/MM/yyyy"), "02/01/2026");
        assert_eq!(crate::columns::parse_date("01/31/2026"), Some((2026, 1, 31)));
        assert_eq!(crate::columns::parse_date("2026-13-01"), None);
    }

    #[test]
    fn list_wall_area_slope_and_geometry_columns() {
        let mut doc = sample();
        doc.markups[0].depth = 2.0;
        doc.markups[2].slope_type = 1;
        doc.markups[2].slope = 12.0;
        let t = MarkupTable::new(&doc);
        // 10 ft square, 2 ft deep: 40 ft perimeter x 2 ft
        assert_eq!(t.cell_by_id(0, "wallarea").text, "80 sf");
        assert_eq!(t.cell_by_id(1, "wallarea").text, "");
        assert_eq!(t.cell_by_id(2, "slope").text, "12:12");
        // 30 ft at 12:12 is 42.43 ft of slope
        near(t.cell_by_id(2, "length").num, 30.0 * 2f64.sqrt(), "sloped length");
        // the area's lower-left corner is 100 pt from the page corner; letter is 8.5 x 11
        assert_eq!(t.cell_by_id(0, "x").text, "1.39 in");
        assert_eq!(
            t.cell_by_id(0, "ycenter").text,
            format!("{:.2} in", (100.0 + 45.0) / 72.0)
        );
        assert_eq!(t.cell_by_id(0, "docwidth").text, "8.50 in");
        assert_eq!(t.cell_by_id(0, "docheight").text, "11.00 in");
    }

    #[test]
    fn list_set_cell_validates_input() {
        let mut doc = sample();
        assert!(!MarkupTable::set_cell(&mut doc, 3, "c:unitcost", "Unknown"));
        assert!(!MarkupTable::set_cell(&mut doc, 0, "c:price", "abc"));
        assert!(!MarkupTable::set_cell(&mut doc, 0, "c:total", "5"));
        assert!(!MarkupTable::set_cell(&mut doc, 99, "subject", "x"));
        assert!(!MarkupTable::set_cell(&mut doc, 0, "measurement", "5"));
        assert!(!MarkupTable::set_cell(
            &mut doc,
            0,
            "comments",
            "a measurement's text is its value"
        ));
        assert_eq!(
            doc.markups[0].column_data.get("price").map(String::as_str),
            Some("1234.5")
        );
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:price", ""));
        assert!(!doc.markups[0].column_data.contains_key("price"));
        assert!(MarkupTable::set_cell(&mut doc, 2, "status", "None"));
        assert_eq!(doc.markups[2].status, "");
        assert!(MarkupTable::set_cell(&mut doc, 3, "lock", "yes"));
        assert!(doc.markups[3].locked());
    }

    #[test]
    fn list_cells_formula_choice_currency_status() {
        let doc = sample();
        let t = MarkupTable::new(&doc);
        let ci = t.column_index("c:total").unwrap_or(usize::MAX);
        near(t.cell(0, ci).num, 100.0 * 2.0, "area 100 sf x 2");
        near(t.cell(1, ci).num, 400.0 * 10.0, "area 400 sf x 10");
        near(t.cell(2, ci).num, 30.0 * 7.0, "choice value by subject: 30 ft x 7");
        near(t.cell(3, ci).num, 0.0, "blank choice counts as 0");
        assert_eq!(t.cell_by_id(0, "c:price").text, "$1,234.50");
        assert_eq!(t.cell_by_id(1, "pagelabel").text, "A1.02");
        assert_eq!(t.cell_by_id(2, "status").text, "Accepted");
        assert_eq!(t.cell_by_id(1, "checkmark").text, "Checked");
        assert_eq!(t.cell_by_id(0, "color").text, "#0000FF");
        assert_eq!(t.cell_by_id(0, "perimeter").text, "40'-0\"");
        assert_eq!(t.column_index("Unit Cost"), t.column_index("c:unitcost"));
        assert_eq!(t.column_index("unit_cost"), t.column_index("c:unitcost"));
        assert_eq!(
            t.choices(2, t.column_index("c:unitcost").unwrap_or(0)),
            ["", "Cheap", "Dear"]
        );
        assert_eq!(t.choices(0, t.column_index("status").unwrap_or(0)).len(), 5);
    }

    #[test]
    fn list_group_sort_and_totals_per_unit() {
        let doc = sample();
        let t = MarkupTable::new(&doc);
        let mc = t.column_index("measurement").unwrap_or(usize::MAX);
        let ci = t.column_index("c:total").unwrap_or(usize::MAX);
        let mut v = View {
            visible: vec!["subject".into(), "measurement".into(), "c:total".into()],
            group_by: vec!["subject".into()],
            sort_column: "measurement".into(),
            sort_descending: true,
            ..Default::default()
        };
        let root = t.build(&v);
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].key, "Duct");
        assert_eq!(root.children[0].rows, [2, 3]); // 30 ft before 5 ft
        assert_eq!(t.totals_text(mc, &root.children[1].totals), "500.00 sf");
        assert_eq!(t.totals_text(mc, &root.totals), "35.00 ft; 500.00 sf");
        assert_eq!(t.totals_text(ci, &root.totals), "4,410.00");
        assert_eq!(root.totals.count, 4);

        // Nested: subject, then page label.
        v.group_by = vec!["subject".into(), "pagelabel".into()];
        let root = t.build(&v);
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].children.len(), 2);
        assert_eq!(root.children[0].children[0].level, 1);

        // Group by any column: status, color, author, page.
        for col in ["status", "color", "author", "page", "layer", "type", "c:unitcost"] {
            v.group_by = vec![col.into()];
            let root = t.build(&v);
            assert_eq!(root.totals.count, 4, "group by {col}");
            assert_eq!(root.all_rows().len(), 4, "group by {col}");
        }
        v.group_by = vec!["status".into()];
        let root = t.build(&v);
        assert_eq!(
            root.children.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(),
            ["", "Accepted"]
        );

        // Ascending numeric sort without grouping.
        v.group_by.clear();
        v.sort_descending = false;
        assert_eq!(t.build(&v).rows, [3, 2, 0, 1]);
    }

    #[test]
    fn list_filters_search_and_scope() {
        let doc = sample();
        let t = MarkupTable::new(&doc);
        let mut v = View {
            visible: vec!["subject".into(), "measurement".into()],
            ..Default::default()
        };
        v.filters.insert("subject".into(), ["Floor".to_string()].into());
        assert_eq!(t.build(&v).rows.len(), 2);
        v.filters.clear();
        v.search = "duc".into();
        assert_eq!(t.build(&v).rows.len(), 2);
        v.search.clear();
        v.scope = Scope::CurrentPage(1);
        assert_eq!(t.build(&v).rows.len(), 2);
        v.scope = Scope::Selected([3].into());
        assert_eq!(t.build(&v).rows, [3]);
        v.scope = Scope::AllPages;
        let subj = t.column_index("subject").unwrap_or(0);
        assert_eq!(t.distinct_values(subj, &v), ["Duct", "Floor"]);
    }

    #[test]
    fn list_formula_errors_and_blanks() {
        let mut d2 = sample();
        d2.columns[1].formula = "Total + 1".into();
        let t2 = MarkupTable::new(&d2);
        assert!(t2.cell_by_id(0, "c:total").error);
        d2.columns[1].formula = "Price / 0".into();
        let t2 = MarkupTable::new(&d2);
        assert!(t2.cell_by_id(0, "c:total").text.contains("zero"));

        // Two formulas that refer to each other: an error, not a hang.
        let mut d4 = sample();
        d4.columns[1].formula = "Other + 1".into();
        d4.columns.push(CustomColumn {
            id: "other".into(),
            name: "Other".into(),
            kind: ColumnType::Formula,
            formula: "Total * 2".into(),
            ..Default::default()
        });
        let t4 = MarkupTable::new(&d4);
        assert!(t4.cell_by_id(0, "c:total").error);

        // A formula whose inputs are all blank (a text box) is blank, not 0.
        let mut d3 = sample();
        d3.markups.push(Markup::new(Kind::Text, 0, Vec::new()));
        let t3 = MarkupTable::new(&d3);
        let c = t3.cell_by_id(4, "c:total");
        assert!(c.num.is_none() && c.text.is_empty());
    }

    #[test]
    fn list_custom_column_types() {
        let mut doc = sample();
        let kinds = [
            ("note", ColumnType::Text, ""),
            ("qty", ColumnType::Number, ""),
            ("waste", ColumnType::Percent, ""),
            ("due", ColumnType::Date, ""),
            ("done", ColumnType::Checkmark, ""),
            ("net", ColumnType::Formula, "Area * (1 + Waste / 100)"),
        ];
        for (id, kind, formula) in kinds {
            doc.columns.push(CustomColumn {
                id: id.into(),
                name: id.into(),
                kind,
                formula: formula.into(),
                decimals: 1,
                ..Default::default()
            });
        }
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:note", "east wing"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:qty", "1,250"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:waste", "10%"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:due", "2026-10-09"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:done", "yes"));
        let t = MarkupTable::new(&doc);
        assert_eq!(t.cell_by_id(0, "c:note").text, "east wing");
        assert_eq!(t.cell_by_id(0, "c:qty").text, "1,250.0");
        assert_eq!(t.cell_by_id(0, "c:waste").text, "10.0%");
        assert_eq!(t.cell_by_id(0, "c:due").text, "2026-10-09");
        assert_eq!(t.cell_by_id(0, "c:done").text, "Checked");
        near(t.cell_by_id(0, "c:net").num, 110.0, "area with 10% waste");
        assert_eq!(t.cell_by_id(0, "c:net").text, "110.0");
        assert_eq!(t.cell_by_id(1, "c:done").text, "");
    }

    #[test]
    fn list_text_markups_and_dates() {
        let mut doc = Document::default();
        let mut tw = Markup::new(Kind::Typewriter, 0, Vec::new());
        tw.contents = "Line one\rLine two".into();
        tw.created = "D:20261009153000Z".into();
        let mut ar = Markup::new(Kind::Arrow, 0, Vec::new());
        ar.contents = "see detail 4".into();
        doc.markups = vec![tw, ar];
        let t = MarkupTable::new(&doc);
        assert_eq!(t.cell_by_id(0, "comments").text, "Line one Line two");
        assert_eq!(t.cell_by_id(0, "type").text, "Typewriter");
        assert_eq!(t.cell_by_id(0, "date").text, "2026-10-09 15:30");
        assert_eq!(t.cell_by_id(0, "pagelabel").text, "1");
        assert!(!MarkupTable::set_cell(&mut doc, 0, "comments", "typed in the list"));
        assert!(MarkupTable::set_cell(&mut doc, 1, "comments", "new note"));
        assert_eq!(doc.markups[1].contents, "new note");
    }

    #[test]
    fn list_helpers() {
        assert_eq!(format_number(1234.5, 2, true), "1,234.50");
        assert_eq!(format_number(-1234567.0, 0, true), "-1,234,567");
        assert_eq!(format_number(-0.001, 2, true), "0.00");
        assert_eq!(format_number(12.0, 99, false), "12.0000000000");
        assert_eq!(trim_number(12.5), "12.5");
        assert_eq!(trim_number(3.0), "3");
        assert_eq!(natural_cmp("A2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("b", "A"), Ordering::Greater);
        assert_eq!(natural_cmp("x01", "x1"), Ordering::Equal);
        assert_eq!(revu_date("D:2026"), "D:2026");
        assert_eq!(revu_date("D:2026é10091530"), "D:2026é10091530");
        assert_eq!(make_id("Unit Cost"), "unitcost");
        assert_eq!(make_id("2nd"), "col2nd");
    }

    fn make_id(n: &str) -> String {
        crate::columns::make_column_id(n, &[])
    }
}
