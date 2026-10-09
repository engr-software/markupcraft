//! Markups List data stored beside the geometry: custom columns (`/PCColumns`), per-markup
//! column values (`/PCColumnData`), review status and checkmark (`/IRT` state replies),
//! replies, groups (`/RT /Group`), page labels (`/PageLabels`) and the lock flag.
//!
//! Storage:
//!
//! ```text
//! Catalog    /PCColumns [ << /Id (unitcost) /Name (Unit Cost) /Type /Currency /Decimals 2
//!                           /Symbol ($) /Default () /Total true /Formula (Length * UnitCost)
//!                           /AllowCustom false /Items [ << /Text (Duct) /Subject (Supply) /Value 12.5 >> ] >> ]
//! Annotation /PCColumnData << /unitcost (12.50) /phase (Rough-in) >>        values as text
//! Status     a /Text reply: /IRT parent /StateModel (Review) /State (Accepted)   ISO 32000-1 12.5.6.4
//! Checkmark  a /Text reply: /IRT parent /StateModel (Marked) /State (Marked | Unmarked)
//! Replies    /Text annotations with /IRT parent and /Contents (no state)
//! Groups     every member but the leader: /IRT leader /RT /Group             ISO 32000-1 12.5.6.2
//! Labels     catalog /PageLabels number tree of << /S /P /St >>             ISO 32000-1 12.4.2
//! Lock       /F bit 8 (Locked), written by write::write_annot                ISO 32000-1 12.5.3
//! ```
//!
//! Revu's own custom column storage (`/BSIColumnData`) is not documented publicly; it is kept
//! untouched on every annotation (unknown keys are never removed).

use std::collections::{BTreeMap, HashMap, HashSet};

use markupcraft_model::{ChoiceItem, ColumnType, CustomColumn, Document, Markup, Reply, make_column_id};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object};

use crate::pdf::{self, n, real, s};
use crate::{new_markup_id, pdf_date_now};

const MAX_TREE_DEPTH: usize = 20;
const MAX_LABEL_RANGES: usize = 100_000;
const MAX_REPLY_HOPS: usize = 8;

/// Text of a string, name, number or boolean.
fn value_text(o: Option<&Object>) -> String {
    match o {
        Some(Object::Int(i)) => i.to_string(),
        Some(Object::Real(r)) => markupcraft_measure::fmt_g(*r, 10),
        Some(Object::Bool(b)) => b.to_string(),
        other => pdf::text(other),
    }
}

/// Author written on status / checkmark replies created here.
pub fn review_author() -> String {
    ["MARKUPCRAFT_USER", "USERNAME", "USER"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "MarkupCraft".into())
}

// ---- reading ---------------------------------------------------------------------------

/// Per-annotation extras, called once per annotation while loading.
pub fn read_markup(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    m.irt = a.reference(b"IRT").map(|r| (r.num, r.generation));
    m.group = String::new();
    if pdf::name(a.get(b"RT")) == "Group" {
        // A group member is not a reply: /IRT names the group's leader.
        if let Some(r) = a.reference(b"IRT")
            && let Some(lead) = cos.get(r).as_dict()
        {
            m.group = pdf::text(lead.get(b"NM"));
        }
        m.irt = None;
    }
    let cd = cos.resolve(a.get(b"PCColumnData").unwrap_or(&Object::Null));
    if let Some(cd) = cd.as_dict() {
        for (k, v) in cd.iter() {
            let key = String::from_utf8_lossy(k).into_owned();
            if !key.is_empty() {
                m.column_data.insert(key, value_text(Some(&cos.resolve(v))));
            }
        }
    }
}

/// Document-level extras, called after every page was loaded: page labels, custom columns,
/// replies folded into their parents, groups completed.
pub fn read_document(cos: &CosDoc, doc: &mut Document) {
    let labels = page_labels(cos, doc.pages.len());
    for (p, l) in doc.pages.iter_mut().zip(labels) {
        p.label = l;
    }
    doc.page_labels_changed = false;
    doc.columns = read_columns(cos);
    doc.columns_changed = false;
    fold_replies(cos, doc);
    finish_groups(doc);
}

fn catalog(cos: &CosDoc) -> Option<(ObjRef, Dict)> {
    let root = cos.root()?;
    let d = cos.get(root).as_dict()?.clone();
    Some((root, d))
}

fn read_columns(cos: &CosDoc) -> Vec<CustomColumn> {
    let mut out: Vec<CustomColumn> = Vec::new();
    let Some((_, cat)) = catalog(cos) else { return out };
    let cols = cos.resolve(cat.get(b"PCColumns").unwrap_or(&Object::Null));
    let Some(arr) = cols.as_array() else { return out };
    for d in arr {
        let d = cos.resolve(d);
        let Some(d) = d.as_dict() else { continue };
        let mut c = CustomColumn {
            id: value_text(d.get(b"Id")),
            name: value_text(d.get(b"Name")),
            ..Default::default()
        };
        if c.id.is_empty() || out.iter().any(|o| o.id == c.id) {
            c.id = make_column_id(&c.name, &out);
        }
        if let Some(t) = ColumnType::from_name(&value_text(d.get(b"Type"))) {
            c.kind = t;
        }
        if let Some(v) = d.int(b"Decimals") {
            c.decimals = v.clamp(0, 10) as i32;
        }
        if d.contains(b"Symbol") {
            c.symbol = value_text(d.get(b"Symbol"));
        }
        c.default_value = value_text(d.get(b"Default"));
        if let Some(b) = pdf::boolean(d.get(b"Total")) {
            c.total = b;
        }
        c.formula = value_text(d.get(b"Formula"));
        if let Some(b) = pdf::boolean(d.get(b"AllowCustom")) {
            c.allow_custom = b;
        }
        let items = cos.resolve(d.get(b"Items").unwrap_or(&Object::Null));
        if let Some(items) = items.as_array() {
            for it in items {
                let it = cos.resolve(it);
                let Some(it) = it.as_dict() else { continue };
                c.items.push(ChoiceItem {
                    text: value_text(it.get(b"Text")),
                    subject: value_text(it.get(b"Subject")),
                    value: pdf::num(it.get(b"Value")),
                });
            }
        }
        out.push(c);
    }
    out
}

fn collect_nums(cos: &CosDoc, node: &Object, out: &mut Vec<(i64, Object)>, depth: usize) {
    if depth > MAX_TREE_DEPTH || out.len() > MAX_LABEL_RANGES {
        return;
    }
    let node = cos.resolve(node);
    let Some(d) = node.as_dict() else { return };
    let nums = cos.resolve(d.get(b"Nums").unwrap_or(&Object::Null));
    if let Some(nums) = nums.as_array() {
        for pair in nums.as_chunks::<2>().0 {
            if let [Object::Int(k), v] = pair {
                out.push((*k, cos.resolve(v).as_ref().clone()));
            }
        }
    }
    let kids = cos.resolve(d.get(b"Kids").unwrap_or(&Object::Null));
    if let Some(kids) = kids.as_array() {
        for k in kids {
            collect_nums(cos, k, out, depth + 1);
        }
    }
}

fn roman(mut n: i64, upper: bool) -> String {
    const T: &[(i64, &str)] = &[
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if !(1..=4999).contains(&n) {
        return n.to_string();
    }
    let mut s = String::new();
    for &(v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    if upper { s.to_uppercase() } else { s }
}

/// The numeric part of a page label in style `style` (`D R r A a`, "" = none).
fn label_number(style: &str, n: i64) -> String {
    match style {
        "D" => n.to_string(),
        "R" | "r" => roman(n, style == "R"),
        "A" | "a" if (1..=2600).contains(&n) => {
            let base = if style == "A" { b'A' } else { b'a' };
            let c = char::from(base + ((n - 1) % 26) as u8);
            c.to_string().repeat(((n - 1) / 26 + 1) as usize)
        }
        "A" | "a" => n.to_string(),
        _ => String::new(),
    }
}

/// Every page's label from the catalog's `/PageLabels` ("" = no label).
pub fn page_labels(cos: &CosDoc, page_count: usize) -> Vec<String> {
    let mut out = vec![String::new(); page_count];
    let Some((_, cat)) = catalog(cos) else { return out };
    let Some(pl) = cat.get(b"PageLabels") else { return out };
    let mut ranges = Vec::new();
    collect_nums(cos, pl, &mut ranges, 0);
    ranges.sort_by_key(|r| r.0);
    for (i, (start, d)) in ranges.iter().enumerate() {
        let end = ranges.get(i + 1).map_or(page_count as i64, |r| r.0);
        let Some(d) = d.as_dict() else { continue };
        let style = pdf::name(d.get(b"S"));
        let prefix = pdf::text(d.get(b"P"));
        let st = d.int(b"St").unwrap_or(1);
        let first = (*start).max(0);
        let last = end.min(page_count as i64);
        for p in first..last {
            let n = st.saturating_add(p - start);
            if let Some(slot) = out.get_mut(p as usize) {
                *slot = format!("{prefix}{}", label_number(&style, n));
            }
        }
    }
    out
}

fn fold_replies(cos: &CosDoc, doc: &mut Document) {
    let by_obj: HashMap<(u32, u16), usize> = doc
        .markups
        .iter()
        .enumerate()
        .filter(|(_, m)| m.in_file())
        .map(|(i, m)| (m.obj, i))
        .collect();
    let mut moves: Vec<(usize, usize)> = Vec::new(); // (reply, parent)
    for (i, r) in doc.markups.iter().enumerate() {
        let Some(irt) = r.irt else { continue };
        let Some(&first) = by_obj.get(&irt) else { continue };
        // A reply to a reply attaches to the root markup.
        let mut parent = first;
        for _ in 0..MAX_REPLY_HOPS {
            let Some(up) = doc.markups.get(parent).and_then(|p| p.irt).and_then(|o| by_obj.get(&o)) else {
                break;
            };
            parent = *up;
        }
        if parent != i {
            moves.push((i, parent));
        }
    }
    if moves.is_empty() {
        return;
    }
    let mut drop = vec![false; doc.markups.len()];
    for (i, parent) in moves {
        let Some(r) = doc.markups.get(i) else { continue };
        let mut y = Reply {
            obj: r.obj,
            id: r.id.clone(),
            author: r.author.clone(),
            date: if r.modified.is_empty() {
                r.created.clone()
            } else {
                r.modified.clone()
            },
            text: r.contents.clone(),
            ..Default::default()
        };
        if let Some(a) = cos.get(ObjRef::new(r.obj.0, r.obj.1)).as_dict() {
            y.state_model = pdf::text(a.get(b"StateModel"));
            y.state = pdf::text(a.get(b"State"));
            if !y.state.is_empty() && y.state_model.is_empty() {
                y.state_model = if matches!(y.state.as_str(), "Marked" | "Unmarked") {
                    "Marked".into()
                } else {
                    "Review".into()
                };
            }
        }
        let Some(p) = doc.markups.get_mut(parent) else { continue };
        if y.state.is_empty() {
            p.replies.push(y);
        } else {
            p.state_replies.push(y);
        }
        if let Some(d) = drop.get_mut(i) {
            *d = true;
        }
    }
    for m in &mut doc.markups {
        m.replies.sort_by(|a, b| a.date.cmp(&b.date));
        m.state_replies.sort_by(|a, b| a.date.cmp(&b.date));
        for st in &m.state_replies {
            match st.state_model.as_str() {
                "Review" => {
                    m.status = if st.state == "None" {
                        String::new()
                    } else {
                        st.state.clone()
                    }
                }
                "Marked" => m.checked = st.state == "Marked",
                _ => {}
            }
        }
    }
    let mut i = 0;
    doc.markups.retain(|_| {
        let keep = !drop.get(i).copied().unwrap_or(false);
        i += 1;
        keep
    });
}

/// Groups with fewer than two members on a page end.
fn dissolve_singles(doc: &mut Document) {
    let mut count: HashMap<(usize, String), usize> = HashMap::new();
    for m in doc.markups.iter().filter(|m| !m.group.is_empty()) {
        *count.entry((m.page, m.group.clone())).or_default() += 1;
    }
    for m in &mut doc.markups {
        if !m.group.is_empty() && count.get(&(m.page, m.group.clone())).copied().unwrap_or(0) < 2 {
            m.group.clear();
            m.dirty = true;
        }
    }
}

fn finish_groups(doc: &mut Document) {
    let leaders: HashSet<String> = doc
        .markups
        .iter()
        .filter(|m| !m.group.is_empty())
        .map(|m| m.group.clone())
        .collect();
    if leaders.is_empty() {
        return;
    }
    for m in &mut doc.markups {
        if m.group.is_empty() && !m.id.is_empty() && leaders.contains(&m.id) {
            m.group = m.id.clone();
        }
    }
    let dirty: Vec<bool> = doc.markups.iter().map(|m| m.dirty).collect();
    dissolve_singles(doc);
    for (m, d) in doc.markups.iter_mut().zip(dirty) {
        m.dirty = d;
    }
}

// ---- groups (model operations) ---------------------------------------------------------

/// Indices of the markups in `index`'s group (just `index` when it is not grouped).
pub fn group_members(doc: &Document, index: usize) -> Vec<usize> {
    let Some(m) = doc.markups.get(index) else {
        return Vec::new();
    };
    if m.group.is_empty() {
        return vec![index];
    }
    doc.markups
        .iter()
        .enumerate()
        .filter(|(_, o)| o.group == m.group && o.page == m.page)
        .map(|(i, _)| i)
        .collect()
}

/// Group `indices` (same page as the first; others are ignored). Returns the group id, ""
/// when fewer than two markups qualify. Marks the markups dirty.
pub fn group_markups(doc: &mut Document, indices: &[usize]) -> String {
    let mut on: Vec<usize> = Vec::new();
    let mut page = None;
    for &i in indices {
        let Some(m) = doc.markups.get(i) else { continue };
        let p = *page.get_or_insert(m.page);
        if m.page == p && !on.contains(&i) {
            on.push(i);
        }
    }
    if on.len() < 2 {
        return String::new();
    }
    on.sort_unstable();
    let mut id = String::new();
    for &i in &on {
        let Some(m) = doc.markups.get_mut(i) else { continue };
        if m.id.is_empty() {
            m.id = new_markup_id();
        }
        if id.is_empty() {
            id = m.id.clone();
        }
        m.group = id.clone();
        m.dirty = true;
    }
    dissolve_singles(doc);
    id
}

/// Ungroup every group touched by `indices`. Returns how many markups left a group.
pub fn ungroup_markups(doc: &mut Document, indices: &[usize]) -> usize {
    let groups: HashSet<(usize, String)> = indices
        .iter()
        .filter_map(|&i| doc.markups.get(i))
        .filter(|m| !m.group.is_empty())
        .map(|m| (m.page, m.group.clone()))
        .collect();
    let mut n = 0;
    for m in &mut doc.markups {
        if !m.group.is_empty() && groups.contains(&(m.page, m.group.clone())) {
            m.group.clear();
            m.dirty = true;
            n += 1;
        }
    }
    n
}

/// Take `indices` out of their groups (Remove From Group); a group left with one member ends.
/// Returns how many markups left a group.
pub fn remove_from_group(doc: &mut Document, indices: &[usize]) -> usize {
    let mut n = 0;
    for &i in indices {
        if let Some(m) = doc.markups.get_mut(i)
            && !m.group.is_empty()
        {
            m.group.clear();
            m.dirty = true;
            n += 1;
        }
    }
    dissolve_singles(doc);
    n
}

// ---- writing ---------------------------------------------------------------------------

/// Per-annotation extras, called after the annotation's own keys were written.
pub fn write_markup(_cos: &mut CosDoc, a: &mut Dict, m: &mut Markup, _page: ObjRef) {
    if m.column_data.is_empty() {
        pdf::remove(a, "PCColumnData");
    } else {
        let mut d = Dict::new();
        for (k, v) in m.column_data.iter().filter(|(k, _)| !k.is_empty()) {
            d.set(k.as_bytes().to_vec(), s(v));
        }
        pdf::set(a, "PCColumnData", Object::Dict(d));
    }
}

fn columns_object(cols: &[CustomColumn]) -> Object {
    Object::Array(
        cols.iter()
            .map(|c| {
                let mut d = pdf::dict(&[
                    ("Id", s(&c.id)),
                    ("Name", s(&c.name)),
                    ("Type", n(c.kind.name())),
                    ("Decimals", Object::Int(i64::from(c.decimals))),
                    ("Total", Object::Bool(c.total)),
                ]);
                if c.kind == ColumnType::Currency {
                    pdf::set(&mut d, "Symbol", s(&c.symbol));
                }
                if !c.default_value.is_empty() {
                    pdf::set(&mut d, "Default", s(&c.default_value));
                }
                if c.kind == ColumnType::Formula {
                    pdf::set(&mut d, "Formula", s(&c.formula));
                }
                if c.kind == ColumnType::Choice {
                    pdf::set(&mut d, "AllowCustom", Object::Bool(c.allow_custom));
                    let items = c
                        .items
                        .iter()
                        .map(|it| {
                            let mut e = pdf::dict(&[("Text", s(&it.text))]);
                            if !it.subject.is_empty() {
                                pdf::set(&mut e, "Subject", s(&it.subject));
                            }
                            if let Some(v) = it.value {
                                pdf::set(&mut e, "Value", real(v));
                            }
                            Object::Dict(e)
                        })
                        .collect();
                    pdf::set(&mut d, "Items", Object::Array(items));
                }
                Object::Dict(d)
            })
            .collect(),
    )
}

/// The `/PageLabels` number tree for `labels`: a range starts wherever a label does not
/// continue the previous one (same prefix, decimal number + 1). `None` when every label is "".
pub fn page_labels_object(labels: &[String]) -> Option<Object> {
    if labels.iter().all(String::is_empty) {
        return None;
    }
    fn split(l: &str) -> (&str, Option<i64>) {
        let digits = l.chars().rev().take_while(char::is_ascii_digit).count();
        let cut = l.len() - digits;
        let num = l.get(cut..).and_then(|d| d.parse::<i64>().ok()).filter(|_| digits > 0);
        // a leading zero ("A01") cannot be rebuilt from /St; keep it in the prefix
        if l.get(cut..).is_some_and(|d| d.len() > 1 && d.starts_with('0')) {
            return (l, None);
        }
        (l.get(..cut).unwrap_or(l), num)
    }
    let mut nums = Vec::new();
    let mut prev: Option<(String, i64)> = None;
    for (i, l) in labels.iter().enumerate() {
        let (prefix, num) = split(l);
        if let (Some((pp, pn)), Some(n)) = (&prev, num)
            && pp == prefix
            && pn.checked_add(1) == Some(n)
        {
            prev = Some((pp.clone(), n));
            continue;
        }
        let mut d = Dict::new();
        if let Some(num) = num {
            pdf::set(&mut d, "S", n("D"));
            if num != 1 {
                pdf::set(&mut d, "St", Object::Int(num));
            }
            prev = Some((prefix.to_string(), num));
        } else {
            prev = None;
        }
        if !prefix.is_empty() {
            pdf::set(&mut d, "P", s(prefix));
        }
        nums.push(Object::Int(i as i64));
        nums.push(Object::Dict(d));
    }
    Some(Object::Dict(pdf::dict(&[("Nums", Object::Array(nums))])))
}

/// A page's `/Annots` array (resolving a reference) and where it lives.
fn annots_of(cos: &CosDoc, page: ObjRef) -> (Vec<Object>, Option<ObjRef>) {
    let p = cos.get(page);
    match p.as_dict().and_then(|d| d.get(b"Annots")) {
        Some(Object::Ref(r)) => (cos.get(*r).as_array().cloned().unwrap_or_default(), Some(*r)),
        Some(Object::Array(a)) => (a.clone(), None),
        _ => (Vec::new(), None),
    }
}

fn set_annots(cos: &mut CosDoc, page: ObjRef, arr: Vec<Object>, holder: Option<ObjRef>) {
    match holder {
        Some(r) => cos.set(r, Object::Array(arr)),
        None => {
            let _ = cos.update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(arr)));
        }
    }
}

/// Document-level extras, called before the markups are written: `/PCColumns`, `/PageLabels`,
/// and the replies of deleted markups.
pub fn write_document(cos: &mut CosDoc, doc: &mut Document) {
    if let Some((root, cat)) = catalog(cos) {
        let have = cat.get(b"PCColumns").map(|o| cos.resolve(o).as_ref().clone());
        let want = (!doc.columns.is_empty()).then(|| columns_object(&doc.columns));
        if have != want {
            let _ = cos.update_dict(root, |d| match want {
                Some(w) => d.set(b"PCColumns".to_vec(), w),
                None => {
                    d.remove(b"PCColumns");
                }
            });
        }
        if doc.page_labels_changed {
            let labels: Vec<String> = doc.pages.iter().map(|p| p.label.clone()).collect();
            let obj = page_labels_object(&labels);
            let _ = cos.update_dict(root, |d| match obj {
                Some(o) => d.set(b"PageLabels".to_vec(), o),
                None => {
                    d.remove(b"PageLabels");
                }
            });
        }
    }
    doc.columns_changed = false;
    doc.page_labels_changed = false;

    // Replies and state annotations of deleted markups go with them (group members stay).
    if doc.deleted.is_empty() {
        return;
    }
    let gone: HashSet<(u32, u16)> = doc.deleted.iter().copied().collect();
    for pg in pdf::pages(cos) {
        let (arr, holder) = annots_of(cos, pg.id);
        let before = arr.len();
        let kept: Vec<Object> = arr
            .into_iter()
            .filter(|o| {
                let Some(r) = o.as_ref() else { return true };
                let a = cos.get(r);
                let Some(d) = a.as_dict() else { return true };
                let irt = d.reference(b"IRT").map(|x| (x.num, x.generation));
                !(irt.is_some_and(|x| gone.contains(&x)) && d.name(b"RT") != Some(b"Group"))
            })
            .collect();
        if kept.len() != before {
            set_annots(cos, pg.id, kept, holder);
        }
    }
}

/// Make the last reply of `model` say `state`; add one when `wanted` and there is none.
fn sync_state(m: &mut Markup, model: &str, state: &str, wanted: bool) {
    if let Some(last) = m.state_replies.iter_mut().rev().find(|r| r.state_model == model) {
        if last.state != state {
            last.state = state.into();
            last.author = review_author();
            last.text = format!("{state} set by {}", last.author);
            last.dirty = true;
        }
        return;
    }
    if !wanted {
        return;
    }
    let author = review_author();
    m.state_replies.push(Reply {
        state_model: model.into(),
        state: state.into(),
        text: format!("{state} set by {author}"),
        author,
        dirty: true,
        ..Default::default()
    });
}

fn write_reply(cos: &mut CosDoc, parent: ObjRef, page: ObjRef, rect: &markupcraft_geom::Rect, y: &mut Reply) {
    let existing = (y.obj.0 != 0)
        .then(|| cos.get(ObjRef::new(y.obj.0, y.obj.1)).as_dict().cloned())
        .flatten();
    let fresh = existing.is_none();
    let mut r = existing.unwrap_or_default();
    if fresh {
        pdf::set(&mut r, "Type", n("Annot"));
        pdf::set(&mut r, "Subtype", n("Text"));
        if y.id.is_empty() {
            y.id = new_markup_id();
        }
        pdf::set(&mut r, "NM", s(&y.id));
        if y.date.is_empty() {
            y.date = pdf_date_now();
        }
        pdf::set(&mut r, "CreationDate", s(&y.date));
        // A small box at the parent's top-left corner; replies are listed, not drawn.
        let (x0, y1) = (rect.x0.min(rect.x1), rect.y0.max(rect.y1));
        pdf::set(
            &mut r,
            "Rect",
            Object::Array(vec![real(x0), real(y1 - 20.0), real(x0 + 20.0), real(y1)]),
        );
        pdf::set(&mut r, "Name", n("Comment"));
        pdf::set(&mut r, "Open", Object::Bool(false));
        // Print + NoZoom + NoRotate; state replies are Hidden too.
        pdf::set(&mut r, "F", Object::Int(if y.state_model.is_empty() { 28 } else { 30 }));
    }
    pdf::set(&mut r, "IRT", Object::Ref(parent));
    pdf::set(&mut r, "P", Object::Ref(page));
    pdf::set(&mut r, "T", s(&y.author));
    pdf::set(&mut r, "M", s(&if fresh { y.date.clone() } else { pdf_date_now() }));
    pdf::set(&mut r, "Contents", s(&y.text));
    if !y.state_model.is_empty() {
        pdf::set(&mut r, "StateModel", s(&y.state_model));
        pdf::set(&mut r, "State", s(&y.state));
    }
    if fresh {
        let rf = cos.add(Object::Dict(r));
        let (mut arr, holder) = annots_of(cos, page);
        arr.push(Object::Ref(rf));
        set_annots(cos, page, arr, holder);
        y.obj = (rf.num, rf.generation);
    } else {
        cos.set(ObjRef::new(y.obj.0, y.obj.1), Object::Dict(r));
    }
    y.dirty = false;
}

/// Document-level extras, called after every markup was written (each has its object):
/// status / checkmark / reply annotations, and group links.
pub fn finish_document(cos: &mut CosDoc, doc: &mut Document) {
    let pages: Vec<ObjRef> = pdf::pages(cos).iter().map(|p| p.id).collect();
    for m in &mut doc.markups {
        if !m.in_file() {
            continue;
        }
        let Some(&page) = pages.get(m.page) else { continue };
        if m.dirty {
            let status = if m.status.is_empty() {
                "None".to_string()
            } else {
                m.status.clone()
            };
            sync_state(m, "Review", &status, !m.status.is_empty());
            let checked = if m.checked { "Marked" } else { "Unmarked" };
            sync_state(m, "Marked", checked, m.checked);
        }
        let parent = ObjRef::new(m.obj.0, m.obj.1);
        let rect = m.rect;
        for y in m.state_replies.iter_mut().chain(m.replies.iter_mut()) {
            if y.dirty || y.obj.0 == 0 {
                write_reply(cos, parent, page, &rect, y);
            }
        }
    }
    write_groups(cos, doc);
}

fn write_groups(cos: &mut CosDoc, doc: &Document) {
    // The leader of each (page, group): the markup whose id names it, else its first member.
    let mut leader: BTreeMap<(usize, &str), (u32, u16)> = BTreeMap::new();
    for m in doc.markups.iter().filter(|m| !m.group.is_empty() && m.in_file()) {
        if m.id == m.group {
            leader.insert((m.page, m.group.as_str()), m.obj);
        }
    }
    for m in doc.markups.iter().filter(|m| !m.group.is_empty() && m.in_file()) {
        leader.entry((m.page, m.group.as_str())).or_insert(m.obj);
    }
    for m in doc.markups.iter().filter(|m| m.in_file()) {
        let r = ObjRef::new(m.obj.0, m.obj.1);
        let want = leader
            .get(&(m.page, m.group.as_str()))
            .filter(|&&l| !m.group.is_empty() && l != m.obj)
            .map(|&(num, generation)| ObjRef::new(num, generation));
        let a = cos.get(r);
        let Some(d) = a.as_dict() else { continue };
        let is_member = d.name(b"RT") == Some(b"Group");
        let have = if is_member { d.reference(b"IRT") } else { None };
        if have == want {
            continue;
        }
        let _ = cos.update_dict(r, |d| match want {
            Some(l) => {
                d.set(b"IRT".to_vec(), Object::Ref(l));
                d.set(b"RT".to_vec(), Object::name("Group"));
            }
            None => {
                d.remove(b"RT");
                d.remove(b"IRT");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use markupcraft_model::{Kind, MarkupTable, PageInfo, Point, Scale};
    use pdfcraft_cos::{SaveOptions, write_full};

    use super::*;
    use crate::{PdfFile, SaveMode, open, open_bytes, save};

    /// A one-page blank PDF built in memory, plus optional extra annotations and catalog keys.
    fn blank_pdf(pages: usize, extra: impl FnOnce(&mut CosDoc, &[ObjRef])) -> Vec<u8> {
        let mut cos = CosDoc::new_empty();
        let root = cos.root().unwrap_or(ObjRef::new(0, 0));
        let pages_ref = cos
            .get(root)
            .as_dict()
            .and_then(|d| d.reference(b"Pages"))
            .unwrap_or(ObjRef::new(0, 0));
        let mut kids = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..pages {
            let p = cos.add(Object::Dict(pdf::dict(&[
                ("Type", n("Page")),
                ("Parent", Object::Ref(pages_ref)),
                (
                    "MediaBox",
                    Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(612), Object::Int(792)]),
                ),
            ])));
            kids.push(Object::Ref(p));
            ids.push(p);
        }
        let count = kids.len() as i64;
        cos.update_dict(pages_ref, |d| {
            d.set(b"Kids".to_vec(), Object::Array(kids));
            d.set(b"Count".to_vec(), Object::Int(count));
        })
        .unwrap();
        extra(&mut cos, &ids);
        write_full(&cos, &SaveOptions::default()).unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("markupcraft-extras-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn load(bytes: Vec<u8>) -> (PdfFile, Document) {
        open_bytes(Arc::new(bytes), &tmp("in.pdf")).unwrap()
    }

    fn area(side_ft: f64, subject: &str) -> Markup {
        let s = side_ft * 9.0;
        let mut m = Markup::new(
            Kind::Area,
            0,
            vec![
                Point::new(100.0, 100.0),
                Point::new(100.0 + s, 100.0),
                Point::new(100.0 + s, 100.0 + s),
                Point::new(100.0, 100.0 + s),
            ],
        );
        m.subject = subject.into();
        m.scale = Some(Scale::architectural(0.125, 1.0));
        m
    }

    fn annot_count(path: &PathBuf) -> usize {
        let (f, _) = open(path).unwrap();
        let pages = pdf::pages(&f.cos);
        let (arr, _) = annots_of(&f.cos, pages[0].id);
        arr.len()
    }

    #[test]
    fn extras_columns_status_replies_lock_round_trip() {
        let (mut f, mut doc) = load(blank_pdf(1, |_, _| {}));
        assert!(doc.markups.is_empty());
        doc.columns = vec![
            CustomColumn {
                id: "phase".into(),
                name: "Phase".into(),
                kind: ColumnType::Choice,
                allow_custom: true,
                items: vec![
                    ChoiceItem {
                        text: "Rough-in".into(),
                        subject: String::new(),
                        value: Some(1.5),
                    },
                    ChoiceItem {
                        text: "Trim".into(),
                        subject: "Floor".into(),
                        value: None,
                    },
                ],
                ..Default::default()
            },
            CustomColumn {
                id: "cost".into(),
                name: "Cost".into(),
                kind: ColumnType::Formula,
                formula: "Area * Phase".into(),
                decimals: 1,
                ..Default::default()
            },
            CustomColumn {
                id: "note".into(),
                name: "Note".into(),
                kind: ColumnType::Text,
                default_value: "n/a".into(),
                total: false,
                ..Default::default()
            },
        ];
        doc.markups.push(area(10.0, "Floor"));
        doc.markups.push(area(4.0, "Duct"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:phase", "Rough-in"));
        assert!(MarkupTable::set_cell(&mut doc, 0, "c:note", "east wing, \"level 2\""));
        assert!(MarkupTable::set_cell(&mut doc, 1, "c:phase", "Custom text"));
        doc.markups[0].status = "Rejected".into();
        doc.markups[0].checked = true;
        doc.markups[1].set_locked(true);
        doc.markups[0].replies.push(Reply {
            author: "Tester".into(),
            text: "Check this area".into(),
            ..Default::default()
        });

        let out1 = tmp("saved1.pdf");
        save(&mut f, &mut doc, &out1, SaveMode::Full).unwrap();
        let (mut f2, mut back) = open(&out1).unwrap();
        assert_eq!(
            back.markups.len(),
            2,
            "replies and state annotations fold into their parent"
        );
        assert_eq!(back.columns, doc.columns);
        let a = &back.markups[0];
        assert_eq!(a.column_data.get("phase").map(String::as_str), Some("Rough-in"));
        assert_eq!(
            a.column_data.get("note").map(String::as_str),
            Some("east wing, \"level 2\"")
        );
        assert_eq!(a.status, "Rejected");
        assert!(a.checked);
        assert!(!a.locked());
        assert_eq!(a.replies.len(), 1);
        assert_eq!(a.replies[0].text, "Check this area");
        assert_eq!(a.replies[0].author, "Tester");
        assert_eq!(a.state_replies.len(), 2);
        assert!(back.markups[1].locked());
        assert_eq!(
            back.markups[1].column_data.get("phase").map(String::as_str),
            Some("Custom text")
        );
        {
            let t = MarkupTable::new(&back);
            let cost = t.cell_by_id(0, "c:cost").num.unwrap_or(0.0);
            assert!(
                (cost - 150.0).abs() < 1e-9,
                "formula after reload: 100 sf x 1.5 = {cost}"
            );
            assert_eq!(t.cell_by_id(1, "c:note").text, "n/a");
        }

        // Change status, uncheck, unlock: the state replies are updated, not duplicated.
        back.markups[0].status = "Completed".into();
        back.markups[0].checked = false;
        back.markups[0].dirty = true;
        back.markups[1].set_locked(false);
        back.markups[1].dirty = true;
        let out2 = tmp("saved2.pdf");
        save(&mut f2, &mut back, &out2, SaveMode::Incremental).unwrap();
        let (mut f3, mut again) = open(&out2).unwrap();
        assert_eq!(again.markups.len(), 2);
        assert_eq!(again.markups[0].status, "Completed");
        assert!(!again.markups[0].checked);
        assert_eq!(again.markups[0].state_replies.len(), 2);
        assert!(!again.markups[1].locked());
        assert_eq!(annot_count(&out2), 5, "2 markups + 2 states + 1 reply");

        // Deleting a markup removes its replies; removing every column removes /PCColumns.
        let gone = again.markups.remove(0);
        again.deleted.push(gone.obj);
        again.columns.clear();
        let out3 = tmp("saved3.pdf");
        save(&mut f3, &mut again, &out3, SaveMode::Full).unwrap();
        assert_eq!(annot_count(&out3), 1);
        let (f4, last) = open(&out3).unwrap();
        assert_eq!(last.markups.len(), 1);
        assert!(last.columns.is_empty());
        let cat = catalog(&f4.cos).unwrap().1;
        assert!(!cat.contains(b"PCColumns"));
    }

    #[test]
    fn extras_groups_round_trip() {
        let (mut f, mut doc) = load(blank_pdf(1, |_, _| {}));
        for side in [4.0, 5.0, 6.0] {
            doc.markups.push(area(side, "Floor"));
        }
        let gid = group_markups(&mut doc, &[0, 2]);
        assert!(!gid.is_empty());
        assert_eq!(group_members(&doc, 2), [0, 2]);
        assert_eq!(group_members(&doc, 1), [1]);
        let out = tmp("groups.pdf");
        save(&mut f, &mut doc, &out, SaveMode::Full).unwrap();
        let (mut f2, mut back) = open(&out).unwrap();
        assert_eq!(back.markups.len(), 3, "group members are not replies");
        assert_eq!(back.markups[0].group, back.markups[0].id);
        assert_eq!(back.markups[2].group, back.markups[0].id);
        assert!(back.markups[1].group.is_empty());
        {
            let m2 = f2.cos.get(ObjRef::new(back.markups[2].obj.0, back.markups[2].obj.1));
            let d = m2.as_dict().unwrap();
            assert_eq!(d.name(b"RT"), Some(&b"Group"[..]));
            assert_eq!(
                d.reference(b"IRT").map(|r| (r.num, r.generation)),
                Some(back.markups[0].obj)
            );
        }
        assert_eq!(ungroup_markups(&mut back, &[2]), 2);
        let out2 = tmp("groups2.pdf");
        save(&mut f2, &mut back, &out2, SaveMode::Incremental).unwrap();
        let (f3, again) = open(&out2).unwrap();
        assert!(again.markups.iter().all(|m| m.group.is_empty()));
        let m2 = f3.cos.get(ObjRef::new(again.markups[2].obj.0, again.markups[2].obj.1));
        assert!(!m2.as_dict().unwrap().contains(b"RT"));
    }

    #[test]
    fn extras_remove_from_group() {
        let mut doc = Document::default();
        for side in [4.0, 5.0, 6.0] {
            doc.markups.push(area(side, "Floor"));
        }
        let gid = group_markups(&mut doc, &[0, 1, 2]);
        assert_eq!(group_members(&doc, 0), [0, 1, 2]);
        assert_eq!(remove_from_group(&mut doc, &[1]), 1);
        assert_eq!(group_members(&doc, 0), [0, 2]);
        assert_eq!(doc.markups[2].group, gid);
        assert_eq!(remove_from_group(&mut doc, &[2]), 1);
        assert!(doc.markups.iter().all(|m| m.group.is_empty()), "a group of one ends");
        assert!(group_markups(&mut doc, &[1, 99]).is_empty());
    }

    #[test]
    fn extras_page_labels_read_and_write() {
        let bytes = blank_pdf(6, |cos, _| {
            let root = cos.root().unwrap();
            let tree = pdf::dict(&[(
                "Nums",
                Object::Array(vec![
                    Object::Int(0),
                    Object::Dict(pdf::dict(&[("S", n("r"))])),
                    Object::Int(2),
                    Object::Dict(pdf::dict(&[("S", n("D")), ("P", s("A-")), ("St", Object::Int(101))])),
                    Object::Int(4),
                    Object::Dict(pdf::dict(&[("S", n("A"))])),
                    Object::Int(5),
                    Object::Dict(pdf::dict(&[("P", s("Cover"))])),
                ]),
            )]);
            let tree = cos.add(Object::Dict(tree));
            cos.update_dict(root, |d| d.set(b"PageLabels".to_vec(), Object::Ref(tree)))
                .unwrap();
        });
        let (mut f, mut doc) = load(bytes);
        let labels: Vec<&str> = doc.pages.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["i", "ii", "A-101", "A-102", "A", "Cover"]);

        doc.pages[0].label = "M1.01".into();
        doc.pages[1].label = "M1.02".into();
        doc.pages[2].label = "M2.01".into();
        doc.page_labels_changed = true;
        let out = tmp("labels.pdf");
        save(&mut f, &mut doc, &out, SaveMode::Full).unwrap();
        let (_, back) = open(&out).unwrap();
        let labels: Vec<&str> = back.pages.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["M1.01", "M1.02", "M2.01", "A-102", "A", "Cover"]);
        let t = MarkupTable::new(&back);
        assert_eq!(t.columns().first().map(|c| c.id.as_str()), Some("subject"));

        // Hostile trees: huge start numbers, cycles of /Kids, odd /Nums never crash.
        assert_eq!(label_number("A", i64::MAX), i64::MAX.to_string());
        assert_eq!(label_number("R", 1_000_000), "1000000");
        assert_eq!(label_number("a", 28), "bb");
        assert_eq!(roman(1994, true), "MCMXCIV");
        let p = page_labels_object(&["".into(), "".into()]);
        assert!(p.is_none());
        let p = page_labels_object(&["A01".into(), "A02".into()]);
        assert!(p.is_some_and(|o| format!("{o:?}").matches("Int").count() == 2));
        let _ = PageInfo::default();
    }

    #[test]
    fn extras_revu_column_data_is_preserved() {
        // A Revu-made markup carrying keys we do not model, rewritten after an edit.
        let bytes = blank_pdf(1, |cos, pages| {
            let a = pdf::dict(&[
                ("Type", n("Annot")),
                ("Subtype", n("Square")),
                (
                    "Rect",
                    Object::Array(vec![real(10.0), real(10.0), real(60.0), real(60.0)]),
                ),
                ("NM", s("REVUMADEMARKUPAA")),
                ("Subj", s("Rectangle")),
                ("BSIColumnData", Object::Array(vec![s("Phase 2"), s("12.50"), s("")])),
                ("PCColumnData", Object::Dict(pdf::dict(&[("qty", Object::Int(4))]))),
            ]);
            let r = cos.add(Object::Dict(a));
            let p = pages[0];
            cos.update_dict(p, |d| d.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(r)])))
                .unwrap();
        });
        let (mut f, mut doc) = load(bytes);
        assert_eq!(doc.markups.len(), 1);
        assert_eq!(doc.markups[0].column_data.get("qty").map(String::as_str), Some("4"));
        let before = f.cos.get(ObjRef::new(doc.markups[0].obj.0, doc.markups[0].obj.1));
        let bsi = before.as_dict().and_then(|d| d.get(b"BSIColumnData")).cloned();
        assert!(bsi.is_some());
        doc.markups[0].subject = "Edited".into();
        doc.markups[0].status = "Accepted".into();
        doc.markups[0].dirty = true;
        let out = tmp("bsi.pdf");
        save(&mut f, &mut doc, &out, SaveMode::Full).unwrap();
        let (g, back) = open(&out).unwrap();
        assert_eq!(back.markups.len(), 1);
        assert_eq!(back.markups[0].subject, "Edited");
        assert_eq!(back.markups[0].status, "Accepted");
        assert_eq!(back.markups[0].column_data.get("qty").map(String::as_str), Some("4"));
        let after = g.cos.get(ObjRef::new(back.markups[0].obj.0, back.markups[0].obj.1));
        assert_eq!(after.as_dict().and_then(|d| d.get(b"BSIColumnData")).cloned(), bsi);
    }
}
