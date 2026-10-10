//! Performance bench of the real app's hot paths (headless). `cargo xtask bench` builds and runs
//! it; run it directly for one input:
//!
//! ```text
//! cargo run --release -p markupcraft-ui-egui --example bench -- [file.pdf | --synthetic PAGES] [--markups N] [--json]
//! ```
//!
//! Without a file it builds a large synthetic drawing set in memory (`PAGES` sheets of
//! 36 x 24 in linework and text, `N` measurements and markups spread over them, default 120
//! pages and 600 markups). Every number is wall time on this machine; the table is printed to
//! stdout, one `name<TAB>ms<TAB>note` row per measurement with `--json` off.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use markupcraft_automation::Automation;
use markupcraft_engine::search::SearchOptions;
use markupcraft_engine::synthetic::{SyntheticPage, line, pdf, text};
use markupcraft_model::{MarkupTable, View, default_visible_columns};
use markupcraft_render::{RenderDoc, page_request};
use markupcraft_ui_egui::{AppState, MarkupCraftApp};
use serde_json::json;

/// Fit-width at a 1600 px wide canvas, and the thumbnail panel's side.
const CANVAS_PX: f32 = 1600.0;
const THUMB_PX: f32 = 200.0;

struct Row {
    name: &'static str,
    ms: f64,
    note: String,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// A sheet of plan-like linework: a grid of walls, door swings as short lines, room names.
fn sheet(i: usize) -> SyntheticPage {
    let (w, h) = (2592.0, 1728.0);
    let mut c = String::new();
    for k in 0..60 {
        let x = 100.0 + k as f64 * 40.0;
        c.push_str(&line(x, 80.0, x, h - 80.0, 1.0 + (k % 3) as f64));
        let y = 80.0 + k as f64 * 26.0;
        c.push_str(&line(80.0, y, w - 80.0, y, 0.5));
    }
    for k in 0..400 {
        let x = 120.0 + ((k * 37) % 2300) as f64;
        let y = 120.0 + ((k * 53) % 1450) as f64;
        c.push_str(&line(x, y, x + 18.0, y + 12.0, 0.25));
    }
    for k in 0..150 {
        let x = 140.0 + ((k * 71) % 2200) as f64;
        let y = 140.0 + ((k * 29) % 1400) as f64;
        c.push_str(&text(x, y, 7.0, &format!("ROOM {i}-{k} CORRIDOR")));
    }
    c.push_str(&text(2200.0, 100.0, 18.0, &format!("SHEET A-{i:03}")));
    SyntheticPage::new(w, h, c)
}

/// The synthetic set with `markups` measurements and markups on it, saved as a Revu-style file.
fn synthetic(dir: &Path, pages: usize, markups: usize) -> Vec<u8> {
    let sheets: Vec<SyntheticPage> = (0..pages.max(1)).map(sheet).collect();
    std::fs::write(dir.join("set.pdf"), pdf(&sheets)).unwrap();
    let mut a = Automation::new()
        .with_root(dir)
        .unwrap()
        .with_config_dir(dir.join("config"));
    let call = |a: &mut Automation, tool: &str, args: serde_json::Value| {
        a.call(tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
    };
    call(&mut a, "doc_open", json!({ "path": "set.pdf" }));
    call(
        &mut a,
        "scale_set",
        json!({ "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } }),
    );
    for k in 0..markups {
        let page = 1 + k % pages.max(1);
        let (x, y) = (200.0 + ((k * 97) % 2000) as f64, 200.0 + ((k * 61) % 1300) as f64);
        let (kind, points) = match k % 5 {
            0 => (
                "Area",
                json!([[x, y], [x + 120.0, y], [x + 120.0, y + 80.0], [x, y + 80.0]]),
            ),
            1 => ("Length", json!([[x, y], [x + 150.0, y + 20.0]])),
            2 => ("Polylength", json!([[x, y], [x + 60.0, y], [x + 60.0, y + 60.0]])),
            3 => ("Count", json!([[x, y], [x + 30.0, y], [x + 60.0, y]])),
            _ => ("Rectangle", json!([[x, y], [x + 50.0, y + 40.0]])),
        };
        call(
            &mut a,
            "markup_add",
            json!({ "page": page, "kind": kind, "points": points, "subject": format!("Bench {}", k % 7) }),
        );
    }
    call(&mut a, "doc_save", json!({ "full": true }));
    std::fs::read(dir.join("set.pdf")).unwrap()
}

/// Render `reqs` on the doc's pool; wall time until every one came back, and the slowest.
fn render_all(r: &RenderDoc, reqs: Vec<markupcraft_render::RenderRequest>) -> (Duration, u32, usize) {
    let n = reqs.len();
    let t = Instant::now();
    r.request(reqs);
    let (mut got, mut slowest, mut errors) = (0, 0, 0);
    while got < n {
        match r.poll() {
            Some(p) => {
                got += 1;
                slowest = slowest.max(p.millis);
                errors += usize::from(p.error.is_some());
            }
            None if t.elapsed() > Duration::from_secs(1800) => break,
            None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    (t.elapsed(), slowest, errors)
}

/// Peak resident memory of this process in MB, where the OS tells us without `unsafe`.
fn peak_mb() -> Option<f64> {
    if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
        let kb: f64 = s
            .lines()
            .find(|l| l.starts_with("VmHWM:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        return Some(kb / 1024.0);
    }
    if cfg!(windows) {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id()),
            ])
            .output()
            .ok()?;
        let b: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some(b / 1_048_576.0);
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let json_out = args.iter().any(|a| a == "--json");
    let dir = std::env::temp_dir().join(format!("markupcraft-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file: Option<PathBuf> = args
        .iter()
        .find(|a| !a.starts_with("--") && Path::new(a).is_file())
        .map(PathBuf::from);
    let (label, bytes) = match &file {
        Some(p) => ("reference".to_string(), std::fs::read(p).unwrap()),
        None => {
            let pages: usize = flag("--synthetic").and_then(|v| v.parse().ok()).unwrap_or(120);
            let n: usize = flag("--markups").and_then(|v| v.parse().ok()).unwrap_or(600);
            (format!("synthetic {pages} pages"), synthetic(&dir, pages, n))
        }
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut row = |name, d: f64, note: String| rows.push(Row { name, ms: d, note });
    let mb = bytes.len() as f64 / 1_048_576.0;

    // ---- open: the app's own path (parse, model, Markups, the renderer and its hidden copy)
    let mut app = AppState::default();
    app.threads = markupcraft_render::RenderOptions::default_threads();
    let at = dir.join("work.pdf");
    let t = Instant::now();
    app.open_bytes("work.pdf", Some(at.clone()), bytes.clone()).unwrap();
    let d = app.doc_mut().unwrap();
    let pages = d.session.page_count();
    let markups = d.session.doc().markups.len();
    row(
        "open",
        ms(t.elapsed()),
        format!("{mb:.1} MB, {pages} pages, {markups} markups"),
    );

    // ---- rendering
    let r = d.render.as_ref().unwrap();
    let fit = |p: usize| r.page(p).map_or(1.0, |g| CANVAS_PX / g.width.max(1.0));
    let (t1, _, _) = render_all(r, vec![page_request(0, fit(0), 1)]);
    row("first page render", ms(t1), format!("fit width {CANVAS_PX} px"));
    let reqs = (0..pages).map(|p| page_request(p, fit(p), 2)).collect();
    let (ta, slow, err) = render_all(r, reqs);
    row(
        "render every page",
        ms(ta),
        format!(
            "{:.0} ms/page, slowest {slow} ms, {err} errors, {} threads",
            ms(ta) / pages.max(1) as f64,
            app.threads
        ),
    );
    let d = app.doc_mut().unwrap();
    let r = d.render.as_ref().unwrap();
    let reqs = (0..pages)
        .map(|p| page_request(p, r.page(p).map_or(0.1, |g| g.scale_for_side(THUMB_PX)), 3))
        .collect();
    let (tt, _, _) = render_all(r, reqs);
    row(
        "thumbnails",
        ms(tt),
        format!("{THUMB_PX} px, {:.1} ms/page", ms(tt) / pages.max(1) as f64),
    );

    // ---- Markups List (what the panel builds each frame it is shown)
    let mut view = View {
        visible: default_visible_columns(),
        ..Default::default()
    };
    view.group_by = vec!["subject".into()];
    let n = 20;
    let t = Instant::now();
    let mut rows_seen = 0;
    for _ in 0..n {
        let table = MarkupTable::new(d.session.doc());
        let root = table.build(&view);
        rows_seen = root.totals.count;
    }
    row(
        "Markups List build",
        ms(t.elapsed()) / f64::from(n),
        format!("per build, {rows_seen} rows grouped by subject"),
    );

    // ---- the Markups List panel in the running interface, per frame
    {
        let threads = app.threads;
        let bytes2 = bytes.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(move |_cc| {
                let mut a = MarkupCraftApp::new();
                a.state.threads = threads;
                a.open_bytes("work.pdf", None, bytes2).unwrap();
                a
            });
        h.run_steps(4);
        let t = Instant::now();
        h.run_steps(30);
        let closed = ms(t.elapsed()) / 30.0;
        h.state_mut().set_option("panel", "markups");
        h.state_mut().set_option("group-by", "subject");
        h.run_steps(4);
        let t = Instant::now();
        h.run_steps(30);
        let open = ms(t.elapsed()) / 30.0;
        row("UI frame, default layout", closed, "per frame".into());
        row(
            "UI frame, Markups List grouped",
            open,
            "per frame, grouped by subject".into(),
        );
    }

    // ---- search (whole document), cold then warm, then after an edit
    let opts = SearchOptions::default();
    let d = app.doc_mut().unwrap();
    let t = Instant::now();
    let hits = d.session.search_text("ROOM", &opts).map(|r| r.hits.len()).unwrap_or(0);
    row("search all pages", ms(t.elapsed()), format!("{hits} hits"));
    let t = Instant::now();
    let _ = d.session.search_text("CORRIDOR", &opts);
    row("search again", ms(t.elapsed()), "second word".into());

    // ---- saves
    let id = d.session.doc().markups.first().map(|m| m.id.clone());
    let edit = |d: &mut markupcraft_ui_egui::DocTab| {
        if let Some(id) = &id {
            let _ = d.session.move_markups(std::slice::from_ref(id), 1.0, 0.0);
        }
    };
    edit(d);
    let t = Instant::now();
    let _ = d.session.search_text("ROOM", &opts);
    row("search after an edit", ms(t.elapsed()), "unsaved markup change".into());
    let t = Instant::now();
    d.session.save_as(&at, true).unwrap();
    row("full save", ms(t.elapsed()), format!("{:.1} MB", file_mb(&at)));
    edit(d);
    let t = Instant::now();
    d.session.save(false).unwrap();
    row("incremental save", ms(t.elapsed()), "one markup moved".into());
    let t = Instant::now();
    let s = markupcraft_engine::Session::open(&at).unwrap();
    row(
        "reopen (model only)",
        ms(t.elapsed()),
        format!("{} markups", s.doc().markups.len()),
    );
    drop(s);

    if let Some(p) = peak_mb() {
        row("peak memory (MB)", p, "working set".into());
    }
    let _ = std::fs::remove_dir_all(&dir);

    if json_out {
        let v: Vec<_> = rows
            .iter()
            .map(|r| json!({ "name": r.name, "ms": r.ms, "note": r.note }))
            .collect();
        println!("{}", json!({ "input": label, "rows": v }));
    } else {
        println!("bench: {label}");
        for r in &rows {
            println!("{}\t{:.1}\t{}", r.name, r.ms, r.note);
        }
    }
}

fn file_mb(p: &Path) -> f64 {
    std::fs::metadata(p).map_or(0.0, |m| m.len() as f64 / 1_048_576.0)
}
