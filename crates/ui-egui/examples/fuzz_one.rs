//! One fuzz input through the app's whole document path, for `cargo xtask fuzz` (which runs it
//! in a child process with a timeout and records panics, aborts and hangs).
//!
//! ```text
//! cargo run --release -p markupcraft-ui-egui --example fuzz_one -- <file.pdf> <work dir>
//! cargo run --release -p markupcraft-ui-egui --example fuzz_one -- --write-seeds <dir>
//! ```
//! Open (model, Markups, renderer), render every page small, read page text and search, build
//! the Markups List grouped two ways and export it, edit (add a measurement, move and delete
//! one), save in full and incrementally, reopen both and list again. A document that cannot be
//! opened, saved or reopened is fine (an error is an answer); only a panic, abort or hang is a
//! finding. Exit status 0 = handled.

use std::path::Path;
use std::time::{Duration, Instant};

use markupcraft_engine::Session;
use markupcraft_engine::search::SearchOptions;
use markupcraft_model::csv::table_csv;
use markupcraft_model::{Kind, Markup, MarkupTable, Point, View, default_visible_columns};
use markupcraft_render::page_request;
use markupcraft_ui_egui::AppState;

/// At most this many pages are rendered and searched (a mutated /Count can claim millions).
const MAX_PAGES: usize = 40;

fn list(s: &Session) -> usize {
    let table = MarkupTable::new(s.doc());
    let mut n = 0;
    for g in [vec!["type".to_string()], vec!["subject".into(), "page".into()]] {
        let view = View {
            visible: default_visible_columns(),
            group_by: g,
            ..Default::default()
        };
        let root = table.build(&view);
        n += table_csv(&table, &root, &view.visible, true).len();
    }
    n
}

/// With `MARKUPCRAFT_FUZZ_TRACE` set, name each step on stderr (to find where a hang is).
fn step(name: &str) {
    if std::env::var_os("MARKUPCRAFT_FUZZ_TRACE").is_some() {
        eprintln!("step: {name}");
    }
}

fn run(file: &Path, work: &Path) -> Result<(), String> {
    step("open");
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let mut app = AppState::default();
    // Render on workers, as the app does: their watchdog gives up on a page stuck in the
    // renderer, so only a hang in MarkupCraft's own steps runs past the fuzzer's timeout.
    app.threads = 2;
    let at = work.join("out.pdf");
    app.open_bytes("fuzz.pdf", Some(at.clone()), bytes)?;
    let d = app.doc_mut().ok_or("no doc")?;
    let pages = d.session.page_count().min(MAX_PAGES);
    step("render");
    if let Some(r) = d.render.as_ref() {
        r.request((0..pages).map(|p| page_request(p, 0.15, 1)).collect());
        let t = Instant::now();
        let mut got = 0;
        while got < pages && t.elapsed() < Duration::from_secs(25) {
            if r.poll().is_some() {
                got += 1;
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    step("text");
    for p in 0..pages.min(4) {
        let _ = d.page_text(p);
    }
    step("search");
    let _ = d.session.search_text(
        "the",
        &SearchOptions {
            pages: Some((0..pages).collect()),
            ..Default::default()
        },
    );
    step("list");
    list(&d.session);

    step("edit");
    // Edit: a new measurement, a move and a delete of what the file had.
    if d.session.page_count() > 0 {
        let pts = vec![Point::new(10.0, 10.0), Point::new(60.0, 10.0), Point::new(60.0, 40.0)];
        let _ = d.session.add_markup(Markup::new(Kind::Area, 0, pts));
    }
    let ids: Vec<String> = d.session.doc().markups.iter().take(3).map(|m| m.id.clone()).collect();
    if let Some(first) = ids.first() {
        let _ = d.session.move_markups(std::slice::from_ref(first), 5.0, -5.0);
    }
    if let Some(last) = ids.get(2) {
        let _ = d.session.delete_markups(std::slice::from_ref(last), false);
    }
    list(&d.session);
    step("save");
    let full = work.join("full.pdf");
    if d.session.save_as(&full, true).is_ok()
        && let Ok(s) = Session::open(&full)
    {
        list(&s);
    }
    step("incremental save");
    let _ = d.session.move_markups(&ids, 1.0, 1.0);
    if d.session.save(false).is_ok()
        && let Ok(s) = Session::open(&full)
    {
        list(&s);
    }
    Ok(())
}

/// Seeds for the fuzzer: the sample plan, a measured set saved by MarkupCraft, and a plain one.
fn write_seeds(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let sample = markupcraft_render::synthetic::sample_pdf();
    std::fs::write(dir.join("sample.pdf"), &sample).map_err(|e| e.to_string())?;
    let mut s = Session::from_bytes(sample, dir.join("measured.pdf")).map_err(|e| e.to_string())?;
    let sq = |x: f64, y: f64| {
        vec![
            Point::new(x, y),
            Point::new(x + 80.0, y),
            Point::new(x + 80.0, y + 50.0),
            Point::new(x, y + 50.0),
        ]
    };
    let mut area = Markup::new(Kind::Area, 0, sq(100.0, 100.0));
    area.holes.push(
        sq(110.0, 110.0)
            .into_iter()
            .map(|p| Point::new(p.x * 0.5 + 60.0, p.y * 0.5 + 60.0))
            .collect(),
    );
    for m in [
        area,
        Markup::new(Kind::Length, 0, vec![Point::new(50.0, 50.0), Point::new(250.0, 90.0)]),
        Markup::new(Kind::Count, 0, vec![Point::new(300.0, 300.0), Point::new(320.0, 300.0)]),
        Markup::new(Kind::Polylength, 1, sq(200.0, 200.0)),
        Markup::new(Kind::Cloud, 1, sq(400.0, 100.0)),
        Markup::new(Kind::Text, 0, sq(500.0, 500.0)),
    ] {
        s.add_markup(m).map_err(|e| e.to_string())?;
    }
    s.save(true).map_err(|e| e.to_string())?;
    let plain = markupcraft_engine::synthetic::pdf(&[markupcraft_engine::synthetic::SyntheticPage::new(
        612.0,
        792.0,
        markupcraft_engine::synthetic::text(72.0, 700.0, 12.0, "Hello fuzz"),
    )]);
    std::fs::write(dir.join("plain.pdf"), plain).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match (args.first().map(String::as_str), args.get(1)) {
        (Some("--write-seeds"), Some(dir)) => write_seeds(Path::new(dir)),
        (Some(file), Some(work)) => {
            let work = Path::new(work);
            let _ = std::fs::create_dir_all(work);
            // Errors are answers: report and exit 0.
            if let Err(e) = run(Path::new(file), work) {
                eprintln!("handled: {e}");
            }
            Ok(())
        }
        _ => Err("usage: fuzz_one <file.pdf> <work dir> | --write-seeds <dir>".into()),
    };
    match r {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::from(2)
        }
    }
}
