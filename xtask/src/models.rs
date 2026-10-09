//! `cargo xtask models [DIR]`: fetch the OCR models into `assets/models/` (git-ignored).
//!
//! The two ocrs models (text detection and recognition, by Robert Knight, CC-BY-SA-4.0,
//! https://github.com/robertknight/ocrs-models) are downloaded with `curl`, checked against
//! their SHA-256 and never committed. The engine finds them there, or wherever
//! `MARKUPCRAFT_OCR_MODELS` points.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

const MODELS: &[(&str, &str, &str)] = &[
    (
        "text-detection.rten",
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
        "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
    ),
    (
        "text-recognition.rten",
        "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
        "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
    ),
];

fn sha256(path: &Path) -> Result<String> {
    let data = std::fs::read(path).with_context(|| path.display().to_string())?;
    Ok(Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect())
}

pub fn run(args: &[String]) -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("assets/models"));
    std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
    for (file, url, sum) in MODELS {
        let path = dir.join(file);
        if path.is_file() && sha256(&path)? == *sum {
            println!("models: {} (already there)", path.display());
            continue;
        }
        let tmp = dir.join(format!("{file}.part"));
        let status = Command::new("curl")
            .args(["--fail", "--location", "--silent", "--show-error", "--output"])
            .arg(&tmp)
            .arg(url)
            .status()
            .context("running curl")?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            bail!("{url}: download failed ({status})");
        }
        let got = sha256(&tmp)?;
        if got != *sum {
            let _ = std::fs::remove_file(&tmp);
            bail!("{url}: SHA-256 {got} is not the expected {sum}; refusing to use it");
        }
        std::fs::rename(&tmp, &path)?;
        println!("models: {}", path.display());
    }
    println!("OCR models: CC-BY-SA-4.0, https://github.com/robertknight/ocrs-models (not committed)");
    Ok(())
}
