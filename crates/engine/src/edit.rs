//! Markup edit commands: add, delete, duplicate, properties, geometry, z-order, clipboard,
//! groups. Each is one undoable step; a step that fails changes nothing.

use std::collections::HashSet;

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup};

use crate::props::{self, MarkupPatch};
use crate::{EngineError, Result, Session, geometry, invalid};

/// Most markups one paste or duplicate may create.
pub const MAX_PASTE: usize = 100_000;

/// Z-order changes (later in a page's list = drawn on top).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    BringToFront,
    SendToBack,
    BringForward,
    SendBackward,
}

impl Arrange {
    pub fn from_name(s: &str) -> Option<Arrange> {
        Some(match s.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "front" | "bring_to_front" => Arrange::BringToFront,
            "back" | "send_to_back" => Arrange::SendToBack,
            "forward" | "bring_forward" => Arrange::BringForward,
            "backward" | "send_backward" => Arrange::SendBackward,
            _ => return None,
        })
    }
}

/// A copy that is written as a new annotation: no file identity, review state or group.
pub fn as_new_copy(m: &Markup) -> Markup {
    let mut c = m.clone();
    c.id.clear();
    c.obj = (0, 0);
    c.annot_index = None;
    c.created.clear();
    c.modified.clear();
    c.subtype.clear();
    c.intent.clear();
    c.set_locked(false);
    c.status.clear();
    c.checked = false;
    c.replies.clear();
    c.state_replies.clear();
    c.irt = None;
    c.group.clear();
    c.stored_look = false;
    c.dirty = true;
    c
}

fn can_copy(m: &Markup) -> bool {
    !m.pts.is_empty() && props::can_create(m.kind)
}

impl Session {
    fn markup_mut_at(&mut self, i: usize) -> Result<&mut Markup> {
        self.doc
            .markups
            .get_mut(i)
            .ok_or_else(|| invalid("markup index out of range"))
    }

    pub(crate) fn indices(&self, ids: &[String]) -> Result<Vec<usize>> {
        if ids.is_empty() {
            return Err(invalid("no markups given (pass ids, or select some first)"));
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for id in ids {
            let i = self.index_of(id)?;
            if seen.insert(i) {
                out.push(i);
            }
        }
        Ok(out)
    }

    /// Add a new markup; returns its id. `m.kind`, `m.page` and `m.pts` are required; the id,
    /// author, subject and (for measurements) the scale at its first point are filled in when
    /// empty. The new markup becomes the selection.
    pub fn add_markup(&mut self, mut m: Markup) -> Result<String> {
        let info = self.page(m.page)?.clone();
        if !props::can_create(m.kind) {
            return Err(invalid(format!(
                "{} markups cannot be created yet (no PDF writer is registered for them); creatable kinds: {}",
                m.kind.name(),
                props::creatable_kinds().join(", ")
            )));
        }
        geometry::check_points(m.kind, &m.pts)?;
        for h in &m.holes {
            geometry::check_finite(h)?;
        }
        geometry::normalize(&mut m);
        m.id = self.new_id();
        m.obj = (0, 0);
        m.annot_index = None;
        m.subtype.clear();
        m.intent.clear();
        m.replies.clear();
        m.state_replies.clear();
        m.irt = None;
        m.stored_look = false;
        m.dirty = true;
        if m.author.is_empty() {
            m.author = self.author.clone();
        }
        if m.layer.is_empty() {
            m.layer = self.doc.markup_layer.clone();
        }
        if m.subject.is_empty() {
            m.subject = props::default_subject(m.kind);
        }
        if m.kind.is_measurement()
            && m.kind != Kind::Count
            && m.scale.is_none()
            && let Some(first) = m.pts.first()
        {
            m.scale = info.scale_at(*first).cloned();
        }
        if m.rect == Rect::default()
            && let Some(b) = bbox(&m.pts)
        {
            m.rect = b.padded(m.line_width + 1.0);
        }
        let id = m.id.clone();
        let label = format!("Add {}", m.kind.name());
        self.edit(&label, move |s| {
            s.doc.markups.push(m);
            Ok(((), true))
        })?;
        self.selection = vec![id.clone()];
        Ok(id)
    }

    /// Delete markups. Locked ones are an error unless `skip_locked`, which leaves them.
    /// Returns how many were deleted.
    pub fn delete_markups(&mut self, ids: &[String], skip_locked: bool) -> Result<usize> {
        let idx = self.indices(ids)?;
        let mut del = Vec::new();
        for i in idx {
            let Some(m) = self.doc.markups.get(i) else { continue };
            if m.locked() {
                if skip_locked {
                    continue;
                }
                return Err(EngineError::Locked(m.id.clone()));
            }
            del.push(i);
        }
        if del.is_empty() {
            return Ok(0);
        }
        del.sort_unstable();
        let n = del.len();
        self.edit("Delete", move |s| {
            for i in del.into_iter().rev() {
                if i >= s.doc.markups.len() {
                    continue;
                }
                let m = s.doc.markups.remove(i);
                if m.in_file() {
                    s.doc.deleted.push(m.obj);
                }
                for r in m.replies.iter().chain(&m.state_replies) {
                    if r.obj.0 != 0 {
                        s.doc.deleted.push(r.obj);
                    }
                }
            }
            Ok((n, true))
        })
    }

    /// Copies of `ids` offset by `(dx, dy)`, added on top; returns the new ids.
    pub fn duplicate_markups(&mut self, ids: &[String], dx: f64, dy: f64) -> Result<Vec<String>> {
        let items: Vec<Markup> = self
            .indices(ids)?
            .into_iter()
            .filter_map(|i| self.doc.markups.get(i).cloned())
            .collect();
        if !(dx.is_finite() && dy.is_finite()) {
            return Err(invalid("dx and dy must be numbers"));
        }
        self.place_copies(&items, None, dx, dy, "Duplicate")
    }

    /// Add copies of `items` to `page` (each to its own page when `None`), moved by (dx, dy).
    pub(crate) fn place_copies(
        &mut self,
        items: &[Markup],
        page: Option<usize>,
        dx: f64,
        dy: f64,
        label: &str,
    ) -> Result<Vec<String>> {
        if items.is_empty() {
            return Err(invalid("nothing to paste: copy some markups first"));
        }
        if items.len() > MAX_PASTE {
            return Err(invalid(format!("at most {MAX_PASTE} markups at once")));
        }
        if let Some(bad) = items.iter().find(|m| !can_copy(m)) {
            return Err(invalid(format!(
                "{} markups cannot be copied yet (no PDF writer is registered for them)",
                bad.kind.name()
            )));
        }
        if let Some(p) = page {
            self.page(p)?;
        }
        let mut copies = Vec::with_capacity(items.len());
        let mut ids = Vec::with_capacity(items.len());
        let mut used: HashSet<String> = self.doc.markups.iter().map(|m| m.id.clone()).collect();
        for src in items {
            let mut c = as_new_copy(src);
            if let Some(p) = page {
                c.page = p;
            }
            if c.page >= self.page_count() {
                return Err(EngineError::NoPage {
                    page: c.page.saturating_add(1),
                    count: self.page_count(),
                });
            }
            geometry::translate(&mut c, dx, dy);
            geometry::check_finite(&c.pts)?;
            c.id = loop {
                let id = markupcraft_revu::new_markup_id();
                if used.insert(id.clone()) {
                    break id;
                }
            };
            ids.push(c.id.clone());
            copies.push(c);
        }
        self.edit(label, move |s| {
            s.doc.markups.extend(copies);
            Ok(((), true))
        })?;
        self.selection = ids.clone();
        Ok(ids)
    }

    /// Change properties of markups; returns how many changed.
    pub fn set_properties(&mut self, ids: &[String], patch: &MarkupPatch) -> Result<usize> {
        patch.validate()?;
        if patch.is_empty() {
            return Err(invalid("no properties given"));
        }
        let idx = self.indices(ids)?;
        self.edit("Properties", |s| {
            for &i in &idx {
                patch.apply(s.markup_mut_at(i)?)?;
            }
            Ok((idx.len(), true))
        })
    }

    /// Run a geometry change on each markup, refusing locked or read-only ones.
    pub(crate) fn geometry_edit(
        &mut self,
        ids: &[String],
        label: &str,
        mut f: impl FnMut(&mut Markup) -> Result<()>,
    ) -> Result<usize> {
        let idx = self.indices(ids)?;
        self.edit(label, |s| {
            for &i in &idx {
                let m = s.markup_mut_at(i)?;
                if m.locked() {
                    return Err(EngineError::Locked(m.id.clone()));
                }
                if !props::geometry_editable(m) {
                    return Err(invalid(format!(
                        "markup {} ({}) keeps the look it was saved with; MarkupCraft cannot redraw its geometry yet",
                        m.id,
                        m.kind.name()
                    )));
                }
                f(m)?;
                geometry::check_finite(&m.pts)?;
            }
            Ok((idx.len(), true))
        })
    }

    /// Move markups by (dx, dy) points.
    pub fn move_markups(&mut self, ids: &[String], dx: f64, dy: f64) -> Result<usize> {
        if !(dx.is_finite() && dy.is_finite()) {
            return Err(invalid("dx and dy must be numbers"));
        }
        self.geometry_edit(ids, "Move", |m| {
            geometry::translate(m, dx, dy);
            Ok(())
        })
    }

    /// Rotate markups counter-clockwise by `degrees`, each about `center` or its own centre.
    pub fn rotate_markups(&mut self, ids: &[String], degrees: f64, center: Option<Point>) -> Result<usize> {
        self.geometry_edit(ids, "Rotate", |m| {
            let c = match center {
                Some(c) => c,
                None => geometry::center(m).ok_or_else(|| invalid("the markup has no points"))?,
            };
            geometry::rotate(m, degrees, c)
        })
    }

    /// Scale a markup so its points' bounding box becomes `to`.
    pub fn resize_markup(&mut self, id: &str, to: Rect) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Resize", |m| geometry::resize(m, to))
            .map(|_| ())
    }

    /// Replace a markup's points.
    pub fn set_points(&mut self, id: &str, pts: Vec<Point>) -> Result<()> {
        let mut pts = Some(pts);
        self.geometry_edit(&[id.to_string()], "Edit Vertices", |m| {
            geometry::set_points(m, pts.take().unwrap_or_default())
        })
        .map(|_| ())
    }

    pub fn move_vertex(&mut self, id: &str, index: usize, p: Point) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Move Vertex", |m| geometry::move_vertex(m, index, p))
            .map(|_| ())
    }

    pub fn insert_vertex(&mut self, id: &str, index: usize, p: Point) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Add Vertex", |m| {
            geometry::insert_vertex(m, index, p)
        })
        .map(|_| ())
    }

    pub fn delete_vertex(&mut self, id: &str, index: usize) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Delete Vertex", |m| {
            geometry::delete_vertex(m, index)
        })
        .map(|_| ())
    }

    /// Change the z-order of markups within their pages. Returns whether anything moved.
    pub fn arrange(&mut self, ids: &[String], how: Arrange) -> Result<bool> {
        let idx = self.indices(ids)?;
        let selected: HashSet<usize> = idx.iter().copied().collect();
        let pages: Vec<usize> = {
            let mut p: Vec<usize> = idx
                .iter()
                .filter_map(|i| self.doc.markups.get(*i).map(|m| m.page))
                .collect();
            p.sort_unstable();
            p.dedup();
            p
        };
        let mut out = self.doc.markups.clone();
        let mut any = false;
        for page in pages {
            let slots: Vec<usize> = (0..self.doc.markups.len())
                .filter(|i| self.doc.markups.get(*i).is_some_and(|m| m.page == page))
                .collect();
            let mut seq = slots.clone();
            let is_sel = |i: &usize| selected.contains(i);
            match how {
                Arrange::BringToFront => {
                    let (sel, rest): (Vec<usize>, Vec<usize>) = seq.iter().partition(|i| is_sel(i));
                    seq = rest.into_iter().chain(sel).collect();
                }
                Arrange::SendToBack => {
                    let (sel, rest): (Vec<usize>, Vec<usize>) = seq.iter().partition(|i| is_sel(i));
                    seq = sel.into_iter().chain(rest).collect();
                }
                Arrange::BringForward => {
                    for k in (0..seq.len().saturating_sub(1)).rev() {
                        if let (Some(a), Some(b)) = (seq.get(k).copied(), seq.get(k + 1).copied())
                            && is_sel(&a)
                            && !is_sel(&b)
                        {
                            seq.swap(k, k + 1);
                        }
                    }
                }
                Arrange::SendBackward => {
                    for k in 1..seq.len() {
                        if let (Some(a), Some(b)) = (seq.get(k).copied(), seq.get(k - 1).copied())
                            && is_sel(&a)
                            && !is_sel(&b)
                        {
                            seq.swap(k, k - 1);
                        }
                    }
                }
            }
            for (slot, src) in slots.iter().zip(&seq) {
                if slot != src {
                    any = true;
                }
                if let (Some(dst), Some(m)) = (out.get_mut(*slot), self.doc.markups.get(*src)) {
                    *dst = m.clone();
                }
            }
        }
        if !any {
            return Ok(false);
        }
        self.edit("Arrange", move |s| {
            s.doc.markups = out;
            s.doc.order_changed = true;
            Ok((true, true))
        })
    }

    /// Copy markups to the clipboard; returns how many.
    pub fn copy_markups(&mut self, ids: &[String]) -> Result<usize> {
        let items: Vec<Markup> = self
            .indices(ids)?
            .into_iter()
            .filter_map(|i| self.doc.markups.get(i).cloned())
            .collect();
        if let Some(bad) = items.iter().find(|m| !can_copy(m)) {
            return Err(invalid(format!(
                "{} markups cannot be copied yet (no PDF writer is registered for them)",
                bad.kind.name()
            )));
        }
        let n = items.len();
        self.clipboard = items;
        Ok(n)
    }

    /// Copy, then delete (locked markups are copied but stay).
    pub fn cut_markups(&mut self, ids: &[String]) -> Result<usize> {
        self.copy_markups(ids)?;
        self.delete_markups(ids, true)
    }

    /// Paste the clipboard onto `page` (each copy on its own page when `None`). With `at`,
    /// the clipboard's centre lands there; without, copies keep their positions (paste in
    /// place). Returns the new ids, which become the selection.
    pub fn paste(&mut self, page: Option<usize>, at: Option<Point>) -> Result<Vec<String>> {
        let items = self.clipboard.clone();
        let (dx, dy) = match at {
            Some(p) => {
                geometry::check_finite(&[p])?;
                let all: Vec<Point> = items.iter().flat_map(|m| m.pts.iter().copied()).collect();
                let b = bbox(&all).ok_or_else(|| invalid("nothing to paste: copy some markups first"))?;
                (p.x - (b.x0 + b.x1) / 2.0, p.y - (b.y0 + b.y1) / 2.0)
            }
            None => (0.0, 0.0),
        };
        self.place_copies(&items, page, dx, dy, "Paste")
    }

    /// Group markups (same page, at least two); returns the group id (its leader's id).
    pub fn group(&mut self, ids: &[String]) -> Result<String> {
        let idx = self.indices(ids)?;
        if idx.len() < 2 {
            return Err(invalid("a group needs at least two markups"));
        }
        let pages: HashSet<usize> = idx
            .iter()
            .filter_map(|i| self.doc.markups.get(*i).map(|m| m.page))
            .collect();
        if pages.len() > 1 {
            return Err(invalid("grouped markups must be on the same page"));
        }
        let leader = idx
            .first()
            .and_then(|i| self.doc.markups.get(*i))
            .map(|m| m.id.clone())
            .ok_or_else(|| invalid("no markups given"))?;
        let gid = leader.clone();
        self.edit("Group", move |s| {
            for &i in &idx {
                let m = s.markup_mut_at(i)?;
                m.group = gid.clone();
                m.dirty = true;
            }
            Ok(((), true))
        })?;
        Ok(leader)
    }

    /// Dissolve the groups these markups belong to; returns how many markups left a group.
    pub fn ungroup(&mut self, ids: &[String]) -> Result<usize> {
        let idx = self.indices(ids)?;
        let groups: HashSet<String> = idx
            .iter()
            .filter_map(|i| self.doc.markups.get(*i))
            .filter(|m| !m.group.is_empty())
            .map(|m| m.group.clone())
            .collect();
        if groups.is_empty() {
            return Ok(0);
        }
        self.edit("Ungroup", |s| {
            let mut n = 0;
            for m in s.doc.markups.iter_mut().filter(|m| groups.contains(&m.group)) {
                m.group.clear();
                m.dirty = true;
                n += 1;
            }
            Ok((n, true))
        })
    }

    /// Lock or unlock markups; returns how many changed.
    pub fn set_locked(&mut self, ids: &[String], locked: bool) -> Result<usize> {
        let idx: Vec<usize> = self
            .indices(ids)?
            .into_iter()
            .filter(|i| self.doc.markups.get(*i).is_some_and(|m| m.locked() != locked))
            .collect();
        if idx.is_empty() {
            return Ok(0);
        }
        self.edit(if locked { "Lock" } else { "Unlock" }, |s| {
            for &i in &idx {
                let m = s.markup_mut_at(i)?;
                m.set_locked(locked);
                m.dirty = true;
            }
            Ok((idx.len(), true))
        })
    }
}
