//! `cargo xtask scorecard`: run `markupcraft-cli check`, `resave` and `markupcheck` on the
//! Revu-marked reference set and fail when a score is below its threshold.
//!
//! The reference PDF is never committed: it comes from `MARKUPCRAFT_REF_PDF` (or `--pdf`). With
//! neither, the scorecard is skipped with a message and succeeds, so it can sit in any gate list.
//!
//! Thresholds are the minimum number of markups that must pass each step:
//!
//! | step        | flag            | environment                      | default |
//! |-------------|-----------------|----------------------------------|---------|
//! | check       | `--check N`     | `MARKUPCRAFT_SCORE_CHECK`        | 410     |
//! | resave      | `--resave N`    | `MARKUPCRAFT_SCORE_RESAVE`       | 410     |
//! | markupcheck | `--markupcheck N` | `MARKUPCRAFT_SCORE_MARKUPCHECK` | 187     |
//!
//! Flags win over the environment. `--debug` uses the dev profile instead of release.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

use crate::gates::{cargo, exec, target_dir};

const REF_ENV: &str = "MARKUPCRAFT_REF_PDF";

/// One scorecard step: the CLI subcommand, whether it writes an output PDF, the text in its
/// summary that precedes `passed/total`, and its threshold settings.
struct Step {
    name: &'static str,
    writes_pdf: bool,
    marker: &'static str,
    flag: &'static str,
    env: &'static str,
    default_min: usize,
}

const STEPS: &[Step] = &[
    Step {
        name: "check",
        writes_pdf: false,
        marker: "value agrees",
        flag: "--check",
        env: "MARKUPCRAFT_SCORE_CHECK",
        default_min: 410,
    },
    Step {
        name: "resave",
        writes_pdf: true,
        marker: "same quantity",
        flag: "--resave",
        env: "MARKUPCRAFT_SCORE_RESAVE",
        default_min: 410,
    },
    Step {
        name: "markupcheck",
        writes_pdf: true,
        marker: "identical",
        flag: "--markupcheck",
        env: "MARKUPCRAFT_SCORE_MARKUPCHECK",
        default_min: 187,
    },
];

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let pdf = flag_value(args, "--pdf")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(REF_ENV).filter(|v| !v.is_empty()).map(PathBuf::from));
    let Some(pdf) = pdf else {
        println!("scorecard: SKIPPED, {REF_ENV} is not set (point it at the Revu-marked reference set)");
        return Ok(());
    };
    if !pdf.is_file() {
        bail!("{REF_ENV}={} is not a file", pdf.display());
    }
    let mut mins = Vec::with_capacity(STEPS.len());
    for step in STEPS {
        mins.push(threshold(args, step)?);
    }

    let debug = args.iter().any(|a| a == "--debug");
    let mut build = cargo();
    build.args(["build", "-p", "markupcraft-cli"]);
    if !debug {
        build.arg("--release");
    }
    exec(
        build,
        &format!(
            "cargo build -p markupcraft-cli{}",
            if debug { "" } else { " --release" }
        ),
    )?;
    let cli = target_dir()
        .join(if debug { "debug" } else { "release" })
        .join(format!("markupcraft-cli{}", std::env::consts::EXE_SUFFIX));
    let out_dir = target_dir().join("scorecard");
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (step, min) in STEPS.iter().zip(mins) {
        let (passed, total) = run_step(&cli, &pdf, &out_dir, step)?;
        let ok = passed >= min;
        if !ok {
            failed.push(step.name);
        }
        rows.push(format!(
            "  {:<12} {passed:>5}/{total:<5} need {min:>5}  {}",
            step.name,
            if ok { "ok" } else { "BELOW THRESHOLD" }
        ));
    }
    println!(
        "\nscorecard ({}):",
        pdf.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    );
    for row in &rows {
        println!("{row}");
    }
    if failed.is_empty() {
        Ok(())
    } else {
        bail!("below threshold: {}", failed.join(", "))
    }
}

/// Runs one CLI step and returns its `(passed, total)`. The CLI exits non-zero whenever any
/// markup fails, so the exit status alone is not the verdict; the summary line is.
fn run_step(cli: &Path, pdf: &Path, out_dir: &Path, step: &Step) -> anyhow::Result<(usize, usize)> {
    let mut cmd = Command::new(cli);
    cmd.arg(step.name).arg(pdf);
    if step.writes_pdf {
        cmd.arg(out_dir.join(format!("{}.pdf", step.name)));
    }
    eprintln!("\n=== scorecard: {} ===", step.name);
    let out = cmd.output().with_context(|| format!("starting {}", cli.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    parse_score(&stdout, step.marker).with_context(|| {
        format!(
            "markupcraft-cli {} printed no `{} N/M` summary (exit status {})",
            step.name, step.marker, out.status
        )
    })
}

/// The last `<marker> N/M` in `text`.
fn parse_score(text: &str, marker: &str) -> Option<(usize, usize)> {
    text.rmatch_indices(marker).find_map(|(i, _)| {
        let rest = text.get(i + marker.len()..)?.trim_start();
        let (a, rest) = rest.split_once('/')?;
        let b: String = rest.chars().take_while(char::is_ascii_digit).collect();
        Some((a.trim().parse().ok()?, b.parse().ok()?))
    })
}

fn threshold(args: &[String], step: &Step) -> anyhow::Result<usize> {
    let (value, source) = match flag_value(args, step.flag) {
        Some(v) => (v.to_string(), step.flag),
        None => match std::env::var(step.env) {
            Ok(v) if !v.is_empty() => (v, step.env),
            _ => return Ok(step.default_min),
        },
    };
    value
        .trim()
        .parse()
        .with_context(|| format!("{source}: `{value}` is not a whole number"))
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_cli_summary() {
        let check = "  p1   Area  ours 1 SF  revu 2 SF  VALUE DIFFERS\n\nref.pdf\nmeasurements 412  no scale 2\nvalue agrees 409/410  label text identical 400/410\n";
        assert_eq!(parse_score(check, "value agrees"), Some((409, 410)));
        let resave = "resaved 410 measurements -> out.pdf\nreloaded and matched 410/410 (same quantity 410/410)\n";
        assert_eq!(parse_score(resave, "same quantity"), Some((410, 410)));
        let markup = "  Callout          3/4   identical\n  Text           10/10  identical\nresaved 187 non-measurement markups -> o.pdf\nreloaded 187/187, identical 150/187\n";
        assert_eq!(parse_score(markup, "identical"), Some((150, 187)));
        assert_eq!(parse_score("nothing here", "identical"), None);
    }

    #[test]
    fn thresholds_come_from_flags_then_defaults() {
        let args: Vec<String> = ["--markupcheck", "150"].iter().map(|s| s.to_string()).collect();
        let step = |name| STEPS.iter().find(|s| s.name == name).expect("step");
        assert_eq!(threshold(&args, step("markupcheck")).expect("ok"), 150);
        assert_eq!(step("check").default_min, 410);
        assert_eq!(step("resave").default_min, 410);
        assert_eq!(step("markupcheck").default_min, 187);
        let bad: Vec<String> = ["--check", "lots"].iter().map(|s| s.to_string()).collect();
        assert!(threshold(&bad, step("check")).is_err());
    }
}
