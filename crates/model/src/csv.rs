//! Markups List summaries: CSV (spreadsheet-safe), XML, and a totals-only CSV.
//!
//! Spreadsheet-safe: a cell that a spreadsheet would run as a formula (`=`, `+`, `@`, a tab or
//! carriage return first, or `-` not followed by a number) gets a leading `'`, and cells with a
//! comma, quote or line break are quoted with doubled quotes (RFC 4180).

use crate::table::{Group, MarkupTable, Totals};
use crate::{Document, Kind};

/// One CSV cell, quoted when needed and defused against formula injection.
pub fn csv_cell(s: &str) -> String {
    let defuse = match s.chars().next() {
        Some('=' | '+' | '@' | '\t' | '\r') => true,
        Some('-') => !looks_numeric(s),
        _ => false,
    };
    let s = if defuse { format!("'{s}") } else { s.to_string() };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// `-12.5`, `-$1,234.50`, `-3 sf`, `-4'-6"`: a negative number, not a formula.
fn looks_numeric(s: &str) -> bool {
    let rest = s.trim_start_matches('-').trim_start_matches(['$', '€', '£']);
    rest.starts_with(|c: char| c.is_ascii_digit() || c == '.')
}

fn row(cells: impl IntoIterator<Item = String>) -> String {
    let v: Vec<String> = cells.into_iter().map(|c| csv_cell(&c)).collect();
    v.join(",") + "\n"
}

fn resolve(t: &MarkupTable<'_>, ids: &[String]) -> Vec<usize> {
    ids.iter().filter_map(|id| t.column_index(id)).collect()
}

fn header(t: &MarkupTable<'_>, col: usize) -> String {
    t.columns().get(col).map(|c| c.header.clone()).unwrap_or_default()
}

/// `Subject: Duct` (`(blank)` for an empty key).
pub fn group_name(t: &MarkupTable<'_>, g: &Group) -> String {
    let h = g.column.map(|c| header(t, c)).unwrap_or_default();
    let k = if g.key.is_empty() { "(blank)" } else { g.key.as_str() };
    format!("{h}: {k}")
}

/// The list as CSV over `columns` (ids, in order): a header row, one row per markup; with
/// `totals`, a subtotal row after each group and a grand total row. A group divider is a row
/// whose first cell names the group.
pub fn table_csv(t: &MarkupTable<'_>, root: &Group, columns: &[String], totals: bool) -> String {
    let cols = resolve(t, columns);
    let mut out = row(cols.iter().map(|&c| header(t, c)));
    let totals_row = |label: &str, tot: &Totals| {
        row(cols.iter().enumerate().map(|(i, &c)| {
            let v = t.totals_text(c, tot);
            if i == 0 {
                let head = format!("{label} ({})", tot.count);
                if v.is_empty() { head } else { format!("{head} {v}") }
            } else {
                v
            }
        }))
    };
    fn walk(
        t: &MarkupTable<'_>,
        g: &Group,
        cols: &[usize],
        totals: bool,
        out: &mut String,
        totals_row: &dyn Fn(&str, &Totals) -> String,
    ) {
        if g.column.is_some() {
            let name = format!("{}{}", "  ".repeat(g.level), group_name(t, g));
            out.push_str(&row(
                std::iter::once(name).chain(cols.iter().skip(1).map(|_| String::new()))
            ));
        }
        for &r in &g.rows {
            out.push_str(&row(cols.iter().map(|&c| t.cell(r, c).text.clone())));
        }
        for c in &g.children {
            walk(t, c, cols, totals, out, totals_row);
        }
        if totals && g.column.is_some() {
            out.push_str(&totals_row(&format!("Subtotal {}", group_name(t, g)), &g.totals));
        }
    }
    walk(t, root, &cols, totals, &mut out, &totals_row);
    if totals {
        out.push_str(&totals_row("Total", &root.totals));
    }
    out
}

/// Totals only: one row per group (nested groups indented) and a grand total row, with the
/// count and every summed column among `columns`.
pub fn totals_csv(t: &MarkupTable<'_>, root: &Group, columns: &[String]) -> String {
    let cols: Vec<usize> = resolve(t, columns)
        .into_iter()
        .filter(|&c| t.columns().get(c).is_some_and(|col| col.total))
        .collect();
    let mut out = row(["Group".to_string(), "Count".to_string()]
        .into_iter()
        .chain(cols.iter().map(|&c| header(t, c))));
    fn walk(t: &MarkupTable<'_>, g: &Group, cols: &[usize], out: &mut String) {
        if g.column.is_some() {
            let name = format!("{}{}", "  ".repeat(g.level), group_name(t, g));
            out.push_str(&row([name, g.totals.count.to_string()]
                .into_iter()
                .chain(cols.iter().map(|&c| t.totals_text(c, &g.totals)))));
        }
        for c in &g.children {
            walk(t, c, cols, out);
        }
    }
    walk(t, root, &cols, &mut out);
    out.push_str(&row(["Total".to_string(), root.totals.count.to_string()]
        .into_iter()
        .chain(cols.iter().map(|&c| t.totals_text(c, &root.totals)))));
    out
}

fn xml_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// An XML element name from a header: `Wall Area` -> `WallArea`, `1st` -> `_1st`.
fn xml_name(h: &str) -> String {
    let out: String = h
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        format!("_{out}")
    } else {
        out
    }
}

/// `<MarkupSummary><Columns/><Group name=...><Markup>...</Markup><Totals/></Group>...`
pub fn table_xml(t: &MarkupTable<'_>, root: &Group, columns: &[String], title: &str) -> String {
    let cols = resolve(t, columns);
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    o += &format!("<MarkupSummary document=\"{}\">\n  <Columns>\n", xml_esc(title));
    for &c in &cols {
        let (id, h) = t
            .columns()
            .get(c)
            .map(|col| (col.id.clone(), col.header.clone()))
            .unwrap_or_default();
        o += &format!(
            "    <Column id=\"{}\" element=\"{}\">{}</Column>\n",
            xml_esc(&id),
            xml_name(&h),
            xml_esc(&h)
        );
    }
    o += "  </Columns>\n";
    fn totals_xml(t: &MarkupTable<'_>, tot: &Totals, cols: &[usize], ind: &str, o: &mut String) {
        let items: Vec<String> = cols
            .iter()
            .filter(|c| tot.sums.contains_key(c))
            .map(|&c| {
                let n = xml_name(&header(t, c));
                format!("{ind}  <{n}>{}</{n}>\n", xml_esc(&t.totals_text(c, tot)))
            })
            .collect();
        if items.is_empty() {
            *o += &format!("{ind}<Totals count=\"{}\"></Totals>\n", tot.count);
        } else {
            *o += &format!(
                "{ind}<Totals count=\"{}\">\n{}{ind}</Totals>\n",
                tot.count,
                items.concat()
            );
        }
    }
    fn walk(t: &MarkupTable<'_>, g: &Group, cols: &[usize], ind: &str, o: &mut String) {
        let inner = if g.column.is_some() {
            *o += &format!(
                "{ind}<Group column=\"{}\" name=\"{}\">\n",
                xml_esc(&g.column.map(|c| header(t, c)).unwrap_or_default()),
                xml_esc(&g.key)
            );
            format!("{ind}  ")
        } else {
            ind.to_string()
        };
        for &r in &g.rows {
            let Some(m) = t.document().markups.get(r) else { continue };
            *o += &format!("{inner}<Markup id=\"{}\" page=\"{}\">\n", xml_esc(&m.id), m.page + 1);
            for &c in cols {
                let n = xml_name(&header(t, c));
                let cell = t.cell(r, c);
                let mut attrs = String::new();
                if let Some(v) = cell.num {
                    attrs += &format!(" value=\"{}\"", markupcraft_measure::fmt_g(v, 6));
                    if !cell.unit.is_empty() {
                        attrs += &format!(" unit=\"{}\"", xml_esc(&cell.unit));
                    }
                }
                *o += &format!("{inner}  <{n}{attrs}>{}</{n}>\n", xml_esc(&cell.text));
            }
            for y in &m.replies {
                *o += &format!(
                    "{inner}  <Reply author=\"{}\" date=\"{}\">{}</Reply>\n",
                    xml_esc(&y.author),
                    xml_esc(&y.date),
                    xml_esc(&y.text)
                );
            }
            *o += &format!("{inner}</Markup>\n");
        }
        for c in &g.children {
            walk(t, c, cols, &inner, o);
        }
        if g.column.is_some() {
            totals_xml(t, &g.totals, cols, &inner, o);
            *o += &format!("{ind}</Group>\n");
        }
    }
    walk(t, root, &cols, "  ", &mut o);
    totals_xml(t, &root.totals, &cols, "  ", &mut o);
    o += "</MarkupSummary>\n";
    o
}

/// A flat export of every markup with the raw quantity (one row per markup, fixed columns).
pub fn markups_csv(doc: &Document, measurements_only: bool) -> String {
    let mut o = row([
        "Page",
        "Subject",
        "Label",
        "Type",
        "Measurement",
        "Value",
        "Unit",
        "Scale",
        "Author",
        "Date",
        "Layer",
        "Id",
    ]
    .map(String::from));
    for m in &doc.markups {
        if measurements_only && !m.kind.is_measurement() {
            continue;
        }
        let q = m.quantity();
        let shown = if m.kind == Kind::Count {
            q.map(|v| format!("{} ea", v as i64)).unwrap_or_default()
        } else {
            m.quantity_text()
        };
        o += &row([
            (m.page + 1).to_string(),
            m.subject.clone(),
            m.label.clone(),
            m.kind.name().to_string(),
            shown,
            q.map(|v| format!("{v:.4}")).unwrap_or_default(),
            if q.is_some() { m.unit() } else { String::new() },
            m.scale.as_ref().map(|s| s.ratio.clone()).unwrap_or_default(),
            m.author.clone(),
            m.modified.clone(),
            m.layer.clone(),
            m.id.clone(),
        ]);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::View;
    use crate::table::tests::sample;

    #[test]
    fn csv_cells_are_spreadsheet_safe() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("=SUM(A1:A2)"), "'=SUM(A1:A2)");
        assert_eq!(csv_cell("+1"), "'+1");
        assert_eq!(csv_cell("@cmd"), "'@cmd");
        assert_eq!(csv_cell("-2+3"), "-2+3");
        assert_eq!(csv_cell("-cmd|' /C calc'!A0"), "'-cmd|' /C calc'!A0");
        assert_eq!(csv_cell("-$1,234.50"), "\"-$1,234.50\"");
        assert_eq!(csv_cell("=1,2"), "\"'=1,2\"");
        assert_eq!(csv_cell("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn csv_summary_with_groups_and_totals() {
        let doc = sample();
        let t = MarkupTable::new(&doc);
        let v = View {
            visible: vec!["subject".into(), "measurement".into(), "c:total".into()],
            group_by: vec!["subject".into()],
            ..Default::default()
        };
        let root = t.build(&v);
        let csv = table_csv(&t, &root, &v.visible, true);
        assert!(csv.starts_with("Subject,Measurement,Total\n"), "{csv}");
        assert!(csv.contains("Subject: Duct,,\n"), "{csv}");
        assert!(csv.contains("Subtotal Subject: Floor (2)"), "{csv}");
        assert!(csv.contains("Total (4),35.00 ft; 500.00 sf,\"4,410.00\"\n"), "{csv}");
        let plain = table_csv(&t, &root, &v.visible, false);
        assert!(!plain.contains("Total ("));

        let tot = totals_csv(&t, &root, &v.visible);
        assert_eq!(
            tot,
            "Group,Count,Measurement,Total\nSubject: Duct,2,35.00 ft,210.00\n\
             Subject: Floor,2,500.00 sf,\"4,200.00\"\nTotal,4,35.00 ft; 500.00 sf,\"4,410.00\"\n"
        );

        let xml = table_xml(&t, &root, &v.visible, "test <1>.pdf");
        assert!(xml.contains("document=\"test &lt;1&gt;.pdf\""));
        assert!(xml.contains("<Group column=\"Subject\" name=\"Duct\">"), "{xml}");
        assert!(xml.contains("<Total value=\"210\">210.00</Total>"), "{xml}");
        assert!(xml.ends_with("</MarkupSummary>\n"));
    }

    #[test]
    fn csv_flat_export() {
        let doc = sample();
        let csv = markups_csv(&doc, true);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("Page,Subject,Label,Type,Measurement,Value,Unit"));
        assert!(
            lines[1].starts_with("1,Floor,,Area,100 sf,100.0000,sf,"),
            "{}",
            lines[1]
        );
    }
}
