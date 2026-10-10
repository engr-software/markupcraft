//! Bulk edits, each one undoable step: links and bookmarks on many places at once (the check
//! options of search results), the same new action on several links, and splitting counts by
//! space (Preferences > Tools > Measure > Split counts by space).

use markupcraft_geom::{Rect, bbox};
use markupcraft_model::Kind;
use markupcraft_model::spaces::spaces_at;

use crate::links::{LinkLook, LinkTarget};
use crate::{Result, Session, invalid};

/// Most items one bulk edit takes.
pub const MAX_ITEMS: usize = 10_000;

impl Session {
    /// Run several edits as one undoable step called `label`; when `f` fails, what it already
    /// changed is undone.
    pub fn as_one_step<T>(&mut self, label: &str, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let key = format!("bulk:{label}:{}", self.state_version());
        let before = self.undo_depth();
        let version = self.state_version();
        let saved = self.merge.take();
        self.last_merge = None;
        self.merge = Some(key);
        let r = f(self);
        self.merge = saved;
        self.last_merge = None;
        if r.is_err() && self.state_version() != version && self.undo_depth() > before {
            let _ = self.undo();
            self.redo.clear();
        }
        r
    }

    /// A link over each `(page, rect)`, all going to `target`. Returns their ids.
    pub fn add_links(&mut self, items: &[(usize, Rect)], target: &LinkTarget, look: LinkLook) -> Result<Vec<String>> {
        if items.is_empty() || items.len() > MAX_ITEMS {
            return Err(invalid(format!("give 1 to {MAX_ITEMS} link areas")));
        }
        self.as_one_step("Add Links", |s| {
            items
                .iter()
                .map(|(page, rect)| s.add_link(*page, rect.padded(0.5), target, look))
                .collect()
        })
    }

    /// Give several links the same target (and look). Returns how many changed.
    pub fn edit_links(&mut self, ids: &[String], target: Option<&LinkTarget>, look: Option<LinkLook>) -> Result<usize> {
        if ids.is_empty() || ids.len() > MAX_ITEMS {
            return Err(invalid(format!("give 1 to {MAX_ITEMS} link ids")));
        }
        let all = self.links();
        if let Some(id) = ids.iter().find(|id| !all.iter().any(|l| &l.id == *id)) {
            return Err(invalid(format!("no link with id {id:?} (link_list shows the ids)")));
        }
        self.as_one_step("Edit Links", |s| {
            for id in ids {
                s.edit_link(id, target, None, look)?;
            }
            Ok(ids.len())
        })
    }

    /// A top-level bookmark for each `(title, page)`, in order. Returns how many.
    pub fn add_bookmarks(&mut self, items: &[(String, usize)]) -> Result<usize> {
        if items.is_empty() || items.len() > MAX_ITEMS {
            return Err(invalid(format!("give 1 to {MAX_ITEMS} bookmarks")));
        }
        self.as_one_step("Add Bookmarks", |s| {
            for (title, page) in items {
                s.add_bookmark(&[], None, title, *page)?;
            }
            Ok(items.len())
        })
    }

    /// Split counts by space: every Count (of `ids`, or all) whose items lie in more than one
    /// space becomes one Count per space, each keeping the original's look and subject. Returns
    /// the ids of the Counts made.
    pub fn split_counts_by_space(&mut self, ids: Option<&[String]>) -> Result<Vec<String>> {
        let mut plan: Vec<(usize, Vec<Vec<markupcraft_geom::Point>>)> = Vec::new();
        for (i, m) in self.doc.markups.iter().enumerate() {
            if m.kind != Kind::Count || m.pts.len() < 2 || ids.is_some_and(|l| !l.contains(&m.id)) {
                continue;
            }
            let mut groups: Vec<(String, Vec<markupcraft_geom::Point>)> = Vec::new();
            for p in &m.pts {
                let path = spaces_at(&self.doc, m.page, *p)
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join("/");
                match groups.iter_mut().find(|(k, _)| *k == path) {
                    Some((_, v)) => v.push(*p),
                    None => groups.push((path, vec![*p])),
                }
            }
            if groups.len() > 1 {
                plan.push((i, groups.into_iter().map(|(_, v)| v).collect()));
            }
        }
        if plan.is_empty() {
            return Ok(Vec::new());
        }
        let mut used: std::collections::HashSet<String> = self.doc.markups.iter().map(|m| m.id.clone()).collect();
        let author = self.author.clone();
        let mut made = Vec::new();
        let mut fresh = Vec::new();
        for (i, groups) in &plan {
            let Some(orig) = self.doc.markups.get(*i) else { continue };
            for g in groups.iter().skip(1) {
                let mut c = orig.clone();
                c.id = loop {
                    let id = markupcraft_revu::new_markup_id();
                    if used.insert(id.clone()) {
                        break id;
                    }
                };
                c.pts = g.clone();
                c.obj = (0, 0);
                c.annot_index = None;
                c.dirty = true;
                c.stored_look = false;
                if c.author.is_empty() {
                    c.author = author.clone();
                }
                if let Some(b) = bbox(&c.pts) {
                    c.rect = b.padded(c.line_width + 6.0);
                }
                made.push(c.id.clone());
                fresh.push(c);
            }
        }
        self.edit("Split Counts by Space", move |s| {
            for (i, groups) in plan {
                if let (Some(m), Some(first)) = (s.doc.markups.get_mut(i), groups.into_iter().next()) {
                    m.pts = first;
                    m.dirty = true;
                    m.stored_look = false;
                    if let Some(b) = bbox(&m.pts) {
                        m.rect = b.padded(m.line_width + 6.0);
                    }
                }
            }
            s.doc.markups.extend(fresh);
            Ok(((), true))
        })?;
        Ok(made)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};
    use markupcraft_geom::Point;
    use markupcraft_model::Markup;

    fn session() -> Session {
        Session::from_bytes(
            pdf(&[
                SyntheticPage::new(612.0, 792.0, String::new()),
                SyntheticPage::new(612.0, 792.0, String::new()),
            ]),
            "b.pdf",
        )
        .unwrap()
    }

    #[test]
    fn links_and_bookmarks_in_one_step() {
        let mut s = session();
        let before = s.undo_depth();
        let ids = s
            .add_links(
                &[
                    (0, Rect::new(10.0, 10.0, 50.0, 20.0)),
                    (1, Rect::new(10.0, 10.0, 50.0, 20.0)),
                ],
                &LinkTarget::Url("https://example.com".into()),
                LinkLook::default(),
            )
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(s.undo_depth(), before + 1, "one undo step");
        assert_eq!(s.edit_links(&ids, Some(&LinkTarget::Page(1)), None).unwrap(), 2);
        assert!(s.links().iter().all(|l| l.target == LinkTarget::Page(1)));
        assert_eq!(s.undo_depth(), before + 2);
        assert!(
            s.edit_links(&["nope".into()], Some(&LinkTarget::Page(0)), None)
                .is_err()
        );
        assert_eq!(s.add_bookmarks(&[("A".into(), 0), ("B".into(), 1)]).unwrap(), 2);
        assert_eq!(s.bookmarks().len(), 2);
        assert_eq!(s.undo_depth(), before + 3);
        // A failure part way leaves nothing behind.
        assert!(s.add_bookmarks(&[("C".into(), 0), ("D".into(), 9)]).is_err());
        assert_eq!(s.bookmarks().len(), 2);
        s.undo().unwrap();
        assert!(s.bookmarks().is_empty());
    }

    #[test]
    fn counts_split_by_space() {
        let mut s = session();
        let sq = |x0: f64, y0: f64, x1: f64, y1: f64| {
            vec![
                Point::new(x0, y0),
                Point::new(x1, y0),
                Point::new(x1, y1),
                Point::new(x0, y1),
            ]
        };
        s.add_space(0, "A", sq(0.0, 0.0, 300.0, 300.0), None, None).unwrap();
        s.add_space(0, "B", sq(300.0, 0.0, 600.0, 300.0), None, None).unwrap();
        let mut m = Markup::new(
            Kind::Count,
            0,
            vec![
                Point::new(10.0, 10.0),
                Point::new(20.0, 20.0),
                Point::new(400.0, 10.0),
                Point::new(10.0, 500.0),
            ],
        );
        m.subject = "Outlet".into();
        let id = s.add_new_markups("Add", vec![m]).unwrap().remove(0);
        let made = s.split_counts_by_space(None).unwrap();
        assert_eq!(made.len(), 2, "B and outside");
        let counts: Vec<_> = s.doc().markups.iter().filter(|m| m.kind == Kind::Count).collect();
        assert_eq!(counts.len(), 3);
        assert_eq!(s.markup(&id).unwrap().pts.len(), 2);
        assert!(counts.iter().all(|c| c.subject == "Outlet"));
        assert!(
            s.split_counts_by_space(None).unwrap().is_empty(),
            "nothing left to split"
        );
        s.undo().unwrap();
        assert_eq!(s.doc().markups.iter().filter(|m| m.kind == Kind::Count).count(), 1);
    }
}
