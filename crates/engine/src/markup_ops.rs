//! Markup operations of the drawing and takeoff tools: the Eraser on ink strokes, Count series
//! (add items, delete one item, split, merge), lasso selection, a cutout turned into its own
//! measurement, and text markups re-created from flattened content MarkupCraft wrote.

use markupcraft_geom::{Point, bbox, dist_to_segment, point_in_polygon};
use markupcraft_model::{Kind, Markup};

use crate::{EngineError, Result, Session, geometry, invalid, props};

/// Most points an eraser path or a lasso may have.
pub const MAX_PATH: usize = 100_000;

/// What the eraser leaves of an ink markup's strokes: every point within `radius` of the
/// eraser `path` is removed and the strokes split where points went. `None` = untouched.
pub fn erase_strokes(m: &Markup, path: &[Point], radius: f64) -> Option<Vec<Vec<Point>>> {
    if !matches!(m.kind, Kind::Ink | Kind::Highlight) || path.is_empty() || radius.is_nan() || radius <= 0.0 {
        return None;
    }
    let reach = radius + m.line_width / 2.0;
    let near = |p: Point| {
        if path.len() == 1 {
            return path.first().is_some_and(|q| q.dist(p) <= reach);
        }
        path.windows(2).any(|s| dist_to_segment(p, s[0], s[1]) <= reach)
    };
    let mut starts = vec![0];
    starts.extend(m.strokes.iter().copied().filter(|s| *s > 0 && *s < m.pts.len()));
    starts.push(m.pts.len());
    starts.dedup();
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut changed = false;
    for w in starts.windows(2) {
        let Some(stroke) = m.pts.get(w[0]..w[1]) else { continue };
        let mut cur: Vec<Point> = Vec::new();
        for p in stroke {
            if near(*p) {
                changed = true;
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            } else {
                cur.push(*p);
            }
        }
        if cur.len() >= 2 {
            out.push(cur);
        } else if !cur.is_empty() {
            // a lone point left over is erased with its neighbours
            changed = true;
        }
    }
    changed.then_some(out)
}

/// The markups on `page` whose whole extent lies inside the closed `ring` (lasso selection).
pub fn inside_lasso<'a>(markups: impl Iterator<Item = &'a Markup>, page: usize, ring: &[Point]) -> Vec<String> {
    if ring.len() < 3 {
        return Vec::new();
    }
    markups
        .filter(|m| m.page == page)
        .filter(|m| {
            let pts: Vec<Point> = if m.pts.is_empty() {
                m.rect.corners().to_vec()
            } else {
                m.pts.iter().take(2000).copied().collect()
            };
            !pts.is_empty() && pts.iter().all(|p| point_in_polygon(*p, ring))
        })
        .map(|m| m.id.clone())
        .collect()
}

impl Session {
    /// The Eraser: drag `path` (page user space) across Pen and Highlight strokes on `page`;
    /// points within `radius` points of it are removed, strokes split, emptied markups
    /// deleted. One undo step; returns how many markups changed.
    pub fn erase_ink(&mut self, page: usize, path: &[Point], radius: f64) -> Result<usize> {
        self.page(page)?;
        if path.len() > MAX_PATH {
            return Err(invalid(format!("an eraser path has at most {MAX_PATH} points")));
        }
        geometry::check_finite(path)?;
        if !(radius.is_finite() && radius > 0.0 && radius <= 500.0) {
            return Err(invalid("the eraser radius must be 0 to 500 points"));
        }
        let mut changes: Vec<(usize, Vec<Vec<Point>>)> = Vec::new();
        for (i, m) in self.doc.markups.iter().enumerate() {
            if m.page != page || m.locked() || !props::geometry_editable(m) {
                continue;
            }
            if let Some(left) = erase_strokes(m, path, radius) {
                changes.push((i, left));
            }
        }
        if changes.is_empty() {
            return Ok(0);
        }
        let n = changes.len();
        self.edit("Erase", move |s| {
            let mut gone = Vec::new();
            for (i, strokes) in changes {
                let Some(m) = s.doc.markups.get_mut(i) else { continue };
                if strokes.is_empty() {
                    gone.push(i);
                    continue;
                }
                m.strokes.clear();
                m.pts.clear();
                for st in strokes {
                    if !m.pts.is_empty() {
                        m.strokes.push(m.pts.len());
                    }
                    m.pts.extend(st);
                }
                if let Some(b) = bbox(&m.pts) {
                    m.rect = b.padded(m.line_width + 1.0);
                }
                m.dirty = true;
            }
            gone.sort_unstable();
            for i in gone.into_iter().rev() {
                if i < s.doc.markups.len() {
                    let m = s.doc.markups.remove(i);
                    if m.in_file() {
                        s.doc.deleted.push(m.obj);
                    }
                }
            }
            Ok((n, true))
        })
    }

    fn count_index(&self, id: &str) -> Result<usize> {
        let i = self.indices(&[id.to_string()])?.first().copied().unwrap_or(usize::MAX);
        match self.doc.markups.get(i) {
            Some(m) if m.kind != Kind::Count => {
                Err(invalid(format!("markup {id} is a {}, not a Count", m.kind.name())))
            }
            Some(m) if m.locked() => Err(EngineError::Locked(m.id.clone())),
            Some(_) => Ok(i),
            None => Err(invalid(format!("no markup {id}"))),
        }
    }

    /// Resume Count: add items to an existing Count series.
    pub fn add_count_items(&mut self, id: &str, pts: &[Point]) -> Result<usize> {
        let i = self.count_index(id)?;
        geometry::check_finite(pts)?;
        if pts.is_empty() {
            return Err(invalid("no points to add"));
        }
        let pts = pts.to_vec();
        self.edit("Resume Count", move |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            if m.pts.len() + pts.len() > geometry::MAX_POINTS {
                return Err(invalid("a count holds at most 100000 items"));
            }
            m.pts.extend(pts);
            m.dirty = true;
            Ok((m.pts.len(), true))
        })
    }

    /// Delete one item (0-based) from a Count series; the series renumbers. Deleting the last
    /// item deletes the markup. Returns the items left.
    pub fn delete_count_item(&mut self, id: &str, item: usize) -> Result<usize> {
        let i = self.count_index(id)?;
        let n = self.doc.markups.get(i).map_or(0, |m| m.pts.len());
        if item >= n {
            return Err(invalid(format!(
                "the count has {n} items; there is no item {}",
                item + 1
            )));
        }
        self.edit("Delete Count Item", move |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            m.pts.remove(item);
            m.dirty = true;
            let left = m.pts.len();
            if left == 0 {
                let m = s.doc.markups.remove(i);
                if m.in_file() {
                    s.doc.deleted.push(m.obj);
                }
            }
            Ok((left, true))
        })
    }

    /// Split items (0-based) out of a Count series into a new series with the same look,
    /// subject and status. Returns the new markup's id.
    pub fn split_count(&mut self, id: &str, items: &[usize]) -> Result<String> {
        let i = self.count_index(id)?;
        let m = self
            .doc
            .markups
            .get(i)
            .cloned()
            .ok_or_else(|| invalid("markup index out of range"))?;
        let mut take: Vec<usize> = items.iter().copied().filter(|k| *k < m.pts.len()).collect();
        take.sort_unstable();
        take.dedup();
        if take.is_empty() {
            return Err(invalid("choose the items to split off"));
        }
        if take.len() == m.pts.len() {
            return Err(invalid("a split leaves at least one item in the series"));
        }
        let mut new = crate::edit::as_new_copy(&m);
        new.id = self.new_id();
        new.pts = take.iter().filter_map(|k| m.pts.get(*k).copied()).collect();
        new.group.clear();
        if let Some(b) = bbox(&new.pts) {
            new.rect = b.padded(8.0);
        }
        let new_id = new.id.clone();
        self.edit("Split Count", move |s| {
            let src = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            for k in take.iter().rev() {
                if *k < src.pts.len() {
                    src.pts.remove(*k);
                }
            }
            src.dirty = true;
            s.doc.markups.insert(i + 1, new);
            Ok(((), true))
        })?;
        self.selection = vec![new_id.clone()];
        Ok(new_id)
    }

    /// Merge Count series into the first: their items join it and they are deleted. All must
    /// be on the same page. Returns the merged series' item count.
    pub fn merge_counts(&mut self, ids: &[String]) -> Result<usize> {
        if ids.len() < 2 {
            return Err(invalid("select two or more counts to merge"));
        }
        let idx: Vec<usize> = ids.iter().map(|id| self.count_index(id)).collect::<Result<_>>()?;
        let first = *idx.first().ok_or_else(|| invalid("no counts"))?;
        let page = self.doc.markups.get(first).map_or(0, |m| m.page);
        if idx
            .iter()
            .any(|i| self.doc.markups.get(*i).is_some_and(|m| m.page != page))
        {
            return Err(invalid("counts on different pages cannot be merged"));
        }
        let keep = self.doc.markups.get(first).map(|m| m.id.clone()).unwrap_or_default();
        let r = self.edit("Merge Counts", move |s| {
            let mut extra: Vec<Point> = Vec::new();
            let mut rest: Vec<usize> = idx.iter().copied().filter(|i| *i != first).collect();
            for i in &rest {
                if let Some(m) = s.doc.markups.get(*i) {
                    extra.extend(m.pts.iter().copied());
                }
            }
            let n = {
                let m = s
                    .doc
                    .markups
                    .get_mut(first)
                    .ok_or_else(|| invalid("markup index out of range"))?;
                m.pts.extend(extra);
                m.dirty = true;
                m.pts.len()
            };
            rest.sort_unstable();
            rest.dedup();
            for i in rest.into_iter().rev() {
                if i < s.doc.markups.len() {
                    let m = s.doc.markups.remove(i);
                    if m.in_file() {
                        s.doc.deleted.push(m.obj);
                    }
                }
            }
            Ok((n, true))
        })?;
        self.selection = vec![keep];
        Ok(r)
    }

    /// Select the markups inside the lasso `ring` on `page` (added to the selection when
    /// `add`). Returns how many are selected.
    pub fn select_lasso(&mut self, page: usize, ring: &[Point], add: bool) -> Result<usize> {
        self.page(page)?;
        if ring.len() > MAX_PATH {
            return Err(invalid(format!("a lasso has at most {MAX_PATH} points")));
        }
        geometry::check_finite(ring)?;
        let hit = inside_lasso(self.doc.markups.iter(), page, ring);
        let mut sel = if add { self.selection.clone() } else { Vec::new() };
        for id in hit {
            if !sel.contains(&id) {
                sel.push(id);
            }
        }
        self.selection = sel;
        Ok(self.selection.len())
    }

    /// Convert to Arc: the segment starting at vertex `start` becomes an arc (a quarter of the
    /// chord deep, outward on closed shapes). Returns the arc's index.
    pub fn convert_segment_to_arc(&mut self, id: &str, start: usize) -> Result<usize> {
        let mut arc = 0;
        self.geometry_edit(&[id.to_string()], "Convert to Arc", |m| {
            arc = markupcraft_model::measure_extras::convert_to_arc(m, start, 0.0)
                .ok_or_else(|| invalid("that segment cannot become an arc (not a segment, or already one)"))?;
            Ok(())
        })?;
        Ok(arc)
    }

    /// Add a vertex to cutout `hole` after vertex `after` (0-based).
    pub fn insert_hole_vertex(&mut self, id: &str, hole: usize, after: usize, p: Point) -> Result<()> {
        geometry::check_finite(&[p])?;
        self.geometry_edit(&[id.to_string()], "Add Cutout Vertex", |m| {
            if markupcraft_model::measure_extras::insert_hole_vertex(m, hole, after, p) {
                Ok(())
            } else {
                Err(invalid("no such cutout segment"))
            }
        })
        .map(|_| ())
    }

    /// Delete vertex `idx` of cutout `hole` (a cutout keeps three).
    pub fn delete_hole_vertex(&mut self, id: &str, hole: usize, idx: usize) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Delete Cutout Vertex", |m| {
            if markupcraft_model::measure_extras::erase_hole_vertex(m, hole, idx) {
                Ok(())
            } else {
                Err(invalid("a cutout keeps at least three vertices"))
            }
        })
        .map(|_| ())
    }

    /// Convert to Line: arc `arc` back to a straight segment.
    pub fn straighten_arc(&mut self, id: &str, arc: usize) -> Result<()> {
        self.geometry_edit(&[id.to_string()], "Convert to Line", |m| {
            if arc >= m.arcs.len() {
                return Err(invalid(format!("the markup has {} arcs", m.arcs.len())));
            }
            markupcraft_model::measure_extras::straighten_arc(m, arc);
            Ok(())
        })
        .map(|_| ())
    }

    /// Bend arc `arc` so it passes as near `toward` as its chord allows.
    pub fn bend_arc(&mut self, id: &str, arc: usize, toward: Point) -> Result<()> {
        geometry::check_finite(&[toward])?;
        self.geometry_edit(&[id.to_string()], "Bend Arc", |m| {
            if arc >= m.arcs.len() {
                return Err(invalid(format!("the markup has {} arcs", m.arcs.len())));
            }
            markupcraft_model::measure_extras::bend_arc(m, arc, toward);
            Ok(())
        })
        .map(|_| ())
    }

    /// Add a reply (`/IRT` comment) to markup `id` by the session's author. Returns how many
    /// replies it has.
    pub fn add_reply(&mut self, id: &str, text: &str) -> Result<usize> {
        let text = text.trim();
        if text.is_empty() || text.len() > props::MAX_TEXT {
            return Err(invalid("a reply needs text (at most 64 KB)"));
        }
        let i = self.indices(&[id.to_string()])?.first().copied().unwrap_or(usize::MAX);
        let reply = markupcraft_model::Reply {
            id: self.new_id(),
            author: self.author.clone(),
            date: markupcraft_revu::pdf_date_now(),
            text: text.to_string(),
            dirty: true,
            ..Default::default()
        };
        self.edit("Add Reply", move |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            m.replies.push(reply);
            m.dirty = true;
            Ok((m.replies.len(), true))
        })
    }

    /// Change the text of reply `index` (0-based) of markup `id`.
    pub fn edit_reply(&mut self, id: &str, index: usize, text: &str) -> Result<()> {
        if text.len() > props::MAX_TEXT {
            return Err(invalid("a reply is at most 64 KB"));
        }
        let i = self.indices(&[id.to_string()])?.first().copied().unwrap_or(usize::MAX);
        let text = text.to_string();
        self.edit("Edit Reply", move |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            let n = m.replies.len();
            let r = m
                .replies
                .get_mut(index)
                .ok_or_else(|| invalid(format!("the markup has {n} replies")))?;
            r.text = text;
            r.dirty = true;
            Ok(((), true))
        })
    }

    /// Delete reply `index` (0-based) of markup `id`. Returns how many replies are left.
    pub fn delete_reply(&mut self, id: &str, index: usize) -> Result<usize> {
        let i = self.indices(&[id.to_string()])?.first().copied().unwrap_or(usize::MAX);
        self.edit("Delete Reply", move |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            if index >= m.replies.len() {
                return Err(invalid(format!("the markup has {} replies", m.replies.len())));
            }
            let r = m.replies.remove(index);
            let left = m.replies.len();
            if r.obj.0 != 0 {
                s.doc.deleted.push(r.obj);
            }
            Ok((left, true))
        })
    }

    /// File Attachment: a markup at `at` (its top-left corner, page user space) that embeds the
    /// file at `path` (at most 50 MB), shown as `icon` (PushPin, Paperclip, Graph, Tag).
    /// Returns its id.
    pub fn add_file_attachment(
        &mut self,
        page: usize,
        at: Point,
        path: &std::path::Path,
        icon: &str,
    ) -> Result<String> {
        let meta = std::fs::metadata(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        if !meta.is_file() || meta.len() > markupcraft_revu::kinds::more::MAX_ATTACHMENT as u64 {
            return Err(invalid(format!("{} is not a file of at most 50 MB", path.display())));
        }
        let data = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".into());
        geometry::check_finite(&[at])?;
        let r = markupcraft_geom::Rect::new(at.x, at.y - 24.0, at.x + 18.0, at.y);
        let mut m = Markup::new(Kind::Attachment, page, r.corners().to_vec());
        m.rect = r;
        m.color = markupcraft_model::Color::rgb(0.2, 0.4, 0.9);
        m.icon = if markupcraft_revu::kinds::more::ATTACHMENT_ICONS.contains(&icon) {
            icon.to_string()
        } else {
            "PushPin".into()
        };
        m.subject = "File Attachment".into();
        m.contents = name.clone();
        m.attachment_name = name;
        m.attachment_data = Some(std::sync::Arc::new(data));
        self.add_markup(m)
    }

    /// The file a File Attachment markup carries: (name, bytes).
    pub fn attachment_file(&self, id: &str) -> Result<(String, Vec<u8>)> {
        let m = self.markup(id)?;
        if m.kind != Kind::Attachment {
            return Err(invalid(format!(
                "markup {id} is a {}, not a File Attachment",
                m.kind.name()
            )));
        }
        if let Some(d) = &m.attachment_data {
            return Ok((m.attachment_name.clone(), d.as_ref().clone()));
        }
        let obj = markupcraft_revu::cos::ObjRef::new(m.obj.0, m.obj.1);
        let bytes = self
            .file
            .cos
            .dict(&markupcraft_revu::cos::Object::Ref(obj))
            .and_then(|a| markupcraft_revu::kinds::more::attachment_bytes(&self.file.cos, &a))
            .ok_or_else(|| invalid("the attachment's file cannot be read"))?;
        Ok((m.attachment_name.clone(), bytes))
    }

    /// Import Markups from another PDF: its markups go onto the same page numbers here (pages
    /// past the end are skipped), as new annotations with their look, subject, status and
    /// custom column values; a markup whose id is already here gets a new id. Kinds MarkupCraft
    /// cannot write (and snapshots, whose content lives in the other file) are skipped. One
    /// undo step; returns (imported, skipped).
    pub fn import_markups_from_pdf(&mut self, path: &std::path::Path) -> Result<(usize, usize)> {
        let (_, other) = markupcraft_revu::open(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let count = self.page_count();
        let mut copies = Vec::new();
        let mut skipped = 0usize;
        let mut used: std::collections::HashSet<String> = self.doc.markups.iter().map(|m| m.id.clone()).collect();
        for m in other.markups.iter().take(100_000) {
            if m.page >= count || !props::can_create(m.kind) || m.kind == Kind::Snapshot || m.pts.is_empty() {
                skipped += 1;
                continue;
            }
            let mut c = crate::edit::as_new_copy(m);
            c.status = m.status.clone();
            c.checked = m.checked;
            c.id = if m.id.is_empty() || used.contains(&m.id) {
                self.new_id()
            } else {
                m.id.clone()
            };
            used.insert(c.id.clone());
            if geometry::check_points(c.kind, &c.pts).is_err() {
                skipped += 1;
                continue;
            }
            copies.push(c);
        }
        if copies.is_empty() {
            return Err(invalid(format!(
                "{} has no markups MarkupCraft can bring in ({skipped} skipped)",
                path.display()
            )));
        }
        let n = copies.len();
        let ids: Vec<String> = copies.iter().map(|m| m.id.clone()).collect();
        self.edit("Import Markups", move |s| {
            s.doc.markups.extend(copies);
            Ok(((), true))
        })?;
        self.selection = ids;
        Ok((n, skipped))
    }

    /// A temporary page scale: `pages` measure with `sc` for this session, but the scale is not
    /// written on save (the file keeps the scale it had, so it comes back when the document is
    /// opened again). Measurements on those pages take it when `apply_to_markups`.
    pub fn set_page_scale_temporary(
        &mut self,
        pages: &[usize],
        sc: &markupcraft_model::Scale,
        apply_to_markups: bool,
    ) -> Result<usize> {
        if !sc.valid() {
            return Err(invalid("the scale has no usable /X conversion"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let pages = pages.to_vec();
        let sc = sc.clone();
        self.edit("Temporary Scale", move |s| {
            for p in &pages {
                if let Some(pi) = s.doc.pages.get_mut(*p) {
                    pi.scale = Some(sc.clone());
                    pi.viewports.clear();
                    pi.scale_changed = false;
                }
            }
            let n = if apply_to_markups {
                markupcraft_model::measure_extras::recalculate(&mut s.doc, &pages)
            } else {
                0
            };
            Ok((n, true))
        })
    }

    /// Recalculate: give every measurement on `pages` (0-based) the scale now in effect where
    /// it sits, keeping its units and precision. One undo step; returns how many changed.
    pub fn recalculate_measurements(&mut self, pages: &[usize]) -> Result<usize> {
        for p in pages {
            self.page(*p)?;
        }
        let pages = pages.to_vec();
        self.edit("Recalculate", move |s| {
            let n = markupcraft_model::measure_extras::recalculate(&mut s.doc, &pages);
            Ok((n, n > 0))
        })
    }

    /// Turn cutout `hole` (0-based) of an Area or Volume into its own measurement of the same
    /// kind and look; the hole stays in the parent. Returns the new markup's id.
    pub fn cutout_to_measurement(&mut self, id: &str, hole: usize) -> Result<String> {
        let i = self.indices(&[id.to_string()])?.first().copied().unwrap_or(usize::MAX);
        let m = self
            .doc
            .markups
            .get(i)
            .cloned()
            .ok_or_else(|| invalid(format!("no markup {id}")))?;
        let ring = m
            .holes
            .get(hole)
            .cloned()
            .ok_or_else(|| invalid(format!("markup {id} has {} cutouts", m.holes.len())))?;
        let mut new = crate::edit::as_new_copy(&m);
        new.pts = ring;
        new.holes.clear();
        new.arcs.clear();
        new.caption_offset = None;
        new.rect = markupcraft_geom::Rect::default();
        self.add_markup(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn erasing_splits_strokes() {
        let mut m = Markup::new(Kind::Ink, 0, (0..=10).map(|i| p(f64::from(i) * 10.0, 0.0)).collect());
        m.line_width = 1.0;
        // through the middle point (50, 0)
        let left = erase_strokes(&m, &[p(50.0, -10.0), p(50.0, 10.0)], 3.0).unwrap();
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].len(), 5);
        assert_eq!(left[1].len(), 5);
        assert!(erase_strokes(&m, &[p(500.0, 500.0)], 3.0).is_none());
        // a rectangle is not ink
        let r = Markup::new(Kind::Rectangle, 0, vec![p(0.0, 0.0)]);
        assert!(erase_strokes(&r, &[p(0.0, 0.0)], 3.0).is_none());
        // erasing it all leaves nothing
        assert!(
            erase_strokes(&m, &[p(-10.0, 0.0), p(110.0, 0.0)], 3.0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn lasso_takes_whole_markups() {
        let a = Markup {
            id: "a".into(),
            ..Markup::new(Kind::Line, 0, vec![p(10.0, 10.0), p(20.0, 20.0)])
        };
        let b = Markup {
            id: "b".into(),
            ..Markup::new(Kind::Line, 0, vec![p(10.0, 10.0), p(200.0, 20.0)])
        };
        let ring = [p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0), p(0.0, 50.0)];
        assert_eq!(inside_lasso([a, b].iter(), 0, &ring), vec!["a".to_string()]);
    }
}
