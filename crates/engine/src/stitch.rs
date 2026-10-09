//! Stitching (Document > Stitching): several drawing pages joined edge to edge into one large
//! page, for sheets that continue across match lines. Each page's content is drawn as a form
//! XObject (vector, nothing rasterized) at its place in a grid of rows; neighbouring sheets may
//! overlap by a set distance so their match lines coincide. Markups come along as markups,
//! moved to where their page landed.

use std::path::Path;
use std::sync::Arc;

use markupcraft_model::Rect;
use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, Stream};

use crate::docutil::page_objs;
use crate::{Result, Session, invalid};

/// Most pages stitched into one.
pub const MAX_STITCH: usize = 64;

/// How the pages are laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StitchLayout {
    /// Pages per row (left to right), rows from the top down. 0 = every page in one row.
    pub columns: usize,
    /// Points neighbouring pages overlap (across match lines); 0 = edge to edge.
    pub overlap: f64,
}

impl Default for StitchLayout {
    fn default() -> Self {
        Self {
            columns: 0,
            overlap: 0.0,
        }
    }
}

/// What a stitch wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct StitchReport {
    pub width: f64,
    pub height: f64,
    pub pages: usize,
    pub markups: usize,
}

/// Each page's lower-left corner on the new page, then the new page's width and height.
type Placement = (Vec<(f64, f64)>, f64, f64);

/// Where each page's crop box goes: its lower-left corner on the new page.
fn place(boxes: &[Rect], layout: StitchLayout) -> Result<Placement> {
    let cols = if layout.columns == 0 {
        boxes.len()
    } else {
        layout.columns
    };
    let ov = layout.overlap;
    let min_side = boxes
        .iter()
        .map(|b| b.width().min(b.height()))
        .fold(f64::INFINITY, f64::min);
    if !(ov.is_finite() && ov >= 0.0 && ov < min_side / 2.0) {
        return Err(invalid(
            "the overlap must be at least 0 and under half the smallest page side",
        ));
    }
    let rows: Vec<&[Rect]> = boxes.chunks(cols.max(1)).collect();
    let row_h: Vec<f64> = rows
        .iter()
        .map(|r| r.iter().map(|b| b.height()).fold(0.0, f64::max))
        .collect();
    let row_w: Vec<f64> = rows
        .iter()
        .map(|r| r.iter().map(|b| b.width()).sum::<f64>() - ov * r.len().saturating_sub(1) as f64)
        .collect();
    let width = row_w.iter().copied().fold(0.0, f64::max);
    let height = row_h.iter().sum::<f64>() - ov * rows.len().saturating_sub(1) as f64;
    let mut at = Vec::with_capacity(boxes.len());
    let mut top = height;
    for (r, row) in rows.iter().enumerate() {
        let h = row_h.get(r).copied().unwrap_or(0.0);
        let mut x = 0.0;
        for b in row.iter() {
            // Pages in a row line up along the row's top edge.
            at.push((x, top - b.height()));
            x += b.width() - ov;
        }
        top -= h - ov;
    }
    Ok((at, width, height))
}

impl Session {
    /// Stitch `pages` (0-based, in order) into a one-page PDF at `out`. Undo is not involved:
    /// this document is unchanged.
    pub fn stitch_pages(&self, pages: &[usize], layout: StitchLayout, out: &Path) -> Result<StitchReport> {
        if pages.len() < 2 || pages.len() > MAX_STITCH {
            return Err(invalid(format!("stitch 2 to {MAX_STITCH} pages")));
        }
        if crate::docutil::same_file(out, self.path()) {
            return Err(invalid("write the stitched drawing to a different file"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let src = CosDoc::open(Arc::new(self.current_bytes()?.as_ref().clone()))?;
        let all = crate::overlay::source_pages(&src)?;
        let mut boxes = Vec::with_capacity(pages.len());
        for p in pages {
            boxes.push(all.get(*p).ok_or_else(|| invalid("a page is missing"))?.box_);
        }
        let (at, width, height) = place(&boxes, layout)?;
        crate::blank::check_size(width, height)?;
        let mut contents = Vec::with_capacity(pages.len());
        for p in pages {
            let sp = all.get(*p).ok_or_else(|| invalid("a page is missing"))?;
            contents.push(crate::overlay::page_content(&src, &sp.dict)?);
        }
        let mut s = Session::new_blank(out, &[(width, height)])?;
        s.graph_edit("Stitch", |cos, _| {
            let pref = *page_objs(cos)?.first().ok_or_else(|| invalid("no page"))?;
            let mut xo = Dict::new();
            let mut content = String::new();
            for (i, p) in pages.iter().enumerate() {
                let sp = all.get(*p).ok_or_else(|| invalid("a page is missing"))?;
                let mut imp = crate::overlay::Importer::new(&src);
                let b = sp.box_;
                let mut fd = Dict::new();
                fd.set(b"Type".to_vec(), Object::name("XObject"));
                fd.set(b"Subtype".to_vec(), Object::name("Form"));
                fd.set(
                    b"BBox".to_vec(),
                    Object::Array(vec![
                        Object::Real(b.x0),
                        Object::Real(b.y0),
                        Object::Real(b.x1),
                        Object::Real(b.y1),
                    ]),
                );
                if let Some(res) = &sp.resources {
                    let r = imp.rewrite(cos, res, 0);
                    fd.set(b"Resources".to_vec(), r);
                }
                imp.drain(cos)?;
                let data = contents.get(i).map(Vec::as_slice).unwrap_or_default();
                let form = cos.add(Object::Stream(Stream::flate(fd, data)));
                xo.set(format!("St{i}").into_bytes(), Object::Ref(form));
                let (x, y) = at.get(i).copied().unwrap_or_default();
                content.push_str(&format!("q 1 0 0 1 {} {} cm /St{i} Do Q\n", x - b.x0, y - b.y0));
            }
            let mut res = Dict::new();
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            let stream = cos.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
            cos.update_dict(pref, |d| {
                d.set(b"Resources".to_vec(), Object::Dict(res));
                d.set(b"Contents".to_vec(), Object::Ref(stream));
            })?;
            Ok(())
        })?;
        // The markups, moved with their pages.
        let mut moved = 0;
        for (i, p) in pages.iter().enumerate() {
            let (Some((x, y)), Some(b)) = (at.get(i).copied(), boxes.get(i)) else {
                continue;
            };
            let list: Vec<_> = self.doc().markups_on(*p).cloned().collect();
            for mut m in list {
                m.page = 0;
                m.obj = Default::default();
                m.annot_index = None;
                m.group = String::new();
                m.irt = None;
                m.stored_look = false;
                crate::geometry::translate(&mut m, x - b.x0, y - b.y0);
                if s.add_markup(m).is_ok() {
                    moved += 1;
                }
            }
        }
        s.save_as(out, true)?;
        Ok(StitchReport {
            width,
            height,
            pages: pages.len(),
            markups: moved,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect, text};

    #[test]
    fn pages_join_edge_to_edge_with_their_markups() {
        let d = std::env::temp_dir().join(format!("markupcraft-stitch-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let bytes = pdf(&[
            SyntheticPage::new(
                300.0,
                200.0,
                format!("{}{}", rect(0.0, 0.0, 20.0, 20.0), text(50.0, 100.0, 12.0, "WEST")),
            ),
            SyntheticPage::new(300.0, 200.0, text(50.0, 100.0, 12.0, "EAST")),
            SyntheticPage::new(300.0, 100.0, text(50.0, 50.0, 12.0, "SOUTH")),
        ]);
        let mut s = Session::from_bytes(bytes, d.join("a.pdf")).unwrap();
        let mut m = crate::Markup::new(
            crate::Kind::Rectangle,
            1,
            Rect::new(10.0, 10.0, 40.0, 40.0).corners().to_vec(),
        );
        m.subject = "Box".into();
        s.add_markup(m).unwrap();
        // Two across, the third below; 10 pt of overlap.
        let out = d.join("stitched.pdf");
        let r = s
            .stitch_pages(
                &[0, 1, 2],
                StitchLayout {
                    columns: 2,
                    overlap: 10.0,
                },
                &out,
            )
            .unwrap();
        assert_eq!((r.width, r.height, r.markups), (590.0, 290.0, 1));
        let t = Session::open(&out).unwrap();
        assert_eq!(t.page_count(), 1);
        let text = t.page_text(0).unwrap();
        assert!(
            text.contains("WEST") && text.contains("EAST") && text.contains("SOUTH"),
            "{text}"
        );
        // The east sheet's box moved 290 pt right and 90 pt up (the row's top edge).
        let b = t.doc().markups.first().unwrap();
        assert!(
            (b.rect.x0 - 300.0).abs() < 1.0 && (b.rect.y0 - 100.0).abs() < 1.0,
            "{:?}",
            b.rect
        );
        assert!(s.stitch_pages(&[0], StitchLayout::default(), &out).is_err());
        assert!(
            s.stitch_pages(
                &[0, 1],
                StitchLayout {
                    columns: 0,
                    overlap: 500.0
                },
                &out
            )
            .is_err()
        );
    }
}
