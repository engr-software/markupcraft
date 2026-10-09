//! PDF Packages (File > Create > PDF Package): a portfolio, ISO 32000-1 §12.3.5. The package is
//! an ordinary PDF with a cover page; its member files are embedded files (`/Names
//! /EmbeddedFiles`) and the catalog's `/Collection` dictionary tells a viewer to present them as
//! a package, listed by name, with the cover page as the initial document.

use std::path::{Path, PathBuf};

use markupcraft_revu::cos::{Dict, Object, PdfString};

use crate::synthetic::{SyntheticPage, pdf, text};
use crate::{Result, Session, invalid};

/// Most files in one package.
pub const MAX_PACKAGE_FILES: usize = 1_000;

/// The cover page: a title, the instruction, and the member files (up to what fits).
fn cover(names: &[String]) -> Vec<u8> {
    let mut c = text(72.0, 720.0, 24.0, "PDF Package");
    c.push_str(&text(
        72.0,
        692.0,
        11.0,
        "This document is a PDF Package. Its files are listed in the Attachments.",
    ));
    for (i, n) in names.iter().take(50).enumerate() {
        let shown: String = n
            .chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .take(90)
            .collect();
        c.push_str(&text(90.0, 660.0 - 12.5 * i as f64, 10.0, &shown));
    }
    if names.len() > 50 {
        c.push_str(&text(
            90.0,
            660.0 - 12.5 * 50.0,
            10.0,
            &format!("and {} more", names.len() - 50),
        ));
    }
    pdf(&[SyntheticPage::new(612.0, 792.0, c)])
}

/// The `/Collection` dictionary: details view, sorted by file name.
fn collection_dict() -> Dict {
    let mut field = Dict::new();
    field.set(b"Type".to_vec(), Object::name("CollectionField"));
    field.set(b"Subtype".to_vec(), Object::name("F"));
    field.set(b"N".to_vec(), Object::String(PdfString::text("Name")));
    field.set(b"O".to_vec(), Object::Int(0));
    let mut schema = Dict::new();
    schema.set(b"Type".to_vec(), Object::name("CollectionSchema"));
    schema.set(b"FileName".to_vec(), Object::Dict(field));
    let mut sort = Dict::new();
    sort.set(b"Type".to_vec(), Object::name("CollectionSort"));
    sort.set(b"S".to_vec(), Object::name("FileName"));
    sort.set(b"A".to_vec(), Object::Bool(true));
    let mut c = Dict::new();
    c.set(b"Type".to_vec(), Object::name("Collection"));
    c.set(b"View".to_vec(), Object::name("D"));
    c.set(b"Schema".to_vec(), Object::Dict(schema));
    c.set(b"Sort".to_vec(), Object::Dict(sort));
    c
}

/// Create a PDF Package at `out` holding `files` (none: an empty package to add files to).
/// Returns the number of member files.
pub fn create_pdf_package(files: &[PathBuf], out: &Path) -> Result<usize> {
    if files.len() > MAX_PACKAGE_FILES {
        return Err(invalid(format!("a package holds at most {MAX_PACKAGE_FILES} files")));
    }
    let names: Vec<String> = files
        .iter()
        .map(|f| {
            f.file_name()
                .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().into_owned())
        })
        .collect();
    let mut s = Session::from_bytes(cover(&names), out)?;
    for f in files {
        s.add_attachment(f, None, "")?;
    }
    s.make_package()?;
    s.save_as(out, true)?;
    Ok(files.len())
}

impl Session {
    /// Whether the document is a PDF Package (its catalog has `/Collection`).
    pub fn is_package(&self) -> bool {
        let cos = &self.file.cos;
        cos.root()
            .and_then(|r| cos.dict(&Object::Ref(r)))
            .is_some_and(|d| d.get(b"Collection").is_some())
    }

    /// Turn this document into a PDF Package (its attachments become the package's files).
    /// Undoable.
    pub fn make_package(&mut self) -> Result<()> {
        if self.is_package() {
            return Ok(());
        }
        self.cos_edit("Make PDF Package", |cos| {
            let root = cos.root().ok_or_else(|| invalid("the document has no catalog"))?;
            cos.update_dict(root, |d| {
                d.set(b"Collection".to_vec(), Object::Dict(collection_dict()));
                d.set(b"PageMode".to_vec(), Object::name("UseAttachments"));
            })?;
            Ok(((), true))
        })
    }

    /// The package's member files (the attachments' names), in order.
    pub fn package_files(&self) -> Vec<String> {
        self.attachments().into_iter().map(|a| a.name).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_embeds_its_files_and_says_it_is_one() {
        let d = std::env::temp_dir().join(format!("markupcraft-package-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.pdf"), pdf(&[SyntheticPage::new(200.0, 200.0, "")])).unwrap();
        std::fs::write(d.join("notes.txt"), b"hello").unwrap();
        let out = d.join("package.pdf");
        assert_eq!(
            create_pdf_package(&[d.join("a.pdf"), d.join("notes.txt")], &out).unwrap(),
            2
        );
        let s = Session::open(&out).unwrap();
        assert!(s.is_package());
        assert_eq!(s.package_files(), vec!["a.pdf".to_string(), "notes.txt".to_string()]);
        assert!(s.page_text(0).unwrap().contains("PDF Package"));
        // An empty package.
        let empty = d.join("empty.pdf");
        assert_eq!(create_pdf_package(&[], &empty).unwrap(), 0);
        assert!(Session::open(&empty).unwrap().is_package());
    }
}
