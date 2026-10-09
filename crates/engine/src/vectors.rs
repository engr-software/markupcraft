//! The vector linework of a page: every stroked or filled path flattened to straight segments
//! in PDF user space, through nested form XObjects. Dynamic Fill traces regions over it.
//!
//! Adapted from PdfCraft's bounded path extraction (`pdfcraft-measure`, MIT OR Apache-2.0),
//! with larger limits for drawing sheets and curves flattened into segments.

use std::collections::HashSet;

use markupcraft_geom::Point;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object};
use pdfcraft_content::{Matrix, parse};

use crate::{EngineError, Result, Session};

/// Most segments extracted from one page.
pub const MAX_SEGMENTS: usize = 1_000_000;
/// Most decoded content bytes read for one page.
const MAX_BYTES: usize = 256 << 20;
/// Most content streams (page plus forms) read for one page.
const MAX_STREAMS: usize = 20_000;
/// Deepest form nesting followed.
const MAX_DEPTH: usize = 24;
/// Pieces each Bézier curve is flattened into.
const CURVE_STEPS: usize = 8;

/// A page's linework.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Linework {
    pub segments: Vec<(Point, Point)>,
    /// A limit stopped extraction early.
    pub truncated: bool,
    /// Content streams that could not be decoded.
    pub unreadable: usize,
}

struct Walker<'a> {
    cos: &'a CosDoc,
    out: Linework,
    seen: HashSet<ObjRef>,
    bytes: usize,
    streams: usize,
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

impl Walker<'_> {
    fn push(&mut self, path: &mut Vec<(Point, Point)>, a: Point, b: Point) {
        if finite(a) && finite(b) && (a.x != b.x || a.y != b.y) {
            path.push((a, b));
        }
    }

    fn walk(&mut self, contents: &Object, resources: &Dict, m: Matrix, depth: usize) {
        if depth >= MAX_DEPTH || self.streams >= MAX_STREAMS || self.out.truncated {
            self.out.truncated = true;
            return;
        }
        let o = self.cos.resolve(contents);
        let data = if let Some(arr) = o.as_array() {
            // a /Contents array is one stream split in parts
            let mut joined = Vec::new();
            for part in arr {
                let s = self.cos.resolve(part);
                if let Object::Stream(s) = s.as_ref() {
                    let left = MAX_BYTES.saturating_sub(self.bytes).saturating_sub(joined.len());
                    if left == 0 {
                        self.out.truncated = true;
                        break;
                    }
                    match s.decoded_within(left) {
                        Ok(d) => {
                            joined.extend(d);
                            joined.push(b'\n');
                        }
                        Err(_) => self.out.unreadable += 1,
                    }
                }
            }
            joined
        } else if let Object::Stream(s) = o.as_ref() {
            let left = MAX_BYTES.saturating_sub(self.bytes);
            if left == 0 {
                self.out.truncated = true;
                return;
            }
            match s.decoded_within(left) {
                Ok(d) => d,
                Err(_) => {
                    self.out.unreadable += 1;
                    return;
                }
            }
        } else {
            return;
        };
        self.ops(&data, resources, m, depth);
    }

    fn ops(&mut self, data: &[u8], resources: &Dict, initial: Matrix, depth: usize) {
        self.bytes = self.bytes.saturating_add(data.len());
        self.streams += 1;
        let mut ctm = initial;
        let mut stack: Vec<Matrix> = Vec::new();
        let mut path: Vec<(Point, Point)> = Vec::new();
        let mut start: Option<Point> = None;
        let mut cur: Option<Point> = None;
        let pt = |m: &Matrix, x: f64, y: f64| {
            let (x, y) = m.apply(x, y);
            Point::new(x, y)
        };
        for op in parse(data).ops {
            if self.out.segments.len().saturating_add(path.len()) >= MAX_SEGMENTS {
                self.out.truncated = true;
                return;
            }
            match op.op.as_slice() {
                b"q" => {
                    if stack.len() >= 256 {
                        self.out.truncated = true;
                        return;
                    }
                    stack.push(ctm);
                }
                b"Q" => ctm = stack.pop().unwrap_or(initial),
                b"cm" => {
                    if let Some(m) = Matrix::from_operands(&op.operands) {
                        ctm = m.then(&ctm);
                    }
                }
                b"m" => {
                    if let Some([x, y]) = op.nums::<2>() {
                        let p = pt(&ctm, x, y);
                        cur = Some(p);
                        start = Some(p);
                    }
                }
                b"l" => {
                    if let (Some(a), Some([x, y])) = (cur, op.nums::<2>()) {
                        let b = pt(&ctm, x, y);
                        self.push(&mut path, a, b);
                        cur = Some(b);
                    }
                }
                b"c" | b"v" | b"y" => {
                    let Some(a) = cur else { continue };
                    let ctrl = match op.op.as_slice() {
                        b"c" => op
                            .nums::<6>()
                            .map(|[x1, y1, x2, y2, x3, y3]| [pt(&ctm, x1, y1), pt(&ctm, x2, y2), pt(&ctm, x3, y3)]),
                        b"v" => op
                            .nums::<4>()
                            .map(|[x2, y2, x3, y3]| [a, pt(&ctm, x2, y2), pt(&ctm, x3, y3)]),
                        _ => op.nums::<4>().map(|[x1, y1, x3, y3]| {
                            let d = pt(&ctm, x3, y3);
                            [pt(&ctm, x1, y1), d, d]
                        }),
                    };
                    let Some([b, c, d]) = ctrl else { continue };
                    let mut prev = a;
                    for k in 1..=CURVE_STEPS {
                        let t = k as f64 / CURVE_STEPS as f64;
                        let u = 1.0 - t;
                        let q = Point::new(
                            u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x,
                            u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y,
                        );
                        self.push(&mut path, prev, q);
                        prev = q;
                    }
                    cur = Some(d);
                }
                b"h" => {
                    if let (Some(a), Some(b)) = (cur, start) {
                        self.push(&mut path, a, b);
                        cur = Some(b);
                    }
                }
                b"re" => {
                    if let Some([x, y, w, h]) = op.nums::<4>() {
                        let a = pt(&ctm, x, y);
                        let b = pt(&ctm, x + w, y);
                        let c = pt(&ctm, x + w, y + h);
                        let d = pt(&ctm, x, y + h);
                        for (f, t) in [(a, b), (b, c), (c, d), (d, a)] {
                            self.push(&mut path, f, t);
                        }
                        cur = Some(a);
                        start = Some(a);
                    }
                }
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                    if matches!(op.op.as_slice(), b"s" | b"b" | b"b*")
                        && let (Some(a), Some(b)) = (cur, start)
                    {
                        self.push(&mut path, a, b);
                    }
                    self.out.segments.append(&mut path);
                    cur = None;
                    start = None;
                }
                b"n" => {
                    path.clear();
                    cur = None;
                    start = None;
                }
                b"Do" => {
                    let Some(name) = op.name(0) else { continue };
                    let xobjects = resources.get(b"XObject").map(|o| self.cos.resolve(o));
                    let Some(entry) = xobjects
                        .as_ref()
                        .and_then(|o| o.as_dict())
                        .and_then(|d| d.get(name))
                        .cloned()
                    else {
                        continue;
                    };
                    let r = entry.as_ref();
                    if r.is_some_and(|r| self.seen.contains(&r)) {
                        continue;
                    }
                    let form = self.cos.resolve(&entry);
                    let Object::Stream(st) = form.as_ref() else { continue };
                    if st.dict.name(b"Subtype") != Some(b"Form") {
                        continue;
                    }
                    let fm = st
                        .dict
                        .get(b"Matrix")
                        .map(|m| self.cos.resolve(m))
                        .and_then(|o| o.as_array().and_then(|a| Matrix::from_operands(a)))
                        .unwrap_or_default()
                        .then(&ctm);
                    let own = st.dict.get(b"Resources").and_then(|o| self.cos.dict(o));
                    let res = own.unwrap_or_else(|| resources.clone());
                    if let Some(r) = r {
                        self.seen.insert(r);
                    }
                    self.walk(&entry, &res, fm, depth + 1);
                    if let Some(r) = r {
                        self.seen.remove(&r);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The linework of page `page` (0-based) of `cos`.
pub fn page_linework(cos: &CosDoc, page: usize) -> Result<Linework> {
    let pages = markupcraft_revu::pdf::pages(cos);
    let p = pages.get(page).ok_or(EngineError::NoPage {
        page: page + 1,
        count: pages.len(),
    })?;
    let resources = p.dict.get(b"Resources").and_then(|o| cos.dict(o)).unwrap_or_default();
    let mut w = Walker {
        cos,
        out: Linework::default(),
        seen: HashSet::new(),
        bytes: 0,
        streams: 0,
    };
    if let Some(c) = p.dict.get(b"Contents") {
        w.walk(c, &resources, Matrix::IDENTITY, 0);
    }
    Ok(w.out)
}

impl Session {
    /// The vector linework of a page as it is in the file (markups are not included).
    pub fn page_linework(&self, page: usize) -> Result<Linework> {
        self.page(page)?;
        page_linework(&self.file.cos, page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf};

    #[test]
    fn linework_follows_transforms_rectangles_and_curves() {
        let content = format!(
            "{}q 2 0 0 2 100 100 cm 0 0 10 10 re S Q\n0 0 m 10 0 10 10 0 10 c S\n",
            line(0.0, 0.0, 50.0, 0.0, 1.0)
        );
        let s = Session::from_bytes(pdf(&[SyntheticPage::new(300.0, 300.0, content)]), "v.pdf").unwrap();
        let lw = s.page_linework(0).unwrap();
        assert!(!lw.truncated);
        assert_eq!(lw.segments.len(), 1 + 4 + CURVE_STEPS);
        let (a, b) = lw.segments[1];
        assert_eq!((a.x, a.y, b.x, b.y), (100.0, 100.0, 120.0, 100.0));
        assert!(s.page_linework(3).is_err());
    }
}
