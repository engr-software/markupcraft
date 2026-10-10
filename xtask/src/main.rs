//! Project automation for MarkupCraft, run via `cargo xtask <command>`.

use std::process::ExitCode;

mod bench;
mod fuzz;
mod gates;
mod models;
mod package;
mod parity;
mod scorecard;
mod web;

type Command = fn(&[String]) -> anyhow::Result<()>;

/// Every subcommand: name, one-line summary, entry point.
const COMMANDS: &[(&str, &str, Command)] = &[
    (
        "ci",
        "fmt --check, clippy -D warnings, test, wasm check of the web app, parity (stops at the first failure)",
        gates::ci,
    ),
    (
        "wasm",
        "cargo clippy -D warnings on the web app for wasm32-unknown-unknown",
        gates::wasm,
    ),
    (
        "web",
        "Build the browser app into dist-web/ (trunk when installed, else cargo + wasm-bindgen; --no-trunk)",
        web::run,
    ),
    (
        "scorecard",
        "check / resave / markupcheck on $MARKUPCRAFT_REF_PDF against thresholds (skips when unset)",
        scorecard::run,
    ),
    (
        "parity",
        "Validate parity/revu-features.toml and report progress",
        parity::run,
    ),
    (
        "package",
        "Build the packages for this OS into dist/ (--skip-build reuses the release binaries)",
        package::run,
    ),
    (
        "bench",
        "Time open, render, thumbnails, Markups List, search and saves (synthetic set + $MARKUPCRAFT_REF_PDF)",
        bench::run,
    ),
    (
        "fuzz",
        "Mutation-fuzz open/render/list/edit/save/reopen in child processes (--time SECS); findings in fuzz-out/",
        fuzz::run,
    ),
    ("version", "Print the workspace version", gates::version),
    (
        "models",
        "Fetch the OCR models into assets/models/ (SHA-256 checked, never committed)",
        models::run,
    ),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        print_help();
        return ExitCode::FAILURE;
    };
    if matches!(name.as_str(), "help" | "-h" | "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }
    let Some((_, _, command)) = COMMANDS.iter().find(|(n, _, _)| n == name) else {
        eprintln!("xtask: unknown command `{name}`\n");
        print_help();
        return ExitCode::FAILURE;
    };
    match command(args.get(1..).unwrap_or_default()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask {name}: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command> [args]\n\nCommands:");
    for (name, about, _) in COMMANDS {
        eprintln!("  {name:<10} {about}");
    }
    eprintln!("  {:<10} Show this list", "help");
}
