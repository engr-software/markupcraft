//! Whole-file behaviour of a document: images opened as PDF pages, the revisions incremental
//! saves keep inside a PDF (and Revert As), Publish As (flattened, compressed or uncompressed),
//! Deskew, the standards and signature state that guards page edits, recovery copies, and an
//! email draft with the PDF attached.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, SaveOptions, Stream, write_full};
use markupcraft_revu::pdf::n;

use crate::blank::MAX_SIDE;
use crate::docutil::{page_objs, save_options};
use crate::flatten::FlattenFilter;
use crate::{Result, Session, invalid};

/// Image files File > Open converts to a PDF page (lower case, without the dot).
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif"];

/// Largest image file read, bytes.
const MAX_IMAGE_FILE: u64 = 256 << 20;
/// Largest image side, pixels.
const MAX_IMAGE_PX: u32 = 30_000;

/// Is `path` an image File > Open converts (by its extension)?
pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()))
}

/// Pixels per inch an image file declares (PNG `pHYs`, JPEG JFIF density); `None` when it
/// says nothing usable.
pub fn image_ppi(bytes: &[u8]) -> Option<f64> {
    let ok = |v: f64| (v.is_finite() && (10.0..=10_000.0).contains(&v)).then_some(v);
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        // Chunks: length (4), type (4), data, CRC (4).
        let mut at = 8usize;
        for _ in 0..64 {
            let len = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
            let kind = bytes.get(at + 4..at + 8)?;
            if kind == b"pHYs" {
                let d = bytes.get(at + 8..at + 17)?;
                let x = u32::from_be_bytes(d.get(0..4)?.try_into().ok()?);
                let unit = *d.get(8)?;
                return if unit == 1 { ok(f64::from(x) * 0.0254) } else { None };
            }
            if kind == b"IDAT" || kind == b"IEND" {
                return None;
            }
            at = at.checked_add(12)?.checked_add(len)?;
        }
        return None;
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // JFIF APP0: "JFIF\0", version (2), units (1), x density (2), y density (2).
        let i = bytes
            .get(..bytes.len().min(4096))?
            .windows(5)
            .position(|w| w == b"JFIF\0")?;
        let d = bytes.get(i + 5..i + 12)?;
        let units = *d.get(2)?;
        let x = f64::from(u16::from_be_bytes([*d.get(3)?, *d.get(4)?]));
        return match units {
            1 => ok(x),
            2 => ok(x * 2.54),
            _ => None,
        };
    }
    None
}

/// A PDF of one page showing the image in `bytes` at its declared resolution (72 ppi when it
/// declares none). Transparent parts are laid over white.
pub fn image_pdf_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| invalid(format!("not a readable image: {e}")))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_PX);
    limits.max_image_height = Some(MAX_IMAGE_PX);
    limits.max_alloc = Some(1 << 30);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| invalid(format!("not a readable image: {e}")))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(invalid("the image is empty"));
    }
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for p in img.pixels() {
        let [r, g, b, a] = p.0;
        let over = |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        rgb.extend_from_slice(&[over(r), over(g), over(b)]);
    }
    let ppi = image_ppi(bytes).unwrap_or(72.0);
    let k = 72.0 / ppi;
    let (pw, ph) = (
        (f64::from(w) * k).clamp(3.0, MAX_SIDE),
        (f64::from(h) * k).clamp(3.0, MAX_SIDE),
    );
    let mut cos = CosDoc::open(Arc::new(crate::blank::pdf_bytes(&[(pw, ph)])?))?;
    let page = *page_objs(&cos)?
        .first()
        .ok_or_else(|| invalid("the new page is missing"))?;
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), n("XObject"));
    d.set(b"Subtype".to_vec(), n("Image"));
    d.set(b"Width".to_vec(), Object::Int(i64::from(w)));
    d.set(b"Height".to_vec(), Object::Int(i64::from(h)));
    d.set(b"ColorSpace".to_vec(), n("DeviceRGB"));
    d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
    let xo = cos.add(Object::Stream(Stream::flate(d, &rgb)));
    let content = format!("q {pw:.4} 0 0 {ph:.4} 0 0 cm /Im0 Do Q\n");
    let cs = cos.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    let mut xobjs = Dict::new();
    xobjs.set(b"Im0".to_vec(), Object::Ref(xo));
    let mut res = Dict::new();
    res.set(b"XObject".to_vec(), Object::Dict(xobjs));
    cos.update_dict(page, |p| {
        p.set(b"Resources".to_vec(), Object::Dict(res));
        p.set(b"Contents".to_vec(), Object::Ref(cs));
    })?;
    Ok(write_full(&cos, &SaveOptions::default())?)
}

/// Read an image file and make it a one-page PDF ([`image_pdf_bytes`]).
pub fn image_file_pdf(path: &Path) -> Result<Vec<u8>> {
    let io = |e| crate::EngineError::Io {
        path: path.display().to_string(),
        source: e,
    };
    let len = std::fs::metadata(path).map_err(io)?.len();
    if len > MAX_IMAGE_FILE {
        return Err(invalid(format!("{} is larger than 256 MB", path.display())));
    }
    // every page of a TIFF, a GIF's first frame, or the one image
    crate::finish::imaging::image_bytes_pdf(&std::fs::read(path).map_err(io)?)
}

impl Session {
    /// A new, unsaved document made from an image file (one page). It saves as `pdf_path`.
    pub fn from_image(image: &Path, pdf_path: impl AsRef<Path>) -> Result<Self> {
        let bytes = image_file_pdf(image)?;
        let mut s = Self::from_bytes(bytes, pdf_path)?;
        s.saved_version = u64::MAX; // never saved
        Ok(s)
    }
}

// ---- revisions -------------------------------------------------------------------------------

/// Where each stored revision of a PDF ends: the byte just after each `%%EOF` that closes a
/// `startxref` section (and its end of line). The last one is the current file.
pub fn revision_ends(bytes: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(i) = bytes.get(from..).and_then(|b| b.windows(5).position(|w| w == b"%%EOF")) {
        let at = from + i;
        // A real end has "startxref <offset>" shortly before it.
        let back = bytes.get(at.saturating_sub(48)..at).unwrap_or_default();
        if back.windows(9).any(|w| w == b"startxref") {
            let mut end = at + 5;
            while matches!(bytes.get(end), Some(b'\r' | b'\n')) && end < at + 7 {
                end += 1;
            }
            out.push(end);
        }
        from = at + 5;
        if out.len() >= 10_000 {
            break;
        }
    }
    out
}

impl Session {
    /// How many revisions the file on disk keeps (incremental saves add one each); 1 for a
    /// file saved in full.
    pub fn revision_count(&self) -> usize {
        revision_ends(self.file.cos.bytes()).len().max(1)
    }

    /// Write revision `index` (0 = the first) of the file as it was saved to `out` (Revert
    /// As). Returns its size. The document is not changed.
    pub fn revert_as(&self, index: usize, out: &Path) -> Result<usize> {
        let bytes = self.file.cos.bytes();
        let ends = revision_ends(bytes);
        let end = *ends.get(index).ok_or_else(|| {
            invalid(format!(
                "revision {} does not exist (the file keeps {})",
                index + 1,
                ends.len().max(1)
            ))
        })?;
        let part = bytes
            .get(..end)
            .ok_or_else(|| invalid("the revision is past the end of the file"))?;
        // Check it opens before writing it.
        CosDoc::open(Arc::new(part.to_vec()))?;
        crate::write_atomic(out, part)?;
        Ok(part.len())
    }
}

// ---- Publish As --------------------------------------------------------------------------------

/// How File > Publish As writes the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishMode {
    /// Markups burned into the page content.
    Flattened,
    /// Compressed object streams (PDF 1.5).
    Compressed,
    /// A classic cross-reference table any reader accepts.
    Uncompressed,
}

impl PublishMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "flattened" | "flatten" => Some(Self::Flattened),
            "compressed" => Some(Self::Compressed),
            "uncompressed" => Some(Self::Uncompressed),
            _ => None,
        }
    }
}

impl Session {
    /// Write the document as it is now to `out` without revision history (Publish As).
    /// Returns the size written. The document is not changed.
    pub fn publish_as(&self, out: &Path, mode: PublishMode) -> Result<usize> {
        let bytes = self.current_bytes()?;
        let cos = if mode == PublishMode::Flattened {
            let mut copy = Session::from_bytes(bytes.as_ref().clone(), out)?;
            copy.flatten_markups(&FlattenFilter::default())?;
            CosDoc::open(copy.current_bytes()?)?
        } else {
            CosDoc::open(bytes)?
        };
        let opts = SaveOptions {
            object_streams: mode != PublishMode::Uncompressed,
            ..save_options()
        };
        let data = write_full(&cos, &opts)?;
        crate::write_atomic(out, &data)?;
        Ok(data.len())
    }
}

// ---- deskew ------------------------------------------------------------------------------------

/// The turn (degrees, counter-clockwise) that makes the line `a`-`b` level: within +-45.
pub fn deskew_angle(a: crate::Point, b: crate::Point) -> f64 {
    let mut d = (b.y - a.y).atan2(b.x - a.x).to_degrees();
    while d > 45.0 {
        d -= 90.0;
    }
    while d <= -45.0 {
        d += 90.0;
    }
    -d
}

impl Session {
    /// Turn the content of `pages` by `degrees` (counter-clockwise) about each page's centre,
    /// to straighten a skewed scan. Markups stay where they are. Undoable.
    pub fn deskew_pages(&mut self, pages: &[usize], degrees: f64) -> Result<()> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        if !degrees.is_finite() || degrees.abs() > 45.0 {
            return Err(invalid("deskew turns at most 45 degrees either way"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let centres: Vec<(f64, f64)> = pages
            .iter()
            .map(|p| {
                let r = self.doc.pages.get(*p).map(|i| i.crop.normalized()).unwrap_or_default();
                ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
            })
            .collect();
        let (s, c) = degrees.to_radians().sin_cos();
        self.graph_edit("Deskew", |cos, _| {
            let objs = page_objs(cos)?;
            for (p, (cx, cy)) in pages.iter().zip(centres) {
                let Some(page) = objs.get(*p).copied() else { continue };
                // Rotate about the centre: T(c) R T(-c).
                let tx = cx - c * cx + s * cy;
                let ty = cy - s * cx - c * cy;
                let pre = format!("q {c:.6} {s:.6} {:.6} {c:.6} {tx:.4} {ty:.4} cm\n", -s);
                let pre = cos.add(Object::Stream(Stream::flate(Dict::new(), pre.as_bytes())));
                let post = cos.add(Object::Stream(Stream::flate(Dict::new(), b"\nQ\n")));
                let old = cos.get(page).as_dict().and_then(|d| d.get(b"Contents").cloned());
                let mut list = vec![Object::Ref(pre)];
                match old.as_ref().map(|o| (o, cos.resolve(o))) {
                    Some((_, r)) if r.as_array().is_some() => {
                        list.extend(r.as_array().cloned().unwrap_or_default());
                    }
                    Some((o, _)) => list.push(o.clone()),
                    None => {}
                }
                list.push(Object::Ref(post));
                cos.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
            }
            Ok(())
        })
    }
}

// ---- standards and signatures ------------------------------------------------------------------

/// What guards page edits: signatures, a certification, PDF/A.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standards {
    /// Signed signature fields.
    pub signatures: usize,
    /// A certification signature (DocMDP) is present.
    pub certified: bool,
    /// The PDF/A part and conformance the XMP metadata claims ("2B"), if any.
    pub pdfa: Option<String>,
}

impl Standards {
    /// Page edits are refused (certified or PDF/A).
    pub fn blocks_page_edits(&self) -> bool {
        self.certified || self.pdfa.is_some()
    }
}

/// `<pdfaid:part>2</pdfaid:part>` or `pdfaid:part="2"`.
fn xmp_attr(x: &str, key: &str) -> Option<String> {
    let open = format!("<{key}>");
    if let Some(i) = x.find(&open) {
        let rest = x.get(i + open.len()..)?;
        let end = rest.find('<')?;
        return Some(rest.get(..end)?.trim().to_string());
    }
    let attr = format!("{key}=\"");
    let i = x.find(&attr)?;
    let rest = x.get(i + attr.len()..)?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.trim().to_string())
}

impl Session {
    /// Signatures, certification and PDF/A claims of the document.
    pub fn standards(&self) -> Standards {
        let cos = &self.file.cos;
        let mut st = Standards::default();
        if let Some(root) = cos.root() {
            let cat = cos.get(root);
            let d = cat.as_dict();
            st.certified = d
                .and_then(|d| d.get(b"Perms"))
                .and_then(|p| cos.dict(p))
                .is_some_and(|p| p.contains(b"DocMDP"));
            if let Some(af) = d.and_then(|d| d.get(b"AcroForm")).and_then(|a| cos.dict(a))
                && let Some(fields) = af.get(b"Fields").map(|f| cos.resolve(f))
                && let Some(list) = fields.as_array()
            {
                st.signatures = list
                    .iter()
                    .take(10_000)
                    .filter_map(|f| cos.dict(f))
                    .filter(|f| f.name(b"FT") == Some(b"Sig") && f.contains(b"V"))
                    .count();
            }
        }
        if let Some(x) = self.xmp()
            && let Some(part) = xmp_attr(&x, "pdfaid:part")
        {
            let conf = xmp_attr(&x, "pdfaid:conformance").unwrap_or_default();
            st.pdfa = Some(format!("{part}{conf}"));
        }
        st
    }
}

// ---- page labels from a region -----------------------------------------------------------------

/// Most regions a label is built from.
pub const MAX_REGIONS: usize = 8;

impl Session {
    /// Labels from the text inside `regions` (user space, the same place on every page): for
    /// each page, the words whose centres fall in each region, in reading order, the regions
    /// joined by `between`, with `before` and `after` around them (Revu's Create Page Labels
    /// from a page region). Pages with no text there are left out. Nothing is changed;
    /// `set_page_labels` applies the result.
    pub fn region_labels(
        &self,
        regions: &[crate::Rect],
        before: &str,
        between: &str,
        after: &str,
    ) -> Result<Vec<(usize, String)>> {
        if regions.is_empty() || regions.len() > MAX_REGIONS {
            return Err(invalid(format!("give 1 to {MAX_REGIONS} regions")));
        }
        let r = self.renderable(true)?;
        let mut out = Vec::new();
        for page in 0..r.page_count().min(crate::blank::MAX_PAGES) {
            let (Some(text), Ok(geom)) = (r.text(page), r.geom(page)) else {
                continue;
            };
            let words = crate::raster::words(&text, geom);
            let mut parts = Vec::new();
            for reg in regions {
                let reg = reg.normalized();
                let mut inside: Vec<&crate::raster::PageWord> = words
                    .iter()
                    .filter(|w| {
                        let (cx, cy) = ((w.rect.x0 + w.rect.x1) / 2.0, (w.rect.y0 + w.rect.y1) / 2.0);
                        cx >= reg.x0 && cx <= reg.x1 && cy >= reg.y0 && cy <= reg.y1
                    })
                    .collect();
                // Reading order: top line first (y up), then left to right.
                inside.sort_by(|a, b| {
                    let line = |w: &crate::raster::PageWord| (-(w.rect.y1 / 4.0).round()) as i64;
                    line(a).cmp(&line(b)).then(a.rect.x0.total_cmp(&b.rect.x0))
                });
                let t: Vec<&str> = inside.iter().map(|w| w.text.as_str()).collect();
                if !t.is_empty() {
                    parts.push(t.join(" "));
                }
            }
            if !parts.is_empty() {
                out.push((page, format!("{before}{}{after}", parts.join(between))));
            }
        }
        Ok(out)
    }
}

// ---- email -------------------------------------------------------------------------------------

/// An unsent email message (`X-Unsent: 1`, which mail programs open as a draft to send) with
/// `name` attached.
pub fn email_draft(subject: &str, body: &str, name: &str, pdf: &[u8]) -> Vec<u8> {
    use std::fmt::Write;
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let clean = |s: &str| s.replace(['\r', '\n', '"'], " ");
    let boundary = "markupcraft-boundary-7f3a";
    let mut m = String::new();
    let _ = write!(
        m,
        "X-Unsent: 1\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n",
        clean(subject)
    );
    let _ = write!(
        m,
        "--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}\r\n",
        body.replace('\n', "\r\n")
    );
    let _ = write!(
        m,
        "--{boundary}\r\nContent-Type: application/pdf; name=\"{0}\"\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"{0}\"\r\n\r\n",
        clean(name)
    );
    let mut line = 0;
    for chunk in pdf.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            let c = if i > chunk.len() {
                '='
            } else {
                char::from(B64[((v >> (18 - 6 * i)) & 63) as usize])
            };
            m.push(c);
        }
        line += 4;
        if line >= 76 {
            m.push_str("\r\n");
            line = 0;
        }
    }
    let _ = write!(m, "\r\n--{boundary}--\r\n");
    m.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, _| image::Rgba([(x * 40) as u8, 10, 200, 255]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn an_image_becomes_a_page_of_its_size() {
        let s = Session::from_bytes(image_pdf_bytes(&png(144, 72)).unwrap(), "img.pdf").unwrap();
        assert_eq!(s.page_count(), 1);
        let m = s.page(0).unwrap().media.normalized();
        assert!(
            (m.width() - 144.0).abs() < 0.01 && (m.height() - 72.0).abs() < 0.01,
            "{m:?}"
        );
        assert!(image_pdf_bytes(b"not an image").is_err());
        assert!(is_image_path(Path::new("scan.TIF")) && !is_image_path(Path::new("a.pdf")));
        // A JFIF header at 300 dpi.
        let jfif = [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 1, 1, 44, 1, 44,
        ];
        assert_eq!(image_ppi(&jfif), Some(300.0));
    }

    #[test]
    fn revisions_publish_and_deskew() {
        let dir = std::env::temp_dir().join(format!("mc-docfile-{}-rev", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.pdf");
        std::fs::write(&path, markupcraft_render::synthetic::sample_pdf()).unwrap();
        let mut s = Session::open(&path).unwrap();
        assert_eq!(s.revision_count(), 1);
        s.move_markups(&["SAMPLESQUAREAAAA".to_string()], 10.0, 0.0).unwrap();
        s.save(false).unwrap();
        let s = Session::open(&path).unwrap();
        assert_eq!(s.revision_count(), 2);
        let first = dir.join("first.pdf");
        let n = s.revert_as(0, &first).unwrap();
        assert_eq!(
            std::fs::read(&first).unwrap(),
            markupcraft_render::synthetic::sample_pdf()
        );
        assert!(n > 0 && s.revert_as(5, &first).is_err());
        // Publish: flattened has no markups; compressed and uncompressed keep them.
        let flat = dir.join("flat.pdf");
        s.publish_as(&flat, PublishMode::Flattened).unwrap();
        assert!(Session::open(&flat).unwrap().doc().markups.is_empty());
        let plain = dir.join("plain.pdf");
        s.publish_as(&plain, PublishMode::Uncompressed).unwrap();
        let p = Session::open(&plain).unwrap();
        assert_eq!(p.doc().markups.len(), s.doc().markups.len());
        assert_eq!(p.revision_count(), 1);
        // Deskew wraps the content in a turn; undo takes it back.
        let mut s = Session::open(&path).unwrap();
        assert_eq!(deskew_angle(Point::new(0.0, 0.0), Point::new(100.0, 2.0)).round(), -1.0);
        assert!(s.deskew_pages(&[0], 60.0).is_err());
        s.deskew_pages(&[0], 1.5).unwrap();
        let bytes = s.current_bytes().unwrap();
        assert!(Session::from_bytes(bytes.to_vec(), "x.pdf").is_ok());
        s.undo().unwrap();
        assert_eq!(s.standards(), Standards::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_come_from_the_title_block_region() {
        use crate::synthetic::{SyntheticPage, pdf, text};
        let page = |num: &str, title: &str| {
            SyntheticPage::new(
                600.0,
                400.0,
                format!(
                    "{}{}{}",
                    text(500.0, 40.0, 12.0, num),
                    text(300.0, 40.0, 10.0, title),
                    text(50.0, 300.0, 12.0, "PLAN NOTES")
                ),
            )
        };
        let bytes = pdf(&[page("A-101", "FLOOR PLAN"), page("A-102", "ROOF PLAN")]);
        let s = Session::from_bytes(bytes, "set.pdf").unwrap();
        let regions = [
            crate::Rect::new(490.0, 30.0, 590.0, 60.0),
            crate::Rect::new(290.0, 30.0, 420.0, 60.0),
        ];
        let l = s.region_labels(&regions, "", " - ", "").unwrap();
        assert_eq!(
            l,
            vec![
                (0, "A-101 - FLOOR PLAN".to_string()),
                (1, "A-102 - ROOF PLAN".to_string())
            ]
        );
        assert!(s.region_labels(&[], "", "", "").is_err());
    }

    #[test]
    fn email_draft_attaches_the_pdf() {
        let m = String::from_utf8(email_draft("Plans", "See attached.", "a.pdf", b"%PDF-1.7 abc")).unwrap();
        assert!(m.starts_with("X-Unsent: 1\r\nSubject: Plans"));
        assert!(m.contains("filename=\"a.pdf\""));
        assert!(m.contains("JVBERi0xLjcgYWJj"), "{m}");
    }
}
