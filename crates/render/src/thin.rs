//! Line weight copies of a PDF, for the renderer only (the document is not changed):
//! - Disable Line Weights: all page linework draws at the thinnest visible width (line width 0
//!   = one device pixel, ISO 32000-2 §8.4.3.2);
//! - Enhance Thin Lines: linework thinner than a minimum width on the page (hairlines included)
//!   draws at that minimum, so thin lines stay visible when zoomed out. The width a `w`
//!   operator gives is measured on the page through the current transformation (`cm`, saved
//!   and restored with `q` / `Q`).
//!
//! Object numbers stay the same, so annotation references (the markups MarkupCraft draws
//! itself) still match.

use std::collections::HashSet;
use std::sync::Arc;

use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object, SaveOptions, Stream, write_incremental};

/// Largest decoded content stream rewritten.
const MAX_STREAM: usize = 64 << 20;
/// Most objects looked at.
const MAX_OBJECTS: usize = 2_000_000;

/// Set every `w` operator's width to 0 in a content stream; `None` when nothing changed.
pub fn thin_content(data: &[u8]) -> Option<Vec<u8>> {
    let mut parsed = pdfcraft_content::parse(data);
    let mut changed = false;
    for op in &mut parsed.ops {
        if op.is("w")
            && let Some(last) = op.operands.last_mut()
        {
            *last = Object::Int(0);
            changed = true;
        }
    }
    changed.then(|| pdfcraft_content::serialize_ops(&parsed.ops))
}

/// The scale a transformation `[a b c d e f]` gives lengths (the square root of its area
/// factor); 1 for anything malformed.
fn matrix_scale(m: &[Object]) -> f64 {
    let v: Vec<f64> = m.iter().filter_map(Object::as_f64).collect();
    match v.as_slice() {
        [a, b, c, d, _, _] => {
            let det = (a * d - b * c).abs();
            if det.is_finite() && det > 1e-12 {
                det.sqrt()
            } else {
                1.0
            }
        }
        _ => 1.0,
    }
}

/// Raise every `w` operator whose width on the page is below `min` (points) to `min`; `None`
/// when nothing changed. Line widths are measured through the current transformation.
pub fn widen_content(data: &[u8], min: f64) -> Option<Vec<u8>> {
    if !(min.is_finite() && min > 0.0) {
        return None;
    }
    let mut parsed = pdfcraft_content::parse(data);
    let mut changed = false;
    let mut scale = 1.0_f64;
    let mut stack: Vec<f64> = Vec::new();
    for op in &mut parsed.ops {
        if op.is("q") {
            if stack.len() < 256 {
                stack.push(scale);
            }
        } else if op.is("Q") {
            scale = stack.pop().unwrap_or(1.0);
        } else if op.is("cm") {
            let k = scale * matrix_scale(&op.operands);
            scale = if k.is_finite() && k > 1e-9 { k } else { scale };
        } else if op.is("w")
            && let Some(last) = op.operands.last_mut()
        {
            let w = last.as_f64().unwrap_or(1.0).max(0.0);
            if w * scale < min {
                *last = Object::Real(min / scale);
                changed = true;
            }
        }
    }
    changed.then(|| pdfcraft_content::serialize_ops(&parsed.ops))
}

fn content_refs(cos: &CosDoc, page: &Dict, out: &mut HashSet<ObjRef>) {
    match page.get(b"Contents") {
        Some(Object::Ref(r)) => match &*cos.get(*r) {
            Object::Array(a) => out.extend(a.iter().filter_map(Object::as_ref)),
            _ => {
                out.insert(*r);
            }
        },
        Some(Object::Array(a)) => out.extend(a.iter().filter_map(Object::as_ref)),
        _ => {}
    }
}

/// What a view copy does to the line widths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lines {
    /// As the document draws them.
    Keep,
    /// Every line one device pixel wide (Disable Line Weights).
    Hairline,
    /// Every line at least this many points wide (Enhance Thin Lines).
    AtLeast(f64),
}

/// A copy of a document for the view: line widths, and blend modes drawn as Normal (the
/// colours of transparency groups and blended objects composited plainly).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewCopy {
    pub lines: Lines,
    pub normal_blend: bool,
}

impl ViewCopy {
    /// Whether the copy is the document itself.
    pub fn is_plain(&self) -> bool {
        self.lines == Lines::Keep && !self.normal_blend
    }
}

/// The PDF in `bytes` with its line weights disabled (see the module notes).
pub fn thin_lines(bytes: &Arc<Vec<u8>>) -> Result<Arc<Vec<u8>>, String> {
    view_copy(
        bytes,
        &ViewCopy {
            lines: Lines::Hairline,
            normal_blend: false,
        },
    )
}

/// The PDF in `bytes` with linework thinner than `min` points drawn `min` wide (Enhance Thin
/// Lines; see the module notes).
pub fn enhance_thin_lines(bytes: &Arc<Vec<u8>>, min: f64) -> Result<Arc<Vec<u8>>, String> {
    if !(min.is_finite() && min > 0.0 && min <= 72.0) {
        return Err("the minimum line width is 0 to 72 points".into());
    }
    view_copy(
        bytes,
        &ViewCopy {
            lines: Lines::AtLeast(min),
            normal_blend: false,
        },
    )
}

/// The copy of the PDF in `bytes` that `how` asks for (the bytes themselves when nothing
/// changes).
pub fn view_copy(bytes: &Arc<Vec<u8>>, how: &ViewCopy) -> Result<Arc<Vec<u8>>, String> {
    if let Lines::AtLeast(m) = how.lines
        && !(m.is_finite() && m > 0.0 && m <= 72.0)
    {
        return Err("the minimum line width is 0 to 72 points".into());
    }
    if how.is_plain() {
        return Ok(bytes.clone());
    }
    let mut cos = CosDoc::open(bytes.clone()).map_err(|e| e.to_string())?;
    let nums: Vec<u32> = cos.object_numbers().into_iter().take(MAX_OBJECTS).collect();
    // Page content streams (pages and forms: anything else is left alone).
    let mut contents: HashSet<ObjRef> = HashSet::new();
    let mut forms: Vec<ObjRef> = Vec::new();
    let mut states: Vec<ObjRef> = Vec::new();
    for n in &nums {
        let r = ObjRef {
            num: *n,
            generation: cos.generation(*n),
        };
        let Ok(o) = cos.try_get(*n) else { continue };
        match &*o {
            Object::Dict(d) if d.name(b"Type") == Some(b"Page") => content_refs(&cos, d, &mut contents),
            Object::Dict(d)
                if (d.contains(b"LW") || d.contains(b"BM")) && d.name(b"Type").is_none_or(|t| t == b"ExtGState") =>
            {
                states.push(r)
            }
            Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Form") => forms.push(r),
            _ => {}
        }
    }
    let mut changed = false;
    let rewrite_content = how.lines != Lines::Keep;
    for r in contents.into_iter().chain(forms).filter(|_| rewrite_content) {
        let o = cos.get(r);
        let Object::Stream(s) = &*o else { continue };
        let Ok(data) = s.decoded_within(MAX_STREAM) else {
            continue;
        };
        let new = match how.lines {
            Lines::Keep => None,
            Lines::Hairline => thin_content(&data),
            Lines::AtLeast(m) => widen_content(&data, m),
        };
        if let Some(new) = new {
            let mut d: Dict = s.dict.clone();
            d.remove(b"Length");
            d.remove(b"DecodeParms");
            d.remove(b"Filter");
            cos.set(r, Object::Stream(Stream::flate(d, &new)));
            changed = true;
        }
    }
    for r in states {
        let (lw, bm) = match &*cos.get(r) {
            Object::Dict(d) => (
                d.get(b"LW").and_then(Object::as_f64),
                d.get(b"BM").is_some_and(|b| b.as_name() != Some(b"Normal")),
            ),
            _ => (None, false),
        };
        let new = match how.lines {
            Lines::Hairline if lw.is_some() => Some(Object::Int(0)),
            Lines::AtLeast(m) if lw.is_some_and(|w| w < m) => Some(Object::Real(m)),
            _ => None,
        };
        if let Some(v) = new
            && cos.update_dict(r, |d| d.set(b"LW".to_vec(), v)).is_ok()
        {
            changed = true;
        }
        if how.normal_blend
            && bm
            && cos
                .update_dict(r, |d| d.set(b"BM".to_vec(), Object::name("Normal")))
                .is_ok()
        {
            changed = true;
        }
    }
    if !changed {
        return Ok(bytes.clone());
    }
    // An incremental update keeps every object number (a full rewrite renumbers them, and the
    // renderer hides the markups MarkupCraft draws by their numbers).
    write_incremental(&cos, &SaveOptions::default())
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_lines_widen_to_a_minimum_on_the_page() {
        let out = widen_content(b"0.1 w 0 0 m 100 100 l S 2 w 0 0 m 1 1 l S", 0.5).unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("0.5 w") && text.contains("2 w"), "{text}");
        // Hairlines (0) widen too.
        assert!(String::from_utf8_lossy(&widen_content(b"0 w 0 0 m 1 1 l S", 1.0).unwrap()).contains("1 w"));
        // Through a scaled transformation: 1 unit at 0.1 scale is 0.1 pt on the page.
        let out = widen_content(b"q 0.1 0 0 0.1 0 0 cm 1 w 0 0 m 9 9 l S Q 1 w", 0.5).unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("5 w"), "{text}");
        assert!(text.trim_end().ends_with("1 w"), "restored by Q: {text}");
        assert!(
            widen_content(b"3 w 0 0 m 1 1 l S", 0.5).is_none(),
            "wide enough already"
        );
        let bytes = Arc::new(crate::synthetic::sample_pdf());
        let wide = enhance_thin_lines(&bytes, 6.0).unwrap();
        assert_ne!(wide.len(), bytes.len(), "the sample's linework is under 6 pt");
        assert!(enhance_thin_lines(&bytes, -1.0).is_err());
        let opts = crate::RenderOptions {
            threads: 0,
            hide: vec![(8, 0), (9, 0), (11, 0), (12, 0), (13, 0), (14, 0)],
            ..Default::default()
        };
        let ink = |doc: &crate::RenderDoc| {
            doc.request(vec![crate::page_request(0, 1.0, 1)]);
            let r = (0..1000).find_map(|_| doc.poll()).unwrap();
            r.rgba.as_chunks::<4>().0.iter().filter(|p| p[0] < 160).count()
        };
        let a = crate::RenderDoc::open(bytes.clone(), &opts).unwrap();
        let b = crate::RenderDoc::open(wide, &opts).unwrap();
        let (na, nb) = (ink(&a), ink(&b));
        assert!(nb > na, "thicker linework covers more of the page: {na} -> {nb}");
    }

    #[test]
    fn blend_modes_draw_as_normal() {
        use crate::synthetic::sample_pdf;
        let bytes = Arc::new(sample_pdf());
        let mut cos = CosDoc::open(bytes.clone()).unwrap();
        let mut gs = Dict::new();
        gs.set(b"Type".to_vec(), Object::name("ExtGState"));
        gs.set(b"BM".to_vec(), Object::name("Multiply"));
        let r = cos.add(Object::Dict(gs));
        let with_bm = Arc::new(write_incremental(&cos, &SaveOptions::default()).unwrap());
        let how = ViewCopy {
            lines: Lines::Keep,
            normal_blend: true,
        };
        let copy = view_copy(&with_bm, &how).unwrap();
        let back = CosDoc::open(copy).unwrap();
        let o = back.get(r);
        let Object::Dict(d) = &*o else { panic!("a dict") };
        assert_eq!(d.name(b"BM"), Some(&b"Normal"[..]));
        // Nothing asked: the same bytes.
        let plain = ViewCopy {
            lines: Lines::Keep,
            normal_blend: false,
        };
        assert!(Arc::ptr_eq(&view_copy(&bytes, &plain).unwrap(), &bytes));
    }

    #[test]
    fn line_widths_become_zero() {
        let out = thin_content(b"2.5 w 0 0 m 100 100 l S").unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("0 w"), "{text}");
        assert!(!text.contains("2.5"), "{text}");
        assert!(thin_content(b"0 0 m 1 1 l S").is_none());
        let bytes = Arc::new(crate::synthetic::sample_pdf());
        let thin = thin_lines(&bytes).unwrap();
        let opts = crate::RenderOptions {
            threads: 0,
            ..Default::default()
        };
        let a = crate::RenderDoc::open(bytes.clone(), &opts).unwrap();
        let b = crate::RenderDoc::open(thin.clone(), &opts).unwrap();
        assert_eq!(a.page_count(), b.page_count());
        // Same object numbers: the page content stays content, and draws.
        let ink = |doc: &crate::RenderDoc| {
            doc.request(vec![crate::page_request(0, 0.25, 1)]);
            let r = (0..1000).find_map(|_| doc.poll()).unwrap();
            r.rgba.as_chunks::<4>().0.iter().filter(|p| p[0] < 128).count()
        };
        let hide = vec![(8, 0), (9, 0), (11, 0), (12, 0), (13, 0), (14, 0)];
        let opts = crate::RenderOptions {
            threads: 0,
            hide,
            ..Default::default()
        };
        let a = crate::RenderDoc::open(bytes.clone(), &opts).unwrap();
        let b = crate::RenderDoc::open(thin.clone(), &opts).unwrap();
        let (na, nb) = (ink(&a), ink(&b));
        assert!(na > 500 && nb * 3 > na, "{na} {nb}");
    }
}
