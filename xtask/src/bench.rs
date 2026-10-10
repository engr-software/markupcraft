//! `cargo xtask bench`: time the app's hot paths (open, first page, every page at fit width,
//! thumbnails, the Markups List, search, full and incremental save, peak memory) on a large
//! synthetic set built in memory, and on the reference set when `MARKUPCRAFT_REF_PDF` is set.
//!
//! ```text
//! cargo xtask bench [--synthetic PAGES] [--markups N] [--no-ref] [--debug]
//! ```
//! It builds and runs the `bench` example of `markupcraft-ui-egui` (release by default) once
//! per input, then prints one table with a column per input. Results go to
//! `target/bench/last.tsv` too, so a before/after comparison is a diff.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, bail};

use crate::gates::{cargo, exec, target_dir};

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Build an example of `markupcraft-ui-egui` and return its path.
pub fn build_example(name: &str, debug: bool) -> anyhow::Result<PathBuf> {
    let mut build = cargo();
    build.args(["build", "-p", "markupcraft-ui-egui", "--example", name]);
    if !debug {
        build.arg("--release");
    }
    exec(
        build,
        &format!(
            "cargo build -p markupcraft-ui-egui --example {name}{}",
            if debug { "" } else { " --release" }
        ),
    )?;
    let exe = target_dir()
        .join(if debug { "debug" } else { "release" })
        .join("examples")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if !exe.is_file() {
        bail!("{} not found after building it", exe.display());
    }
    Ok(exe)
}

/// One measurement: name, milliseconds, note.
type Row = (String, f64, String);

/// The input's label and its rows, from one bench run.
fn run_one(exe: &PathBuf, args: &[String]) -> anyhow::Result<(String, Vec<Row>)> {
    let out = Command::new(exe)
        .args(args)
        .output()
        .with_context(|| format!("running {}", exe.display()))?;
    if !out.status.success() {
        bail!(
            "bench {} failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut label = String::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if let Some(l) = line.strip_prefix("bench: ") {
            label = l.to_string();
            continue;
        }
        let mut f = line.splitn(3, '\t');
        if let (Some(n), Some(v)) = (f.next(), f.next())
            && let Ok(ms) = v.parse::<f64>()
        {
            rows.push((n.to_string(), ms, f.next().unwrap_or("").to_string()));
        }
    }
    Ok((label, rows))
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let debug = args.iter().any(|a| a == "--debug");
    let exe = build_example("bench", debug)?;
    let mut synth = vec![
        "--synthetic".to_string(),
        flag(args, "--synthetic").unwrap_or("120").to_string(),
        "--markups".to_string(),
        flag(args, "--markups").unwrap_or("600").to_string(),
    ];
    if let Some(p) = flag(args, "--pdf") {
        synth = vec![p.to_string()];
    }
    let mut runs = vec![run_one(&exe, &synth)?];
    let reference = std::env::var_os("MARKUPCRAFT_REF_PDF")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    match reference {
        Some(p) if !args.iter().any(|a| a == "--no-ref") => {
            if !p.is_file() {
                bail!("MARKUPCRAFT_REF_PDF={} is not a file", p.display());
            }
            runs.push(run_one(&exe, &[p.to_string_lossy().into_owned()])?);
        }
        _ => println!("bench: the reference set is skipped (MARKUPCRAFT_REF_PDF is not set)"),
    }

    // One table: a row per measurement, a column per input.
    let mut names: Vec<String> = Vec::new();
    for (_, rows) in &runs {
        for (n, _, _) in rows {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
    }
    let mut tsv = String::from("measurement");
    print!("\n{:<30}", "measurement (ms)");
    for (label, _) in &runs {
        print!(" {label:>24}");
        tsv.push('\t');
        tsv.push_str(label);
    }
    println!();
    tsv.push('\n');
    for n in &names {
        print!("{n:<30}");
        tsv.push_str(n);
        for (_, rows) in &runs {
            let cell = rows.iter().find(|r| &r.0 == n).map(|r| r.1);
            match cell {
                Some(v) => print!(" {v:>24.1}"),
                None => print!(" {:>24}", "-"),
            }
            tsv.push('\t');
            tsv.push_str(&cell.map_or("-".into(), |v| format!("{v:.1}")));
        }
        println!();
        tsv.push('\n');
    }
    println!("\nnotes:");
    for (label, rows) in &runs {
        for (n, _, note) in rows {
            if !note.is_empty() {
                println!("  [{label}] {n}: {note}");
            }
        }
    }
    let dir = target_dir().join("bench");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(dir.join("last.tsv"), tsv).context("writing target/bench/last.tsv")?;
    Ok(())
}
