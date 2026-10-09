//! Project automation for MarkupCraft, run via `cargo xtask <command>`.

use std::process::ExitCode;

mod gates;
mod package;
mod parity;
mod scorecard;

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
        "cargo check the web app for wasm32-unknown-unknown",
        gates::wasm,
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
    ("version", "Print the workspace version", gates::version),
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
