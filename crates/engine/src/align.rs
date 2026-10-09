//! Arranging several markups against each other (Revu's Markup > Arrange menu): align their
//! edges or centres, distribute them evenly, flip them, copy them to other pages and take one
//! out of its group. Each is one undoable step.

use std::collections::HashSet;

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::Markup;

use crate::{Result, Session, geometry, invalid};

/// Which edge or centre line to align to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

impl Align {
    pub fn from_name(s: &str) -> Option<Align> {
        Some(match s.to_ascii_lowercase().as_str() {
            "left" => Align::Left,
            "center" | "centre" => Align::Center,
            "right" => Align::Right,
            "top" => Align::Top,
            "middle" => Align::Middle,
            "bottom" => Align::Bottom,
            _ => return None,
        })
    }

    fn horizontal(self) -> bool {
        matches!(self, Align::Left | Align::Center | Align::Right)
    }
}

/// The box an arrange command lines up: the markup's points (its outline), else its `/Rect`.
pub fn extent(m: &Markup) -> Rect {
    let mut all: Vec<Point> = m.pts.clone();
    for h in &m.holes {
        all.extend(h.iter().copied());
    }
    bbox(&all).unwrap_or(m.rect).normalized()
}

fn union(rs: &[Rect]) -> Option<Rect> {
    let pts: Vec<Point> = rs
        .iter()
        .flat_map(|r| [Point::new(r.x0, r.y0), Point::new(r.x1, r.y1)])
        .collect();
    bbox(&pts)
}

impl Session {
    fn extents(&self, ids: &[String]) -> Result<Vec<(String, Rect)>> {
        let idx = self.indices(ids)?;
        Ok(idx
            .iter()
            .filter_map(|i| self.doc.markups.get(*i))
            .map(|m| (m.id.clone(), extent(m)))
            .collect())
    }

    /// Line markups up on the selection's outer edge (Left, Right, Top, Bottom) or its centre
    /// line (Center, Middle). Needs two or more markups; returns how many moved.
    pub fn align_markups(&mut self, ids: &[String], how: Align) -> Result<usize> {
        let ext = self.extents(ids)?;
        if ext.len() < 2 {
            return Err(invalid("align needs at least two markups"));
        }
        let rects: Vec<Rect> = ext.iter().map(|(_, r)| *r).collect();
        let all = union(&rects).ok_or_else(|| invalid("the markups have no extent"))?;
        let moves: Vec<(String, f64, f64)> = ext
            .into_iter()
            .map(|(id, r)| {
                let d = match how {
                    Align::Left => all.x0 - r.x0,
                    Align::Right => all.x1 - r.x1,
                    Align::Center => (all.x0 + all.x1) / 2.0 - (r.x0 + r.x1) / 2.0,
                    Align::Top => all.y1 - r.y1,
                    Align::Bottom => all.y0 - r.y0,
                    Align::Middle => (all.y0 + all.y1) / 2.0 - (r.y0 + r.y1) / 2.0,
                };
                if how.horizontal() { (id, d, 0.0) } else { (id, 0.0, d) }
            })
            .filter(|(_, dx, dy)| dx.abs() > 1e-9 || dy.abs() > 1e-9)
            .collect();
        self.apply_moves(moves, "Align")
    }

    /// Space markups evenly between the outermost two: equal gaps between neighbours, across
    /// (`horizontal`) or up the page. Needs three or more; returns how many moved.
    pub fn distribute_markups(&mut self, ids: &[String], horizontal: bool) -> Result<usize> {
        let mut ext = self.extents(ids)?;
        if ext.len() < 3 {
            return Err(invalid("distribute needs at least three markups"));
        }
        let lo = |r: &Rect| if horizontal { r.x0 } else { r.y0 };
        let size = |r: &Rect| if horizontal { r.width() } else { r.height() };
        ext.sort_by(|a, b| lo(&a.1).total_cmp(&lo(&b.1)));
        let (Some(first), Some(last)) = (ext.first(), ext.last()) else {
            return Ok(0);
        };
        let start = lo(&first.1);
        let end = lo(&last.1) + size(&last.1);
        let total: f64 = ext.iter().map(|(_, r)| size(r)).sum();
        let gap = (end - start - total) / (ext.len() - 1) as f64;
        let mut at = start;
        let mut moves = Vec::new();
        for (id, r) in &ext {
            let d = at - lo(r);
            if d.abs() > 1e-9 {
                moves.push(if horizontal {
                    (id.clone(), d, 0.0)
                } else {
                    (id.clone(), 0.0, d)
                });
            }
            at += size(r) + gap;
        }
        self.apply_moves(moves, "Distribute")
    }

    fn apply_moves(&mut self, moves: Vec<(String, f64, f64)>, label: &str) -> Result<usize> {
        if moves.is_empty() {
            return Ok(0);
        }
        let ids: Vec<String> = moves.iter().map(|(id, _, _)| id.clone()).collect();
        let mut it = moves.into_iter();
        self.geometry_edit(&ids, label, |m| {
            if let Some((_, dx, dy)) = it.next() {
                geometry::translate(m, dx, dy);
            }
            Ok(())
        })
    }

    /// Mirror markups left-right (`horizontal`) or top-bottom about the centre of their joint
    /// extent. Returns how many changed.
    pub fn flip_markups(&mut self, ids: &[String], horizontal: bool) -> Result<usize> {
        let ext = self.extents(ids)?;
        let rects: Vec<Rect> = ext.iter().map(|(_, r)| *r).collect();
        let all = union(&rects).ok_or_else(|| invalid("the markups have no extent"))?;
        let axis = if horizontal {
            (all.x0 + all.x1) / 2.0
        } else {
            (all.y0 + all.y1) / 2.0
        };
        let label = if horizontal { "Flip Horizontal" } else { "Flip Vertical" };
        self.geometry_edit(ids, label, |m| geometry::flip(m, horizontal, axis))
    }

    /// Copies of markups at the same place on other pages (Apply to All Pages; Paste to
    /// pages). `pages` empty = every page but each markup's own. Returns the new ids.
    pub fn copy_to_pages(&mut self, ids: &[String], pages: &[usize]) -> Result<Vec<String>> {
        let idx = self.indices(ids)?;
        let items: Vec<Markup> = idx.iter().filter_map(|i| self.doc.markups.get(*i).cloned()).collect();
        let n = self.page_count();
        for p in pages {
            if *p >= n {
                return Err(crate::EngineError::NoPage {
                    page: p.saturating_add(1),
                    count: n,
                });
            }
        }
        let targets: Vec<usize> = if pages.is_empty() {
            (0..n).collect()
        } else {
            pages.to_vec()
        };
        let mut copies = Vec::new();
        for m in &items {
            for &p in &targets {
                if p != m.page {
                    let mut c = m.clone();
                    c.page = p;
                    copies.push(c);
                }
            }
        }
        if copies.is_empty() {
            return Err(invalid("no other pages to copy to"));
        }
        self.place_copies(&copies, None, 0.0, 0.0, "Apply to Pages")
    }

    /// Take markups out of their groups (a group left with one member ends). Returns how many
    /// left a group.
    pub fn remove_from_group(&mut self, ids: &[String]) -> Result<usize> {
        let idx = self.indices(ids)?;
        let leaving: HashSet<usize> = idx
            .into_iter()
            .filter(|i| self.doc.markups.get(*i).is_some_and(|m| !m.group.is_empty()))
            .collect();
        if leaving.is_empty() {
            return Ok(0);
        }
        let groups: HashSet<String> = leaving
            .iter()
            .filter_map(|i| self.doc.markups.get(*i).map(|m| m.group.clone()))
            .collect();
        self.edit("Remove From Group", |s| {
            for &i in &leaving {
                if let Some(m) = s.doc.markups.get_mut(i) {
                    m.group.clear();
                    m.dirty = true;
                }
            }
            for g in &groups {
                let members: Vec<usize> = (0..s.doc.markups.len())
                    .filter(|i| s.doc.markups.get(*i).is_some_and(|m| &m.group == g))
                    .collect();
                if members.len() == 1
                    && let Some(m) = members.first().and_then(|i| s.doc.markups.get_mut(*i))
                {
                    m.group.clear();
                    m.dirty = true;
                }
            }
            Ok((leaving.len(), true))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::Kind;

    fn session_with(rects: &[Rect]) -> (Session, Vec<String>) {
        let mut s = Session::new_blank("t.pdf", &[(612.0, 792.0), (612.0, 792.0)]).unwrap();
        let mut ids = Vec::new();
        for r in rects {
            let m = Markup::new(Kind::Rectangle, 0, r.corners().to_vec());
            ids.push(s.add_markup(m).unwrap());
        }
        (s, ids)
    }

    #[test]
    fn align_distribute_flip_and_copy() {
        let (mut s, ids) = session_with(&[
            Rect::new(10.0, 10.0, 30.0, 30.0),
            Rect::new(100.0, 50.0, 140.0, 70.0),
            Rect::new(300.0, 200.0, 310.0, 220.0),
        ]);
        assert_eq!(s.align_markups(&ids, Align::Left).unwrap(), 2);
        for id in &ids {
            assert!((extent(s.markup(id).unwrap()).x0 - 10.0).abs() < 1e-9);
        }
        s.undo().unwrap();
        s.align_markups(&ids, Align::Middle).unwrap();
        let mids: Vec<f64> = ids
            .iter()
            .map(|id| {
                let r = extent(s.markup(id).unwrap());
                (r.y0 + r.y1) / 2.0
            })
            .collect();
        assert!(mids.iter().all(|y| (y - 115.0).abs() < 1e-9), "{mids:?}");
        s.undo().unwrap();
        // gaps: total width 20 + 40 + 10 = 70 over 10..310 = 300: gap 115
        assert_eq!(s.distribute_markups(&ids, true).unwrap(), 1);
        let r = extent(s.markup(&ids[1]).unwrap());
        assert!((r.x0 - 145.0).abs() < 1e-9, "{r:?}");
        assert!(s.distribute_markups(&ids[..2], true).is_err());

        let one = vec![ids[0].clone(), ids[1].clone()];
        s.flip_markups(&one, true).unwrap();
        // joint extent 10..140 (after distribute: second at 145..185): axis at 97.5
        let a = extent(s.markup(&ids[0]).unwrap());
        assert!((a.x1 - 185.0).abs() < 1e-9, "{a:?}");

        let copies = s.copy_to_pages(&ids[..1], &[]).unwrap();
        assert_eq!(copies.len(), 1);
        assert_eq!(s.markup(&copies[0]).unwrap().page, 1);
        assert!(s.copy_to_pages(&ids[..1], &[7]).is_err());
    }

    #[test]
    fn removing_the_last_but_one_member_ends_the_group() {
        let (mut s, ids) = session_with(&[Rect::new(0.0, 0.0, 9.0, 9.0), Rect::new(20.0, 0.0, 29.0, 9.0)]);
        s.group(&ids).unwrap();
        assert_eq!(s.remove_from_group(&ids[..1]).unwrap(), 1);
        assert!(s.markup(&ids[1]).unwrap().group.is_empty());
        assert_eq!(s.remove_from_group(&ids[..1]).unwrap(), 0);
    }
}
