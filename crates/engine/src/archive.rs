//! Document > Repair PDF, Archive as PDF/A (with Verify and Unlock), and Color Processing.
//!
//! - Repair rewrites the file in full from the object graph: the cross-reference table is
//!   rebuilt (the object layer already reconstructs a damaged one when it opens the file, and
//!   says so in its repair log), annotation lists lose entries that are not annotations, page
//!   contents lose references that are not streams, and unused objects are dropped by the full
//!   save.
//! - PDF/A: PdfCraft's preflight (`pdfcraft-preflight`, MIT OR Apache-2.0) converts to PDF/A-2b
//!   or 3b (XMP identification, sRGB output intent, forbidden actions, annotation flags) and
//!   verifies. Unlock removes the PDF/A identification so the file is ordinary again.
//! - Color Processing recolours the page content (vector and raster alike) with a blend-mode
//!   overlay drawn after it: grayscale (Saturation blend with gray), one tint (Color blend), or
//!   lightened toward white (Screen blend). Markups are drawn after page content, so they keep
//!   their colours. The overlay is a separate tagged content stream, so it can be removed again.

use markupcraft_model::Color;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, Stream, write_full};

use crate::docutil::{annots_of, err, page_objs, save_options, set_annots};
use crate::{Result, Session, invalid};

pub use pdfcraft_preflight::{Issue as PdfaIssue, Level as PdfaLevel};

/// What Repair did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepairReport {
    /// What the object layer fixed while reading the file.
    pub read_fixes: Vec<String>,
    pub bad_annotations: usize,
    pub bad_contents: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

/// What PDF/A conversion did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfaReport {
    pub fixed: Vec<String>,
    /// Problems left (not conforming while any remain).
    pub remaining: Vec<PdfaIssue>,
}

/// How Color Processing recolours page content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorMode {
    /// Every colour to its gray.
    Grayscale,
    /// Every colour to shades of one colour (white stays white).
    Tint(Color),
    /// Fade toward white: 0 (unchanged) to 1 (white).
    Lighten(f64),
}

/// The tag on Color Processing's content streams (`/PCColorProcess true`).
const TAG: &[u8] = b"PCColorProcess";

fn page_contents(cos: &CosDoc, page: ObjRef) -> Vec<Object> {
    let p = cos.get(page);
    let Some(c) = p.as_dict().and_then(|d| d.get(b"Contents")).cloned() else {
        return Vec::new();
    };
    match &*cos.resolve(&c) {
        Object::Array(a) => a.clone(),
        _ => vec![c],
    }
}

fn is_tagged(cos: &CosDoc, o: &Object) -> bool {
    match &*cos.resolve(o) {
        Object::Stream(s) => s.dict.get(TAG).is_some(),
        _ => false,
    }
}

fn media_box(cos: &CosDoc, page: ObjRef) -> [f64; 4] {
    let mut r = page;
    for _ in 0..32 {
        let d = cos.dict(&Object::Ref(r)).unwrap_or_default();
        if let Some(b) = d.get(b"MediaBox").map(|b| cos.resolve(b))
            && let Some(a) = b.as_array()
        {
            let v: Vec<f64> = a.iter().filter_map(|x| cos.resolve(x).as_f64()).collect();
            if let [a, b, c, d] = v.as_slice() {
                return [a.min(*c), b.min(*d), a.max(*c), b.max(*d)];
            }
        }
        match d.reference(b"Parent") {
            Some(p) => r = p,
            None => break,
        }
    }
    [0.0, 0.0, 612.0, 792.0]
}

impl Session {
    /// Repair the document (Document > Repair PDF); the next save is a full rewrite. Undoable
    /// until saved.
    pub fn repair_pdf(&mut self) -> Result<RepairReport> {
        let read_fixes = self.file.cos.repair_log().to_vec();
        let bytes_before = self.file.cos.bytes().len();
        self.graph_edit("Repair PDF", |cos, _| {
            let mut rep = RepairReport {
                read_fixes,
                bytes_before,
                ..Default::default()
            };
            for page in page_objs(cos)? {
                let list = annots_of(cos, page);
                let kept: Vec<Object> = list
                    .iter()
                    .filter(|a| cos.dict(a).is_some_and(|d| d.name(b"Subtype").is_some()))
                    .cloned()
                    .collect();
                if kept.len() != list.len() {
                    rep.bad_annotations += list.len() - kept.len();
                    set_annots(cos, page, kept)?;
                }
                let contents = page_contents(cos, page);
                let good: Vec<Object> = contents
                    .iter()
                    .filter(|c| matches!(&*cos.resolve(c), Object::Stream(_)))
                    .cloned()
                    .collect();
                if good.len() != contents.len() {
                    rep.bad_contents += contents.len() - good.len();
                    cos.update_dict(page, |d| {
                        if good.is_empty() {
                            d.remove(b"Contents");
                        } else {
                            d.set(b"Contents".to_vec(), Object::Array(good));
                        }
                    })?;
                }
            }
            cos.require_full_save();
            rep.bytes_after = write_full(cos, &save_options())?.len();
            Ok(rep)
        })
    }

    /// Check the document against PDF/A (`level`).
    pub fn pdfa_verify(&self, level: PdfaLevel) -> Vec<PdfaIssue> {
        pdfcraft_preflight::verify(&self.current_copy(), level)
    }

    /// The PDF/A part and conformance the document declares, e.g. `Some((2, "B"))`.
    pub fn pdfa_declared(&self) -> Option<(u8, String)> {
        pdfcraft_preflight::declared(&self.file.cos).pdfa
    }

    /// Archive as PDF/A: convert the document (undoable until saved; the next save is a full
    /// rewrite).
    pub fn archive_pdfa(&mut self, level: PdfaLevel) -> Result<PdfaReport> {
        self.graph_edit("Archive as PDF/A", |cos, _| {
            let r = pdfcraft_preflight::convert(cos, level).map_err(err)?;
            cos.require_full_save();
            Ok(PdfaReport {
                fixed: r.fixed,
                remaining: r.remaining,
            })
        })
    }

    /// Unlock a PDF/A archive: remove its PDF/A identification (XMP `pdfaid`) so editing it does
    /// not break a conformance claim. Returns whether there was one.
    pub fn unlock_pdfa(&mut self) -> Result<bool> {
        if self.pdfa_declared().is_none() {
            return Ok(false);
        }
        self.cos_edit("Unlock PDF/A", |cos| {
            let Some(root) = cos.root() else {
                return Ok((false, false));
            };
            let cat = cos.dict(&Object::Ref(root)).unwrap_or_default();
            let Some(meta) = cat.get(b"Metadata").cloned() else {
                return Ok((false, false));
            };
            let Object::Stream(s) = &*cos.resolve(&meta) else {
                return Ok((false, false));
            };
            let xml = String::from_utf8_lossy(&s.decoded().map_err(err)?).into_owned();
            let cleaned = strip_pdfaid(&xml);
            let mut d = s.dict.clone();
            d.remove(b"Filter");
            d.remove(b"DecodeParms");
            let stream = Object::Stream(Stream::from_raw(d, cleaned.into_bytes()));
            match meta.as_ref() {
                Some(r) => cos.set(r, stream),
                None => {
                    let r = cos.add(stream);
                    cos.update_dict(root, |c| c.set(b"Metadata".to_vec(), Object::Ref(r)))?;
                }
            }
            cos.require_full_save();
            Ok((true, true))
        })
    }

    /// Color Processing on `pages` (0-based; empty = all). Replaces an earlier one on those
    /// pages. Undoable.
    pub fn color_process(&mut self, pages: &[usize], mode: ColorMode) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let (gray, bm) = match mode {
            ColorMode::Grayscale => ([0.5, 0.5, 0.5], "Saturation"),
            ColorMode::Tint(c) => ([c.r, c.g, c.b], "Color"),
            ColorMode::Lighten(v) => {
                if !(v.is_finite() && (0.0..=1.0).contains(&v)) {
                    return Err(invalid("lighten is from 0 to 1"));
                }
                ([v, v, v], "Screen")
            }
        };
        let chosen: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        self.graph_edit("Color Processing", |cos, _| {
            let objs = page_objs(cos)?;
            let mut n = 0;
            for p in chosen {
                let Some(page) = objs.get(p).copied() else { continue };
                let mut list: Vec<Object> = page_contents(cos, page)
                    .into_iter()
                    .filter(|c| !is_tagged(cos, c))
                    .collect();
                let b = media_box(cos, page);
                let mut gs = Dict::new();
                gs.set(b"Type".to_vec(), Object::name("ExtGState"));
                gs.set(b"BM".to_vec(), Object::name(bm));
                let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
                let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
                let mut ext = res.get(b"ExtGState").and_then(|x| cos.dict(x)).unwrap_or_default();
                ext.set(b"MCColorProcess".to_vec(), Object::Dict(gs));
                res.set(b"ExtGState".to_vec(), Object::Dict(ext));
                let mut tag = Dict::new();
                tag.set(TAG.to_vec(), Object::Bool(true));
                // Paper white under the content, so the blend has a backdrop everywhere.
                let paper = format!("q 1 g {} {} {} {} re f Q q\n", b[0], b[1], b[2] - b[0], b[3] - b[1]);
                let open = cos.add(Object::Stream(Stream::from_raw(tag.clone(), paper.into_bytes())));
                let body = format!(
                    "\nQ\nq /MCColorProcess gs {} {} {} rg {} {} {} {} re f Q\n",
                    gray[0],
                    gray[1],
                    gray[2],
                    b[0],
                    b[1],
                    b[2] - b[0],
                    b[3] - b[1]
                );
                let close = cos.add(Object::Stream(Stream::from_raw(tag, body.into_bytes())));
                list.insert(0, Object::Ref(open));
                list.push(Object::Ref(close));
                cos.update_dict(page, |d| {
                    d.set(b"Resources".to_vec(), Object::Dict(res));
                    d.set(b"Contents".to_vec(), Object::Array(list));
                })?;
                n += 1;
            }
            Ok(n)
        })
    }

    /// Remove Color Processing from `pages` (empty = all). Returns how many pages had it.
    pub fn color_process_remove(&mut self, pages: &[usize]) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let chosen: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        self.graph_edit("Remove Color Processing", |cos, _| {
            let objs = page_objs(cos)?;
            let mut n = 0;
            for p in chosen {
                let Some(page) = objs.get(p).copied() else { continue };
                let list = page_contents(cos, page);
                let kept: Vec<Object> = list.iter().filter(|c| !is_tagged(cos, c)).cloned().collect();
                if kept.len() != list.len() {
                    cos.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(kept)))?;
                    n += 1;
                }
            }
            Ok(n)
        })
    }
}

/// XMP without the `pdfaid:` properties (attribute and element forms).
fn strip_pdfaid(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(i) = rest.find("pdfaid:") {
        let (before, after) = rest.split_at(i);
        // element form: <pdfaid:part>2</pdfaid:part>
        if let Some(head) = before.strip_suffix('<') {
            out.push_str(head);
            let close = after
                .find("</pdfaid:")
                .and_then(|c| after[c..].find('>').map(|e| c + e + 1));
            match close {
                Some(e) => rest = &after[e..],
                None => {
                    rest = "";
                }
            }
            continue;
        }
        // attribute form: pdfaid:part="2"
        out.push_str(before);
        let end = after
            .find('"')
            .and_then(|q| after[q + 1..].find('"').map(|e| q + e + 2))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn strips_pdfa_identification() {
        let x = "<rdf:Description pdfaid:part=\"2\" pdfaid:conformance=\"B\" x=\"1\"/><pdfaid:part>2</pdfaid:part>ok";
        let s = strip_pdfaid(x);
        assert!(!s.contains("pdfaid"), "{s}");
        assert!(s.contains("x=\"1\"") && s.ends_with("ok"));
    }

    #[test]
    fn grayscale_turns_red_gray_and_comes_off_again() {
        let red = "1 0 0 rg 100 100 200 200 re f";
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, red)]), "c.pdf").unwrap();
        let px = |s: &Session| {
            s.renderable(false)
                .unwrap()
                .render_rgba(0, 0.5)
                .unwrap()
                .pixel(200.0, 200.0)
        };
        let before = px(&s);
        assert!(before[0] > 200 && before[1] < 60, "{before:?}");
        assert_eq!(s.color_process(&[], ColorMode::Grayscale).unwrap(), 1);
        let after = px(&s);
        assert!(
            after[0].abs_diff(after[1]) < 8 && after[1].abs_diff(after[2]) < 8,
            "{after:?}"
        );
        // white stays white
        let w = s
            .renderable(false)
            .unwrap()
            .render_rgba(0, 0.5)
            .unwrap()
            .pixel(500.0, 700.0);
        assert!(w[0] > 245, "white {w:?} red {after:?}");
        s.color_process(&[0], ColorMode::Lighten(0.8)).unwrap();
        let light = px(&s);
        assert!(light[1] > 150, "lightened {light:?}");
        assert_eq!(s.color_process_remove(&[]).unwrap(), 1);
        assert_eq!(px(&s), before);
        assert!(s.color_process(&[5], ColorMode::Grayscale).is_err());
        assert!(s.color_process(&[], ColorMode::Lighten(2.0)).is_err());
    }

    #[test]
    fn repair_drops_broken_entries_and_archive_round_trips() {
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "0 0 m 10 10 l S")]), "r.pdf").unwrap();
        let page = page_objs(&s.file.cos).unwrap()[0];
        s.file
            .cos
            .update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(vec![Object::Int(5)])))
            .unwrap();
        let r = s.repair_pdf().unwrap();
        assert_eq!(r.bad_annotations, 1);
        assert!(r.bytes_after > 0);
        assert!(s.pdfa_declared().is_none());
        let rep = s.archive_pdfa(PdfaLevel::A2b).unwrap();
        assert!(!rep.fixed.is_empty());
        assert_eq!(s.pdfa_declared().map(|d| d.0), Some(2));
        assert!(s.unlock_pdfa().unwrap());
        assert!(s.pdfa_declared().is_none());
        assert!(!s.unlock_pdfa().unwrap());
    }
}
