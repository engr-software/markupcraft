//! Page labels (`/PageLabels`, ISO 32000-1 §12.4.2): read per page, written back as runs.
//!
//! Page operations keep each page's label: the label of every output page is taken from where
//! the page came from, then consecutive pages that continue the same numbering are merged
//! into one `/Nums` entry.

use std::collections::HashSet;

use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString};
use markupcraft_revu::pdf;

/// Numbering style (`/S`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    /// `D` 1, 2, 3
    Decimal,
    /// `R` I, II, III
    UpperRoman,
    /// `r` i, ii, iii
    LowerRoman,
    /// `A` A..Z, AA..ZZ
    UpperAlpha,
    /// `a`
    LowerAlpha,
}

impl LabelStyle {
    pub fn code(self) -> &'static str {
        match self {
            LabelStyle::Decimal => "D",
            LabelStyle::UpperRoman => "R",
            LabelStyle::LowerRoman => "r",
            LabelStyle::UpperAlpha => "A",
            LabelStyle::LowerAlpha => "a",
        }
    }

    pub fn from_code(s: &str) -> Option<LabelStyle> {
        Some(match s {
            "D" => LabelStyle::Decimal,
            "R" => LabelStyle::UpperRoman,
            "r" => LabelStyle::LowerRoman,
            "A" => LabelStyle::UpperAlpha,
            "a" => LabelStyle::LowerAlpha,
            _ => return None,
        })
    }
}

/// One page's label: prefix plus a number in a style (no style = the prefix alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSpec {
    pub style: Option<LabelStyle>,
    pub prefix: String,
    /// The number this page shows (`/St` adjusted to the page).
    pub number: i64,
}

impl LabelSpec {
    pub fn decimal(number: i64) -> Self {
        Self {
            style: Some(LabelStyle::Decimal),
            prefix: String::new(),
            number,
        }
    }

    /// The label as a viewer shows it.
    pub fn text(&self) -> String {
        let n = self.number.max(1);
        let num = match self.style {
            None => String::new(),
            Some(LabelStyle::Decimal) => n.to_string(),
            Some(LabelStyle::UpperRoman) => roman(n),
            Some(LabelStyle::LowerRoman) => roman(n).to_lowercase(),
            Some(LabelStyle::UpperAlpha) => alpha(n),
            Some(LabelStyle::LowerAlpha) => alpha(n).to_lowercase(),
        };
        format!("{}{num}", self.prefix)
    }

    /// Does `self` on page `idx` continue the run that `prev` started on page `prev_idx`?
    fn continues(&self, idx: usize, prev: &LabelSpec, prev_idx: usize) -> bool {
        if self.style != prev.style || self.prefix != prev.prefix {
            return false;
        }
        if self.style.is_none() {
            return true;
        }
        let gap = idx.saturating_sub(prev_idx) as i64;
        prev.number.checked_add(gap) == Some(self.number)
    }

    fn to_dict(&self) -> Dict {
        let mut d = Dict::new();
        if let Some(s) = self.style {
            d.set(b"S".to_vec(), Object::name(s.code()));
        }
        if !self.prefix.is_empty() {
            d.set(b"P".to_vec(), Object::String(PdfString::text(&self.prefix)));
        }
        if self.number != 1 {
            d.set(b"St".to_vec(), Object::Int(self.number));
        }
        d
    }
}

fn roman(mut n: i64) -> String {
    if n > 100_000 {
        return n.to_string();
    }
    const T: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for (v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// 1 = A, 26 = Z, 27 = AA, 28 = BB (ISO 32000-1 Table 159).
fn alpha(n: i64) -> String {
    let n = (n - 1).clamp(0, 26 * 100);
    let letter = char::from(b'A' + (n % 26) as u8);
    std::iter::repeat_n(letter, (n / 26 + 1) as usize).collect()
}

const MAX_DEPTH: usize = 32;
const MAX_ENTRIES: usize = 100_000;

fn collect(cos: &CosDoc, node: &Object, out: &mut Vec<(i64, Dict)>, seen: &mut HashSet<ObjRef>, depth: usize) {
    if depth > MAX_DEPTH || out.len() > MAX_ENTRIES {
        return;
    }
    if let Some(r) = node.as_ref()
        && !seen.insert(r)
    {
        return;
    }
    let node = cos.resolve(node);
    let Some(d) = node.as_dict() else { return };
    if let Some(nums) = d.get(b"Nums").map(|o| cos.resolve(o))
        && let Some(a) = nums.as_array()
    {
        for pair in a.as_chunks::<2>().0 {
            let [k, v] = pair;
            let (Some(k), Some(v)) = (cos.resolve(k).as_int(), cos.dict(v)) else {
                continue;
            };
            out.push((k, v));
        }
    }
    if let Some(kids) = d.get(b"Kids").map(|o| cos.resolve(o))
        && let Some(a) = kids.as_array()
    {
        for k in a {
            collect(cos, k, out, seen, depth + 1);
        }
    }
}

/// Each page's label, or `None` when the document has no `/PageLabels`. A page before the
/// first entry has no label.
pub fn read(cos: &CosDoc, pages: usize) -> Option<Vec<Option<LabelSpec>>> {
    let root = cos.root()?;
    let cat = cos.get(root);
    let tree = cat.as_dict()?.get(b"PageLabels")?.clone();
    let mut entries = Vec::new();
    collect(cos, &tree, &mut entries, &mut HashSet::new(), 0);
    entries.sort_by_key(|(k, _)| *k);
    let mut out = Vec::with_capacity(pages);
    for i in 0..pages {
        let idx = i as i64;
        let at = entries.partition_point(|(k, _)| *k <= idx);
        let spec = at.checked_sub(1).and_then(|j| entries.get(j)).map(|(k, d)| {
            let start = d.get(b"St").and_then(|o| cos.resolve(o).as_int()).unwrap_or(1);
            LabelSpec {
                style: LabelStyle::from_code(&pdf::name(d.get(b"S"))),
                prefix: pdf::text(d.get(b"P").map(|o| cos.resolve(o)).as_deref()),
                number: start.saturating_add(idx.saturating_sub(*k)),
            }
        });
        out.push(spec);
    }
    Some(out)
}

/// Write one label per page as `/PageLabels`, merging runs.
pub fn write(cos: &mut CosDoc, specs: &[LabelSpec]) {
    let Some(root) = cos.root() else { return };
    let mut nums = Vec::new();
    let mut prev: Option<(usize, &LabelSpec)> = None;
    for (i, s) in specs.iter().enumerate() {
        if let Some((pi, p)) = prev
            && s.continues(i, p, pi)
        {
            continue;
        }
        nums.push(Object::Int(i as i64));
        nums.push(Object::Dict(s.to_dict()));
        prev = Some((i, s));
    }
    let mut tree = Dict::new();
    tree.set(b"Nums".to_vec(), Object::Array(nums));
    let _ = cos.update_dict(root, |d| d.set(b"PageLabels".to_vec(), Object::Dict(tree)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles() {
        let s = |style, prefix: &str, number| LabelSpec {
            style,
            prefix: prefix.into(),
            number,
        };
        assert_eq!(s(Some(LabelStyle::UpperRoman), "", 14).text(), "XIV");
        assert_eq!(s(Some(LabelStyle::LowerRoman), "", 4).text(), "iv");
        assert_eq!(s(Some(LabelStyle::UpperAlpha), "", 28).text(), "BB");
        assert_eq!(s(Some(LabelStyle::Decimal), "A-", 3).text(), "A-3");
        assert_eq!(s(None, "Cover", 1).text(), "Cover");
    }
}
