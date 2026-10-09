//! Places (named destinations, the Links panel's Places list) and more on links: editing a
//! link's action or box, links made from the text under a box, links made from the web
//! addresses written on the pages, and the action of a markup (Edit Action).
//!
//! A Place is an entry of the catalog's `/Names /Dests` tree: a page and a view (left, top,
//! zoom). Links and bookmarks that go to a Place name it, so moving the Place moves where they
//! go; renaming it breaks them (as in Revu).

use markupcraft_model::Rect;
use markupcraft_revu::cos::{Dict, Object, PdfString};

use crate::docutil::{annots_of, name_tree_entries, names_tree, page_objs, set_annots, text_of, write_name_tree};
use crate::links::{LinkInfo, LinkLook, LinkTarget, target_entry, target_of};
use crate::raster::PageWord;
use crate::{EngineError, Result, Session, invalid};

/// Longest Place name.
pub const MAX_PLACE: usize = 200;
/// Most links one URL scan adds.
pub const MAX_URL_LINKS: usize = 10_000;

/// A named destination.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    /// 0-based; `None` when it goes nowhere in this document.
    pub page: Option<usize>,
    /// The view's left and top (user space) and zoom (`None` = keep the reader's).
    pub left: Option<f64>,
    pub top: Option<f64>,
    pub zoom: Option<f64>,
}

fn place_dest(page: Object, left: Option<f64>, top: Option<f64>, zoom: Option<f64>) -> Object {
    let n = |v: Option<f64>| v.filter(|x| x.is_finite()).map_or(Object::Null, Object::Real);
    Object::Array(vec![page, Object::name("XYZ"), n(left), n(top), n(zoom)])
}

fn check_place_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_PLACE || n.chars().any(char::is_control) {
        return Err(invalid(format!("a Place name has 1 to {MAX_PLACE} characters")));
    }
    Ok(n.to_string())
}

/// The web addresses in a page's words: `http://`, `https://` or `www.`, without trailing
/// punctuation.
pub fn url_words(words: &[PageWord]) -> Vec<(String, Rect)> {
    words
        .iter()
        .filter_map(|w| {
            let t = w.text.trim_matches(|c: char| "()[]<>\"'".contains(c));
            let t = t.trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let lower = t.to_ascii_lowercase();
            let url = if lower.starts_with("http://") || lower.starts_with("https://") {
                t.to_string()
            } else if lower.starts_with("www.") && t.len() > 6 && t[4..].contains('.') {
                format!("http://{t}")
            } else {
                return None;
            };
            (url.len() > 10).then_some((url, w.rect))
        })
        .collect()
}

impl Session {
    /// Every Place, in name order.
    pub fn places(&self) -> Vec<Place> {
        let cos = &self.file.cos;
        let pages = page_objs(cos).unwrap_or_default();
        let Some(tree) = names_tree(cos, b"Dests") else {
            return Vec::new();
        };
        let mut out: Vec<Place> = name_tree_entries(cos, &tree)
            .into_iter()
            .map(|(k, v)| {
                let d = cos.resolve(&v);
                let arr = match &*d {
                    Object::Array(a) => Some(a.clone()),
                    Object::Dict(x) => x.get(b"D").and_then(|a| cos.resolve(a).as_array().cloned()),
                    _ => None,
                }
                .unwrap_or_default();
                let num = |i: usize| arr.get(i).map(|o| cos.resolve(o)).and_then(|o| o.as_f64());
                let page = match arr.first() {
                    Some(Object::Ref(r)) => pages.iter().position(|p| p == r),
                    _ => None,
                };
                let xyz = arr.get(1).and_then(Object::as_name) == Some(b"XYZ");
                Place {
                    name: PdfString::literal(k).to_text(),
                    page,
                    left: if xyz { num(2) } else { None },
                    top: if xyz { num(3) } else { None },
                    zoom: if xyz { num(4).filter(|z| *z > 0.0) } else { None },
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn write_places(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Vec<(Vec<u8>, Object)>, &[markupcraft_revu::cos::ObjRef]) -> Result<()>,
    ) -> Result<()> {
        self.cos_edit(label, |cos| {
            let pages = page_objs(cos)?;
            let mut entries = names_tree(cos, b"Dests")
                .map(|t| name_tree_entries(cos, &t))
                .unwrap_or_default();
            f(&mut entries, &pages)?;
            write_name_tree(cos, b"Dests", entries)?;
            Ok(((), true))
        })
    }

    /// Add a Place (or move one that exists): `page` with the view at `left`, `top`, `zoom`.
    pub fn set_place(
        &mut self,
        name: &str,
        page: usize,
        left: Option<f64>,
        top: Option<f64>,
        zoom: Option<f64>,
    ) -> Result<()> {
        let name = check_place_name(name)?;
        self.page(page)?;
        if zoom.is_some_and(|z| !(z.is_finite() && (0.01..=64.0).contains(&z))) {
            return Err(invalid("zoom is 0.01 to 64 (1 = 100%)"));
        }
        self.write_places("Set Place", |entries, pages| {
            let to = pages.get(page).copied().ok_or(EngineError::NoPage {
                page: page + 1,
                count: pages.len(),
            })?;
            let key = PdfString::text(&name).bytes;
            entries.retain(|(k, _)| *k != key);
            entries.push((key, place_dest(Object::Ref(to), left, top, zoom)));
            Ok(())
        })
    }

    /// Rename a Place (links and bookmarks that name the old one stop working).
    pub fn rename_place(&mut self, old: &str, new: &str) -> Result<()> {
        let new = check_place_name(new)?;
        if !self.places().iter().any(|p| p.name == old) {
            return Err(invalid(format!("no Place named {old:?}")));
        }
        if self.places().iter().any(|p| p.name == new) {
            return Err(invalid(format!("a Place named {new:?} exists")));
        }
        let old_key = PdfString::text(old).bytes;
        let alt_key = old.as_bytes().to_vec();
        self.write_places("Rename Place", |entries, _| {
            for (k, _) in entries.iter_mut() {
                if *k == old_key || *k == alt_key {
                    *k = PdfString::text(&new).bytes;
                }
            }
            Ok(())
        })
    }

    /// Delete Places by name; returns how many went.
    pub fn delete_places(&mut self, names: &[String]) -> Result<usize> {
        let have = self.places();
        for n in names {
            if !have.iter().any(|p| &p.name == n) {
                return Err(invalid(format!("no Place named {n:?}")));
            }
        }
        let mut n = 0;
        self.write_places("Delete Places", |entries, _| {
            let before = entries.len();
            entries.retain(|(k, _)| !names.contains(&PdfString::literal(k.clone()).to_text()));
            n = before - entries.len();
            Ok(())
        })?;
        Ok(n)
    }

    /// Change a link's target, box or look (Edit Action). One undoable step.
    pub fn edit_link(
        &mut self,
        id: &str,
        target: Option<&LinkTarget>,
        rect: Option<Rect>,
        look: Option<LinkLook>,
    ) -> Result<()> {
        let info: LinkInfo = self
            .links()
            .into_iter()
            .find(|l| l.id == id)
            .ok_or_else(|| invalid(format!("no link with id {id:?} (link_list shows the ids)")))?;
        if let Some(t) = target {
            target_entry(&self.file.cos, t)?;
        }
        if let Some(r) = rect {
            let r = r.normalized();
            if !(r.as_array().iter().all(|v| v.is_finite()) && r.width() >= 1.0 && r.height() >= 1.0) {
                return Err(invalid("the link rectangle must be at least 1 point wide and high"));
            }
        }
        let target = target.cloned();
        self.graph_edit("Edit Link", |cos, _| {
            let pages = page_objs(cos)?;
            let page = *pages.get(info.page).ok_or_else(|| invalid("the link's page is gone"))?;
            for (k, a) in annots_of(cos, page).iter().enumerate() {
                let Some(mut d) = cos.dict(a) else { continue };
                if d.name(b"Subtype") != Some(b"Link") {
                    continue;
                }
                let nm = text_of(cos, d.get(b"NM"));
                let lid = if nm.is_empty() {
                    format!("{}:{k}", info.page + 1)
                } else {
                    nm
                };
                if lid != id {
                    continue;
                }
                if let Some(t) = &target {
                    d.remove(b"Dest");
                    d.remove(b"A");
                    let (key, v) = target_entry(cos, t)?;
                    d.set(key.to_vec(), v);
                }
                if let Some(r) = rect {
                    d.set(
                        b"Rect".to_vec(),
                        Object::Array(r.normalized().as_array().iter().map(|v| Object::Real(*v)).collect()),
                    );
                }
                if let Some(l) = look {
                    d.set(
                        b"Border".to_vec(),
                        Object::Array(vec![
                            Object::Int(0),
                            Object::Int(0),
                            Object::Real(l.width.clamp(0.0, 12.0)),
                        ]),
                    );
                    d.set(
                        b"C".to_vec(),
                        Object::Array(vec![
                            Object::Real(l.color.r),
                            Object::Real(l.color.g),
                            Object::Real(l.color.b),
                        ]),
                    );
                }
                match a.as_ref() {
                    Some(r) => cos.set(r, Object::Dict(d)),
                    None => {
                        let mut list = annots_of(cos, page);
                        if let Some(slot) = list.get_mut(k) {
                            *slot = Object::Dict(d);
                        }
                        set_annots(cos, page, list)?;
                    }
                }
                return Ok(());
            }
            Err(invalid("the link is gone"))
        })
    }

    /// The Hyperlink tool over text: a link on the words inside `rect` (their box). Returns
    /// its id.
    pub fn add_link_on_text(&mut self, page: usize, rect: Rect, target: &LinkTarget, look: LinkLook) -> Result<String> {
        let r = rect.normalized();
        let ws: Vec<PageWord> = self
            .page_words(page)?
            .into_iter()
            .filter(|w| {
                let (cx, cy) = ((w.rect.x0 + w.rect.x1) / 2.0, (w.rect.y0 + w.rect.y1) / 2.0);
                cx >= r.x0 && cx <= r.x1 && cy >= r.y0 && cy <= r.y1
            })
            .collect();
        let first = ws.first().ok_or_else(|| invalid("there is no text in the box"))?;
        let mut b = first.rect;
        for w in &ws {
            b = Rect::new(
                b.x0.min(w.rect.x0),
                b.y0.min(w.rect.y0),
                b.x1.max(w.rect.x1),
                b.y1.max(w.rect.y1),
            );
        }
        self.add_link(page, b.padded(1.0), target, look)
    }

    /// Create Hyperlinks from URLs: a web link on every web address written on `pages`
    /// (0-based; empty = all) that has no link yet. Returns how many were added.
    pub fn links_from_urls(&mut self, pages: &[usize], look: LinkLook) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let list: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        let existing = self.links();
        let mut todo: Vec<(usize, Rect, String)> = Vec::new();
        for p in list {
            for (url, r) in url_words(&self.page_words(p)?) {
                let covered = existing.iter().any(|l| {
                    l.page == p && l.rect.x0 <= r.x1 && r.x0 <= l.rect.x1 && l.rect.y0 <= r.y1 && r.y0 <= l.rect.y1
                });
                if !covered && todo.len() < MAX_URL_LINKS {
                    todo.push((p, r.padded(1.0), url));
                }
            }
        }
        if todo.is_empty() {
            return Ok(0);
        }
        let n = todo.len();
        self.graph_edit("Create Hyperlinks from URLs", |cos, _| {
            let pages = page_objs(cos)?;
            for (p, r, url) in &todo {
                let page = *pages.get(*p).ok_or_else(|| invalid("page gone"))?;
                let mut d = Dict::new();
                d.set(b"Type".to_vec(), Object::name("Annot"));
                d.set(b"Subtype".to_vec(), Object::name("Link"));
                d.set(
                    b"Rect".to_vec(),
                    Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect()),
                );
                d.set(b"F".to_vec(), Object::Int(4));
                d.set(b"H".to_vec(), Object::name("I"));
                d.set(
                    b"Border".to_vec(),
                    Object::Array(vec![Object::Int(0), Object::Int(0), Object::Real(look.width)]),
                );
                let c = look.color;
                d.set(
                    b"C".to_vec(),
                    Object::Array(vec![Object::Real(c.r), Object::Real(c.g), Object::Real(c.b)]),
                );
                let (k, v) = target_entry(cos, &LinkTarget::Url(url.clone()))?;
                d.set(k.to_vec(), v);
                d.set(
                    b"NM".to_vec(),
                    Object::String(PdfString::text(&markupcraft_revu::new_markup_id())),
                );
                d.set(b"P".to_vec(), Object::Ref(page));
                let link = cos.add(Object::Dict(d));
                let mut list = annots_of(cos, page);
                list.push(Object::Ref(link));
                set_annots(cos, page, list)?;
            }
            Ok(())
        })?;
        Ok(n)
    }

    /// The action a markup runs when clicked (Edit Action), if any.
    pub fn markup_action(&self, id: &str) -> Result<Option<LinkTarget>> {
        let m = self.markup(id)?;
        if m.obj.0 == 0 {
            return Ok(None);
        }
        let cos = &self.file.cos;
        let pages = page_objs(cos)?;
        let r = markupcraft_revu::cos::ObjRef::new(m.obj.0, m.obj.1);
        let Some(d) = cos.dict(&Object::Ref(r)) else {
            return Ok(None);
        };
        if d.get(b"A").is_none() && d.get(b"Dest").is_none() {
            return Ok(None);
        }
        Ok(Some(target_of(cos, &d, &pages)))
    }

    /// Edit Action on a markup: the action it runs when clicked (`None` removes it). One
    /// undoable step; the markup is written into the file first.
    pub fn set_markup_action(&mut self, id: &str, target: Option<&LinkTarget>) -> Result<()> {
        self.markup(id)?;
        if let Some(t) = target {
            target_entry(&self.file.cos, t)?;
        }
        let id = id.to_string();
        let target = target.cloned();
        self.graph_edit("Edit Action", |cos, doc| {
            let m = doc.find(&id).ok_or_else(|| EngineError::NoMarkup(id.clone()))?;
            let r = markupcraft_revu::cos::ObjRef::new(m.obj.0, m.obj.1);
            let mut d = cos
                .dict(&Object::Ref(r))
                .ok_or_else(|| invalid("the markup has no annotation yet"))?;
            d.remove(b"A");
            d.remove(b"Dest");
            if let Some(t) = &target {
                let (k, v) = target_entry(cos, t)?;
                // Markups run actions; a destination is written as a GoTo action.
                if k == b"Dest" {
                    let mut a = Dict::new();
                    a.set(b"S".to_vec(), Object::name("GoTo"));
                    a.set(b"D".to_vec(), v);
                    d.set(b"A".to_vec(), Object::Dict(a));
                } else {
                    d.set(k.to_vec(), v);
                }
            }
            cos.set(r, Object::Dict(d));
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::Zoom;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn doc() -> Session {
        let p1 = format!(
            "{}{}",
            text(72.0, 700.0, 12.0, "See https://example.com/specs for details."),
            text(72.0, 650.0, 12.0, "Also www.example.org and SHEET A-101")
        );
        Session::from_bytes(
            pdf(&[
                SyntheticPage::new(612.0, 792.0, p1),
                SyntheticPage::new(612.0, 792.0, ""),
            ]),
            "l.pdf",
        )
        .unwrap()
    }

    #[test]
    fn places_add_move_rename_delete_and_links_follow_them() {
        let mut s = doc();
        s.set_place("Detail 5", 1, Some(100.0), Some(500.0), Some(2.0)).unwrap();
        let p = &s.places()[0];
        assert_eq!((p.page, p.left, p.zoom), (Some(1), Some(100.0), Some(2.0)));
        let id = s
            .add_link(
                0,
                Rect::new(10.0, 10.0, 60.0, 30.0),
                &LinkTarget::Place("Detail 5".into()),
                LinkLook::default(),
            )
            .unwrap();
        assert_eq!(s.links()[0].target, LinkTarget::Place("Detail 5".into()));
        // Moving the Place keeps the link working.
        s.set_place("Detail 5", 0, None, None, None).unwrap();
        assert_eq!(s.places().len(), 1);
        assert_eq!(s.places()[0].page, Some(0));
        s.rename_place("Detail 5", "Detail 6").unwrap();
        assert!(s.rename_place("nope", "x").is_err());
        assert_eq!(s.delete_places(&["Detail 6".into()]).unwrap(), 1);
        assert!(s.places().is_empty());
        // Edit Action: the link now goes to a zoomed page, then a view rectangle.
        s.edit_link(
            &id,
            Some(&LinkTarget::Zoomed {
                page: 1,
                zoom: Zoom::FitWidth,
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            s.links()[0].target,
            LinkTarget::Zoomed {
                page: 1,
                zoom: Zoom::FitWidth
            }
        );
        let view = LinkTarget::View {
            page: 1,
            rect: Rect::new(100.0, 100.0, 300.0, 200.0),
        };
        s.edit_link(&id, Some(&view), Some(Rect::new(20.0, 20.0, 90.0, 40.0)), None)
            .unwrap();
        let l = &s.links()[0];
        assert_eq!(l.target, view);
        assert_eq!(l.rect, Rect::new(20.0, 20.0, 90.0, 40.0));
        let other = LinkTarget::FileView {
            path: "other.pdf".into(),
            page: 2,
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
        };
        s.edit_link(&id, Some(&other), None, None).unwrap();
        assert_eq!(s.links()[0].target, other);
        assert!(s.edit_link("nope", None, None, None).is_err());
    }

    #[test]
    fn urls_text_links_and_markup_actions() {
        let mut s = doc();
        assert_eq!(s.links_from_urls(&[], LinkLook::default()).unwrap(), 2);
        let urls: Vec<String> = s
            .links()
            .iter()
            .filter_map(|l| match &l.target {
                LinkTarget::Url(u) => Some(u.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(urls, ["https://example.com/specs", "http://www.example.org"]);
        assert_eq!(
            s.links_from_urls(&[], LinkLook::default()).unwrap(),
            0,
            "already linked"
        );
        // A link over the words in a box.
        let id = s
            .add_link_on_text(
                0,
                Rect::new(250.0, 640.0, 330.0, 665.0),
                &LinkTarget::Page(1),
                LinkLook::default(),
            )
            .unwrap();
        let l = s.links().into_iter().find(|l| l.id == id).unwrap();
        assert!(l.rect.width() < 80.0 && l.rect.x0 > 240.0, "{:?}", l.rect);
        assert!(
            s.add_link_on_text(
                1,
                Rect::new(0.0, 0.0, 50.0, 50.0),
                &LinkTarget::Page(0),
                LinkLook::default()
            )
            .is_err()
        );
        // Edit Action on a markup.
        let m = markupcraft_model::Markup::new(
            markupcraft_model::Kind::Rectangle,
            0,
            vec![
                markupcraft_geom::Point::new(300.0, 300.0),
                markupcraft_geom::Point::new(350.0, 350.0),
            ],
        );
        let mid = s.add_markup(m).unwrap();
        assert_eq!(s.markup_action(&mid).unwrap(), None);
        s.set_markup_action(&mid, Some(&LinkTarget::Url("https://example.com".into())))
            .unwrap();
        assert_eq!(
            s.markup_action(&mid).unwrap(),
            Some(LinkTarget::Url("https://example.com".into()))
        );
        s.set_markup_action(&mid, Some(&LinkTarget::Page(1))).unwrap();
        assert_eq!(s.markup_action(&mid).unwrap(), Some(LinkTarget::Page(1)));
        let out = std::env::temp_dir().join(format!("markupcraft-places-{}.pdf", std::process::id()));
        s.save_as(&out, false).unwrap();
        let mut t = Session::open(&out).unwrap();
        let mid2 = t.doc().markups[0].id.clone();
        assert_eq!(
            t.markup_action(&mid2).unwrap(),
            Some(LinkTarget::Page(1)),
            "kept on save"
        );
        t.set_markup_action(&mid2, None).unwrap();
        assert_eq!(t.markup_action(&mid2).unwrap(), None);
    }
}
