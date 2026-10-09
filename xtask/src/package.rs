//! `cargo xtask package [--skip-build] [script args...]`: build the packages for the host OS into
//! `dist/` (or `$DIST`) by running the per-OS script in `packaging/`:
//!
//! | host    | script                          | output                                         |
//! |---------|---------------------------------|------------------------------------------------|
//! | Windows | `packaging/windows/package.ps1` | portable `.zip` and NSIS installer `-setup.exe` |
//! | macOS   | `packaging/macos/package.sh`    | `MarkupCraft.app` in a `.dmg`                   |
//! | Linux   | `packaging/linux/package.sh`    | `.AppImage` and `.tar.gz`                       |
//!
//! The scripts are the source of truth (CI runs them directly); this is the same call with the
//! right interpreter.

use std::process::Command;

use anyhow::bail;

use crate::gates::{exec, root};

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let root = root();
    let dist = std::env::var_os("DIST").map_or_else(|| root.join("dist"), |d| root.join(d));
    let skip_build = args.iter().any(|a| a == "--skip-build");
    let rest: Vec<&String> = args.iter().filter(|a| *a != "--skip-build").collect();

    let mut cmd = match std::env::consts::OS {
        "windows" => {
            let mut c = Command::new(powershell());
            c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
            c.arg(root.join("packaging").join("windows").join("package.ps1"));
            if skip_build {
                c.arg("-SkipBuild");
            }
            c
        }
        os @ ("macos" | "linux") => {
            let mut c = Command::new("bash");
            c.arg(root.join("packaging").join(os).join("package.sh"));
            if skip_build {
                c.arg("--skip-build");
            }
            c
        }
        other => bail!("no packaging for {other}; supported: windows, macos, linux"),
    };
    cmd.args(rest).env("DIST", &dist).current_dir(&root);
    exec(
        cmd,
        &format!("package for {} into {}", std::env::consts::OS, dist.display()),
    )
}

/// PowerShell 7 (`pwsh`) when installed, else Windows PowerShell 5.1; the script runs on both.
fn powershell() -> &'static str {
    let has_pwsh = Command::new("pwsh")
        .args(["-NoProfile", "-Command", "exit 0"])
        .output()
        .is_ok_and(|o| o.status.success());
    if has_pwsh { "pwsh" } else { "powershell" }
}

#[cfg(test)]
mod tests {
    use crate::gates::root;

    fn read(rel: &str) -> String {
        std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// Each OS gets its own GitHub release whose notes come from packaging/release-notes/<os>.md.
    #[test]
    fn every_release_has_its_notes_and_script() {
        let release = read(".github/workflows/release.yml");
        for os in ["windows", "macos", "linux"] {
            let notes = read(&format!("packaging/release-notes/{os}.md"));
            assert!(notes.contains("SHA256SUMS.txt"), "{os} notes mention the checksums");
            assert!(
                release.contains(&format!("publish {os} ")),
                "release.yml publishes {os}"
            );
        }
        assert!(
            release.contains("packaging/release-notes/$os.md"),
            "release.yml uses the notes files"
        );
        for script in [
            "packaging/windows/package.ps1",
            "packaging/windows/installer.nsi",
            "packaging/macos/package.sh",
            "packaging/macos/Info.plist.in",
            "packaging/linux/package.sh",
            "packaging/linux/markupcraft.desktop",
        ] {
            assert!(!read(script).is_empty(), "{script}");
        }
    }

    /// Only plain version tags start a release; the per-OS tags it creates must not re-trigger it.
    #[test]
    fn release_runs_on_plain_version_tags_only() {
        let release = read(".github/workflows/release.yml");
        assert!(release.contains("- \"v[0-9]+.[0-9]+.[0-9]+\""), "tag filter");
        assert!(!release.contains("branches:"), "release.yml is tag-driven");
    }

    /// The bundle names the executable the script copies in, and claims PDFs.
    #[test]
    fn bundle_and_desktop_entry_point_at_the_app() {
        let plist = read("packaging/macos/Info.plist.in");
        assert!(
            plist.contains("<string>io.github.engr-software.markupcraft</string>"),
            "bundle id"
        );
        assert!(plist.contains("<key>CFBundleExecutable</key>\n  <string>MarkupCraft</string>"));
        assert!(plist.contains("<string>com.adobe.pdf</string>"), "claims PDFs by UTI");
        let desktop = read("packaging/linux/markupcraft.desktop");
        assert!(desktop.contains("Exec=markupcraft %F") && desktop.contains("Icon=markupcraft"));
    }
}
