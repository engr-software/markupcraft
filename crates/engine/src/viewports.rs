//! Viewport management beyond add/delete (`scales.rs`): rename, rescale, clear a page's
//! viewports, and copy viewports to other pages (Revu's Measurements panel > Viewports).

use markupcraft_model::{Rect, Scale, Viewport};

use crate::{Result, Session, invalid};

fn full_page(vp: &Viewport, media: &Rect) -> bool {
    let (a, b) = (vp.bbox.normalized(), media.normalized());
    (a.x0 - b.x0).abs() < 0.5 && (a.y0 - b.y0).abs() < 0.5 && (a.x1 - b.x1).abs() < 0.5 && (a.y1 - b.y1).abs() < 0.5
}

impl Session {
    fn viewport_index(&self, page: usize, index: usize) -> Result<()> {
        let count = self.page(page)?.viewports.len();
        if index >= count {
            return Err(invalid(format!(
                "page {} has {count} viewports; there is no viewport {}",
                page + 1,
                index + 1
            )));
        }
        Ok(())
    }

    /// Rename viewport `index` of `page`.
    pub fn rename_viewport(&mut self, page: usize, index: usize, name: &str) -> Result<()> {
        self.viewport_index(page, index)?;
        if name.len() > crate::props::MAX_TEXT {
            return Err(invalid("the name is too long"));
        }
        self.edit("Rename Viewport", |s| {
            if let Some(v) = s.doc.pages.get_mut(page).and_then(|p| p.viewports.get_mut(index)) {
                v.name = name.to_string();
            }
            s.vp_changed.insert(page);
            Ok(((), true))
        })
    }

    /// Give viewport `index` of `page` a new scale; measurements inside it follow when
    /// `apply_to_markups`. Returns how many measurements changed.
    pub fn set_viewport_scale(
        &mut self,
        page: usize,
        index: usize,
        sc: &Scale,
        apply_to_markups: bool,
    ) -> Result<usize> {
        self.viewport_index(page, index)?;
        if !sc.valid() {
            return Err(invalid("the scale has no usable /X conversion"));
        }
        self.edit("Viewport Scale", |s| {
            let Some(info) = s.doc.pages.get_mut(page) else {
                return Err(invalid("page vanished"));
            };
            let media = info.media;
            let Some(v) = info.viewports.get_mut(index) else {
                return Err(invalid("viewport vanished"));
            };
            v.scale = sc.clone();
            let b = v.bbox;
            if full_page(v, &media) {
                info.scale = Some(sc.clone());
            }
            s.vp_changed.insert(page);
            let mut n = 0;
            if apply_to_markups {
                for m in s.doc.markups.iter_mut().filter(|m| m.page == page) {
                    let inside = m.pts.first().is_some_and(|p| b.contains(*p));
                    if inside && m.kind.is_measurement() && m.kind != markupcraft_model::Kind::Count && !m.locked() {
                        m.scale = Some(sc.clone());
                        m.dirty = true;
                        n += 1;
                    }
                }
            }
            Ok((n, true))
        })
    }

    /// Remove every viewport from `page` that is not the page scale (with `keep_page_scale`),
    /// or all of them. Returns how many went.
    pub fn clear_viewports(&mut self, page: usize, keep_page_scale: bool) -> Result<usize> {
        let info = self.page(page)?.clone();
        let n = info
            .viewports
            .iter()
            .filter(|v| !(keep_page_scale && full_page(v, &info.media)))
            .count();
        if n == 0 {
            return Ok(0);
        }
        self.edit("Clear Viewports", |s| {
            if let Some(p) = s.doc.pages.get_mut(page) {
                let media = p.media;
                p.viewports.retain(|v| keep_page_scale && full_page(v, &media));
                if p.viewports.is_empty() {
                    p.scale = None;
                }
            }
            s.vp_changed.insert(page);
            Ok((n, true))
        })
    }

    /// Copy the partial viewports of `from` (or just `index`) to `pages`, at the same place.
    /// Returns how many viewports were added.
    pub fn copy_viewports(&mut self, from: usize, index: Option<usize>, pages: &[usize]) -> Result<usize> {
        let info = self.page(from)?.clone();
        if let Some(i) = index {
            self.viewport_index(from, i)?;
        }
        let vps: Vec<Viewport> = info
            .viewports
            .iter()
            .enumerate()
            .filter(|(i, v)| index.is_none_or(|k| k == *i) && !full_page(v, &info.media))
            .map(|(_, v)| v.clone())
            .collect();
        if vps.is_empty() {
            return Err(invalid(format!("page {} has no viewports to copy", from + 1)));
        }
        let targets: Vec<usize> = pages.iter().copied().filter(|p| *p != from).collect();
        for p in &targets {
            self.page(*p)?;
        }
        if targets.is_empty() {
            return Err(invalid("no other pages to copy to"));
        }
        let mut ids = Vec::new();
        for _ in 0..targets.len() * vps.len() {
            ids.push(self.new_id());
        }
        self.edit("Copy Viewports", |s| {
            let mut n = 0;
            let mut it = ids.into_iter();
            for &p in &targets {
                let Some(pi) = s.doc.pages.get_mut(p) else { continue };
                for v in &vps {
                    let at = pi
                        .viewports
                        .iter()
                        .position(|w| full_page(w, &pi.media))
                        .unwrap_or(pi.viewports.len());
                    let mut c = v.clone();
                    c.id = it.next().unwrap_or_default();
                    pi.viewports.insert(at, c);
                    n += 1;
                }
                s.vp_changed.insert(p);
            }
            Ok((n, true))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_rescale_copy_and_clear() {
        let mut s = Session::new_blank("v.pdf", &[(612.0, 792.0), (612.0, 792.0), (612.0, 792.0)]).unwrap();
        let sc = Scale::architectural(0.125, 1.0);
        s.add_viewport(0, Rect::new(10.0, 10.0, 200.0, 200.0), "Plan", &sc)
            .unwrap();
        s.rename_viewport(0, 0, "Detail A").unwrap();
        assert_eq!(s.page(0).unwrap().viewports[0].name, "Detail A");
        s.set_viewport_scale(0, 0, &Scale::architectural(0.25, 1.0), true)
            .unwrap();
        assert!(s.page(0).unwrap().viewports[0].scale.ratio.starts_with("0.25"));
        assert_eq!(s.copy_viewports(0, None, &[0, 1, 2]).unwrap(), 2);
        assert_eq!(s.page(2).unwrap().viewports.len(), 1);
        assert_eq!(s.clear_viewports(2, true).unwrap(), 1);
        assert!(s.page(2).unwrap().viewports.is_empty());
        assert!(s.copy_viewports(2, None, &[1]).is_err());
        assert!(s.rename_viewport(0, 5, "x").is_err());
        s.undo().unwrap();
        assert_eq!(s.page(2).unwrap().viewports.len(), 1);
    }
}
