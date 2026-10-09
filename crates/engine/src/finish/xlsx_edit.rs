//! Write values into an existing Excel workbook in place (Quantity Link's Update): only the
//! linked cells change (their style is kept), a sheet that is missing is added, and our Links
//! sheet is replaced; every other part of the workbook (styles, formulas, charts, other sheets)
//! is copied as it was. Excel recalculates formulas on open (`fullCalcOnLoad`).

use std::path::Path;

use super::zip::{self, Entry};
use crate::convert::{TextTable, xlsx};
use crate::quantity::{QuantityLink, QuantityValue, cell_index, quantity_totals};
use crate::{Result, invalid};

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `A`, `B`, ... `AA` for a 0-based column.
fn col_name(mut c: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (c % 26) as u8);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s.reverse();
    String::from_utf8_lossy(&s).into_owned()
}

/// A cell's value: a number, or text.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Number(f64),
    Text(String),
}

fn cell_xml(at: &str, style: Option<&str>, v: &CellValue) -> String {
    let s = style.map(|s| format!(" s=\"{}\"", xml_escape(s))).unwrap_or_default();
    match v {
        CellValue::Number(n) if n.is_finite() => format!("<c r=\"{at}\"{s}><v>{n}</v></c>"),
        CellValue::Number(_) => format!("<c r=\"{at}\"{s}/>"),
        CellValue::Text(t) => format!(
            "<c r=\"{at}\"{s} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            xml_escape(t)
        ),
    }
}

/// `(row, column)` of a cell reference attribute.
fn cell_rc(r: &str) -> Option<(usize, usize)> {
    cell_index(r)
}

/// Set one cell in a worksheet's XML; returns the new XML.
pub fn set_cell(sheet_xml: &str, row: usize, col: usize, v: &CellValue) -> Result<String> {
    let doc = roxmltree::Document::parse(sheet_xml).map_err(|e| invalid(format!("worksheet: {e}")))?;
    let at = format!("{}{}", col_name(col), row + 1);
    let data = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "sheetData")
        .ok_or_else(|| invalid("worksheet without sheetData"))?;
    let new_row = |cell: String| format!("<row r=\"{}\">{cell}</row>", row + 1);
    let splice = |range: std::ops::Range<usize>, with: &str| -> Result<String> {
        let (a, b) = (
            sheet_xml.get(..range.start).ok_or_else(|| invalid("worksheet range"))?,
            sheet_xml.get(range.end..).ok_or_else(|| invalid("worksheet range"))?,
        );
        Ok(format!("{a}{with}{b}"))
    };
    let rows: Vec<roxmltree::Node> = data
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "row")
        .collect();
    let row_no = |n: &roxmltree::Node| n.attribute("r").and_then(|r| r.parse::<usize>().ok());
    if let Some(rn) = rows.iter().find(|n| row_no(n) == Some(row + 1)) {
        let cells: Vec<roxmltree::Node> = rn
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "c")
            .collect();
        if let Some(c) = cells
            .iter()
            .find(|c| c.attribute("r").and_then(cell_rc) == Some((row, col)))
        {
            return splice(c.range(), &cell_xml(&at, c.attribute("s"), v));
        }
        // a new cell before the first one to its right (or at the row's end)
        let style = rn.attribute("s").filter(|_| rn.attribute("customFormat") == Some("1"));
        let xml = cell_xml(&at, style, v);
        if let Some(next) = cells
            .iter()
            .find(|c| c.attribute("r").and_then(cell_rc).is_some_and(|(_, cc)| cc > col))
        {
            let p = next.range().start;
            return splice(p..p, &xml);
        }
        let r = rn.range();
        let src = sheet_xml.get(r.clone()).unwrap_or_default();
        if src.ends_with("/>") {
            // <row r="3"/>: open it up
            let open = src.trim_end_matches("/>").trim_end();
            return splice(r, &format!("{open}>{xml}</row>"));
        }
        let close = r.end.saturating_sub("</row>".len());
        return splice(close..close, &xml);
    }
    let xml = new_row(cell_xml(&at, None, v));
    if let Some(next) = rows.iter().find(|n| row_no(n).is_some_and(|r| r > row + 1)) {
        let p = next.range().start;
        return splice(p..p, &xml);
    }
    let r = data.range();
    let src = sheet_xml.get(r.clone()).unwrap_or_default();
    if src.ends_with("/>") {
        return splice(r, &format!("<sheetData>{xml}</sheetData>"));
    }
    let close = r.end.saturating_sub("</sheetData>".len());
    splice(close..close, &xml)
}

/// A whole worksheet holding `table` (text and numbers).
fn sheet_of(table: &TextTable) -> String {
    let mut sh = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    for (r, row) in table.rows.iter().enumerate() {
        sh.push_str(&format!("<row r=\"{}\">", r + 1));
        for (c, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            let at = format!("{}{}", col_name(c), r + 1);
            let v = match cell.trim().parse::<f64>() {
                Ok(n) if n.is_finite() => CellValue::Number(n),
                _ => CellValue::Text(cell.clone()),
            };
            sh.push_str(&cell_xml(&at, None, &v));
        }
        sh.push_str("</row>");
    }
    sh.push_str("</sheetData></worksheet>");
    sh
}

/// An open workbook's parts.
pub struct Workbook {
    pub entries: Vec<Entry>,
}

impl Workbook {
    pub fn open(bytes: &[u8]) -> Result<Self> {
        let entries = zip::read(bytes)?;
        if zip::find(&entries, "xl/workbook.xml").is_none() {
            return Err(invalid("not an Excel workbook (no xl/workbook.xml)"));
        }
        Ok(Self { entries })
    }

    fn text(&self, name: &str) -> Result<String> {
        let e = zip::find(&self.entries, name).ok_or_else(|| invalid(format!("the workbook has no {name}")))?;
        Ok(String::from_utf8_lossy(&e.data).into_owned())
    }

    fn put(&mut self, name: &str, data: Vec<u8>) {
        let want = name.to_ascii_lowercase();
        match self.entries.iter_mut().find(|e| e.name.to_ascii_lowercase() == want) {
            Some(e) => e.data = data,
            None => self.entries.push(Entry {
                name: name.to_string(),
                data,
            }),
        }
    }

    /// Sheet names with their part names (`xl/worksheets/sheet1.xml`), in workbook order.
    pub fn sheets(&self) -> Result<Vec<(String, String)>> {
        let wb = self.text("xl/workbook.xml")?;
        let rels = self.text("xl/_rels/workbook.xml.rels")?;
        let wd = roxmltree::Document::parse(&wb).map_err(|e| invalid(format!("workbook.xml: {e}")))?;
        let rd = roxmltree::Document::parse(&rels).map_err(|e| invalid(format!("workbook rels: {e}")))?;
        let mut out = Vec::new();
        for s in wd
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "sheet")
        {
            let name = s.attribute("name").unwrap_or_default().to_string();
            let rid = s
                .attributes()
                .find(|a| a.name() == "id")
                .map(|a| a.value().to_string())
                .unwrap_or_default();
            let target = rd
                .descendants()
                .find(|n| n.is_element() && n.attribute("Id") == Some(rid.as_str()))
                .and_then(|n| n.attribute("Target"))
                .unwrap_or_default();
            let part = if let Some(abs) = target.strip_prefix('/') {
                abs.to_string()
            } else {
                format!("xl/{target}")
            };
            out.push((name, part));
        }
        Ok(out)
    }

    /// The part of sheet `name` (case-insensitive), adding an empty sheet when it is missing.
    pub fn sheet_part(&mut self, name: &str) -> Result<String> {
        if let Some((_, p)) = self.sheets()?.into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return Ok(p);
        }
        let clean: String = name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
        let clean = if clean.trim().is_empty() {
            "Sheet".to_string()
        } else {
            clean
        };
        let mut k = 1;
        while zip::find(&self.entries, &format!("xl/worksheets/sheet{k}.xml")).is_some() {
            k += 1;
        }
        let part = format!("xl/worksheets/sheet{k}.xml");
        let mut wb = self.text("xl/workbook.xml")?;
        let mut rels = self.text("xl/_rels/workbook.xml.rels")?;
        let mut rid = k;
        while rels.contains(&format!("Id=\"rIdMC{rid}\"")) {
            rid += 1;
        }
        let sid = wb.matches("sheetId=").count() + 1000;
        let sheet = format!(
            "<sheet name=\"{}\" sheetId=\"{sid}\" r:id=\"rIdMC{rid}\"/>",
            xml_escape(&clean)
        );
        let at = wb
            .find("</sheets>")
            .ok_or_else(|| invalid("workbook.xml has no sheets list"))?;
        wb.insert_str(at, &sheet);
        if !wb.contains("xmlns:r=")
            && let Some(p) = wb.find("<workbook")
        {
            wb.insert_str(
                p + "<workbook".len(),
                " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"",
            );
        }
        let rel = format!(
            "<Relationship Id=\"rIdMC{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{k}.xml\"/>"
        );
        let at = rels
            .rfind("</Relationships>")
            .ok_or_else(|| invalid("workbook rels are not readable"))?;
        rels.insert_str(at, &rel);
        let mut ct = self.text("[Content_Types].xml")?;
        let over = format!(
            "<Override PartName=\"/{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        );
        let at = ct
            .rfind("</Types>")
            .ok_or_else(|| invalid("[Content_Types].xml is not readable"))?;
        ct.insert_str(at, &over);
        self.put("xl/workbook.xml", wb.into_bytes());
        self.put("xl/_rels/workbook.xml.rels", rels.into_bytes());
        self.put("[Content_Types].xml", ct.into_bytes());
        self.put(&part, sheet_of(&TextTable::default()).into_bytes());
        Ok(part)
    }

    /// Set `cell` (A1) of sheet `sheet` to `v`.
    pub fn set(&mut self, sheet: &str, cell: &str, v: &CellValue) -> Result<()> {
        let (r, c) = cell_index(cell).ok_or_else(|| invalid(format!("{cell} is not a cell like B4")))?;
        let part = self.sheet_part(sheet)?;
        let xml = self.text(&part)?;
        let next = set_cell(&xml, r, c, v)?;
        self.put(&part, next.into_bytes());
        Ok(())
    }

    /// Replace sheet `name`'s contents with `table` (added when missing).
    pub fn replace_sheet(&mut self, name: &str, table: &TextTable) -> Result<()> {
        let part = self.sheet_part(name)?;
        self.put(&part, sheet_of(table).into_bytes());
        Ok(())
    }

    /// Ask Excel to recalculate every formula when the file opens.
    fn recalc_on_load(&mut self) -> Result<()> {
        let mut wb = self.text("xl/workbook.xml")?;
        if wb.contains("fullCalcOnLoad") {
            return Ok(());
        }
        if let Some(p) = wb.find("<calcPr") {
            wb.insert_str(p + "<calcPr".len(), " fullCalcOnLoad=\"1\"");
        } else {
            let after = [
                "<oleSize",
                "<customWorkbookViews",
                "<pivotCaches",
                "<smartTagPr",
                "<smartTagTypes",
                "<webPublishing",
                "<fileRecoveryPr",
                "<webPublishObjects",
                "<extLst",
                "</workbook>",
            ];
            let at = after
                .iter()
                .filter_map(|t| wb.find(t))
                .min()
                .ok_or_else(|| invalid("workbook.xml is not readable"))?;
            wb.insert_str(at, "<calcPr fullCalcOnLoad=\"1\"/>");
        }
        self.put("xl/workbook.xml", wb.into_bytes());
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>> {
        self.recalc_on_load()?;
        Ok(zip::write(&self.entries))
    }
}

/// The Links sheet Quantity Link keeps beside the values.
fn links_table(links: &[QuantityLink], values: &[QuantityValue]) -> TextTable {
    let mut list = TextTable::default();
    list.rows.push(
        ["Link", "Sheet", "Cell", "Measure", "Value", "Unit", "Markups"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    for (l, v) in links.iter().zip(values) {
        list.rows.push(vec![
            l.name.clone(),
            l.sheet.clone(),
            l.cell.to_ascii_uppercase(),
            l.measure.name(),
            format!("{}", (v.value * 1e4).round() / 1e4),
            v.units.join(" / "),
            v.markups.to_string(),
        ]);
    }
    list
}

/// Update: compute the totals and write them into the workbook at `path`. An existing workbook
/// is edited in place (only the linked cells and the Links sheet change); a missing one is
/// created. Returns the totals.
pub fn update_quantity_workbook_in_place(links: &[QuantityLink], path: &Path) -> Result<Vec<QuantityValue>> {
    if !path.exists() {
        return crate::quantity::update_quantity_workbook(links, path);
    }
    let len = std::fs::metadata(path)
        .map_err(|e| crate::EngineError::Io {
            path: path.display().to_string(),
            source: e,
        })?
        .len();
    if len > zip::MAX_ARCHIVE {
        return Err(invalid("the workbook is too large"));
    }
    let bytes = std::fs::read(path).map_err(|e| crate::EngineError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut wb = Workbook::open(&bytes)?;
    let values = quantity_totals(links)?;
    for (l, v) in links.iter().zip(&values) {
        let sheet = if l.sheet.trim().is_empty() {
            "Quantities"
        } else {
            l.sheet.trim()
        };
        wb.set(sheet, &l.cell, &CellValue::Number((v.value * 1e4).round() / 1e4))?;
    }
    wb.replace_sheet("Links", &links_table(links, &values))?;
    crate::write_atomic(path, &wb.bytes()?)?;
    Ok(values)
}

/// A workbook of our own with these sheets (for tests and new files).
pub fn new_workbook(sheets: &[(String, TextTable)]) -> Vec<u8> {
    xlsx(sheets)
}

/// The text of every non-empty cell of a sheet (row, column, text), read back from a workbook
/// (shared strings, inline strings and numbers).
pub fn read_sheet(bytes: &[u8], name: &str) -> Result<Vec<(usize, usize, String)>> {
    let wb = Workbook::open(bytes)?;
    let part = wb
        .sheets()?
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, p)| p)
        .ok_or_else(|| invalid(format!("no sheet {name}")))?;
    let shared: Vec<String> = match wb.text("xl/sharedStrings.xml") {
        Ok(t) => {
            let d = roxmltree::Document::parse(&t).map_err(|e| invalid(format!("sharedStrings: {e}")))?;
            d.descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "si")
                .map(|si| {
                    si.descendants()
                        .filter(|n| n.is_element() && n.tag_name().name() == "t")
                        .filter_map(|t| t.text())
                        .collect::<String>()
                })
                .collect()
        }
        Err(_) => Vec::new(),
    };
    let xml = wb.text(&part)?;
    let d = roxmltree::Document::parse(&xml).map_err(|e| invalid(format!("worksheet: {e}")))?;
    let mut out = Vec::new();
    for c in d.descendants().filter(|n| n.is_element() && n.tag_name().name() == "c") {
        let Some((r, col)) = c.attribute("r").and_then(cell_rc) else {
            continue;
        };
        let t = c.attribute("t").unwrap_or("n");
        let v = c
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "v")
            .and_then(|n| n.text())
            .unwrap_or_default();
        let text = match t {
            "s" => v
                .parse::<usize>()
                .ok()
                .and_then(|i| shared.get(i).cloned())
                .unwrap_or_default(),
            "inlineStr" => c
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "t")
                .filter_map(|n| n.text())
                .collect(),
            _ => v.to_string(),
        };
        if !text.is_empty() {
            out.push((r, col, text));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_set_in_place_and_other_parts_are_kept() {
        let mut t = TextTable::default();
        t.rows.push(vec!["Item".into(), "Qty".into()]);
        t.rows.push(vec!["Doors".into(), "".into()]);
        t.rows.push(vec![]);
        t.rows.push(vec!["Total".into(), "9".into()]);
        let bytes = new_workbook(&[("Bid".into(), t)]);
        let mut wb = Workbook::open(&bytes).unwrap();
        // a part we do not know survives untouched
        wb.put("xl/custom/keep.xml", b"<keep/>".to_vec());
        wb.set("Bid", "B2", &CellValue::Number(12.0)).unwrap();
        wb.set("Bid", "C4", &CellValue::Number(3.5)).unwrap();
        wb.set("Bid", "A3", &CellValue::Text("Windows & frames".into()))
            .unwrap();
        wb.set("Takeoff", "D10", &CellValue::Number(7.0)).unwrap();
        let out = wb.bytes().unwrap();
        let cells = read_sheet(&out, "Bid").unwrap();
        assert!(cells.contains(&(1, 1, "12".into())), "{cells:?}");
        assert!(cells.contains(&(3, 2, "3.5".into())));
        assert!(cells.contains(&(3, 1, "9".into())), "the cell beside stays");
        assert!(cells.contains(&(2, 0, "Windows & frames".into())));
        assert_eq!(read_sheet(&out, "Takeoff").unwrap(), vec![(9, 3, "7".into())]);
        let entries = zip::read(&out).unwrap();
        assert!(zip::find(&entries, "xl/custom/keep.xml").is_some());
        assert!(
            String::from_utf8_lossy(&zip::find(&entries, "xl/workbook.xml").unwrap().data).contains("fullCalcOnLoad")
        );
        assert!(Workbook::open(b"not a zip").is_err());
        assert_eq!(col_name(27), "AB");
    }
}
