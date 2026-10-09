//! `list` and `summary`: the Markups List from the command line (the same table as the app).
//!
//! View options shared by both:
//!   --group a[,b]        group by columns (ids, headers or custom column names)
//!   --sort col [--desc]  sort rows
//!   --filter col=value   keep rows whose cell text is value (repeatable; same column = OR)
//!   --search text        quick filter over the shown columns
//!   --columns a,b,...    columns shown / exported (default: the standard visible set + custom)
//!   --page N             only page N (1-based)
//!   --measurements       only measurements

use std::collections::BTreeMap;

use markupcraft_model::csv::{group_name, table_csv, table_xml, totals_csv};
use markupcraft_model::table::page_label;
use markupcraft_model::{Document, Group, MarkupTable, Scope, View, default_visible_columns};
use markupcraft_revu::open;

type Res = Result<bool, Box<dyn std::error::Error>>;

/// The value after `flag`, if any.
fn opt<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn list_arg(args: &[String], flag: &str) -> Vec<String> {
    opt(args, flag)
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The view described by the command line.
pub fn view_from_args(doc: &Document, args: &[String], default_group: &[&str]) -> Result<View, String> {
    let mut v = View {
        visible: list_arg(args, "--columns"),
        group_by: list_arg(args, "--group"),
        sort_column: opt(args, "--sort").unwrap_or_default().to_string(),
        sort_descending: args.iter().any(|a| a == "--desc"),
        search: opt(args, "--search").unwrap_or_default().to_string(),
        measurements_only: args.iter().any(|a| a == "--measurements"),
        ..Default::default()
    };
    if v.visible.is_empty() {
        v.visible = default_visible_columns();
        v.visible.extend(doc.columns.iter().map(|c| format!("c:{}", c.id)));
    }
    if v.group_by.is_empty() && !args.iter().any(|a| a == "--group") {
        v.group_by = default_group.iter().map(|s| s.to_string()).collect();
    }
    if let Some(p) = opt(args, "--page") {
        let n: usize = p.parse().map_err(|_| format!("--page {p}: not a page number"))?;
        if n == 0 || n > doc.pages.len() {
            return Err(format!("--page {p}: the document has {} pages", doc.pages.len()));
        }
        v.scope = Scope::CurrentPage(n - 1);
    }
    for (i, a) in args.iter().enumerate() {
        if a != "--filter" {
            continue;
        }
        let Some((col, val)) = args.get(i + 1).and_then(|f| f.split_once('=')) else {
            return Err("--filter wants column=value".into());
        };
        v.filters.entry(col.to_string()).or_default().insert(val.to_string());
    }
    Ok(v)
}

fn check_columns(t: &MarkupTable<'_>, v: &View) -> Result<(), String> {
    let ids = v
        .visible
        .iter()
        .chain(&v.group_by)
        .chain(v.filters.keys())
        .chain(std::iter::once(&v.sort_column).filter(|s| !s.is_empty()));
    for id in ids {
        if t.column_index(id).is_none() {
            return Err(format!("unknown column '{id}'"));
        }
    }
    Ok(())
}

fn print_totals(t: &MarkupTable<'_>, g: &Group, cols: &[usize]) {
    if g.column.is_some() {
        let name = format!("{}{}", "  ".repeat(g.level), group_name(t, g));
        println!("{name:<40} {:>6}  {}", g.totals.count, sums_text(t, g, cols));
    }
    for c in &g.children {
        print_totals(t, c, cols);
    }
}

fn sums_text(t: &MarkupTable<'_>, g: &Group, cols: &[usize]) -> String {
    let sums: Vec<String> = cols
        .iter()
        .map(|&c| t.totals_text(c, &g.totals))
        .map(|s| if s.is_empty() { "-".into() } else { s })
        .collect();
    sums.join("  |  ")
}

fn total_columns(t: &MarkupTable<'_>, v: &View) -> Vec<usize> {
    v.visible
        .iter()
        .filter_map(|id| t.column_index(id))
        .filter(|&c| t.columns().get(c).is_some_and(|col| col.total))
        .collect()
}

/// `list <pdf> [--csv out.csv] [view options]`: kinds, totals by subject, optional CSV.
pub fn list(path: &str, args: &[String]) -> Res {
    let (_f, doc) = open(path)?;
    let t = MarkupTable::new(&doc);
    let v = view_from_args(&doc, args, &[])?;
    check_columns(&t, &v)?;
    let root = t.build(&v);

    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &doc.markups {
        *kinds.entry(m.kind.name()).or_default() += 1;
    }
    println!("{path}\n{} pages, {} markups\n", doc.pages.len(), doc.markups.len());
    for (k, n) in &kinds {
        println!("  {k:<14} {n:>6}");
    }
    if !doc.columns.is_empty() {
        let names: Vec<&str> = doc.columns.iter().map(|c| c.name.as_str()).collect();
        println!("\ncustom columns: {}", names.join(", "));
    }

    // Totals by subject (the view's filters apply).
    let by_subject = View {
        group_by: vec!["subject".into()],
        measurements_only: true,
        ..v.clone()
    };
    let sroot = t.build(&by_subject);
    let mc = t.column_index("measurement").unwrap_or(usize::MAX);
    println!("\n{:<40} {:>6}  TOTAL", "SUBJECT", "COUNT");
    print_totals(&t, &sroot, &[mc]);
    println!(
        "{:<40} {:>6}  {}",
        "Total",
        sroot.totals.count,
        t.totals_text(mc, &sroot.totals)
    );

    if let Some(p) = doc.pages.first().filter(|p| !p.label.is_empty()) {
        println!(
            "\npage labels: {} .. {}",
            p.label,
            page_label(&doc, doc.pages.len().saturating_sub(1))
        );
    }
    if let Some(csv) = opt(args, "--csv") {
        std::fs::write(csv, table_csv(&t, &root, &v.visible, true))?;
        println!("\nwrote {csv} ({} rows)", root.totals.count);
    }
    Ok(true)
}

/// `summary <pdf> [--csv out.csv] [--totals out.csv] [--xml out.xml] [view options]`: totals
/// per group (default: by subject) and a grand total per unit.
pub fn summary(path: &str, args: &[String]) -> Res {
    let (_f, doc) = open(path)?;
    let t = MarkupTable::new(&doc);
    let v = view_from_args(&doc, args, &["subject"])?;
    check_columns(&t, &v)?;
    let root = t.build(&v);
    let cols = total_columns(&t, &v);
    let headers: Vec<&str> = cols
        .iter()
        .filter_map(|&c| t.columns().get(c).map(|col| col.header.as_str()))
        .collect();
    println!("{path}\n{:<40} {:>6}  {}", "GROUP", "COUNT", headers.join("  |  "));
    print_totals(&t, &root, &cols);
    println!(
        "{:<40} {:>6}  {}",
        "Total",
        root.totals.count,
        sums_text(&t, &root, &cols)
    );

    if let Some(out) = opt(args, "--csv") {
        std::fs::write(out, table_csv(&t, &root, &v.visible, true))?;
        println!("wrote {out}");
    }
    if let Some(out) = opt(args, "--totals") {
        std::fs::write(out, totals_csv(&t, &root, &v.visible))?;
        println!("wrote {out}");
    }
    if let Some(out) = opt(args, "--xml") {
        let title = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::write(out, table_xml(&t, &root, &v.visible, &title))?;
        println!("wrote {out}");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use markupcraft_model::{ColumnType, CustomColumn, Kind, Markup, Point, Scale};
    use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, SaveOptions, write_full};
    use markupcraft_revu::{SaveMode, open_bytes, save};

    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("markupcraft-cli-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn blank_pdf() -> Vec<u8> {
        let mut cos = CosDoc::new_empty();
        let root = cos.root().unwrap();
        let pages = cos.get(root).as_dict().and_then(|d| d.reference(b"Pages")).unwrap();
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        let mb = [0, 0, 612, 792].map(Object::Int).to_vec();
        page.set(b"MediaBox".to_vec(), Object::Array(mb));
        let p = cos.add(Object::Dict(page));
        cos.update_dict(pages, |d| {
            d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(p)]));
            d.set(b"Count".to_vec(), Object::Int(1));
        })
        .unwrap();
        write_full(&cos, &SaveOptions::default()).unwrap()
    }

    fn square(side_ft: f64, subject: &str, rate: &str) -> Markup {
        let s = side_ft * 9.0;
        let pts = [(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)].map(|(x, y)| Point::new(100.0 + x, 100.0 + y));
        let mut m = Markup::new(Kind::Area, 0, pts.to_vec());
        m.subject = subject.into();
        m.scale = Some(Scale::architectural(0.125, 1.0));
        m.column_data.insert("rate".into(), rate.into());
        m
    }

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cli_list_and_summary_end_to_end() {
        let (mut f, mut doc) = open_bytes(Arc::new(blank_pdf()), &tmp("blank.pdf")).unwrap();
        doc.columns = vec![
            CustomColumn {
                id: "rate".into(),
                name: "Rate".into(),
                kind: ColumnType::Currency,
                ..Default::default()
            },
            CustomColumn {
                id: "cost".into(),
                name: "Cost".into(),
                kind: ColumnType::Formula,
                formula: "Area * Rate".into(),
                ..Default::default()
            },
        ];
        doc.markups = vec![
            square(10.0, "Slab", "2"),
            square(20.0, "Slab", "3"),
            square(5.0, "=Roof", "4"),
        ];
        let pdf = tmp("takeoff.pdf");
        save(&mut f, &mut doc, &pdf, SaveMode::Full).unwrap();
        let pdf = pdf.to_string_lossy().into_owned();

        let csv = tmp("summary.csv").to_string_lossy().into_owned();
        let totals = tmp("totals.csv").to_string_lossy().into_owned();
        let xml = tmp("summary.xml").to_string_lossy().into_owned();
        assert!(summary(&pdf, &strings(&["--csv", &csv, "--totals", &totals, "--xml", &xml])).unwrap());
        let t = std::fs::read_to_string(&totals).unwrap();
        assert_eq!(
            t,
            "Group,Count,Measurement,Rate,Cost\nSubject: =Roof,1,25.00 sf,$4.00,100.00\n\
             Subject: Slab,2,500.00 sf,$5.00,\"1,400.00\"\nTotal,3,525.00 sf,$9.00,\"1,500.00\"\n"
        );
        let c = std::fs::read_to_string(&csv).unwrap();
        assert!(
            c.starts_with(
                "Subject,Page Label,Label,Measurement,Author,Date,Status,Checkmark,Color,Comments,Rate,Cost\n"
            ),
            "{c}"
        );
        assert!(c.contains("Subtotal Subject: Slab (2)"), "{c}");
        assert!(c.contains("\n'=Roof,1,"), "a formula-like subject is defused: {c}");
        let x = std::fs::read_to_string(&xml).unwrap();
        assert!(x.contains("<Cost value=\"1200\">1,200.00</Cost>"), "{x}");
        assert!(x.contains("<Cost>1,400.00</Cost>"), "{x}");

        let list_csv = tmp("list.csv").to_string_lossy().into_owned();
        let args = strings(&[
            "--csv",
            &list_csv,
            "--sort",
            "area",
            "--desc",
            "--filter",
            "subject=Slab",
            "--columns",
            "subject,area,cost",
        ]);
        assert!(list(&pdf, &args).unwrap());
        let l = std::fs::read_to_string(&list_csv).unwrap();
        assert_eq!(
            l,
            "Subject,Area,Cost\nSlab,400 sf,\"1,200.00\"\nSlab,100 sf,200.00\nTotal (2),500.00 sf,\"1,400.00\"\n"
        );

        assert!(summary(&pdf, &strings(&["--group", "nope"])).is_err());
        assert!(list(&pdf, &strings(&["--page", "7"])).is_err());
    }
}
