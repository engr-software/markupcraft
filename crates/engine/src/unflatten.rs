//! Unflatten: bring back markups MarkupCraft flattened. When `flatten.rs` burns markups into a
//! page it keeps, on the page, what it would need to undo that (`/PCFlattened`: the content
//! streams it added around the page's own and each annotation's dictionary); Unflatten puts
//! the annotations back and takes those streams out. Content another program flattened has no
//! such record and stays as it is.

use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString};

use crate::docutil::{annots_of, page_objs, set_annots};
use crate::{Result, Session, invalid};

/// The page key holding what Unflatten needs.
const KEY: &[u8] = b"PCFlattened";

/// Keep, on `page`, the streams `flatten` added (`open`, `close`) and the dictionaries and
/// drawings of the annotations it burned in.
pub(crate) fn record(
    cos: &mut CosDoc,
    page: ObjRef,
    (open, close): (ObjRef, ObjRef),
    items: Vec<(Dict, String)>,
) -> Result<()> {
    let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
    let mut list = pd
        .get(KEY)
        .and_then(|o| cos.resolve(o).as_array().cloned())
        .unwrap_or_default();
    let entries: Vec<Object> = items
        .into_iter()
        .map(|(mut d, line)| {
            // its pop-up and replies went with it
            d.remove(b"Popup");
            let mut e = Dict::new();
            e.set(b"Annot".to_vec(), Object::Dict(d));
            e.set(b"Draw".to_vec(), Object::String(PdfString::text(&line)));
            Object::Dict(e)
        })
        .collect();
    let mut rec = Dict::new();
    rec.set(b"Open".to_vec(), Object::Ref(open));
    rec.set(b"Close".to_vec(), Object::Ref(close));
    rec.set(b"Items".to_vec(), Object::Array(entries));
    list.push(Object::Dict(rec));
    cos.update_dict(page, |d| d.set(KEY.to_vec(), Object::Array(list)))?;
    Ok(())
}

/// How many markups MarkupCraft flattened on `page` that Unflatten can bring back.
pub fn flattened_count(cos: &CosDoc, page: ObjRef) -> usize {
    let Some(pd) = cos.dict(&Object::Ref(page)) else {
        return 0;
    };
    let recs = pd
        .get(KEY)
        .and_then(|o| cos.resolve(o).as_array().cloned())
        .unwrap_or_default();
    recs.iter()
        .filter_map(|r| cos.dict(r))
        .map(|r| {
            r.get(b"Items")
                .and_then(|i| cos.resolve(i).as_array().map(Vec::len))
                .unwrap_or(0)
        })
        .sum()
}

impl Session {
    /// Unflatten: the markups MarkupCraft flattened on `pages` (0-based; empty = every page)
    /// become annotations again and their drawings leave the page content. Returns how many
    /// came back. One undo step; the next save rewrites the file in full.
    pub fn unflatten(&mut self, pages: &[usize]) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let pages = pages.to_vec();
        self.graph_edit("Unflatten", move |cos, _doc| {
            let refs = page_objs(cos)?;
            let mut n = 0usize;
            for (i, page) in refs.into_iter().enumerate() {
                if !pages.is_empty() && !pages.contains(&i) {
                    continue;
                }
                let pd = cos.dict(&Object::Ref(page)).unwrap_or_default();
                let recs = pd
                    .get(KEY)
                    .and_then(|o| cos.resolve(o).as_array().cloned())
                    .unwrap_or_default();
                if recs.is_empty() {
                    continue;
                }
                let mut contents: Vec<Object> = match pd.get(b"Contents").map(|c| cos.resolve(c)) {
                    Some(c) => match &*c {
                        Object::Array(a) => a.clone(),
                        _ => pd.get(b"Contents").cloned().into_iter().collect(),
                    },
                    None => Vec::new(),
                };
                let mut annots = annots_of(cos, page);
                let recs: Vec<Dict> = recs.iter().filter_map(|r| cos.dict(r)).collect();
                for rec in recs {
                    let added: Vec<ObjRef> = [b"Open".as_slice(), b"Close".as_slice()]
                        .iter()
                        .filter_map(|k| rec.reference(k))
                        .collect();
                    contents.retain(|o| !o.as_ref().is_some_and(|r| added.contains(&r)));
                    let items = rec
                        .get(b"Items")
                        .and_then(|i| cos.resolve(i).as_array().cloned())
                        .unwrap_or_default();
                    for it in items.iter().take(100_000) {
                        let Some(d) = cos.dict(it).and_then(|e| e.get(b"Annot").and_then(|a| cos.dict(a))) else {
                            continue;
                        };
                        let mut d = d;
                        d.set(b"P".to_vec(), Object::Ref(page));
                        let r = cos.add(Object::Dict(d));
                        annots.push(Object::Ref(r));
                        n += 1;
                    }
                }
                set_annots(cos, page, annots)?;
                cos.update_dict(page, |d| {
                    d.set(b"Contents".to_vec(), Object::Array(contents));
                    d.remove(KEY);
                })?;
            }
            if n == 0 {
                return Err(invalid(
                    "nothing here was flattened by MarkupCraft (only its own flattening can be undone)",
                ));
            }
            cos.require_full_save();
            Ok(n)
        })
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{Kind, Markup, Point, Rect};

    use super::*;
    use crate::flatten::FlattenFilter;

    #[test]
    fn unflatten_brings_back_what_flatten_burned_in() {
        let dir = std::env::temp_dir().join(format!("markupcraft-unflatten-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut s = Session::new_blank(dir.join("u.pdf"), &[(612.0, 792.0)]).expect("blank");
        let mut r = Markup::new(
            Kind::Rectangle,
            0,
            Rect::new(100.0, 100.0, 200.0, 160.0).corners().to_vec(),
        );
        r.subject = "Box".into();
        s.add_markup(r).expect("add");
        let mut l = Markup::new(Kind::Line, 0, vec![Point::new(300.0, 300.0), Point::new(400.0, 300.0)]);
        l.subject = "Keep".into();
        s.add_markup(l).expect("add");
        let box_id = s
            .doc()
            .markups
            .iter()
            .find(|m| m.subject == "Box")
            .map(|m| m.id.clone())
            .unwrap();
        let n = s
            .flatten_markups(&FlattenFilter {
                ids: vec![box_id.clone()],
                ..Default::default()
            })
            .expect("flatten");
        assert_eq!(n, 1);
        assert_eq!(s.doc().markups.len(), 1);
        let before = annots_of(&s.file.cos, page_objs(&s.file.cos).unwrap()[0]).len();
        assert_eq!(flattened_count(&s.file.cos, page_objs(&s.file.cos).unwrap()[0]), 1);
        assert_eq!(s.unflatten(&[]).expect("unflatten"), 1);
        assert_eq!(s.doc().markups.len(), 2);
        let back = s
            .doc()
            .markups
            .iter()
            .find(|m| m.subject == "Box")
            .expect("the box is back");
        assert_eq!(back.kind, Kind::Rectangle);
        let page = page_objs(&s.file.cos).unwrap()[0];
        assert_eq!(annots_of(&s.file.cos, page).len(), before + 1);
        assert_eq!(flattened_count(&s.file.cos, page), 0, "the record is gone");
        // nothing left to unflatten; undo puts the flattened state back
        assert!(s.unflatten(&[]).is_err());
        s.undo().expect("undo");
        assert_eq!(s.doc().markups.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
