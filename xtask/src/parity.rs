//! `cargo xtask parity [--check] [--write-md]`: the Revu feature parity table.
//!
//! Loads `parity/revu-features.toml` (one row per row of `docs/revu_features/01..05`), prints
//! totals per area and status, and:
//!   --check     fails when a row is malformed, an inventory row has no entry (or an entry points
//!               at a line that is not its row), or a have/proven row cites evidence that does
//!               not exist (a test function in the workspace, or `cli: <subcommand>` of
//!               markupcraft-cli)
//!   --write-md  regenerates FEATURES.md from the table

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const TABLE: &str = "parity/revu-features.toml";
const STATUSES: [&str; 4] = ["proven", "have", "partial", "missing"];
const CPP: [&str; 3] = ["have", "partial", "missing"];
const AREAS: [(&str, &str); 5] = [
    ("measurement", "Measurement and takeoff"),
    ("markups", "Markup tools, Properties, Tool Chest, layers, Markups List"),
    ("documents", "Documents, pages, batch, print, search, security"),
    (
        "compare-ui-collab",
        "Compare/overlay, interface, preferences, mouse, Studio",
    ),
    ("shortcuts", "Default keyboard shortcuts"),
];
/// Inventory files and the ID letter of their rows (shortcut rows have no ID).
const INVENTORY: [(&str, &str); 4] = [
    ("docs/revu_features/01_measurement.md", "M"),
    ("docs/revu_features/02_markups.md", "K"),
    ("docs/revu_features/03_documents.md", "D"),
    ("docs/revu_features/04_compare_ui_collab.md", "U"),
];
const SHORTCUTS: &str = "docs/revu_features/05_shortcuts.md";
const CLI_MAIN: &str = "apps/markupcraft-cli/src/main.rs";

#[derive(Debug, Deserialize)]
struct Table {
    #[serde(default)]
    feature: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Row {
    id: String,
    #[serde(rename = "ref")]
    reference: String,
    area: String,
    #[serde(default)]
    section: String,
    name: String,
    #[serde(default)]
    priority: String,
    source: String,
    status: String,
    cpp: String,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    excluded: bool,
}

fn workspace_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent().map(Path::to_path_buf).unwrap_or(here)
}

fn read(root: &Path, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"))
}

fn load(root: &Path) -> Result<Vec<Row>, String> {
    let text = read(root, TABLE)?;
    let t: Table = toml::from_str(&text).map_err(|e| format!("{TABLE}: {e}"))?;
    Ok(t.feature)
}

/// Run with the arguments after `parity`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    run_inner(args).map_err(anyhow::Error::msg)
}

fn run_inner(args: &[String]) -> Result<(), String> {
    let args: Vec<&str> = args.iter().map(String::as_str).skip_while(|a| *a == "parity").collect();
    if let Some(bad) = args.iter().find(|a| !matches!(**a, "--check" | "--write-md")) {
        return Err(format!(
            "unknown option {bad}; usage: cargo xtask parity [--check] [--write-md]"
        ));
    }
    let root = workspace_root();
    let rows = load(&root)?;
    print!("{}", totals_text(&rows));
    if args.contains(&"--check") {
        let problems = check(&root, &rows)?;
        if !problems.is_empty() {
            for p in problems.iter().take(50) {
                eprintln!("  {p}");
            }
            return Err(format!("{TABLE}: {} problem(s)", problems.len()));
        }
        println!("check: ok ({} rows, evidence verified)", rows.len());
    }
    if args.contains(&"--write-md") {
        let md = features_md(&rows);
        std::fs::write(root.join("FEATURES.md"), md).map_err(|e| format!("FEATURES.md: {e}"))?;
        println!("wrote FEATURES.md");
    }
    Ok(())
}

/// status -> count over rows in scope, plus the excluded count.
fn counts<'a>(rows: impl Iterator<Item = &'a Row>) -> (BTreeMap<&'a str, usize>, usize) {
    let mut by = BTreeMap::new();
    let mut excluded = 0;
    for r in rows {
        if r.excluded {
            excluded += 1;
        } else {
            *by.entry(r.status.as_str()).or_insert(0) += 1;
        }
    }
    (by, excluded)
}

fn totals_text(rows: &[Row]) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "{:<20} {:>6} {:>7} {:>6} {:>8} {:>8} {:>9}",
        "area", "rows", "proven", "have", "partial", "missing", "excluded"
    );
    let line = |o: &mut String, name: &str, sel: &[&Row]| {
        let (by, ex) = counts(sel.iter().copied());
        let g = |s: &str| by.get(s).copied().unwrap_or(0);
        let _ = writeln!(
            o,
            "{name:<20} {:>6} {:>7} {:>6} {:>8} {:>8} {:>9}",
            sel.len(),
            g("proven"),
            g("have"),
            g("partial"),
            g("missing"),
            ex
        );
    };
    for (area, _) in AREAS {
        let sel: Vec<&Row> = rows.iter().filter(|r| r.area == area).collect();
        line(&mut o, area, &sel);
    }
    let all: Vec<&Row> = rows.iter().collect();
    line(&mut o, "total", &all);
    o
}

/// Every test function name in the workspace (`#[test]` followed by `fn name`).
fn test_names(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !matches!(name.as_str(), "target" | ".git" | "node_modules") && !name.starts_with('.') {
                    stack.push(p);
                }
                continue;
            }
            if !name.ends_with(".rs") {
                continue;
            }
            seen += 1;
            if seen > 20_000 {
                return out;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let mut after_test = false;
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with("#[test]") {
                    after_test = true;
                    continue;
                }
                if after_test && t.starts_with("#[") {
                    continue;
                }
                if after_test && let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("async fn ")) {
                    let n: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if !n.is_empty() {
                        out.insert(n);
                    }
                }
                after_test = false;
            }
        }
    }
    out
}

/// Subcommands markupcraft-cli dispatches (`("name", ...) =>` arms in its main.rs).
fn cli_commands(root: &Path) -> BTreeSet<String> {
    let Ok(text) = read(root, CLI_MAIN) else {
        return BTreeSet::new();
    };
    text.lines()
        .filter_map(|l| {
            let t = l.trim().strip_prefix("(\"")?;
            let end = t.find('"')?;
            let name = t.get(..end)?;
            t.get(end..)?.contains("=>").then(|| name.to_string())
        })
        .collect()
}

/// Inventory rows as (reference, file, line): IDs from 01..04, `Category: Command` from 05.
fn inventory(root: &Path) -> Result<Vec<(String, String, usize)>, String> {
    let mut out = Vec::new();
    for (file, letter) in INVENTORY {
        let text = read(root, file)?;
        for (i, line) in text.lines().enumerate() {
            let Some(rest) = line.strip_prefix("| ") else { continue };
            let id: String = rest.chars().take_while(|c| *c != ' ').collect();
            let Some(num) = id.strip_prefix(letter).and_then(|n| n.strip_prefix('-')) else {
                continue;
            };
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                out.push((id, file.to_string(), i + 1));
            }
        }
    }
    let text = read(root, SHORTCUTS)?;
    let mut in_map = false;
    for (i, line) in text.lines().enumerate() {
        if let Some(h) = line.strip_prefix("## ") {
            in_map = h.starts_with("1.");
            continue;
        }
        if !in_map || !line.starts_with("| ") || line.starts_with("| Category") {
            continue;
        }
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
        if let [cat, cmd, _, _, _, ..] = cells.as_slice() {
            out.push((format!("{cat}: {cmd}"), SHORTCUTS.to_string(), i + 1));
        }
    }
    Ok(out)
}

fn check(root: &Path, rows: &[Row]) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let tests = test_names(root);
    let cli = cli_commands(root);
    let areas: BTreeSet<&str> = AREAS.iter().map(|a| a.0).collect();
    let mut ids = BTreeSet::new();
    let mut refs: BTreeMap<String, &Row> = BTreeMap::new();
    for r in rows {
        if !ids.insert(r.id.as_str()) {
            problems.push(format!("{}: duplicate id", r.id));
        }
        if refs.insert(r.reference.clone(), r).is_some() {
            problems.push(format!("{}: duplicate ref {}", r.id, r.reference));
        }
        if r.name.trim().is_empty() {
            problems.push(format!("{}: empty name", r.id));
        }
        if !areas.contains(r.area.as_str()) {
            problems.push(format!("{}: unknown area {}", r.id, r.area));
        }
        if !STATUSES.contains(&r.status.as_str()) {
            problems.push(format!("{}: status {} is not one of {STATUSES:?}", r.id, r.status));
        }
        if !CPP.contains(&r.cpp.as_str()) {
            problems.push(format!("{}: cpp {} is not one of {CPP:?}", r.id, r.cpp));
        }
        if matches!(r.status.as_str(), "have" | "proven") && r.evidence.is_empty() {
            problems.push(format!("{}: status {} needs evidence", r.id, r.status));
        }
        if r.status == "proven" && !r.evidence.iter().any(|e| e.starts_with("cli:")) && r.notes.is_empty() {
            problems.push(format!(
                "{}: proven needs a recording (a cli scorecard command or a note)",
                r.id
            ));
        }
        for e in &r.evidence {
            let ok = match e.strip_prefix("cli:") {
                Some(cmd) => cmd.split_whitespace().next().is_some_and(|c| cli.contains(c)),
                None => tests.contains(e.as_str()),
            };
            if !ok {
                problems.push(format!("{}: evidence `{e}` does not exist", r.id));
            }
        }
    }
    // Every inventory row has an entry whose source points at it.
    let inv = inventory(root)?;
    for (reference, file, line) in &inv {
        match refs.get(reference) {
            None => problems.push(format!("{file}:{line}: inventory row {reference} has no entry")),
            Some(r) if r.source != format!("{file}:{line}") => {
                problems.push(format!("{}: source {} but the row is at {file}:{line}", r.id, r.source))
            }
            Some(_) => {}
        }
    }
    let known: BTreeSet<&str> = inv.iter().map(|i| i.0.as_str()).collect();
    for r in rows {
        if !known.contains(r.reference.as_str()) {
            problems.push(format!("{}: ref {} is not in the inventory", r.id, r.reference));
        }
    }
    Ok(problems)
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn features_md(rows: &[Row]) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# MarkupCraft vs Revu 21: feature parity\n");
    let _ = writeln!(
        o,
        "Generated by `cargo xtask parity --write-md` from `{TABLE}` (do not edit by hand). One row per row of\n\
         the clean-room inventory in `docs/revu_features/` (built from public documentation only).\n\n\
         Status is the Rust workspace today: **proven** = matches a recording of real Revu \
         (`docs/EQUIVALENCE.md`), **have** = built and tested from documentation, **partial** = some of it \
         (often the file format and engine without UI), **missing** = not started. *C++* is what the C++ \
         reference build had, the port backlog. Excluded rows (Studio server, Bluebeam Cloud, DMS, \
         Office/CAD plugins, 3D PDF) are out of scope and not counted.\n"
    );
    let _ = writeln!(o, "## Totals\n");
    let _ = writeln!(
        o,
        "| Area | Rows | Proven | Have | Partial | Missing | Excluded | C++ have | C++ partial |"
    );
    let _ = writeln!(o, "|---|---|---|---|---|---|---|---|---|");
    let line = |o: &mut String, label: &str, sel: &[&Row]| {
        let (by, ex) = counts(sel.iter().copied());
        let g = |s: &str| by.get(s).copied().unwrap_or(0);
        let cpp = |s: &str| sel.iter().filter(|r| !r.excluded && r.cpp == s).count();
        let _ = writeln!(
            o,
            "| {label} | {} | {} | {} | {} | {} | {ex} | {} | {} |",
            sel.len(),
            g("proven"),
            g("have"),
            g("partial"),
            g("missing"),
            cpp("have"),
            cpp("partial")
        );
    };
    for (area, title) in AREAS {
        let sel: Vec<&Row> = rows.iter().filter(|r| r.area == area).collect();
        line(&mut o, title, &sel);
    }
    let all: Vec<&Row> = rows.iter().collect();
    line(&mut o, "**All**", &all);

    let _ = writeln!(o, "\n### By priority (rows in scope)\n");
    let _ = writeln!(o, "| Priority | Proven | Have | Partial | Missing |");
    let _ = writeln!(o, "|---|---|---|---|---|");
    for p in ["P0", "P1", "P2", "P3"] {
        let (by, _) = counts(rows.iter().filter(|r| r.priority == p));
        let g = |s: &str| by.get(s).copied().unwrap_or(0);
        let _ = writeln!(
            o,
            "| {p} | {} | {} | {} | {} |",
            g("proven"),
            g("have"),
            g("partial"),
            g("missing")
        );
    }

    for (area, title) in AREAS {
        let _ = writeln!(o, "\n## {title}");
        let mut section = None;
        for r in rows.iter().filter(|r| r.area == area) {
            if section != Some(r.section.as_str()) {
                section = Some(r.section.as_str());
                let _ = writeln!(o, "\n### {}\n", md_cell(&r.section));
                let _ = writeln!(o, "| ID | Feature | Pri | Status | C++ | Evidence / notes |");
                let _ = writeln!(o, "|---|---|---|---|---|---|");
            }
            let status = if r.excluded {
                "excluded".to_string()
            } else {
                r.status.clone()
            };
            let mut ev: Vec<String> = r.evidence.iter().map(|e| format!("`{e}`")).collect();
            if !r.notes.is_empty() {
                ev.push(md_cell(&r.notes));
            }
            let _ = writeln!(
                o,
                "| {} | {} | {} | {status} | {} | {} |",
                r.id,
                md_cell(&r.name),
                r.priority,
                r.cpp,
                ev.join(" ")
            );
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_table_is_consistent() {
        let root = workspace_root();
        let rows = load(&root).unwrap();
        assert!(rows.len() >= 769 + 177);
        let problems = check(&root, &rows).unwrap();
        assert!(problems.is_empty(), "{problems:#?}");
        let md = features_md(&rows);
        assert!(md.contains("| **All** |"));
        assert!(totals_text(&rows).contains("total"));
    }

    #[test]
    fn parity_check_rejects_missing_evidence() {
        let root = workspace_root();
        let mut rows = load(&root).unwrap();
        let r = rows.iter_mut().find(|r| r.status == "missing" && !r.excluded).unwrap();
        r.status = "have".into();
        r.evidence = vec!["no_such_test_anywhere".into(), "cli: frobnicate".into()];
        let problems = check(&root, &rows).unwrap();
        assert_eq!(problems.len(), 2, "{problems:#?}");
    }
}
