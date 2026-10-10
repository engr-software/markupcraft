//! Archive as PDF/A-1b (ISO 19005-1, level B), the level Revu exports, beside the 2b and 3b
//! levels PdfCraft's preflight handles.
//!
//! The rules PDF/A-1 adds to the ones PDF/A-2 shares with it, and what Archive does about them:
//!
//! - the file is PDF 1.4: the header says 1.4 and there are no object streams or
//!   cross-reference streams. A document that declares PDF/A-1 is always written with a
//!   classic cross-reference table and a 1.4 header (`markupcraft_revu::save`), and the
//!   catalog's `/Version` goes;
//! - no transparency: transparency groups (`/Group /S /Transparency`) are removed from pages
//!   and forms; soft masks, constant alpha below 1 and blend modes other than Normal can not be
//!   removed without changing how the page looks, so they are reported (flatten or print the
//!   page as an image first);
//! - no optional content (`/OCProperties` is removed: every layer then shows), no embedded
//!   files and no file attachment annotations (reported: remove the attachments or archive as
//!   PDF/A-3b), no JPEG 2000 images (reported);
//! - XMP identification `pdfaid:part 1`, conformance B, and the sRGB output intent.

use markupcraft_revu::cos::{Document as CosDoc, Object, Stream};

use crate::archive::{PdfaIssue, PdfaLevel, PdfaReport};
use crate::docutil::err;
use crate::{Result, Session};

fn issue(clause: &'static str, message: impl Into<String>, page: Option<usize>, fixable: bool) -> PdfaIssue {
    PdfaIssue {
        clause,
        message: message.into(),
        page,
        fixable,
    }
}

fn catalog(cos: &CosDoc) -> markupcraft_revu::cos::Dict {
    cos.root().and_then(|r| cos.dict(&Object::Ref(r))).unwrap_or_default()
}

/// The XMP packet of the catalog, as text.
fn metadata(cos: &CosDoc) -> Option<String> {
    let m = catalog(cos).get(b"Metadata").cloned()?;
    match &*cos.resolve(&m) {
        Object::Stream(s) => s
            .decoded_within(4 << 20)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned()),
        _ => None,
    }
}

/// Whether the document declares PDF/A part 1.
pub fn declares_part1(cos: &CosDoc) -> bool {
    markupcraft_revu::pdfa1::declares_part1(cos)
}

fn num(cos: &CosDoc, o: Option<&Object>) -> Option<f64> {
    o.and_then(|v| cos.resolve(v).as_f64())
}

/// Every PDF/A-1 rule `cos` breaks beyond the ones PDF/A-2 shares. `file` is the bytes the
/// document was read from (its header and cross-reference form), when they describe it.
fn part1_issues(cos: &CosDoc, file: Option<&[u8]>) -> Vec<PdfaIssue> {
    let mut out = Vec::new();
    let cat = catalog(cos);
    if let Some(b) = file {
        let head = b.get(..8).unwrap_or_default();
        let ok_head = head.starts_with(b"%PDF-1.") && head.get(7).is_some_and(|v| (b'0'..=b'4').contains(v));
        if !ok_head {
            out.push(issue("6.1.2", "The file header is not PDF 1.4 or lower", None, true));
        }
        let has = |needle: &[u8]| b.windows(needle.len()).any(|w| w == needle);
        if has(b"/ObjStm") {
            out.push(issue("6.1.4", "The file uses object streams (PDF 1.5)", None, true));
        }
        if has(b"/XRef") && !has(b"\nxref") && !has(b"\rxref") {
            out.push(issue(
                "6.1.4",
                "The file uses a cross-reference stream (PDF 1.5)",
                None,
                true,
            ));
        }
    }
    if cat
        .name(b"Version")
        .is_some_and(|v| v.get(..3).is_some_and(|x| x > &b"1.4"[..]))
    {
        out.push(issue(
            "6.1.2",
            "The catalog asks for a PDF version above 1.4",
            None,
            true,
        ));
    }
    if cat.get(b"OCProperties").is_some() {
        out.push(issue(
            "6.1.13",
            "Optional content (layers) is not allowed in PDF/A-1",
            None,
            true,
        ));
    }
    if let Some(names) = cat.get(b"Names").and_then(|n| cos.dict(n))
        && names.get(b"EmbeddedFiles").is_some()
    {
        out.push(issue(
            "6.1.11",
            "Embedded files are not allowed in PDF/A-1 (remove the attachments, or archive as PDF/A-3b)",
            None,
            false,
        ));
    }
    let pages = crate::docutil::page_objs(cos).unwrap_or_default();
    for (i, p) in pages.iter().enumerate() {
        let Some(d) = cos.dict(&Object::Ref(*p)) else { continue };
        if d.get(b"Group").is_some() {
            out.push(issue(
                "6.4",
                format!("Page {} has a transparency group", i + 1),
                Some(i),
                true,
            ));
        }
        let annots = d
            .get(b"Annots")
            .map(|a| cos.resolve(a))
            .and_then(|a| a.as_array().cloned())
            .unwrap_or_default();
        for a in annots.iter().take(100_000) {
            let Some(ad) = cos.dict(a) else { continue };
            let sub = ad.name(b"Subtype").map(|s| String::from_utf8_lossy(s).into_owned());
            if sub.as_deref() == Some("FileAttachment") {
                out.push(issue(
                    "6.5.3",
                    "File attachment annotations are not allowed in PDF/A-1",
                    Some(i),
                    false,
                ));
            }
            if num(cos, ad.get(b"CA")).is_some_and(|v| (v - 1.0).abs() > 1e-9) {
                out.push(issue(
                    "6.5.3",
                    format!(
                        "A {} markup is transparent (opacity below 100%)",
                        sub.unwrap_or_default()
                    ),
                    Some(i),
                    false,
                ));
            }
        }
    }
    for n in cos.object_numbers().into_iter().take(2_000_000) {
        let r = markupcraft_revu::cos::ObjRef::new(n, cos.generation(n));
        let o = cos.get(r);
        let d = match &*o {
            Object::Dict(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        // Graphics states: soft masks, constant alpha, blend modes.
        if d.name(b"Type") == Some(b"ExtGState")
            || d.get(b"ca").is_some()
            || d.get(b"CA").is_some() && d.get(b"Subtype").is_none()
        {
            let smask = d.get(b"SMask").map(|m| cos.resolve(m));
            if smask.is_some_and(|m| m.as_name() != Some(&b"None"[..])) {
                out.push(issue(
                    "6.4",
                    "A graphics state uses a soft mask (transparency)",
                    None,
                    false,
                ));
            }
            for k in [&b"ca"[..], b"CA"] {
                if num(cos, d.get(k)).is_some_and(|v| (v - 1.0).abs() > 1e-9) {
                    out.push(issue(
                        "6.4",
                        "A graphics state draws with transparency (alpha below 1)",
                        None,
                        false,
                    ));
                }
            }
            if let Some(bm) = d.get(b"BM").map(|b| cos.resolve(b)) {
                let ok = match &*bm {
                    Object::Name(n) => n == b"Normal" || n == b"Compatible",
                    Object::Array(a) => a
                        .iter()
                        .all(|x| matches!(x.as_name(), Some(b"Normal") | Some(b"Compatible"))),
                    _ => true,
                };
                if !ok {
                    out.push(issue(
                        "6.4",
                        "A graphics state uses a blend mode other than Normal",
                        None,
                        false,
                    ));
                }
            }
        }
        if let Object::Stream(s) = &*o {
            let sub = s.dict.name(b"Subtype");
            if sub == Some(b"Image") && s.dict.get(b"SMask").is_some() {
                out.push(issue("6.4", "An image has a soft mask (transparency)", None, false));
            }
            if sub == Some(b"Form") && s.dict.get(b"Group").is_some() {
                out.push(issue("6.4", "A form has a transparency group", None, true));
            }
            let jpx = match s.dict.get(b"Filter").map(|f| cos.resolve(f)).as_deref() {
                Some(Object::Name(n)) => n == b"JPXDecode",
                Some(Object::Array(a)) => a.iter().any(|x| x.as_name() == Some(b"JPXDecode")),
                _ => false,
            };
            if jpx {
                out.push(issue(
                    "6.1.10",
                    "JPEG 2000 images are not allowed in PDF/A-1",
                    None,
                    false,
                ));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|i| seen.insert((i.clause, i.message.clone(), i.page)));
    out
}

/// The PDF/A-2b issues that PDF/A-1b shares (identification is checked for part 1 instead).
fn shared_issues(cos: &CosDoc) -> Vec<PdfaIssue> {
    pdfcraft_preflight::verify(cos, PdfaLevel::A2b)
        .into_iter()
        .filter(|i| !(i.clause == "6.6.4" && i.message.contains("PDF/A-2b")))
        .collect()
}

fn identification(cos: &CosDoc) -> Option<PdfaIssue> {
    match pdfcraft_preflight::declared(cos).pdfa {
        Some((1, c)) if c.eq_ignore_ascii_case("B") || c.eq_ignore_ascii_case("A") => None,
        _ => metadata(cos).map(|_| {
            issue(
                "6.7.11",
                "The metadata doesn't identify the file as PDF/A-1b",
                None,
                true,
            )
        }),
    }
}

/// Rewrite the XMP identification from part 2 to part 1.
fn to_part1(xml: &str) -> String {
    xml.replace("<pdfaid:part>2</pdfaid:part>", "<pdfaid:part>1</pdfaid:part>")
        .replace("pdfaid:part=\"2\"", "pdfaid:part=\"1\"")
}

impl Session {
    /// Check the document against PDF/A-1b.
    pub fn pdfa1b_verify(&self) -> Vec<PdfaIssue> {
        let cos = self.current_copy();
        let mut out = shared_issues(&cos);
        out.extend(identification(&cos));
        // The file's own header and cross-reference form count once it is written; a document
        // that declares PDF/A-1 is always written as PDF 1.4 with a classic table.
        let file = (!self.is_dirty()).then(|| self.file.cos.bytes().as_slice());
        out.extend(part1_issues(&cos, file));
        out
    }

    /// Archive as PDF/A-1b: convert the document as far as possible (undoable until saved;
    /// the next save writes a PDF 1.4 file with a classic cross-reference table).
    pub fn archive_pdfa1b(&mut self) -> Result<PdfaReport> {
        self.graph_edit("Archive as PDF/A", |cos, _| {
            let r = pdfcraft_preflight::convert(cos, PdfaLevel::A2b).map_err(err)?;
            let mut fixed: Vec<String> = r.fixed.into_iter().map(|f| f.replace("PDF/A-2b", "PDF/A-1b")).collect();
            let root = cos.root().ok_or_else(|| crate::invalid("the PDF has no catalog"))?;
            let mut cat = catalog(cos);
            // Identification: part 1.
            if let Some(m) = cat.get(b"Metadata").cloned()
                && let Object::Stream(s) = &*cos.resolve(&m)
            {
                let xml = String::from_utf8_lossy(&s.decoded().map_err(err)?).into_owned();
                let mut d = s.dict.clone();
                d.remove(b"Filter");
                d.remove(b"DecodeParms");
                let stream = Object::Stream(Stream::from_raw(d, to_part1(&xml).into_bytes()));
                if let Some(r) = m.as_ref() {
                    cos.set(r, stream);
                }
            }
            if cat.remove(b"Version").is_some() {
                fixed.push("Removed the catalog's PDF version (the file is written as PDF 1.4)".into());
            }
            if cat.remove(b"OCProperties").is_some() {
                fixed.push("Removed optional content (every layer shows)".into());
            }
            cos.update_dict(root, |c| *c = cat.clone())?;
            // Transparency groups on pages and forms.
            let mut groups = 0usize;
            for p in crate::docutil::page_objs(cos)? {
                let mut changed = false;
                cos.update_dict(p, |d| changed = d.remove(b"Group").is_some())?;
                groups += usize::from(changed);
            }
            for n in cos.object_numbers() {
                let r = markupcraft_revu::cos::ObjRef::new(n, cos.generation(n));
                if let Object::Stream(s) = &*cos.get(r)
                    && s.dict.name(b"Subtype") == Some(b"Form")
                    && s.dict.get(b"Group").is_some()
                {
                    let mut ns = s.clone();
                    ns.dict.remove(b"Group");
                    cos.set(r, Object::Stream(ns));
                    groups += 1;
                }
            }
            if groups > 0 {
                fixed.push(format!("{groups}: removed transparency groups"));
            }
            cos.require_full_save();
            let mut remaining = shared_issues(cos);
            remaining.extend(identification(cos));
            remaining.extend(part1_issues(cos, None));
            Ok(PdfaReport { fixed, remaining })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn archive_as_pdfa1b_writes_a_pdf_14_file_and_verify_knows_part_1() {
        let dir = std::env::temp_dir().join(format!("markupcraft-pdfa1-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let bytes = pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 50.0, 50.0))]);
        let mut s = Session::from_bytes(bytes, "a.pdf").unwrap();
        let rep = s.archive_pdfa1b().unwrap();
        assert!(rep.fixed.iter().any(|f| f.contains("PDF/A-1b")), "{:?}", rep.fixed);
        let out = dir.join("a1.pdf");
        s.save_as(&out, true).unwrap();
        let b = std::fs::read(&out).unwrap();
        assert!(b.starts_with(b"%PDF-1.4"), "{:?}", String::from_utf8_lossy(&b[..10]));
        assert!(!b.windows(7).any(|w| w == b"/ObjStm"));
        let s = Session::open(&out).unwrap();
        assert_eq!(s.pdfa_declared(), Some((1, "B".into())));
        let v = s.pdfa1b_verify();
        assert!(
            !v.iter()
                .any(|i| i.clause == "6.1.2" || i.clause == "6.1.4" || i.clause == "6.7.11"),
            "{v:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
