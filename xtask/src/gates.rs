//! Quality gates: the CI bundle, the wasm check, and shared helpers for the other commands.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

/// The workspace root (xtask lives one level below it).
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).to_path_buf()
}

/// Cargo's target directory: `CARGO_TARGET_DIR` when it is set (parallel agents use their own),
/// else `target/`. A relative value is taken relative to the workspace root, as cargo does when
/// it runs there.
pub fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), |d| root().join(d))
}

/// True when running under a CI service (GitHub Actions sets `CI=true`). Optional tools that
/// are skipped locally are required there.
pub fn in_ci() -> bool {
    std::env::var_os("CI").is_some_and(|v| !v.is_empty() && v != "false")
}

/// A `cargo` command in the workspace root, using the same cargo that runs xtask.
pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

/// Run a command, echoing `what`, and fail on a non-zero exit.
pub fn exec(mut cmd: Command, what: &str) -> anyhow::Result<()> {
    eprintln!("$ {what}");
    let status = cmd.status().with_context(|| format!("{what}: failed to start"))?;
    if !status.success() {
        bail!("{what}: exited with {status}");
    }
    Ok(())
}

pub fn run_args(args: &[&str]) -> anyhow::Result<()> {
    let mut c = cargo();
    c.args(args);
    exec(c, &format!("cargo {}", args.join(" ")))
}

/// `[workspace.package] version` from the root `Cargo.toml`, the one place the version lives.
pub fn workspace_version() -> anyhow::Result<String> {
    let path = root().join("Cargo.toml");
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse_workspace_version(&text).context("no [workspace.package] version in Cargo.toml")
}

fn parse_workspace_version(toml: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            in_pkg = line == "[workspace.package]";
            continue;
        }
        if in_pkg && let Some(rest) = line.strip_prefix("version") {
            let value = rest.trim_start().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

pub fn version(_: &[String]) -> anyhow::Result<()> {
    println!("{}", workspace_version()?);
    Ok(())
}

const WEB_APP: &str = "markupcraft-web";
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// `cargo check` the web app for wasm32. Skipped (with a message) when the web app does not exist
/// yet, or locally when the wasm32 target is not installed; under CI a missing target fails.
pub fn wasm(_: &[String]) -> anyhow::Result<()> {
    if !root().join("apps").join(WEB_APP).join("Cargo.toml").is_file() {
        eprintln!("wasm: SKIPPED, apps/{WEB_APP} does not exist yet");
        return Ok(());
    }
    if !wasm_target_installed() {
        if in_ci() {
            bail!("the {WASM_TARGET} target is required in CI: rustup target add {WASM_TARGET}");
        }
        eprintln!("wasm: SKIPPED, the {WASM_TARGET} target is not installed (rustup target add {WASM_TARGET})");
        return Ok(());
    }
    run_args(&["check", "--target", WASM_TARGET, "-p", WEB_APP])
}

fn wasm_target_installed() -> bool {
    // Without rustup (a distro toolchain) we cannot tell; assume it is there and let cargo say.
    match Command::new("rustup").args(["target", "list", "--installed"]).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.trim() == WASM_TARGET),
        _ => true,
    }
}

pub fn ci(_: &[String]) -> anyhow::Result<()> {
    type Step = (&'static str, fn() -> anyhow::Result<()>);
    let steps: [Step; 5] = [
        ("fmt", || run_args(&["fmt", "--all", "--", "--check"])),
        ("clippy", || {
            run_args(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        }),
        ("test", || run_args(&["test", "--workspace"])),
        ("wasm", || wasm(&[])),
        ("parity", || crate::parity::run(&[])),
    ];
    for (i, (name, step)) in steps.iter().enumerate() {
        eprintln!("\n=== ci: {name} ===");
        if let Err(e) = step() {
            println!("\nCI: {i} step(s) passed, FAILED at `{name}`: {e:#}");
            bail!("ci failed at `{name}`");
        }
    }
    println!("\nCI: all {} steps passed", steps.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_workspace_version() {
        let toml = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"1.2.3\"\nedition = \"2024\"\n\n[package]\nversion = \"9.9.9\"\n";
        assert_eq!(parse_workspace_version(toml).as_deref(), Some("1.2.3"));
        assert_eq!(parse_workspace_version("[package]\nversion = \"1.0.0\"\n"), None);
    }

    #[test]
    fn the_real_manifest_has_a_version() {
        let v = workspace_version().expect("version");
        assert_eq!(v.split('.').count(), 3, "{v}");
    }
}
