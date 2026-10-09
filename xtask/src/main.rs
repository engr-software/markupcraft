//! `cargo xtask <task>`: repository chores.
//!
//!   parity [--check] [--write-md]   the Revu feature parity table (parity/revu-features.toml)

mod parity;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("parity") => parity::run(args.get(1..).unwrap_or_default()),
        _ => {
            eprintln!("usage: cargo xtask parity [--check] [--write-md]");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::from(1)
        }
    }
}
