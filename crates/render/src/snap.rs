//! Page linework for Snap to Content: the straight segments of every stroked or filled path in
//! a page's content (and the Form XObjects it draws), in PDF user space, ready for
//! `markupcraft_geom::snap::SnapIndex`.
//!
//! Curves are flattened; only a curve's real ends are snap endpoints (the flattened pieces have
//! no midpoints). Clipping paths (`W n`) and paths ended with `n` do not snap. Extraction is
//! lenient and bounded: unreadable streams are skipped and counted, and limits on segments,
//! bytes, streams and nesting stop it early with `truncated` set.
//!
//! The content walk follows PdfCraft's `pdfcraft-measure` snapping extraction (MIT OR
//! Apache-2.0), written again over `markupcraft-geom`'s snap segments.

use std::collections::HashSet;
use std::sync::Arc;

use markupcraft_geom::Point;
use markupcraft_geom::snap::SnapSegment;
use pdfcraft_content::{Matrix, parse};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object};

/// Most segments taken from one page.
pub const MAX_SEGMENTS: usize = 500_000;
/// Most decoded content bytes read for one page.
const MAX_BYTES: usize = 64 << 20;
/// Most content streams (page parts and forms) read for one page.
const MAX_STREAMS: usize = 4096;
/// Deepest Form XObject nesting followed.
const MAX_DEPTH: usize = 24;
/// Deepest `q` nesting tracked.
const MAX_SAVES: usize = 256;
/// Deepest page tree walked.
const MAX_TREE: usize = 64;

/// The linework of one page.
#[derive(Debug, Clone, Default)]
pub struct Linework {
    pub segments: Vec<SnapSegment>,
    /// a limit stopped extraction early
    pub truncated: bool,
    /// content streams that could not be decoded (skipped)
    pub unreadable: usize,
}

/// The page objects of `doc` in order (cycles, missing kids and deep trees tolerated).
pub fn page_refs(doc: &CosDoc) -> Vec<ObjRef> {
    let mut out = Vec::new();
    let Some(root) = doc.root() else { return out };
    let catalog = doc.get(root);
    let Some(pages) = catalog.as_dict().and_then(|d| d.reference(b"Pages")) else {
        return out;
    };
    let mut seen = HashSet::new();
    walk_tree(doc, pages, &mut seen, &mut out, 0);
    out
}

fn walk_tree(doc: &CosDoc, node: ObjRef, seen: &mut HashSet<ObjRef>, out: &mut Vec<ObjRef>, depth: usize) {
    if depth > MAX_TREE || !seen.insert(node) {
        return;
    }
    let obj = doc.get(node);
    let Some(d) = obj.as_dict() else { return };
    if d.name(b"Type") == Some(b"Pages") || d.get(b"Kids").is_some() {
        let kids = doc.resolve(d.get(b"Kids").unwrap_or(&Object::Null));
        for k in kids.as_array().map(Vec::as_slice).unwrap_or_default() {
            if let Some(r) = k.as_ref() {
                walk_tree(doc, r, seen, out, depth + 1);
            }
        }
    } else {
        out.push(node);
    }
}

/// The `/Resources` of a page, inherited from its parents when it has none.
fn page_resources(doc: &CosDoc, page: ObjRef) -> Dict {
    let mut node = Some(page);
    let mut seen = HashSet::new();
    while let Some(r) = node {
        if !seen.insert(r) || seen.len() > MAX_TREE {
            break;
        }
        let obj = doc.get(r);
        let Some(d) = obj.as_dict() else { break };
        if let Some(res) = d.get(b"Resources")
            && let Some(rd) = doc.resolve(res).as_dict()
        {
            return rd.clone();
        }
        node = d.reference(b"Parent");
    }
    Dict::new()
}

/// The linework of page `page` (a page object from [`page_refs`]).
pub fn page_linework(doc: &CosDoc, page: ObjRef) -> Linework {
    let obj = doc.get(page);
    let Some(d) = obj.as_dict() else {
        return Linework::default();
    };
    let resources = page_resources(doc, page);
    let mut w = Walker {
        doc,
        out: Linework::default(),
        seen: HashSet::new(),
        bytes: 0,
        streams: 0,
    };
    if let Some(contents) = d.get(b"Contents") {
        w.walk(contents, &resources, Matrix::default(), 0);
    }
    w.out
}

/// [`page_linework`] of page `index` (0-based) of a PDF in memory; empty when it cannot be read.
pub fn linework_of(bytes: Arc<Vec<u8>>, index: usize) -> Linework {
    match CosDoc::open(bytes) {
        Ok(doc) => match page_refs(&doc).get(index) {
            Some(r) => page_linework(&doc, *r),
            None => Linework::default(),
        },
        Err(_) => Linework::default(),
    }
}

fn pt(m: &Matrix, x: f64, y: f64) -> Point {
    let (x, y) = m.apply(x, y);
    Point::new(x, y)
}

fn finite(p: &Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// The path being built.
#[derive(Default)]
struct PathBuf {
    segs: Vec<SnapSegment>,
    start: Option<Point>,
    current: Option<Point>,
}

impl PathBuf {
    fn line(&mut self, a: Point, b: Point) {
        if finite(&a) && finite(&b) && (a.x != b.x || a.y != b.y) {
            self.segs.push(SnapSegment::new(a, b));
        }
    }

    /// A cubic Bézier flattened into pieces; only its real ends snap.
    fn curve(&mut self, p: [Point; 4]) {
        if !p.iter().all(finite) {
            return;
        }
        let [a, b, c, d] = p;
        let len = a.dist(b) + b.dist(c) + c.dist(d);
        let n = ((len / 4.0).ceil() as usize).clamp(2, 24);
        let at = |t: f64| {
            let u = 1.0 - t;
            let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            Point::new(
                k0 * a.x + k1 * b.x + k2 * c.x + k3 * d.x,
                k0 * a.y + k1 * b.y + k2 * c.y + k3 * d.y,
            )
        };
        let mut prev = a;
        for i in 1..=n {
            let q = at(i as f64 / n as f64);
            let mut flags = 0;
            if i == 1 {
                flags |= SnapSegment::END_A;
            }
            if i == n {
                flags |= SnapSegment::END_B;
            }
            if prev.x != q.x || prev.y != q.y {
                self.segs.push(SnapSegment::with_flags(prev, q, flags));
            }
            prev = q;
        }
    }
}

struct Walker<'a> {
    doc: &'a CosDoc,
    out: Linework,
    seen: HashSet<ObjRef>,
    bytes: usize,
    streams: usize,
}

impl Walker<'_> {
    fn full(&self, pending: usize) -> bool {
        self.out.segments.len().saturating_add(pending) >= MAX_SEGMENTS
    }

    fn walk(&mut self, object: &Object, resources: &Dict, ctm: Matrix, depth: usize) {
        if depth >= MAX_DEPTH || self.streams >= MAX_STREAMS || self.out.truncated {
            self.out.truncated = true;
            return;
        }
        let o = self.doc.resolve(object);
        if let Some(parts) = o.as_array() {
            // A contents array is one stream: paths and q/Q may span its parts.
            let mut joined = Vec::new();
            for part in parts {
                let s = self.doc.resolve(part);
                if let Object::Stream(s) = s.as_ref() {
                    let room = MAX_BYTES.saturating_sub(self.bytes).saturating_sub(joined.len());
                    if room == 0 {
                        self.out.truncated = true;
                        break;
                    }
                    match s.decoded_within(room) {
                        Ok(data) => {
                            joined.extend(data);
                            joined.push(b'\n');
                        }
                        Err(_) => self.out.unreadable = self.out.unreadable.saturating_add(1),
                    }
                }
            }
            self.ops(&joined, resources, ctm, depth);
            return;
        }
        let Object::Stream(stream) = o.as_ref() else { return };
        let room = MAX_BYTES.saturating_sub(self.bytes);
        if room == 0 {
            self.out.truncated = true;
            return;
        }
        match stream.decoded_within(room) {
            Ok(data) => self.ops(&data, resources, ctm, depth),
            Err(_) => self.out.unreadable = self.out.unreadable.saturating_add(1),
        }
    }

    fn ops(&mut self, data: &[u8], resources: &Dict, initial: Matrix, depth: usize) {
        self.bytes = self.bytes.saturating_add(data.len());
        self.streams = self.streams.saturating_add(1);
        let mut ctm = initial;
        let mut stack: Vec<Matrix> = Vec::new();
        let mut path = PathBuf::default();
        for op in parse(data).ops {
            if self.full(path.segs.len()) {
                self.out.truncated = true;
                break;
            }
            match op.op.as_slice() {
                b"q" => {
                    if stack.len() >= MAX_SAVES {
                        self.out.truncated = true;
                        break;
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
                        path.start = Some(p);
                        path.current = Some(p);
                    }
                }
                b"l" => {
                    if let (Some(a), Some([x, y])) = (path.current, op.nums::<2>()) {
                        let b = pt(&ctm, x, y);
                        path.line(a, b);
                        path.current = Some(b);
                    }
                }
                b"c" | b"v" | b"y" => {
                    let Some(a) = path.current else { continue };
                    let curve = match op.op.as_slice() {
                        b"c" => op
                            .nums::<6>()
                            .map(|[x1, y1, x2, y2, x3, y3]| [a, pt(&ctm, x1, y1), pt(&ctm, x2, y2), pt(&ctm, x3, y3)]),
                        b"v" => op
                            .nums::<4>()
                            .map(|[x2, y2, x3, y3]| [a, a, pt(&ctm, x2, y2), pt(&ctm, x3, y3)]),
                        _ => op.nums::<4>().map(|[x1, y1, x3, y3]| {
                            let d = pt(&ctm, x3, y3);
                            [a, pt(&ctm, x1, y1), d, d]
                        }),
                    };
                    if let Some(c) = curve {
                        path.curve(c);
                        path.current = Some(c[3]);
                    }
                }
                b"h" => {
                    if let (Some(a), Some(b)) = (path.current, path.start) {
                        path.line(a, b);
                        path.current = Some(b);
                    }
                }
                b"re" => {
                    if let Some([x, y, w, h]) = op.nums::<4>() {
                        let c = [
                            pt(&ctm, x, y),
                            pt(&ctm, x + w, y),
                            pt(&ctm, x + w, y + h),
                            pt(&ctm, x, y + h),
                        ];
                        for i in 0..4 {
                            path.line(c[i], c[(i + 1) % 4]);
                        }
                        path.start = Some(c[0]);
                        path.current = Some(c[0]);
                    }
                }
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                    if matches!(op.op.as_slice(), b"s" | b"b" | b"b*")
                        && let (Some(a), Some(b)) = (path.current, path.start)
                    {
                        path.line(a, b);
                    }
                    let room = MAX_SEGMENTS.saturating_sub(self.out.segments.len());
                    if path.segs.len() > room {
                        path.segs.truncate(room);
                        self.out.truncated = true;
                    }
                    self.out.segments.append(&mut path.segs);
                    path = PathBuf::default();
                }
                b"n" => path = PathBuf::default(),
                b"Do" => {
                    if let Some(name) = op.name(0) {
                        self.form(name, resources, ctm, depth);
                    }
                }
                _ => {}
            }
        }
    }

    /// Draw Form XObject `name` (images and unknown names are ignored).
    fn form(&mut self, name: &[u8], resources: &Dict, ctm: Matrix, depth: usize) {
        let xobjects = resources.get(b"XObject").map(|o| self.doc.resolve(o));
        let Some(entry) = xobjects.as_ref().and_then(|o| o.as_dict()).and_then(|d| d.get(name)) else {
            return;
        };
        let r = entry.as_ref();
        if r.is_some_and(|r| self.seen.contains(&r)) {
            return;
        }
        let form = self.doc.resolve(entry);
        let Object::Stream(s) = form.as_ref() else { return };
        if s.dict.name(b"Subtype") != Some(b"Form") {
            return;
        }
        let m = s
            .dict
            .get(b"Matrix")
            .map(|m| self.doc.resolve(m))
            .and_then(|o| o.as_array().and_then(|a| Matrix::from_operands(a)))
            .unwrap_or_default()
            .then(&ctm);
        let own = s.dict.get(b"Resources").map(|o| self.doc.resolve(o));
        let res = own.as_ref().and_then(|o| o.as_dict()).unwrap_or(resources).clone();
        if let Some(r) = r {
            self.seen.insert(r);
        }
        self.walk(entry, &res, m, depth + 1);
        if let Some(r) = r {
            self.seen.remove(&r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_pages_have_linework() {
        let bytes = Arc::new(crate::synthetic::sample_pdf());
        let doc = CosDoc::open(bytes.clone()).unwrap();
        let pages = page_refs(&doc);
        assert_eq!(pages.len(), 2);
        let lw = page_linework(&doc, pages[0]);
        assert!(lw.segments.len() > 10, "{} segments", lw.segments.len());
        assert!(!lw.truncated);
        // Everything is on the page.
        for s in &lw.segments {
            for p in [s.a, s.b] {
                assert!(p.x > -1.0 && p.x < 1300.0 && p.y > -1.0 && p.y < 1300.0, "{p:?}");
            }
        }
        assert_eq!(linework_of(bytes.clone(), 0).segments.len(), lw.segments.len());
        assert!(linework_of(bytes, 9).segments.is_empty());
    }

    #[test]
    fn curves_snap_only_at_their_ends() {
        let mut p = PathBuf::default();
        p.curve([
            Point::new(0.0, 0.0),
            Point::new(0.0, 50.0),
            Point::new(50.0, 50.0),
            Point::new(50.0, 0.0),
        ]);
        assert!(p.segs.len() >= 2);
        let first = p.segs.first().unwrap();
        let last = p.segs.last().unwrap();
        assert_eq!(first.flags, SnapSegment::END_A);
        assert_eq!(last.flags, SnapSegment::END_B);
        assert!(p.segs.iter().all(|s| s.flags & SnapSegment::MID == 0));
    }

    #[test]
    fn garbage_content_is_harmless() {
        let mut w = Walker {
            doc: &CosDoc::new_empty(),
            out: Linework::default(),
            seen: HashSet::new(),
            bytes: 0,
            streams: 0,
        };
        w.ops(
            b"q q 1 0 0 1 cm 0 0 m 10 10 l (unclosed BI ] >> l l re S Q Q Q f",
            &Dict::new(),
            Matrix::default(),
            0,
        );
        assert!(w.out.segments.len() <= 1);
    }
}
