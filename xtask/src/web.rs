//! `cargo xtask web`: build the browser app (apps/markupcraft-web) into `dist-web/`.
//!
//! With trunk on PATH this is `trunk build --release` in apps/markupcraft-web (trunk fetches the
//! matching wasm-bindgen and wasm-opt itself). Without it, the same site is put together by hand:
//! `cargo build --release --target wasm32-unknown-unknown -p markupcraft-web`, then
//! `wasm-bindgen --target web` (the wasm-bindgen-cli version must equal the `wasm-bindgen` crate
//! in Cargo.lock), then `wasm-opt -Oz` when binaryen is installed, and `index.html` with its
//! trunk links replaced by a module script and plain links.
//!
//! Serve the result over HTTP (browsers do not run WebAssembly from file:// pages), e.g.
//! `python -m http.server -d dist-web 8767` and open http://127.0.0.1:8767/.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

use crate::gates::{exec, root, target_dir};

const APP: &str = "markupcraft-web";
const TARGET: &str = "wasm32-unknown-unknown";
const DIST: &str = "dist-web";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let force_bindgen = args.iter().any(|a| a == "--no-trunk");
    if !force_bindgen && found("trunk") {
        let mut c = Command::new("trunk");
        c.args(["build", "--release"])
            .current_dir(root().join("apps").join(APP));
        exec(c, "trunk build --release (apps/markupcraft-web)")?;
    } else {
        if !found("wasm-bindgen") {
            bail!(
                "neither trunk nor wasm-bindgen is installed: `cargo install trunk --locked`, or \
                 `cargo install wasm-bindgen-cli --version {} --locked` (the version in Cargo.lock)",
                lock_version("wasm-bindgen").unwrap_or_else(|| "<see Cargo.lock>".into())
            );
        }
        by_hand()?;
    }
    report()
}

/// The trunk-less build: cargo, wasm-bindgen, optional wasm-opt, then index.html.
fn by_hand() -> anyhow::Result<()> {
    let dist = root().join(DIST);
    if dist.exists() {
        std::fs::remove_dir_all(&dist).with_context(|| format!("clearing {}", dist.display()))?;
    }
    std::fs::create_dir_all(&dist)?;
    crate::gates::run_args(&["build", "--release", "--target", TARGET, "-p", APP])?;
    let wasm = target_dir().join(TARGET).join("release").join(format!("{APP}.wasm"));
    let mut c = Command::new("wasm-bindgen");
    c.arg("--target")
        .arg("web")
        .arg("--no-typescript")
        .arg("--out-dir")
        .arg(&dist)
        .arg("--out-name")
        .arg(APP)
        .arg(&wasm);
    exec(c, "wasm-bindgen --target web")?;
    let bg = dist.join(format!("{APP}_bg.wasm"));
    if found("wasm-opt") {
        let mut c = Command::new("wasm-opt");
        c.arg("-Oz")
            .arg("--enable-bulk-memory")
            .arg("--enable-nontrapping-float-to-int")
            .arg("--enable-sign-ext")
            .arg("--enable-mutable-globals")
            .arg("--enable-reference-types")
            .arg("--enable-multivalue")
            .arg("-o")
            .arg(&bg)
            .arg(&bg);
        exec(c, "wasm-opt -Oz")?;
    } else {
        eprintln!("web: wasm-opt not found, the module is not size-optimised (install binaryen)");
    }
    let page = root().join("apps").join(APP).join("index.html");
    let html = std::fs::read_to_string(&page).with_context(|| format!("reading {}", page.display()))?;
    let (html, copies) = untrunk(&html)?;
    for from in copies {
        let src = page.parent().unwrap_or(Path::new(".")).join(&from);
        let name = Path::new(&from).file_name().context("a copied file has no name")?;
        std::fs::copy(&src, dist.join(name)).with_context(|| format!("copying {}", src.display()))?;
    }
    std::fs::write(dist.join("index.html"), html)?;
    Ok(())
}

/// Replace trunk's `<link data-trunk ...>` lines: the rust link becomes a module script that
/// starts the app, an icon a plain icon link, a copied file nothing. Returns the page and the
/// files (relative to index.html) to copy next to it.
fn untrunk(html: &str) -> anyhow::Result<(String, Vec<String>)> {
    let mut out = String::with_capacity(html.len());
    let mut copies = Vec::new();
    for line in html.lines() {
        let t = line.trim_start();
        if !t.starts_with("<link data-trunk") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let indent = &line[..line.len() - t.len()];
        let href = attr(t, "href");
        match attr(t, "rel").as_deref() {
            Some("rust") => {
                out.push_str(&format!(
                    "{indent}<script type=\"module\">import init from \"./{APP}.js\"; init();</script>\n"
                ));
            }
            Some("icon") => {
                let href = href.context("an icon link without href")?;
                let name = Path::new(&href).file_name().map(|n| n.to_string_lossy().into_owned());
                let name = name.context("an icon link without a file name")?;
                out.push_str(&format!("{indent}<link rel=\"icon\" href=\"{name}\" />\n"));
                copies.push(href);
            }
            Some("copy-file") => copies.push(href.context("a copy-file link without href")?),
            other => bail!("index.html: unsupported trunk link rel={other:?}"),
        }
    }
    Ok((out, copies))
}

/// The value of `name="..."` in a tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = tag.get(start..)?;
    Some(rest.get(..rest.find('"')?)?.to_string())
}

/// The version of `krate` pinned in Cargo.lock.
fn lock_version(krate: &str) -> Option<String> {
    let lock = std::fs::read_to_string(root().join("Cargo.lock")).ok()?;
    let mut lines = lock.lines();
    while let Some(l) = lines.next() {
        if l.trim() == format!("name = \"{krate}\"") {
            let v = lines.next()?.trim().strip_prefix("version = ")?;
            return Some(v.trim_matches('"').to_string());
        }
    }
    None
}

fn found(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Print the files and the total size of the site.
fn report() -> anyhow::Result<()> {
    let dist = root().join(DIST);
    let mut total = 0u64;
    let mut entries: Vec<_> = std::fs::read_dir(&dist)
        .with_context(|| format!("reading {}", dist.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let len = e.metadata().map(|m| m.len()).unwrap_or(0);
        total += len;
        println!("{:>12}  {}", len, e.file_name().to_string_lossy());
    }
    println!("{total:>12}  total in {DIST}/");
    println!("serve it: python -m http.server -d {DIST} 8767, then open http://127.0.0.1:8767/");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_loses_its_trunk_links() {
        let page = std::fs::read_to_string(root().join("apps").join(APP).join("index.html")).unwrap();
        let (html, copies) = untrunk(&page).unwrap();
        assert!(!html.contains("data-trunk"));
        assert!(html.contains(&format!("import init from \"./{APP}.js\"")));
        assert!(html.contains("<link rel=\"icon\" href=\"logo-256.png\" />"));
        assert_eq!(copies, ["../../assets/logo-256.png", "../../assets/logo.svg"]);
        for c in copies {
            assert!(root().join("apps").join(APP).join(c).is_file());
        }
    }

    #[test]
    fn attributes_are_read() {
        assert_eq!(
            attr("<link rel=\"icon\" href=\"a/b.png\" />", "href").as_deref(),
            Some("a/b.png")
        );
        assert_eq!(attr("<link rel=\"icon\" />", "href"), None);
    }

    #[test]
    fn the_lock_names_a_wasm_bindgen_version() {
        assert!(lock_version("wasm-bindgen").is_some_and(|v| v.starts_with("0.2.")));
    }
}
