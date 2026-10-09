//! Bookmarks from the source application's structure: what Revu's Office and CAD plugins do
//! when they create a PDF (headings, sheet tabs, slides and layouts become bookmarks), done
//! here from the PDF itself.
//!
//! - A tagged PDF (ISO 32000-1 §14.7, `/StructTreeRoot`): every heading element (`H`, `H1` to
//!   `H6`, `Title`) becomes a bookmark nested by its level, titled by its `/T`, `/ActualText`,
//!   `/Alt` or the text of its marked content, going to its page.
//! - Otherwise, the headings are found from the text: lines set clearly larger than the body
//!   text (at least 1.3 times the most common line height), the largest size being level 1, the
//!   next level 2 and so on (three levels). A slide or a sheet's title is its largest line.

use std::collections::{BTreeMap, HashMap, HashSet};

use markupcraft_revu::cos::{Document as CosDoc, ObjRef, Object};

use crate::docutil::{page_objs, text_of};
use crate::{Result, Session, invalid};

/// Most bookmarks made.
const MAX_MADE: usize = 5_000;
const MAX_DEPTH: usize = 64;
const MAX_ELEMENTS: usize = 200_000;

/// Where the bookmarks came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// The tagged structure tree.
    Structure,
    /// Heading lines found from the text sizes.
    Headings,
}

impl SourceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Headings => "headings",
        }
    }
}

/// A heading: level (1 = top), title, page.
pub type Heading = (usize, String, usize);

fn heading_level(tag: &[u8], depth: usize) -> Option<usize> {
    match tag {
        b"Title" => Some(1),
        b"H" => Some(depth.clamp(1, 6)),
        [b'H', d] if (b'1'..=b'6').contains(d) => Some(usize::from(d - b'0')),
        _ => None,
    }
}

/// The text of each marked-content id on a page.
fn mcid_text(cos: &CosDoc, page: ObjRef) -> HashMap<i64, String> {
    let mut out: HashMap<i64, String> = HashMap::new();
    let Some(pd) = cos.dict(&Object::Ref(page)) else {
        return out;
    };
    let Ok(data) = crate::overlay::page_content(cos, &pd) else {
        return out;
    };
    let parsed = pdfcraft_content::parse(&data);
    let mut stack: Vec<Option<i64>> = Vec::new();
    let push_str = |out: &mut HashMap<i64, String>, id: i64, o: &Object| {
        if let Some(s) = o.as_string() {
            let t: String = s
                .bytes
                .iter()
                .map(|b| char::from(*b))
                .filter(|c| !c.is_control())
                .collect();
            out.entry(id).or_default().push_str(&t);
        }
    };
    for op in parsed.ops.iter().take(2_000_000) {
        if op.is("BDC") {
            let id = op
                .operands
                .get(1)
                .and_then(|o| cos.dict(o))
                .and_then(|d| d.int(b"MCID"));
            stack.push(id);
        } else if op.is("BMC") {
            stack.push(None);
        } else if op.is("EMC") {
            stack.pop();
        } else if let Some(id) = stack.iter().rev().find_map(|x| *x) {
            if op.is("Tj") || op.is("'") || op.is("\"") {
                if let Some(o) = op.operands.last() {
                    push_str(&mut out, id, o);
                }
            } else if op.is("TJ")
                && let Some(arr) = op.operands.first().and_then(Object::as_array)
            {
                for o in arr {
                    push_str(&mut out, id, o);
                }
            }
        }
    }
    out
}

struct Walk<'a> {
    cos: &'a CosDoc,
    pages: Vec<ObjRef>,
    texts: HashMap<ObjRef, HashMap<i64, String>>,
    seen: HashSet<ObjRef>,
    visited: usize,
    out: Vec<Heading>,
}

impl Walk<'_> {
    fn page_index(&self, r: Option<ObjRef>) -> Option<usize> {
        r.and_then(|r| self.pages.iter().position(|p| *p == r))
    }

    /// The text under an element: its marked content, depth first.
    fn element_text(&mut self, d: &markupcraft_revu::cos::Dict, pg: Option<ObjRef>, depth: usize) -> String {
        if depth > MAX_DEPTH {
            return String::new();
        }
        let pg = d.reference(b"Pg").or(pg);
        let kids = match d.get(b"K").map(|k| self.cos.resolve(k)).as_deref() {
            Some(Object::Array(a)) => a.clone(),
            Some(o) => vec![o.clone()],
            None => Vec::new(),
        };
        let mut s = String::new();
        for k in kids.iter().take(10_000) {
            match k {
                Object::Int(id) => {
                    if let Some(p) = pg {
                        let cos = self.cos;
                        let t = self.texts.entry(p).or_insert_with(|| mcid_text(cos, p));
                        if let Some(x) = t.get(id) {
                            s.push_str(x);
                        }
                    }
                }
                other => {
                    if let Some(kd) = self.cos.dict(other) {
                        if let Some(id) = kd.int(b"MCID") {
                            let p = kd.reference(b"Pg").or(pg);
                            if let Some(p) = p {
                                let cos = self.cos;
                                let t = self.texts.entry(p).or_insert_with(|| mcid_text(cos, p));
                                if let Some(x) = t.get(&id) {
                                    s.push_str(x);
                                }
                            }
                        } else {
                            s.push_str(&self.element_text(&kd, pg, depth + 1));
                        }
                    }
                }
            }
            if s.len() > 1_000 {
                break;
            }
        }
        s
    }

    /// The first page an element's content is on.
    fn element_page(&self, d: &markupcraft_revu::cos::Dict, pg: Option<ObjRef>, depth: usize) -> Option<usize> {
        if let Some(p) = self.page_index(d.reference(b"Pg")) {
            return Some(p);
        }
        if depth > MAX_DEPTH {
            return self.page_index(pg);
        }
        let kids = match d.get(b"K").map(|k| self.cos.resolve(k)).as_deref() {
            Some(Object::Array(a)) => a.clone(),
            Some(o) => vec![o.clone()],
            None => Vec::new(),
        };
        for k in kids.iter().take(1_000) {
            if let Some(kd) = self.cos.dict(k)
                && let Some(p) = self.element_page(&kd, pg, depth + 1)
            {
                return Some(p);
            }
        }
        self.page_index(pg)
    }

    fn walk(&mut self, node: &Object, pg: Option<ObjRef>, depth: usize, sect: usize) {
        if depth > MAX_DEPTH || self.visited > MAX_ELEMENTS || self.out.len() >= MAX_MADE {
            return;
        }
        self.visited += 1;
        if let Object::Ref(r) = node
            && !self.seen.insert(*r)
        {
            return;
        }
        let resolved = self.cos.resolve(node);
        if let Object::Array(a) = &*resolved {
            for k in a.iter().take(100_000) {
                self.walk(k, pg, depth + 1, sect);
            }
            return;
        }
        let Some(d) = self.cos.dict(node) else { return };
        let pg = d.reference(b"Pg").or(pg);
        let tag = d.name(b"S").map(<[u8]>::to_vec).unwrap_or_default();
        if let Some(level) = heading_level(&tag, sect) {
            let mut title = text_of(self.cos, d.get(b"T"));
            if title.trim().is_empty() {
                title = text_of(self.cos, d.get(b"ActualText"));
            }
            if title.trim().is_empty() {
                title = text_of(self.cos, d.get(b"Alt"));
            }
            if title.trim().is_empty() {
                title = self.element_text(&d, pg, 0);
            }
            let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ");
            if !title.is_empty()
                && let Some(page) = self.element_page(&d, pg, 0)
            {
                self.out.push((level, title.chars().take(200).collect(), page));
            }
            return;
        }
        let sect = if tag == b"Sect" || tag == b"Part" {
            sect + 1
        } else {
            sect
        };
        if let Some(k) = d.get(b"K") {
            let k = k.clone();
            self.walk(&k, pg, depth + 1, sect);
        }
    }
}

/// The headings of a tagged PDF's structure tree, in document order.
pub fn structure_headings(cos: &CosDoc) -> Vec<Heading> {
    let Some(root) = cos.root().and_then(|r| cos.dict(&Object::Ref(r))) else {
        return Vec::new();
    };
    let Some(tree) = root.get(b"StructTreeRoot").and_then(|t| cos.dict(t)) else {
        return Vec::new();
    };
    let Some(k) = tree.get(b"K").cloned() else {
        return Vec::new();
    };
    let mut w = Walk {
        cos,
        pages: page_objs(cos).unwrap_or_default(),
        texts: HashMap::new(),
        seen: HashSet::new(),
        visited: 0,
        out: Vec::new(),
    };
    w.walk(&k, None, 0, 0);
    w.out
}

/// Headings found from text sizes: `lines` per page are (height, text) in reading order.
pub fn size_headings(lines: &[Vec<(f64, String)>]) -> Vec<Heading> {
    // The most common line height (to 0.5 pt) is the body text.
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for page in lines {
        for (h, t) in page {
            *counts.entry((h * 2.0).round() as i64).or_default() += t.chars().count().max(1);
        }
    }
    let Some(body) = counts.iter().max_by_key(|(_, n)| **n).map(|(h, _)| *h as f64 / 2.0) else {
        return Vec::new();
    };
    let is_heading =
        |h: f64, t: &str| h >= body * 1.3 && t.split_whitespace().count() <= 14 && t.chars().any(char::is_alphanumeric);
    let mut sizes: Vec<i64> = lines
        .iter()
        .flatten()
        .filter(|(h, t)| is_heading(*h, t))
        .map(|(h, _)| (h * 2.0).round() as i64)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    let level = |h: f64| -> usize {
        let k = (h * 2.0).round() as i64;
        sizes.iter().position(|s| *s == k).map_or(3, |i| (i + 1).min(3))
    };
    let mut out = Vec::new();
    for (p, page) in lines.iter().enumerate() {
        for (h, t) in page {
            if is_heading(*h, t) && out.len() < MAX_MADE {
                out.push((level(*h), t.trim().chars().take(200).collect(), p));
            }
        }
    }
    out
}

/// A page's words joined into lines: (line height, text), top to bottom.
fn page_lines(words: &[crate::raster::PageWord]) -> Vec<(f64, String)> {
    let mut ws: Vec<&crate::raster::PageWord> = words.iter().take(50_000).collect();
    ws.sort_by(|a, b| {
        let (ca, cb) = ((a.rect.y0 + a.rect.y1) / 2.0, (b.rect.y0 + b.rect.y1) / 2.0);
        cb.total_cmp(&ca).then(a.rect.x0.total_cmp(&b.rect.x0))
    });
    let mut lines: Vec<(f64, f64, Vec<&crate::raster::PageWord>)> = Vec::new();
    for w in ws {
        let c = (w.rect.y0 + w.rect.y1) / 2.0;
        let h = (w.rect.y1 - w.rect.y0).abs();
        match lines.last_mut() {
            Some((lc, lh, v)) if (c - *lc).abs() <= lh.max(h) * 0.5 => {
                *lh = lh.max(h);
                v.push(w);
            }
            _ => lines.push((c, h, vec![w])),
        }
    }
    lines
        .into_iter()
        .map(|(_, h, mut v)| {
            v.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
            (h, v.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "))
        })
        .collect()
}

impl Session {
    /// The headings bookmarks would be made from, and where they come from.
    pub fn source_headings(&self) -> Result<(Vec<Heading>, SourceKind)> {
        let cos = &self.file.cos;
        let tagged = structure_headings(cos);
        if !tagged.is_empty() {
            return Ok((tagged, SourceKind::Structure));
        }
        let mut lines = Vec::with_capacity(self.page_count());
        for p in 0..self.page_count().min(2_000) {
            lines.push(page_lines(&self.page_words(p)?));
        }
        Ok((size_headings(&lines), SourceKind::Headings))
    }

    /// Make bookmarks from the source structure (see the module notes); `replace` clears the
    /// current bookmarks first. One undo step. Returns the count and the source.
    pub fn bookmarks_from_source(&mut self, replace: bool) -> Result<(usize, SourceKind)> {
        let (heads, kind) = self.source_headings()?;
        if heads.is_empty() {
            return Err(invalid(
                "no headings found: the PDF is not tagged and has no text set larger than its body text",
            ));
        }
        self.set_merge_key(Some("bookmarks-from-source"));
        let r = (|| -> Result<usize> {
            if replace && !self.bookmarks().is_empty() {
                self.clear_bookmarks()?;
            }
            let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
            let mut made = 0;
            for (level, title, page) in heads {
                while stack.last().is_some_and(|(l, _)| *l >= level) {
                    stack.pop();
                }
                let parent = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
                let path = self.add_bookmark(&parent, None, &title, page)?;
                stack.push((level, path));
                made += 1;
            }
            Ok(made)
        })();
        self.set_merge_key(None);
        self.seal();
        r.map(|n| (n, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    #[test]
    fn headings_from_text_sizes_nest_by_size() {
        let page = |t: &str, sub: &str| {
            format!(
                "{}{}{}{}",
                text(72.0, 720.0, 24.0, t),
                text(72.0, 680.0, 16.0, sub),
                text(
                    72.0,
                    650.0,
                    10.0,
                    "Body text runs here in the usual size of the document."
                ),
                text(72.0, 635.0, 10.0, "More body text so the body size is the common one.")
            )
        };
        let bytes = pdf(&[
            SyntheticPage::new(612.0, 792.0, page("Mechanical", "Ductwork")),
            SyntheticPage::new(612.0, 792.0, page("Electrical", "Lighting")),
        ]);
        let mut s = Session::from_bytes(bytes, std::env::temp_dir().join("srcbm.pdf")).unwrap();
        let (n, kind) = s.bookmarks_from_source(true).unwrap();
        assert_eq!((n, kind), (4, SourceKind::Headings));
        let b = s.bookmarks();
        let titles: Vec<(usize, &str, Option<usize>)> =
            b.iter().map(|x| (x.path.len(), x.title.as_str(), x.page)).collect();
        assert_eq!(
            titles,
            [
                (1, "Mechanical", Some(0)),
                (2, "Ductwork", Some(0)),
                (1, "Electrical", Some(1)),
                (2, "Lighting", Some(1)),
            ]
        );
        s.undo().unwrap();
        assert!(s.bookmarks().is_empty());
    }

    #[test]
    fn a_tagged_pdf_gives_its_heading_elements() {
        // A hand-made tagged PDF: H1 and H2 elements over marked content on two pages.
        let c1 = "/H1 <</MCID 0>> BDC BT /F1 20 Tf 72 700 Td (Overview) Tj ET EMC\n/P <</MCID 1>> BDC BT /F1 10 Tf 72 650 Td (Body) Tj ET EMC\n";
        let c2 = "/H2 <</MCID 0>> BDC BT /F1 14 Tf 72 700 Td [(Det) 20 (ails)] TJ ET EMC\n";
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 8 0 R /MarkInfo << /Marked true >> >>".to_string(),
            "<< /Type /Pages /Kids [4 0 R 6 0 R] /Count 2 >>".into(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R /StructParents 0 >>".into(),
            format!("<< /Length {} >>\nstream\n{c1}\nendstream", c1.len() + 1),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 7 0 R /StructParents 1 >>".into(),
            format!("<< /Length {} >>\nstream\n{c2}\nendstream", c2.len() + 1),
            "<< /Type /StructTreeRoot /K 9 0 R >>".into(),
            "<< /Type /StructElem /S /Document /P 8 0 R /K [10 0 R 11 0 R 12 0 R] >>".into(),
            "<< /Type /StructElem /S /H1 /P 9 0 R /Pg 4 0 R /K 0 >>".into(),
            "<< /Type /StructElem /S /P /P 9 0 R /Pg 4 0 R /K 1 >>".into(),
            "<< /Type /StructElem /S /H2 /P 9 0 R /Pg 6 0 R /K [0] >>".into(),
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offs = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let x = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offs {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        let mut s = Session::from_bytes(out, std::env::temp_dir().join("tagged.pdf")).unwrap();
        let (n, kind) = s.bookmarks_from_source(false).unwrap();
        assert_eq!((n, kind), (2, SourceKind::Structure));
        let b = s.bookmarks();
        assert_eq!(
            b.first().map(|x| (x.title.as_str(), x.page)),
            Some(("Overview", Some(0)))
        );
        assert_eq!(
            b.get(1).map(|x| (x.path.len(), x.title.as_str(), x.page)),
            Some((2, "Details", Some(1)))
        );
    }
}
