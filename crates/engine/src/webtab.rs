//! The Web Tab without an embedded browser: web pages as documents.
//!
//! - A *link page*: a PDF listing web addresses (favourites, the page asked for) as clickable
//!   links, which MarkupCraft opens in the system browser.
//! - A *capture*: the web page printed to PDF by a Chromium-family browser run headless
//!   (`--headless --print-to-pdf`), when one is installed, so the page itself can be opened,
//!   marked up and measured. The browser runs with a throwaway profile and a time limit.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use markupcraft_model::Rect;

use crate::links::{LinkLook, LinkTarget};
use crate::synthetic::{SyntheticPage, pdf, text};
use crate::{EngineError, Result, Session, invalid};

/// Longest web address taken.
pub const MAX_URL: usize = 2_048;
/// Most links on a link page document.
pub const MAX_LINKS: usize = 500;

/// Check a web address: `http://` or `https://`, no spaces or control characters.
pub fn check_url(url: &str) -> Result<String> {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(invalid("a web address starts with http:// or https://"));
    }
    if u.len() > MAX_URL || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid("the web address is too long or holds spaces"));
    }
    if u.split("://")
        .nth(1)
        .is_none_or(|rest| rest.is_empty() || rest.starts_with('/'))
    {
        return Err(invalid("the web address has no host"));
    }
    Ok(u.to_string())
}

/// Turn what the user typed into a web address (`example.com` becomes `https://example.com`).
pub fn normalize_url(typed: &str) -> Result<String> {
    let t = typed.trim();
    if t.contains("://") {
        check_url(t)
    } else {
        // `scheme:rest` without `//` (javascript:, mailto:, data:) is not a web address; a
        // `host:port` is.
        let host = t.split(['/', '?', '#']).next().unwrap_or_default();
        if let Some((_, after)) = host.split_once(':')
            && !(after.chars().all(|c| c.is_ascii_digit()) && !after.is_empty())
        {
            return Err(invalid("a web address starts with http:// or https://"));
        }
        check_url(&format!("https://{t}"))
    }
}

/// The host part of an address (for a tab title).
pub fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn ascii(s: &str, n: usize) -> String {
    s.chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .take(n)
        .collect()
}

/// A link page document: `title`, then each `(name, url)` as a line that links to it.
pub fn link_page_pdf(title: &str, links: &[(String, String)], path: &Path) -> Result<Session> {
    let mut checked = Vec::new();
    for (n, u) in links.iter().take(MAX_LINKS) {
        checked.push((n.clone(), check_url(u)?));
    }
    let per = 30;
    let mut pages = Vec::new();
    for (i, chunk) in checked.chunks(per).enumerate() {
        let mut c = text(54.0, 740.0, 18.0, &ascii(title, 70));
        if i == 0 {
            c.push_str(&text(
                54.0,
                716.0,
                9.5,
                "Web Tab: click a link to open it in your browser; Capture Page saves a page as PDF.",
            ));
        }
        for (k, (n, u)) in chunk.iter().enumerate() {
            let y = 680.0 - 21.0 * k as f64;
            let label = if n.trim().is_empty() {
                u.clone()
            } else {
                format!("{n}  -  {u}")
            };
            c.push_str(&format!("0 0 0.8 rg {}0 g\n", text(54.0, y, 11.0, &ascii(&label, 95))));
        }
        pages.push(SyntheticPage::new(612.0, 792.0, c));
    }
    if pages.is_empty() {
        let mut c = text(54.0, 740.0, 18.0, &ascii(title, 70));
        c.push_str(&text(54.0, 716.0, 9.5, "No web pages yet: type an address to open."));
        pages.push(SyntheticPage::new(612.0, 792.0, c));
    }
    let mut s = Session::from_bytes(pdf(&pages), path)?;
    for (i, (_, u)) in checked.iter().enumerate() {
        let (page, k) = (i / per, i % per);
        let y = 680.0 - 21.0 * k as f64;
        s.add_link(
            page,
            Rect::new(50.0, y - 4.0, 560.0, y + 13.0),
            &LinkTarget::Url(u.clone()),
            LinkLook::default(),
        )?;
    }
    Ok(s)
}

/// Program names of Chromium-family browsers that print to PDF headless.
fn browser_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["msedge.exe", "chrome.exe", "chromium.exe", "brave.exe"]
    } else {
        &[
            "chromium",
            "chromium-browser",
            "google-chrome",
            "google-chrome-stable",
            "microsoft-edge",
            "microsoft-edge-stable",
            "brave-browser",
        ]
    }
}

/// Where these browsers are usually installed when not on `PATH`.
fn well_known() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        for base in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Some(b) = std::env::var_os(base) {
                let b = PathBuf::from(b);
                v.push(b.join("Microsoft/Edge/Application/msedge.exe"));
                v.push(b.join("Google/Chrome/Application/chrome.exe"));
            }
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        ));
        v.push(PathBuf::from(
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ));
        v.push(PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"));
    }
    v
}

/// A headless-capable browser: `explicit` when it exists, else one on `PATH`, else one in its
/// usual install folder.
pub fn find_browser(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(e) = explicit.filter(|p| !p.as_os_str().is_empty()) {
        return e.is_file().then(|| e.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for n in browser_names() {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    well_known().into_iter().find(|p| p.is_file())
}

/// The arguments a headless print to PDF takes.
pub fn capture_args(url: &str, out: &Path, profile: &Path) -> Vec<String> {
    vec![
        "--headless".into(),
        "--disable-gpu".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        format!("--user-data-dir={}", profile.display()),
        "--no-pdf-header-footer".into(),
        format!("--print-to-pdf={}", out.display()),
        url.to_string(),
    ]
}

/// Print the web page at `url` to the PDF `out` with `browser` (headless), waiting at most
/// `timeout`. Returns the page count.
pub fn capture_web_page(url: &str, out: &Path, browser: &Path, timeout: Duration) -> Result<usize> {
    let url = check_url(url)?;
    let stamp = format!("{}-{}", std::process::id(), crate::stamps::now_secs());
    let profile = std::env::temp_dir().join(format!("markupcraft-webtab-profile-{stamp}"));
    let tmp = out.with_extension(format!("capture-{stamp}.pdf"));
    let io = |e: std::io::Error| EngineError::Io {
        path: browser.display().to_string(),
        source: e,
    };
    let mut child = Command::new(browser)
        .args(capture_args(&url, &tmp, &profile))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(io)?;
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(io)? {
            break Some(s);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = std::fs::remove_dir_all(&profile);
    let finish = || -> Result<usize> {
        if status.is_none() {
            return Err(invalid(format!(
                "the browser did not finish within {} s",
                timeout.as_secs()
            )));
        }
        let bytes = std::fs::read(&tmp).map_err(|_| invalid("the browser wrote no PDF (is the address reachable?)"))?;
        let s = Session::from_bytes(bytes.clone(), out)?;
        let n = s.page_count();
        crate::write_atomic(out, &bytes)?;
        Ok(n)
    };
    let r = finish();
    let _ = std::fs::remove_file(&tmp);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_checked_and_normalized() {
        assert_eq!(normalize_url("example.com/a").unwrap(), "https://example.com/a");
        assert_eq!(normalize_url(" http://x.org ").unwrap(), "http://x.org");
        assert!(normalize_url("file:///c:/secret").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(check_url("https://").is_err());
        assert!(check_url("https://a b").is_err());
        assert_eq!(host_of("https://docs.example.com/x?y"), "docs.example.com");
    }

    #[test]
    fn a_link_page_links_every_address() {
        let p = std::env::temp_dir().join("webtab-links.pdf");
        let links: Vec<(String, String)> = (0..35)
            .map(|i| (format!("Site {i}"), format!("https://example.com/{i}")))
            .collect();
        let s = link_page_pdf("Favorites", &links, &p).unwrap();
        assert_eq!(s.page_count(), 2);
        let l = s.links();
        assert_eq!(l.len(), 35);
        assert!(
            l.iter()
                .any(|x| x.target == LinkTarget::Url("https://example.com/34".into()) && x.page == 1)
        );
        assert!(s.page_text(0).unwrap().contains("Site 0"));
        assert!(link_page_pdf("x", &[("a".into(), "ftp://x".into())], &p).is_err());
    }

    #[test]
    fn capture_runs_a_browser_and_checks_its_output() {
        let d = std::env::temp_dir().join(format!("markupcraft-webtab-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let args = capture_args("https://example.com", &d.join("o.pdf"), &d.join("prof"));
        assert!(args.iter().any(|a| a == "--headless"));
        assert!(args.iter().any(|a| a.starts_with("--print-to-pdf=")));
        assert_eq!(args.last().map(String::as_str), Some("https://example.com"));
        assert!(find_browser(Some(&d.join("no-such-browser"))).is_none());
        // A stand-in browser that writes no PDF: the capture reports it.
        #[cfg(windows)]
        let fake = {
            let f = d.join("fake.cmd");
            std::fs::write(&f, "@exit /b 0\r\n").unwrap();
            f
        };
        #[cfg(not(windows))]
        let fake = {
            use std::os::unix::fs::PermissionsExt;
            let f = d.join("fake.sh");
            std::fs::write(&f, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            f
        };
        let e = capture_web_page("https://example.com", &d.join("o.pdf"), &fake, Duration::from_secs(20)).unwrap_err();
        assert!(e.to_string().contains("no PDF"), "{e}");
        assert!(capture_web_page("file:///x", &d.join("o.pdf"), &fake, Duration::from_secs(1)).is_err());
        // A stand-in that prints: copies a PDF to the --print-to-pdf path.
        let src = d.join("page.pdf");
        std::fs::write(
            &src,
            crate::synthetic::pdf(&[
                SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "Example Domain")),
                SyntheticPage::new(612.0, 792.0, ""),
            ]),
        )
        .unwrap();
        #[cfg(windows)]
        let printer = {
            let f = d.join("printer.cmd");
            let script = format!(
                "@echo off\r\n:loop\r\nif \"%~1\"==\"\" goto done\r\nset \"a=%~1\"\r\nif \"%a:~0,15%\"==\"--print-to-pdf=\" copy /y \"{src}\" \"%a:~15%\" >nul\r\nif \"%~1\"==\"--print-to-pdf\" copy /y \"{src}\" \"%~2\" >nul\r\nshift\r\ngoto loop\r\n:done\r\n",
                src = src.display()
            );
            std::fs::write(&f, script).unwrap();
            f
        };
        #[cfg(not(windows))]
        let printer = {
            use std::os::unix::fs::PermissionsExt;
            let f = d.join("printer.sh");
            let script = format!(
                "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --print-to-pdf=*) cp '{}' \"${{a#--print-to-pdf=}}\";; esac; done\n",
                src.display()
            );
            std::fs::write(&f, script).unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            f
        };
        let n = capture_web_page(
            "https://example.com",
            &d.join("o.pdf"),
            &printer,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(n, 2);
        assert!(
            Session::open(d.join("o.pdf"))
                .unwrap()
                .page_text(0)
                .unwrap()
                .contains("Example Domain")
        );
    }
}
