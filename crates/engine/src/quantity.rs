//! Quantity Link: named links from a workbook cell to a measurement total over one or more PDFs,
//! filtered by subject, layer, label, colour, author and pages. Totals are recomputed from the
//! files on Update and written into an Excel workbook of our own at each link's sheet and cell
//! (a Links sheet lists every link with its value and where it came from).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use markupcraft_model::{Kind, Markup};
use serde::{Deserialize, Serialize};

use crate::convert::{TextTable, xlsx};
use crate::{Result, Session, invalid};

/// What a link totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuantityMeasure {
    /// Items (a Count markup counts its points; other markups one each).
    #[default]
    Count,
    /// Length, Polylength and Perimeter.
    Length,
    Area,
    Volume,
    /// Every measurement's quantity, whatever its type (units should agree).
    Quantity,
    /// The sum of a custom Markups List column (its id).
    Column(String),
}

impl QuantityMeasure {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "count" => Self::Count,
            "length" => Self::Length,
            "area" => Self::Area,
            "volume" => Self::Volume,
            "quantity" | "total" => Self::Quantity,
            other => {
                let col = other.strip_prefix("column:")?;
                let orig = s.trim();
                let id = orig.get(orig.len() - col.len()..).unwrap_or(col).trim();
                if id.is_empty() {
                    return None;
                }
                Self::Column(id.to_string())
            }
        })
    }

    pub fn name(&self) -> String {
        match self {
            Self::Count => "count".into(),
            Self::Length => "length".into(),
            Self::Area => "area".into(),
            Self::Volume => "volume".into(),
            Self::Quantity => "quantity".into(),
            Self::Column(c) => format!("column:{c}"),
        }
    }
}

/// One Quantity Link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QuantityLink {
    pub name: String,
    /// The workbook sheet and cell (A1 style) the total goes into.
    pub sheet: String,
    pub cell: String,
    pub files: Vec<PathBuf>,
    pub measure: QuantityMeasure,
    /// Filters (empty = any). Text filters match whole values, ignoring case.
    #[serde(default)]
    pub subjects: Vec<String>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    /// Colour as `#rrggbb`.
    #[serde(default)]
    pub colors: Vec<String>,
    /// Page labels the markups are on.
    #[serde(default)]
    pub pages: Vec<String>,
}

/// A link's computed total.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantityValue {
    pub name: String,
    pub value: f64,
    /// The unit(s) of the totalled markups (several = mixed units).
    pub units: Vec<String>,
    pub markups: usize,
    pub errors: Vec<String>,
}

/// A cell address (`B4`) as 0-based (row, column).
pub fn cell_index(cell: &str) -> Option<(usize, usize)> {
    let c = cell.trim().to_ascii_uppercase();
    let letters: String = c.chars().take_while(char::is_ascii_alphabetic).collect();
    let digits = &c[letters.len()..];
    if letters.is_empty() || letters.len() > 3 || digits.is_empty() || !digits.chars().all(|d| d.is_ascii_digit()) {
        return None;
    }
    let mut col = 0usize;
    for ch in letters.chars() {
        col = col * 26 + (ch as usize - 'A' as usize + 1);
    }
    let row: usize = digits.parse().ok()?;
    ((1..=100_000).contains(&row) && col <= 16_384).then(|| (row - 1, col - 1))
}

fn matches(list: &[String], v: &str) -> bool {
    list.is_empty() || list.iter().any(|x| x.trim().eq_ignore_ascii_case(v.trim()))
}

fn wanted(l: &QuantityLink, m: &Markup, page_label: &str) -> bool {
    let subject = if m.subject.is_empty() {
        m.kind.name()
    } else {
        m.subject.as_str()
    };
    matches(&l.subjects, subject)
        && matches(&l.layers, &m.layer)
        && matches(&l.labels, &m.label)
        && matches(&l.authors, &m.author)
        && matches(&l.colors, &m.color.hex())
        && matches(&l.pages, page_label)
}

/// The amount one markup adds to a link's total, and its unit.
fn amount(l: &QuantityLink, m: &Markup) -> Option<(f64, String)> {
    let q = m.quantity();
    match &l.measure {
        QuantityMeasure::Count => Some((if m.kind == Kind::Count { q.unwrap_or(1.0) } else { 1.0 }, "ea".into())),
        QuantityMeasure::Length => matches!(m.kind, Kind::Length | Kind::Polylength | Kind::Perimeter)
            .then_some(())
            .and_then(|_| q.map(|v| (v, m.unit()))),
        QuantityMeasure::Area => (m.kind == Kind::Area)
            .then_some(())
            .and_then(|_| q.map(|v| (v, m.unit()))),
        QuantityMeasure::Volume => (m.kind == Kind::Volume)
            .then_some(())
            .and_then(|_| q.map(|v| (v, m.unit()))),
        QuantityMeasure::Quantity => m
            .kind
            .is_measurement()
            .then_some(())
            .and_then(|_| q.map(|v| (v, m.unit()))),
        QuantityMeasure::Column(c) => m
            .column_data
            .get(c)
            .and_then(|t| crate::convert::cell_number(t))
            .map(|v| (v, String::new())),
    }
}

/// Validate a link's name, place and files.
pub fn check_link(l: &QuantityLink) -> Result<()> {
    if l.name.trim().is_empty() || l.name.chars().count() > 100 {
        return Err(invalid("a Quantity Link's name has 1 to 100 characters"));
    }
    if cell_index(&l.cell).is_none() {
        return Err(invalid(format!("{:?} is not a cell address (like B4)", l.cell)));
    }
    if l.files.is_empty() || l.files.len() > crate::batch::MAX_FILES {
        return Err(invalid("a Quantity Link totals 1 to 2000 files"));
    }
    Ok(())
}

/// Compute every link's total from its files (each file is read once).
pub fn quantity_totals(links: &[QuantityLink]) -> Result<Vec<QuantityValue>> {
    for l in links {
        check_link(l)?;
    }
    let mut cache: BTreeMap<PathBuf, std::result::Result<Session, String>> = BTreeMap::new();
    let mut out = Vec::new();
    for l in links {
        let mut v = QuantityValue {
            name: l.name.clone(),
            value: 0.0,
            units: Vec::new(),
            markups: 0,
            errors: Vec::new(),
        };
        for f in &l.files {
            let s = cache
                .entry(f.clone())
                .or_insert_with(|| Session::open(f).map_err(|e| e.to_string()));
            let s = match s {
                Ok(s) => s,
                Err(e) => {
                    v.errors.push(format!("{}: {e}", f.display()));
                    continue;
                }
            };
            let labels = s.page_labels();
            for m in &s.doc().markups {
                let label = labels.get(m.page).map_or("", String::as_str);
                if !wanted(l, m, label) {
                    continue;
                }
                if let Some((a, unit)) = amount(l, m) {
                    v.value += a;
                    v.markups += 1;
                    if !unit.is_empty() && !v.units.contains(&unit) {
                        v.units.push(unit);
                    }
                }
            }
        }
        out.push(v);
    }
    Ok(out)
}

pub fn load_links(path: &Path) -> Result<Vec<QuantityLink>> {
    let t = std::fs::read_to_string(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&t).map_err(|e| invalid(format!("{}: {e}", path.display())))
}

pub fn save_links(path: &Path, links: &[QuantityLink]) -> Result<()> {
    for l in links {
        check_link(l)?;
    }
    let t = serde_json::to_string_pretty(links).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, t.as_bytes())
}

fn num_text(v: f64) -> String {
    let r = (v * 1e4).round() / 1e4;
    if r == r.trunc() && r.abs() < 1e15 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Update: compute the totals and write the workbook at `out` (each value at its sheet and
/// cell, plus a Links sheet). Returns the totals.
pub fn update_quantity_workbook(links: &[QuantityLink], out: &Path) -> Result<Vec<QuantityValue>> {
    let values = quantity_totals(links)?;
    let mut sheets: Vec<(String, TextTable)> = Vec::new();
    for (l, v) in links.iter().zip(&values) {
        let name = if l.sheet.trim().is_empty() {
            "Quantities".to_string()
        } else {
            l.sheet.trim().to_string()
        };
        let Some((r, c)) = cell_index(&l.cell) else { continue };
        let i = match sheets.iter().position(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            Some(i) => i,
            None => {
                sheets.push((name, TextTable::default()));
                sheets.len() - 1
            }
        };
        let Some((_, t)) = sheets.get_mut(i) else { continue };
        while t.rows.len() <= r {
            t.rows.push(Vec::new());
        }
        if let Some(row) = t.rows.get_mut(r) {
            while row.len() <= c {
                row.push(String::new());
            }
            if let Some(cell) = row.get_mut(c) {
                *cell = num_text(v.value);
            }
        }
    }
    let mut list = TextTable::default();
    list.rows.push(
        ["Link", "Sheet", "Cell", "Measure", "Value", "Unit", "Markups", "Files"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    for (l, v) in links.iter().zip(&values) {
        list.rows.push(vec![
            l.name.clone(),
            l.sheet.clone(),
            l.cell.to_ascii_uppercase(),
            l.measure.name(),
            num_text(v.value),
            v.units.join(" / "),
            v.markups.to_string(),
            l.files
                .iter()
                .map(|f| {
                    f.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("; "),
        ]);
    }
    sheets.push(("Links".into(), list));
    crate::write_atomic(out, &xlsx(&sheets))?;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};
    use markupcraft_model::{Color, Point};

    #[test]
    fn links_total_by_type_and_filters_and_write_the_workbook() {
        let d = std::env::temp_dir().join(format!("markupcraft-qty-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("plan.pdf");
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), &f).unwrap();
        let mut c = Markup::new(
            Kind::Count,
            0,
            vec![Point::new(10.0, 10.0), Point::new(20.0, 20.0), Point::new(30.0, 30.0)],
        );
        c.subject = "Door".into();
        s.add_markup(c).unwrap();
        let mut c2 = Markup::new(Kind::Count, 0, vec![Point::new(40.0, 40.0)]);
        c2.subject = "Window".into();
        c2.color = Color::rgb(0.0, 0.0, 1.0);
        c2.column_data.insert("Cost".into(), "12.5".into());
        s.add_markup(c2).unwrap();
        s.save(true).unwrap();
        let link = |name: &str, cell: &str, measure: QuantityMeasure, subjects: &[&str]| QuantityLink {
            name: name.into(),
            sheet: "Takeoff".into(),
            cell: cell.into(),
            files: vec![f.clone()],
            measure,
            subjects: subjects.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let links = vec![
            link("Doors", "B2", QuantityMeasure::Count, &["Door"]),
            link("All", "B3", QuantityMeasure::Count, &[]),
            QuantityLink {
                colors: vec!["#0000ff".into()],
                ..link("Blue", "C4", QuantityMeasure::Column("Cost".into()), &[])
            },
        ];
        let v = quantity_totals(&links).unwrap();
        assert_eq!((v[0].value, v[1].value, v[2].value), (3.0, 4.0, 12.5));
        let out = d.join("q.xlsx");
        update_quantity_workbook(&links, &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 500);
        save_links(&d.join("links.json"), &links).unwrap();
        assert_eq!(load_links(&d.join("links.json")).unwrap(), links);
        assert_eq!(cell_index("b4"), Some((3, 1)));
        assert_eq!(cell_index("AA10"), Some((9, 26)));
        assert!(cell_index("4B").is_none());
        assert!(quantity_totals(&[link("x", "nope", QuantityMeasure::Area, &[])]).is_err());
        assert_eq!(
            QuantityMeasure::from_name("column:Cost"),
            Some(QuantityMeasure::Column("Cost".into()))
        );
    }
}
