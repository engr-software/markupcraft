//! Creating documents: the Stapler's one-PDF-per-source output (into each source's folder or a
//! chosen one), page templates for File > New PDF from Template (kept as PDFs in a folder),
//! and email templates that prefill File > Email's message.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, Session, invalid};

fn io(p: &Path) -> impl Fn(std::io::Error) -> crate::EngineError + '_ {
    move |e| crate::EngineError::Io {
        path: p.display().to_string(),
        source: e,
    }
}

/// A file name without path or wildcard characters.
fn clean_name(s: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > 120 || t.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
        return Err(invalid("a name of 1 to 120 characters without / \\ : * ? \" < > |"));
    }
    Ok(t.to_string())
}

/// Stapler, one PDF per source: each file converted on its own to `<stem>.pdf` in `out_dir`
/// (or beside the source when `None`). PDFs are skipped (they are PDFs already). Returns the
/// files written, in order; the first failure stops the run.
pub fn create_each(files: &[PathBuf], out_dir: Option<&Path>) -> Result<Vec<PathBuf>> {
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("give 1 to 2000 files"));
    }
    if let Some(d) = out_dir
        && !d.is_dir()
    {
        return Err(invalid(format!("{} is not a folder", d.display())));
    }
    let mut out = Vec::new();
    for f in files {
        if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            continue;
        }
        let stem = f
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "converted".into());
        let dir = match out_dir {
            Some(d) => d.to_path_buf(),
            None => f.parent().map(Path::to_path_buf).unwrap_or_default(),
        };
        let target = dir.join(format!("{stem}.pdf"));
        crate::docs_more::create_pdf_from_files(std::slice::from_ref(f), &target)?;
        out.push(target);
    }
    Ok(out)
}

// ---- page templates ----------------------------------------------------------------------------

/// The page templates in `dir` (each a PDF), by name, sorted.
pub fn list_templates(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p.clone())))
        .take(1_000)
        .collect();
    out.sort_by_key(|a| a.0.to_lowercase());
    out
}

/// Keep `pdf` as template `name` in `dir` (replacing one of that name).
pub fn save_template(dir: &Path, name: &str, pdf: &[u8]) -> Result<PathBuf> {
    let name = clean_name(name)?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    // it must open as a PDF
    Session::from_bytes(pdf.to_vec(), "template.pdf")?;
    let p = dir.join(format!("{name}.pdf"));
    crate::write_atomic(&p, pdf)?;
    Ok(p)
}

/// Remove template `name` from `dir`.
pub fn remove_template(dir: &Path, name: &str) -> Result<()> {
    let name = clean_name(name)?;
    let p = dir.join(format!("{name}.pdf"));
    if !p.is_file() {
        return Err(invalid(format!("no template {name}")));
    }
    std::fs::remove_file(&p).map_err(io(&p))
}

impl Session {
    /// A new, unsaved document that starts as a copy of the template at `template` (its pages,
    /// markups and form fields); `path` is where it would save.
    pub fn from_template(template: &Path, path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(template).map_err(io(template))?;
        let mut s = Self::from_bytes(bytes, path)?;
        s.saved_version = u64::MAX; // never saved
        Ok(s)
    }
}

// ---- email templates ---------------------------------------------------------------------------

/// A saved email: recipients, subject and body. `{file}` in the subject or body is replaced by
/// the attached file's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EmailTemplate {
    pub name: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
}

pub fn load_email_templates(path: &Path) -> Result<Vec<EmailTemplate>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let len = std::fs::metadata(path).map_err(io(path))?.len();
    if len > 1 << 20 {
        return Err(invalid("the email templates file is too large"));
    }
    let b = std::fs::read(path).map_err(io(path))?;
    serde_json::from_slice(&b).map_err(|e| invalid(format!("{}: {e}", path.display())))
}

pub fn save_email_templates(path: &Path, list: &[EmailTemplate]) -> Result<()> {
    for t in list {
        clean_name(&t.name)?;
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(io(d))?;
    }
    let b = serde_json::to_vec_pretty(list).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, &b)
}

/// An unsent message from template `t` with `name` attached: To and Cc headers, the subject and
/// body with `{file}` filled in.
pub fn email_from_template(t: &EmailTemplate, name: &str, pdf: &[u8]) -> Vec<u8> {
    let fill = |s: &str| s.replace("{file}", name.trim_end_matches(".pdf"));
    let subject = if t.subject.trim().is_empty() {
        name.trim_end_matches(".pdf").to_string()
    } else {
        fill(&t.subject)
    };
    let draft = crate::docfile::email_draft(&subject, &fill(&t.body), name, pdf);
    let clean = |s: &str| s.replace(['\r', '\n'], " ");
    let mut head = String::new();
    if !t.to.trim().is_empty() {
        head.push_str(&format!("To: {}\r\n", clean(&t.to)));
    }
    if !t.cc.trim().is_empty() {
        head.push_str(&format!("Cc: {}\r\n", clean(&t.cc)));
    }
    // after "X-Unsent: 1\r\n"
    let split = b"X-Unsent: 1\r\n".len().min(draft.len());
    let mut out = draft.get(..split).unwrap_or_default().to_vec();
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(draft.get(split..).unwrap_or_default());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_staple_each_and_email_templates() {
        let d = std::env::temp_dir().join(format!("mc-finish-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let tdir = d.join("templates");
        let blank = crate::blank::pdf_bytes(&[(792.0, 612.0)]).unwrap();
        save_template(&tdir, "Landscape Grid", &blank).unwrap();
        assert!(save_template(&tdir, "bad/name", &blank).is_err());
        assert!(save_template(&tdir, "Junk", b"not a pdf").is_err());
        let list = list_templates(&tdir);
        assert_eq!(list.len(), 1);
        let s = Session::from_template(&list[0].1, "Untitled.pdf").unwrap();
        assert!(s.is_dirty() && s.page_count() == 1);
        remove_template(&tdir, "Landscape Grid").unwrap();
        assert!(list_templates(&tdir).is_empty());

        let a = d.join("a.txt");
        std::fs::write(&a, "hello").unwrap();
        let b = d.join("b.docx");
        std::fs::write(&b, super::super::office::tests::docx(&[("", "Spec text")])).unwrap();
        let out = d.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let made = create_each(&[a.clone(), b.clone()], Some(&out)).unwrap();
        assert_eq!(made, vec![out.join("a.pdf"), out.join("b.pdf")]);
        let made = create_each(&[a], None).unwrap();
        assert_eq!(made, vec![d.join("a.pdf")]);

        let t = EmailTemplate {
            name: "Submittal".into(),
            to: "pm@example.com".into(),
            cc: "".into(),
            subject: "Submittal {file}".into(),
            body: "Attached: {file}".into(),
        };
        let p = d.join("email.json");
        save_email_templates(&p, std::slice::from_ref(&t)).unwrap();
        assert_eq!(load_email_templates(&p).unwrap(), vec![t.clone()]);
        let m = String::from_utf8(email_from_template(&t, "plans.pdf", b"%PDF")).unwrap();
        assert!(
            m.starts_with("X-Unsent: 1\r\nTo: pm@example.com\r\nSubject: Submittal plans"),
            "{m}"
        );
        assert!(m.contains("Attached: plans"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
