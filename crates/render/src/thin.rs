//! Disable Line Weights: a copy of a PDF whose page linework all draws at the thinnest visible
//! width (line width 0 = one device pixel, ISO 32000-2 §8.4.3.2). Only the renderer sees the
//! copy; the document is not changed. Object numbers stay the same, so annotation references
//! (the markups MarkupCraft draws itself) still match.

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

/// The PDF in `bytes` with its line weights disabled (see the module notes).
pub fn thin_lines(bytes: &Arc<Vec<u8>>) -> Result<Arc<Vec<u8>>, String> {
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
            Object::Dict(d) if d.contains(b"LW") && d.name(b"Type").is_none_or(|t| t == b"ExtGState") => states.push(r),
            Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Form") => forms.push(r),
            _ => {}
        }
    }
    let mut changed = false;
    for r in contents.into_iter().chain(forms) {
        let o = cos.get(r);
        let Object::Stream(s) = &*o else { continue };
        let Ok(data) = s.decoded_within(MAX_STREAM) else {
            continue;
        };
        if let Some(new) = thin_content(&data) {
            let mut d: Dict = s.dict.clone();
            d.remove(b"Length");
            d.remove(b"DecodeParms");
            d.remove(b"Filter");
            cos.set(r, Object::Stream(Stream::flate(d, &new)));
            changed = true;
        }
    }
    for r in states {
        if cos.update_dict(r, |d| d.set(b"LW".to_vec(), Object::Int(0))).is_ok() {
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
