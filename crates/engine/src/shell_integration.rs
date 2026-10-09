//! File-manager integration: "Combine in MarkupCraft" and "Convert to PDF with MarkupCraft"
//! on the files selected in Windows Explorer, macOS Finder or a Linux file manager.
//!
//! MarkupCraft never changes the system itself. [`write_integration`] writes the files a user
//! applies on their own (a `.reg` file for the current user plus a Send To command on Windows,
//! Quick Actions on macOS, a KDE service menu and Nautilus scripts on Linux), with a README
//! saying how to install and remove them. They run `markupcraft-cli shell combine|convert`,
//! which [`shell_combine`] and [`shell_convert`] implement.

use std::path::{Path, PathBuf};

use crate::{Result, invalid};

/// Which file manager's files to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    MacOs,
    Linux,
}

impl TargetOs {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "windows" => Self::Windows,
            "macos" | "mac" => Self::MacOs,
            "linux" => Self::Linux,
            _ => return None,
        })
    }
}

/// A free output name: `<dir>/<stem>.pdf`, else `<stem> (2).pdf` and so on.
fn free_name(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.pdf"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}).pdf")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Combine the selected files (PDFs, images, text) in the order given into
/// `<first file's name> combined.pdf` beside the first file. Returns the new file.
pub fn shell_combine(files: &[PathBuf]) -> Result<PathBuf> {
    let first = files.first().ok_or_else(|| invalid("select the files to combine"))?;
    let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = first
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Combined".into());
    let out = free_name(&dir, &format!("{stem} combined"));
    crate::docs_more::create_pdf_from_files(files, &out)?;
    Ok(out)
}

/// Convert each selected file that is not a PDF to `<name>.pdf` beside it. Returns the new
/// files (a file that cannot be converted is reported in the error after the others).
pub fn shell_convert(files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        return Err(invalid("select the files to convert"));
    }
    let mut out = Vec::new();
    let mut failed = Vec::new();
    for f in files {
        if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            continue;
        }
        let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = f
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Converted".into());
        let target = free_name(&dir, &stem);
        match crate::docs_more::create_pdf_from_files(std::slice::from_ref(f), &target) {
            Ok(_) => out.push(target),
            Err(e) => failed.push(format!("{}: {e}", f.display())),
        }
    }
    if !failed.is_empty() {
        return Err(invalid(failed.join("; ")));
    }
    Ok(out)
}

fn reg_escape(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
}

fn windows_files(cli: &Path) -> Vec<(String, String)> {
    let exe = reg_escape(cli);
    let verb = |key: &str, label: &str, args: &str| {
        format!(
            "[HKEY_CURRENT_USER\\Software\\Classes\\{key}]\r\n@=\"{label}\"\r\n\"Icon\"=\"{exe}\"\r\n\r\n[HKEY_CURRENT_USER\\Software\\Classes\\{key}\\command]\r\n@=\"\\\"{exe}\\\" shell {args} \\\"%1\\\"\"\r\n\r\n"
        )
    };
    let mut reg = String::from(
        "Windows Registry Editor Version 5.00\r\n\r\n; MarkupCraft: Explorer menu entries for the current user only.\r\n\r\n",
    );
    reg.push_str(&verb(
        "*\\shell\\MarkupCraftConvert",
        "Convert to PDF with MarkupCraft",
        "convert",
    ));
    reg.push_str(&verb(
        "SystemFileAssociations\\.pdf\\shell\\MarkupCraftCombine",
        "Combine in MarkupCraft",
        "combine",
    ));
    let unreg = "Windows Registry Editor Version 5.00\r\n\r\n; Removes MarkupCraft's Explorer menu entries.\r\n\r\n[-HKEY_CURRENT_USER\\Software\\Classes\\*\\shell\\MarkupCraftConvert]\r\n\r\n[-HKEY_CURRENT_USER\\Software\\Classes\\SystemFileAssociations\\.pdf\\shell\\MarkupCraftCombine]\r\n".to_string();
    let sendto = |args: &str| format!("@echo off\r\n\"{}\" shell {args} %*\r\n", cli.display());
    let readme = "MarkupCraft in Windows Explorer\r\n\r\n\
Install (nothing here changes your system until you do this):\r\n\
1. Double-click markupcraft-explorer.reg and confirm. Right-click a file: Convert to PDF with MarkupCraft;\r\n   right-click a PDF: Combine in MarkupCraft (one file at a time).\r\n\
2. To combine several selected files at once: press Win+R, type shell:sendto, and copy\r\n   \"Combine in MarkupCraft.cmd\" and \"Convert with MarkupCraft.cmd\" there. Then select files,\r\n   right-click, Send to.\r\n\r\n\
Remove: double-click markupcraft-explorer-remove.reg, and delete the two .cmd files from shell:sendto.\r\n\
The combined file is written beside the first file as \"<name> combined.pdf\".\r\n"
        .to_string();
    vec![
        ("markupcraft-explorer.reg".into(), reg),
        ("markupcraft-explorer-remove.reg".into(), unreg),
        ("Combine in MarkupCraft.cmd".into(), sendto("combine")),
        ("Convert with MarkupCraft.cmd".into(), sendto("convert")),
        ("README.txt".into(), readme),
    ]
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "'\\''"))
}

/// An Automator Quick Action (a Finder service) running `cli shell <verb>` on the files.
fn mac_workflow(cli: &Path, name: &str, verb: &str) -> Vec<(String, String)> {
    let cmd = xml_escape(&format!("{} shell {verb} \"$@\"", sh_quote(cli)));
    let info = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict><key>default</key><string>{name}</string></dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>
			<key>NSSendFileTypes</key>
			<array><string>public.item</string></array>
		</dict>
	</array>
</dict>
</plist>
"#
    );
    let wflow = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key><string>521</string>
	<key>AMApplicationVersion</key><string>2.10</string>
	<key>AMDocumentVersion</key><string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict><key>Container</key><string>List</string><key>Optional</key><true/><key>Types</key><array><string>com.apple.cocoa.string</string></array></dict>
				<key>AMActionVersion</key><string>2.0.3</string>
				<key>AMApplication</key><array><string>Automator</string></array>
				<key>AMParameterProperties</key>
				<dict><key>COMMAND_STRING</key><dict/><key>CheckedForUserDefaultShell</key><dict/><key>inputMethod</key><dict/><key>shell</key><dict/><key>source</key><dict/></dict>
				<key>AMProvides</key>
				<dict><key>Container</key><string>List</string><key>Types</key><array><string>com.apple.cocoa.string</string></array></dict>
				<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key><string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key><string>{cmd}</string>
					<key>CheckedForUserDefaultShell</key><true/>
					<key>inputMethod</key><integer>1</integer>
					<key>shell</key><string>/bin/sh</string>
					<key>source</key><string></string>
				</dict>
				<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key><string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key><false/>
				<key>CanShowWhenRun</key><true/>
				<key>Category</key><array><string>AMCategoryUtilities</string></array>
				<key>Class Name</key><string>RunShellScriptAction</string>
				<key>InputUUID</key><string>6B0E0E61-6A1B-4E43-9C3A-0D5F8A6E2B10</string>
				<key>OutputUUID</key><string>0F1B7A33-2C4D-4E7A-8D0E-5B6A1C2D3E40</string>
				<key>UUID</key><string>9C7D2E14-3B5A-4C6D-8E9F-1A2B3C4D5E60</string>
				<key>isViewVisible</key><integer>1</integer>
			</dict>
		</dict>
	</array>
	<key>connectors</key><dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key><dict/>
		<key>applicationPaths</key><array/>
		<key>inputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>presentationMode</key><integer>15</integer>
		<key>processesInput</key><false/>
		<key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key><false/>
		<key>systemImageName</key><string>NSActionTemplate</string>
		<key>useAutomaticInputType</key><false/>
		<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#
    );
    vec![
        (format!("{name}.workflow/Contents/Info.plist"), info),
        (format!("{name}.workflow/Contents/document.wflow"), wflow),
    ]
}

fn mac_files(cli: &Path) -> Vec<(String, String)> {
    let mut v = mac_workflow(cli, "Combine in MarkupCraft", "combine");
    v.extend(mac_workflow(cli, "Convert with MarkupCraft", "convert"));
    v.push((
        "README.txt".into(),
        "MarkupCraft in the Finder\n\nInstall (nothing here changes your system until you do this): double-click each .workflow\n\
and choose Install, or copy them to ~/Library/Services. Then select files in the Finder,\n\
Control-click, Quick Actions (or Services): Combine in MarkupCraft, Convert with MarkupCraft.\n\n\
Remove: delete the two workflows from ~/Library/Services.\n"
            .into(),
    ));
    v
}

fn linux_files(cli: &Path) -> Vec<(String, String)> {
    let exe = cli.display().to_string().replace('"', "\\\"");
    let kde = format!(
        "[Desktop Entry]\nType=Service\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nMimeType=application/octet-stream;application/pdf;image/png;image/jpeg;image/tiff;text/plain;\nActions=combine;convert;\nX-KDE-Submenu=MarkupCraft\n\n[Desktop Action combine]\nName=Combine in MarkupCraft\nIcon=application-pdf\nExec=\"{exe}\" shell combine %F\n\n[Desktop Action convert]\nName=Convert to PDF with MarkupCraft\nIcon=application-pdf\nExec=\"{exe}\" shell convert %F\n"
    );
    let script = |verb: &str| {
        format!(
            "#!/bin/sh\n# MarkupCraft: the selected files are the arguments.\nexec {} shell {verb} \"$@\"\n",
            sh_quote(cli)
        )
    };
    vec![
        ("markupcraft-servicemenu.desktop".into(), kde),
        ("Combine in MarkupCraft".into(), script("combine")),
        ("Convert with MarkupCraft".into(), script("convert")),
        (
            "README.txt".into(),
            "MarkupCraft in Linux file managers\n\nInstall (nothing here changes your system until you do this):\n\
- KDE Dolphin: copy markupcraft-servicemenu.desktop to ~/.local/share/kio/servicemenus/ and make it executable.\n\
- GNOME Files (Nautilus): copy the two scripts to ~/.local/share/nautilus/scripts/ and make them executable;\n\
  they appear under Scripts when you right-click selected files.\n\n\
Remove: delete those copies.\n"
                .into(),
        ),
    ]
}

/// Write the integration files for `os` into `dir`, running `cli` (the path of
/// `markupcraft-cli`). Returns the files written. Nothing outside `dir` is touched.
pub fn write_integration(dir: &Path, cli: &Path, os: TargetOs) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Err(invalid(format!("{} is not a folder", dir.display())));
    }
    let c = cli.display().to_string();
    if c.is_empty() || c.chars().any(|ch| ch.is_control() || ch == '%') {
        return Err(invalid("the program path holds characters a file manager cannot run"));
    }
    let files = match os {
        TargetOs::Windows => windows_files(cli),
        TargetOs::MacOs => mac_files(cli),
        TargetOs::Linux => linux_files(cli),
    };
    let mut out = Vec::new();
    for (name, body) in files {
        let p = dir.join(&name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| crate::EngineError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        crate::write_atomic(&p, body.as_bytes())?;
        #[cfg(unix)]
        if !name.ends_with(".txt") && !name.ends_with(".plist") && !name.ends_with(".wflow") && !name.ends_with(".reg")
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
        }
        out.push(p);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    #[test]
    fn integration_files_for_each_system_name_the_cli() {
        let d = std::env::temp_dir().join(format!("markupcraft-shellint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cli = PathBuf::from(if cfg!(windows) {
            "C:\\Apps\\MarkupCraft\\markupcraft-cli.exe"
        } else {
            "/opt/markupcraft/markupcraft-cli"
        });
        for (os, sub) in [
            (TargetOs::Windows, "win"),
            (TargetOs::MacOs, "mac"),
            (TargetOs::Linux, "linux"),
        ] {
            let dir = d.join(sub);
            std::fs::create_dir_all(&dir).unwrap();
            let files = write_integration(&dir, &cli, os).unwrap();
            assert!(files.iter().any(|f| f.ends_with("README.txt")));
            let all: String = files.iter().map(|f| std::fs::read_to_string(f).unwrap()).collect();
            assert!(all.contains("shell combine") && all.contains("shell convert"), "{sub}");
            assert!(all.contains("markupcraft-cli"), "{sub}");
        }
        let reg = std::fs::read_to_string(d.join("win/markupcraft-explorer.reg")).unwrap();
        assert!(reg.starts_with("Windows Registry Editor Version 5.00"));
        assert!(reg.contains("HKEY_CURRENT_USER\\Software\\Classes") && !reg.contains("HKEY_LOCAL_MACHINE"));
        assert!(
            d.join("mac/Combine in MarkupCraft.workflow/Contents/document.wflow")
                .is_file()
        );
        assert!(write_integration(&d.join("missing"), &cli, TargetOs::Linux).is_err());
        assert!(write_integration(&d, Path::new("bad%1"), TargetOs::Linux).is_err());
    }

    #[test]
    fn combine_and_convert_write_beside_the_files() {
        let d = std::env::temp_dir().join(format!("markupcraft-shellcmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("a.pdf"),
            pdf(&[SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "A"))]),
        )
        .unwrap();
        std::fs::write(d.join("notes.txt"), "Site notes\nline two\n").unwrap();
        let out = shell_combine(&[d.join("a.pdf"), d.join("notes.txt")]).unwrap();
        assert_eq!(out, d.join("a combined.pdf"));
        assert_eq!(crate::Session::open(&out).unwrap().page_count(), 2);
        // A second combine does not overwrite the first.
        assert_eq!(
            shell_combine(&[d.join("a.pdf"), d.join("a.pdf")]).unwrap(),
            d.join("a combined (2).pdf")
        );
        let made = shell_convert(&[d.join("notes.txt"), d.join("a.pdf")]).unwrap();
        assert_eq!(made, [d.join("notes.pdf")]);
        std::fs::write(d.join("x.bin"), b"\x00\x01").unwrap();
        assert!(shell_convert(&[d.join("x.bin")]).is_err());
    }
}
