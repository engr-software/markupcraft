//! Markup Summary: the Markups List of a document as a report, written as CSV, XML, an Excel
//! workbook (.xlsx) or a PDF. The rows, columns, filters, grouping and totals are the Markups
//! List's own (`markupcraft_model::table`), so a summary always matches the list.
//!
//! The .xlsx writer is our own: a workbook is a ZIP of a few XML parts, written here with
//! stored (uncompressed) entries, which every spreadsheet reader accepts. Numbers go in as
//! numbers (so Excel can sum them), text as inline strings, the header and total rows bold.

use std::collections::BTreeSet;
use std::path::Path;

use markupcraft_model::Document;
use markupcraft_model::table::{Group, MarkupTable, Scope, View};

use crate::{Result, Session, invalid};

/// Which file a summary is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryFormat {
    Csv,
    Xml,
    Xlsx,
    Pdf,
}

impl SummaryFormat {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "xml" => Some(Self::Xml),
            "xlsx" | "excel" => Some(Self::Xlsx),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xml => "xml",
            Self::Xlsx => "xlsx",
            Self::Pdf => "pdf",
        }
    }

    /// The format of a file name's extension.
    pub fn from_path(p: &Path) -> Option<Self> {
        p.extension().and_then(|e| e.to_str()).and_then(Self::from_name)
    }
}

/// What a summary contains.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SummaryOptions {
    /// Column ids (`subject`, `pagelabel`, `measurement`, `c:<custom>` ...); empty = the
    /// Markups List's default columns.
    pub columns: Vec<String>,
    /// 0-based pages; empty = every page.
    pub pages: Vec<usize>,
    pub measurements_only: bool,
    /// Column ids, outermost first.
    pub group_by: Vec<String>,
    /// Column id to sort by ("" = document order).
    pub sort: String,
    pub descending: bool,
    /// Column id -> allowed cell texts.
    pub filters: std::collections::BTreeMap<String, BTreeSet<String>>,
    /// Report title (PDF and workbook); "" = "Markup Summary".
    pub title: String,
    /// More sort keys after `sort`: (column id, descending), applied in order.
    pub then_by: Vec<(String, bool)>,
    /// Append today's date to the title.
    pub date_in_title: bool,
    /// CSV and XML: markups, totals or both.
    pub content: SummaryContent,
    /// CSV: leave out the header row.
    pub no_headers: bool,
    /// PDF report layout.
    pub layout: PdfLayout,
}

/// What a CSV or XML summary holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SummaryContent {
    #[default]
    Both,
    Markups,
    Totals,
}

/// The PDF report's layout (Summary > Output > PDF).
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLayout {
    /// Flow style: each markup as a block of "column: value" lines (else a table).
    pub flow: bool,
    /// Page size in points.
    pub page_size: (f64, f64),
    /// A new page for each top-level group.
    pub break_per_group: bool,
    /// Each markup line links to its page in the source PDF.
    pub links: bool,
    /// Group and grand totals.
    pub totals: bool,
    /// Space around cell text, points.
    pub padding: f64,
    /// A PNG logo at the top right of every page.
    pub logo: Option<std::path::PathBuf>,
}

impl Default for PdfLayout {
    fn default() -> Self {
        Self {
            flow: false,
            page_size: (792.0, 612.0),
            break_per_group: false,
            links: true,
            totals: true,
            padding: 2.0,
            logo: None,
        }
    }
}

/// One cell of the report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SummaryCell {
    pub text: String,
    /// The number behind the text (measurements, number columns).
    pub num: Option<f64>,
}

/// One line of the report: a group heading, a markup or a group total.
#[derive(Debug, Clone, PartialEq)]
pub enum SummaryLine {
    /// A group heading (`level` 0 = outermost) with its markup count.
    Group {
        level: usize,
        title: String,
        count: usize,
    },
    Row(Vec<SummaryCell>),
    /// A group's or the grand total: the label, then one text per column.
    Total {
        label: String,
        cells: Vec<String>,
    },
}

/// A computed summary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SummaryReport {
    pub title: String,
    pub document: String,
    pub headers: Vec<String>,
    pub lines: Vec<SummaryLine>,
    /// Markups in the summary.
    pub count: usize,
    /// The source page (0-based) of each line (markup rows only).
    pub line_pages: Vec<Option<usize>>,
    pub content: SummaryContent,
    pub no_headers: bool,
    pub layout: PdfLayout,
}

/// The summary of `doc` (named `name` in the report).
pub fn summary(doc: &Document, name: &str, o: &SummaryOptions) -> SummaryReport {
    let table = MarkupTable::new(doc);
    let visible: Vec<String> = if o.columns.is_empty() {
        table
            .columns()
            .iter()
            .filter(|c| c.visible_by_default)
            .map(|c| c.id.clone())
            .collect()
    } else {
        o.columns
            .iter()
            .filter(|c| table.column_index(c).is_some())
            .cloned()
            .collect()
    };
    let scope = if o.pages.is_empty() {
        Scope::AllPages
    } else {
        Scope::Selected(
            doc.markups
                .iter()
                .enumerate()
                .filter(|(_, m)| o.pages.contains(&m.page))
                .map(|(i, _)| i)
                .collect(),
        )
    };
    let view = View {
        visible: visible.clone(),
        sort_column: o.sort.clone(),
        sort_descending: o.descending,
        group_by: o.group_by.clone(),
        filters: o.filters.clone(),
        search: String::new(),
        scope,
        measurements_only: o.measurements_only,
    };
    let mut root = table.build(&view);
    let cols: Vec<usize> = visible.iter().filter_map(|id| table.column_index(id)).collect();
    // Multi-level sort: the first key is the table's own; then the others, within each group.
    if !o.then_by.is_empty() {
        let mut keys: Vec<(usize, bool)> = Vec::new();
        if let Some(c) = table.column_index(&o.sort) {
            keys.push((c, o.descending));
        }
        keys.extend(
            o.then_by
                .iter()
                .filter_map(|(id, d)| table.column_index(id).map(|c| (c, *d))),
        );
        sort_rows(&table, &mut root, &keys, 0);
    }
    let headers = cols
        .iter()
        .filter_map(|c| table.columns().get(*c).map(|c| c.header.clone()))
        .collect();
    let mut lines = Vec::new();
    let mut line_pages = Vec::new();
    walk(&table, doc, &root, &cols, &mut lines, &mut line_pages, 0);
    lines.push(SummaryLine::Total {
        label: format!("Total ({})", root.totals.count),
        cells: cols.iter().map(|c| table.totals_text(*c, &root.totals)).collect(),
    });
    line_pages.push(None);
    let mut title = if o.title.trim().is_empty() {
        "Markup Summary".to_string()
    } else {
        o.title.trim().to_string()
    };
    if o.date_in_title {
        let (y, m, d) = crate::docutil::today();
        title.push_str(&format!(" {y:04}-{m:02}-{d:02}"));
    }
    SummaryReport {
        title,
        document: name.to_string(),
        headers,
        lines,
        count: root.totals.count,
        line_pages,
        content: o.content,
        no_headers: o.no_headers,
        layout: o.layout.clone(),
    }
}

fn cmp_cells(a: &crate::summary::SummaryCell, b: &crate::summary::SummaryCell) -> std::cmp::Ordering {
    match (a.num, b.num) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => a.text.to_lowercase().cmp(&b.text.to_lowercase()),
    }
}

fn sort_rows(t: &MarkupTable<'_>, g: &mut Group, keys: &[(usize, bool)], depth: usize) {
    if depth > 32 {
        return;
    }
    let cell = |r: usize, c: usize| {
        let x = t.cell(r, c);
        SummaryCell {
            text: x.text.clone(),
            num: x.num.filter(|v| v.is_finite() && !x.error),
        }
    };
    g.rows.sort_by(|a, b| {
        for (c, desc) in keys {
            let o = cmp_cells(&cell(*a, *c), &cell(*b, *c));
            let o = if *desc { o.reverse() } else { o };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    for child in &mut g.children {
        sort_rows(t, child, keys, depth + 1);
    }
}

fn walk(
    t: &MarkupTable<'_>,
    doc: &Document,
    g: &Group,
    cols: &[usize],
    out: &mut Vec<SummaryLine>,
    pages: &mut Vec<Option<usize>>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    for r in &g.rows {
        pages.push(doc.markups.get(*r).map(|m| m.page));
        out.push(SummaryLine::Row(
            cols.iter()
                .map(|c| {
                    let cell = t.cell(*r, *c);
                    SummaryCell {
                        text: cell.text.clone(),
                        num: cell.num.filter(|v| v.is_finite() && !cell.error),
                    }
                })
                .collect(),
        ));
    }
    for child in &g.children {
        let title = if child.key.is_empty() {
            "(blank)".to_string()
        } else {
            child.key.clone()
        };
        out.push(SummaryLine::Group {
            level: child.level,
            title: title.clone(),
            count: child.totals.count,
        });
        pages.push(None);
        walk(t, doc, child, cols, out, pages, depth + 1);
        pages.push(None);
        out.push(SummaryLine::Total {
            label: format!("{title} total ({})", child.totals.count),
            cells: cols.iter().map(|c| t.totals_text(*c, &child.totals)).collect(),
        });
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The summary as CSV (a heading row, the rows, group and grand totals).
pub fn summary_csv(r: &SummaryReport) -> String {
    let mut out = String::new();
    let mut line = |cells: Vec<String>| {
        out.push_str(&cells.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    };
    if !r.no_headers {
        line(r.headers.clone());
    }
    let (rows, totals) = match r.content {
        SummaryContent::Both => (true, true),
        SummaryContent::Markups => (true, false),
        SummaryContent::Totals => (false, true),
    };
    for l in &r.lines {
        match l {
            SummaryLine::Group { title, count, .. } => line(vec![format!("{title} ({count})")]),
            SummaryLine::Row(cells) if rows => line(cells.iter().map(|c| c.text.clone()).collect()),
            SummaryLine::Row(_) => {}
            SummaryLine::Total { .. } if !totals => {}
            SummaryLine::Total { label, cells } => {
                let mut v = cells.clone();
                if let Some(first) = v.first_mut()
                    && first.is_empty()
                {
                    *first = label.clone();
                }
                line(v);
            }
        }
    }
    out
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 forbids most control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The summary as XML.
pub fn summary_xml(r: &SummaryReport) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<MarkupSummary Title=\"{}\" Document=\"{}\" Count=\"{}\">\n",
        xml_escape(&r.title),
        xml_escape(&r.document),
        r.count
    ));
    let tag = |h: &str| -> String {
        let t: String = h.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if t.is_empty() || t.starts_with(|c: char| c.is_ascii_digit()) {
            format!("Column{t}")
        } else {
            t
        }
    };
    for l in &r.lines {
        match l {
            SummaryLine::Group { level, title, count } => out.push_str(&format!(
                "  <Group Level=\"{level}\" Title=\"{}\" Count=\"{count}\"/>\n",
                xml_escape(title)
            )),
            SummaryLine::Row(_) if r.content == SummaryContent::Totals => {}
            SummaryLine::Total { .. } if r.content == SummaryContent::Markups => {}
            SummaryLine::Row(cells) => {
                out.push_str("  <Markup>");
                for (h, c) in r.headers.iter().zip(cells) {
                    let t = tag(h);
                    out.push_str(&format!("<{t}>{}</{t}>", xml_escape(&c.text)));
                }
                out.push_str("</Markup>\n");
            }
            SummaryLine::Total { label, cells } => {
                out.push_str(&format!("  <Total Label=\"{}\">", xml_escape(label)));
                for (h, c) in r.headers.iter().zip(cells) {
                    if !c.is_empty() {
                        let t = tag(h);
                        out.push_str(&format!("<{t}>{}</{t}>", xml_escape(c)));
                    }
                }
                out.push_str("</Total>\n");
            }
        }
    }
    out.push_str("</MarkupSummary>\n");
    out
}

// ---- .xlsx --------------------------------------------------------------------------------

/// The column letters of 0-based column `i` (A, B, ..., Z, AA, ...).
fn col_name(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

enum XCell<'a> {
    Text(&'a str),
    Num(f64),
}

/// The summary as an Excel workbook (one sheet).
pub fn summary_xlsx(r: &SummaryReport) -> Vec<u8> {
    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    let n = r.headers.len().max(1);
    sheet.push_str("<cols>");
    for i in 0..n {
        sheet.push_str(&format!(
            "<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>",
            i + 1,
            if i == 0 { 28 } else { 16 }
        ));
    }
    sheet.push_str("</cols><sheetData>");
    let mut row_no = 0usize;
    let mut row = |cells: Vec<XCell<'_>>, bold: bool| {
        row_no += 1;
        sheet.push_str(&format!("<row r=\"{row_no}\">"));
        for (i, c) in cells.iter().enumerate() {
            let at = format!("{}{row_no}", col_name(i));
            let s = if bold { " s=\"1\"" } else { "" };
            match c {
                XCell::Num(v) => sheet.push_str(&format!("<c r=\"{at}\"{s}><v>{v}</v></c>")),
                XCell::Text("") => {}
                XCell::Text(t) => sheet.push_str(&format!(
                    "<c r=\"{at}\" t=\"inlineStr\"{s}><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml_escape(t)
                )),
            }
        }
        sheet.push_str("</row>");
    };
    row(vec![XCell::Text(&r.title)], true);
    row(vec![XCell::Text(&r.document)], false);
    row(r.headers.iter().map(|h| XCell::Text(h)).collect(), true);
    for l in &r.lines {
        match l {
            SummaryLine::Group { title, .. } => row(vec![XCell::Text(title)], true),
            SummaryLine::Row(cells) => row(
                cells
                    .iter()
                    .map(|c| match c.num {
                        Some(v) => XCell::Num(v),
                        None => XCell::Text(&c.text),
                    })
                    .collect(),
                false,
            ),
            SummaryLine::Total { label, cells } => {
                let mut v: Vec<XCell<'_>> = cells.iter().map(|c| XCell::Text(c)).collect();
                if let Some(first) = v.first_mut()
                    && matches!(first, XCell::Text(t) if t.is_empty())
                {
                    *first = XCell::Text(label);
                }
                row(v, true);
            }
        }
    }
    sheet.push_str("</sheetData></worksheet>");

    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let workbook = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"Summary\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    let wb_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>";
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>";
    zip_stored(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/styles.xml", styles.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = !0u32;
    for b in data {
        let idx = ((crc ^ u32::from(*b)) & 0xFF) as usize;
        crc = table.get(idx).copied().unwrap_or(0) ^ (crc >> 8);
    }
    !crc
}

/// A ZIP archive with stored (uncompressed) entries. Entries over 4 GiB are not supported
/// (summaries are far smaller); sizes saturate rather than wrap.
fn zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let le16 = |v: usize| (u16::try_from(v).unwrap_or(u16::MAX)).to_le_bytes();
    let le32 = |v: usize| (u32::try_from(v).unwrap_or(u32::MAX)).to_le_bytes();
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len();
        let crc = crc32(data).to_le_bytes();
        // local file header
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        out.extend_from_slice(&crc);
        out.extend_from_slice(&le32(data.len()));
        out.extend_from_slice(&le32(data.len()));
        out.extend_from_slice(&le16(name.len()));
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // central directory entry
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend_from_slice(&crc);
        central.extend_from_slice(&le32(data.len()));
        central.extend_from_slice(&le32(data.len()));
        central.extend_from_slice(&le16(name.len()));
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&le32(offset));
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len();
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend_from_slice(&le16(files.len()));
    out.extend_from_slice(&le16(files.len()));
    out.extend_from_slice(&le32(central.len()));
    out.extend_from_slice(&le32(cd_offset));
    out.extend_from_slice(&[0, 0]);
    out
}

// ---- PDF ----------------------------------------------------------------------------------

/// Text Helvetica (WinAnsi) can show: other characters become `?`.
fn pdf_text(s: &str, max: usize) -> String {
    let mut t: String = s
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .take(max)
        .collect();
    if s.chars().count() > max && max > 1 {
        t.pop();
        t.push('~');
    }
    t
}

/// The summary as a PDF report: landscape letter pages, a heading, a table and totals.
pub fn summary_pdf(r: &SummaryReport) -> Vec<u8> {
    summary_pdf_pages(r).0
}

/// Where a line landed in the PDF report: (report page, y of its baseline, source page).
type Placed = Vec<(usize, f64, usize)>;

// The table layout keeps the report's constant-style names (W, H, ROW) now that they vary.
#[allow(non_snake_case)]
/// The PDF report and where each markup line landed (for links). Honours the layout's page
/// size, padding, totals, a page per top-level group, and the flow style.
fn summary_pdf_pages(r: &SummaryReport) -> (Vec<u8>, Placed) {
    use crate::synthetic::{SyntheticPage, line, text};
    let (pw, ph) = r.layout.page_size;
    let (W, H) =
        if pw.is_finite() && ph.is_finite() && (72.0..=14_400.0).contains(&pw) && (72.0..=14_400.0).contains(&ph) {
            (pw, ph)
        } else {
            (792.0, 612.0)
        };
    const M: f64 = 36.0;
    let pad = r.layout.padding.clamp(0.0, 20.0);
    let ROW: f64 = 11.0 + pad;
    let mut placed: Placed = Vec::new();
    if r.layout.flow {
        return flow_pdf(r, W, H, M, ROW);
    }
    let n = r.headers.len().max(1);
    let col_w = (W - 2.0 * M) / n as f64;
    let chars = ((col_w / 4.6) as usize).clamp(4, 200);
    let mut pages: Vec<SyntheticPage> = Vec::new();
    let mut content = String::new();
    let mut y = 0.0;
    let mut page_no = 0;
    let header = |content: &mut String, page_no: usize| -> f64 {
        let mut y = H - M;
        content.push_str(&text(M, y - 14.0, 16.0, &pdf_text(&r.title, 90)));
        content.push_str(&text(
            W - M - 160.0,
            y - 12.0,
            9.0,
            &format!("{} - page {page_no}", pdf_text(&r.document, 30)),
        ));
        y -= 34.0;
        for (i, h) in r.headers.iter().enumerate() {
            content.push_str(&text(M + i as f64 * col_w + 2.0, y, 8.5, &pdf_text(h, chars)));
        }
        content.push_str(&line(M, y - 4.0, W - M, y - 4.0, 0.8));
        y - ROW - 2.0
    };
    let lines: Vec<&SummaryLine> = r.lines.iter().collect();
    let mut i = 0;
    while i < lines.len() || page_no == 0 {
        if page_no == 0 || y < M + ROW {
            if page_no > 0 {
                pages.push(SyntheticPage::new(W, H, std::mem::take(&mut content)));
            }
            page_no += 1;
            y = header(&mut content, page_no);
            if page_no > 10_000 {
                break;
            }
        }
        let Some(l) = lines.get(i) else { break };
        // A page per top-level group.
        if r.layout.break_per_group && matches!(l, SummaryLine::Group { level: 0, .. }) && y < H - M - 34.0 - ROW - 3.0
        {
            pages.push(SyntheticPage::new(W, H, std::mem::take(&mut content)));
            page_no += 1;
            y = header(&mut content, page_no);
        }
        if !r.layout.totals && matches!(l, SummaryLine::Total { .. }) {
            i += 1;
            continue;
        }
        match l {
            SummaryLine::Group { level, title, count } => {
                content.push_str(&text(
                    M + 2.0 + 10.0 * (*level).min(8) as f64,
                    y,
                    9.0,
                    &pdf_text(&format!("{title} ({count})"), 120),
                ));
            }
            SummaryLine::Row(cells) => {
                for (k, c) in cells.iter().enumerate() {
                    content.push_str(&text(M + k as f64 * col_w + pad, y, 8.0, &pdf_text(&c.text, chars)));
                }
                if let Some(Some(p)) = r.line_pages.get(i) {
                    placed.push((page_no - 1, y, *p));
                }
            }
            SummaryLine::Total { label, cells } => {
                content.push_str(&line(M, y + ROW - 3.0, W - M, y + ROW - 3.0, 0.4));
                for (k, c) in cells.iter().enumerate() {
                    let t = if k == 0 && c.is_empty() { label.as_str() } else { c };
                    content.push_str(&text(M + k as f64 * col_w + 2.0, y, 8.0, &pdf_text(t, chars)));
                }
            }
        }
        y -= ROW;
        i += 1;
    }
    pages.push(SyntheticPage::new(W, H, content));
    (crate::synthetic::pdf(&pages), placed)
}

/// Flow style: each markup a block of "header: value" lines; groups and totals as headings.
fn flow_pdf(r: &SummaryReport, w: f64, h: f64, m: f64, row: f64) -> (Vec<u8>, Placed) {
    use crate::synthetic::{SyntheticPage, line, text};
    let mut pages = Vec::new();
    let mut content = String::new();
    let mut placed = Vec::new();
    let mut y = h - m;
    let mut page = 0usize;
    let head = |content: &mut String| {
        content.push_str(&text(m, h - m - 14.0, 16.0, &pdf_text(&r.title, 90)));
        content.push_str(&text(w - m - 160.0, h - m - 12.0, 9.0, &pdf_text(&r.document, 30)));
    };
    head(&mut content);
    y -= 40.0;
    for (i, l) in r.lines.iter().enumerate() {
        let need = match l {
            SummaryLine::Row(cells) => row * (cells.len() as f64 + 1.0),
            _ => row * 1.5,
        };
        let new_group =
            r.layout.break_per_group && matches!(l, SummaryLine::Group { level: 0, .. }) && y < h - m - 41.0;
        if y - need < m || new_group {
            pages.push(SyntheticPage::new(w, h, std::mem::take(&mut content)));
            page += 1;
            head(&mut content);
            y = h - m - 40.0;
            if page > 10_000 {
                break;
            }
        }
        match l {
            SummaryLine::Group { level, title, count } => {
                content.push_str(&text(
                    m + 10.0 * (*level).min(8) as f64,
                    y,
                    11.0,
                    &pdf_text(&format!("{title} ({count})"), 100),
                ));
                y -= row * 1.5;
            }
            SummaryLine::Row(cells) => {
                let top = y;
                for (hd, c) in r.headers.iter().zip(cells) {
                    content.push_str(&text(m + 12.0, y, 8.5, &pdf_text(&format!("{hd}: {}", c.text), 130)));
                    y -= row;
                }
                content.push_str(&line(m + 8.0, y + row * 0.6, w - m, y + row * 0.6, 0.3));
                y -= row * 0.5;
                if let Some(Some(p)) = r.line_pages.get(i) {
                    placed.push((page, top, *p));
                }
            }
            SummaryLine::Total { label, cells } if r.layout.totals => {
                let t: Vec<String> = cells.iter().filter(|c| !c.is_empty()).cloned().collect();
                content.push_str(&text(m, y, 9.0, &pdf_text(&format!("{label}: {}", t.join("  ")), 130)));
                y -= row * 1.5;
            }
            SummaryLine::Total { .. } => {}
        }
    }
    pages.push(SyntheticPage::new(w, h, content));
    (crate::synthetic::pdf(&pages), placed)
}

/// The bytes of `r` in `format`.
pub fn summary_bytes(r: &SummaryReport, format: SummaryFormat) -> Vec<u8> {
    match format {
        SummaryFormat::Csv => summary_csv(r).into_bytes(),
        SummaryFormat::Xml => summary_xml(r).into_bytes(),
        SummaryFormat::Xlsx => summary_xlsx(r),
        SummaryFormat::Pdf => summary_pdf(r),
    }
}

impl Session {
    /// The Markup Summary of this document.
    pub fn summary(&self, o: &SummaryOptions) -> SummaryReport {
        let name = self
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        summary(self.doc(), &name, o)
    }

    /// Write the Markup Summary to `out` (format from `format`, else the file extension).
    /// Returns how many markups it lists.
    pub fn export_summary(&self, out: &Path, format: Option<SummaryFormat>, o: &SummaryOptions) -> Result<usize> {
        let format = format
            .or_else(|| SummaryFormat::from_path(out))
            .ok_or_else(|| invalid("summary format: csv, xml, xlsx or pdf"))?;
        for p in &o.pages {
            self.page(*p)?;
        }
        let r = self.summary(o);
        if format == SummaryFormat::Pdf && (r.layout.links || r.layout.logo.is_some()) {
            self.write_summary_pdf(&r, out)?;
        } else {
            crate::write_atomic(out, &summary_bytes(&r, format))?;
        }
        Ok(r.count)
    }

    /// The PDF report with links to the source pages and the logo.
    fn write_summary_pdf(&self, r: &SummaryReport, out: &Path) -> Result<()> {
        let (bytes, placed) = summary_pdf_pages(r);
        let mut s = Session::from_bytes(bytes, out)?;
        let (w, _) = r.layout.page_size;
        if r.layout.links {
            let source = self.path().display().to_string();
            for (page, y, src) in placed.iter().take(20_000) {
                let target = crate::links::LinkTarget::File {
                    path: source.clone(),
                    page: Some(*src),
                };
                let rect = markupcraft_model::Rect::new(30.0, y - 3.0, w.min(14_400.0) - 30.0, y + 9.0);
                s.add_link(*page, rect, &target, crate::links::LinkLook::default())?;
            }
        }
        if let Some(logo) = &r.layout.logo {
            for p in 0..s.page_count() {
                let (pw, ph) = {
                    let c = s.page(p)?.crop.normalized();
                    (c.width(), c.height())
                };
                let at = crate::stamps::StampPlace::Rect(markupcraft_model::Rect::new(
                    pw - 36.0 - 90.0,
                    ph - 30.0,
                    pw - 36.0,
                    ph - 4.0,
                ));
                let src = crate::stamps::StampSource::File {
                    path: logo.clone(),
                    page: 0,
                };
                s.place_stamp(p, at, &src, None, &Default::default(), None)?;
            }
        }
        s.save_as(out, true)
    }

    /// One report per value of the first column (Summary > Output): `<stem> - <value>.<ext>`
    /// beside `out`. Returns the files written.
    pub fn export_summary_per_value(
        &self,
        out: &Path,
        format: Option<SummaryFormat>,
        o: &SummaryOptions,
    ) -> Result<Vec<std::path::PathBuf>> {
        let format = format
            .or_else(|| SummaryFormat::from_path(out))
            .ok_or_else(|| invalid("summary format: csv, xml, xlsx or pdf"))?;
        let table = MarkupTable::new(self.doc());
        let first = match o.columns.first() {
            Some(c) => c.clone(),
            None => table
                .columns()
                .iter()
                .find(|c| c.visible_by_default)
                .map(|c| c.id.clone())
                .ok_or_else(|| invalid("no columns"))?,
        };
        let col = table
            .column_index(&first)
            .ok_or_else(|| invalid(format!("no column {first:?}")))?;
        let mut values: Vec<String> = (0..self.doc().markups.len())
            .map(|r| table.cell(r, col).text.clone())
            .collect();
        values.sort();
        values.dedup();
        let stem = out
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Summary".into());
        let dir = out.parent().unwrap_or(Path::new(""));
        let mut files = Vec::new();
        for v in values.into_iter().take(1_000) {
            let mut oo = o.clone();
            oo.filters.insert(first.clone(), [v.clone()].into_iter().collect());
            let safe: String = v
                .chars()
                .map(|c| {
                    if "/\\:*?\"<>|".contains(c) || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .take(80)
                .collect();
            let name = if safe.trim().is_empty() {
                "(blank)".to_string()
            } else {
                safe
            };
            let p = dir.join(format!("{stem} - {name}.{}", format.extension()));
            self.export_summary(&p, Some(format), &oo)?;
            files.push(p);
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::from_bytes(markupcraft_render::synthetic::sample_pdf(), "sample.pdf").unwrap()
    }

    #[test]
    fn summary_matches_the_list_and_writes_every_format() {
        let s = session();
        let r = s.summary(&SummaryOptions {
            group_by: vec!["type".into()],
            ..Default::default()
        });
        assert_eq!(r.count, 6);
        assert!(r.headers.contains(&"Subject".to_string()));
        assert!(matches!(r.lines.last(), Some(SummaryLine::Total { label, .. }) if label == "Total (6)"));
        assert!(r.lines.iter().any(|l| matches!(l, SummaryLine::Group { .. })));
        assert!(summary_csv(&r).starts_with("Subject,"));
        assert!(summary_xml(&r).contains("<MarkupSummary"));
        let pdf = summary_pdf(&r);
        let back = Session::from_bytes(pdf, "summary.pdf").unwrap();
        assert!(back.page_count() >= 1);
        let txt = back.page_text(0).unwrap();
        assert!(txt.contains("Markup Summary"), "{txt}");
        let x = summary_xlsx(&r);
        assert_eq!(x.get(..4), Some(&b"PK\x03\x04"[..]));
        // The end-of-central-directory record lists the six parts.
        let eocd = x.len() - 22;
        assert_eq!(x.get(eocd..eocd + 4), Some(&b"PK\x05\x06"[..]));
        assert_eq!(x.get(eocd + 10), Some(&6));
        let sheet = String::from_utf8_lossy(&x);
        assert!(sheet.contains("<sheetData>") && sheet.contains("inlineStr"));
    }

    #[test]
    fn summary_pages_and_columns_filter() {
        let s = session();
        let r = s.summary(&SummaryOptions {
            columns: vec!["subject".into(), "measurement".into(), "nope".into()],
            pages: vec![1],
            ..Default::default()
        });
        assert_eq!(r.headers, vec!["Subject".to_string(), "Measurement".to_string()]);
        assert!(r.count < 6);
        let dir = std::env::temp_dir().join(format!("mc-summary-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("s.xlsx");
        assert_eq!(s.export_summary(&out, None, &SummaryOptions::default()).unwrap(), 6);
        assert!(std::fs::read(&out).unwrap().starts_with(b"PK"));
        assert!(
            s.export_summary(&dir.join("s.doc"), None, &SummaryOptions::default())
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_sort_content_layouts_links_and_one_report_per_value() {
        let s = session();
        let r = s.summary(&SummaryOptions {
            sort: "type".into(),
            then_by: vec![("subject".into(), true)],
            content: SummaryContent::Totals,
            no_headers: true,
            date_in_title: true,
            ..Default::default()
        });
        assert!(r.title.starts_with("Markup Summary 20"), "{}", r.title);
        let csv = summary_csv(&r);
        assert!(!csv.starts_with("Subject,") && csv.contains("Total (6)"), "{csv}");
        assert_eq!(csv.lines().count(), 1, "totals only: {csv}");
        assert_eq!(r.line_pages.len(), r.lines.len());
        let dir = std::env::temp_dir().join(format!("mc-summary2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Flow layout, a page per group, links to the source pages.
        let o = SummaryOptions {
            group_by: vec!["type".into()],
            layout: PdfLayout {
                flow: true,
                break_per_group: true,
                page_size: (612.0, 792.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let out = dir.join("flow.pdf");
        s.export_summary(&out, None, &o).unwrap();
        let back = Session::open(&out).unwrap();
        assert!(back.page_count() >= 2, "a page per group");
        assert_eq!(back.links().len(), 6, "each markup links to its page");
        assert!(back.page_text(0).unwrap().contains("Subject:"));
        let files = s
            .export_summary_per_value(&dir.join("per.csv"), None, &SummaryOptions::default())
            .unwrap();
        assert!(files.len() >= 2, "{files:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn helpers() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(SummaryFormat::from_name("XLSX"), Some(SummaryFormat::Xlsx));
        assert_eq!(pdf_text("a\u{e9}b", 10), "a?b");
    }
}
