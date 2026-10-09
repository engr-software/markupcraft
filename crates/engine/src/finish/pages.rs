//! Page operations beyond the basics: inserting pages from several PDFs at once (each with its
//! own page range), blank pages ruled with a grid or copied from a template page, extracting
//! one file per page (named by number or page label, with or without overwriting), and Page
//! Setup's content scaling, offsets, rotation, centring and borders.

use std::path::{Path, PathBuf};

use markupcraft_geom::Point;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, Stream};

use crate::docutil::page_objs;
use crate::pages::{ForeignPdf, PagePlan, PageReport};
use crate::{Result, Session, invalid};

/// Prepend `pre` and append `post` to a page's content (each its own stream).
pub(crate) fn wrap_content(cos: &mut CosDoc, page: ObjRef, pre: &str, post: &str) -> Result<()> {
    let old = cos.get(page).as_dict().and_then(|d| d.get(b"Contents").cloned());
    let mut list = Vec::new();
    if !pre.is_empty() {
        list.push(Object::Ref(
            cos.add(Object::Stream(Stream::flate(Dict::new(), pre.as_bytes()))),
        ));
    }
    match old.as_ref().map(|o| (o, cos.resolve(o))) {
        Some((_, r)) if r.as_array().is_some() => list.extend(r.as_array().cloned().unwrap_or_default()),
        Some((o, _)) => list.push(o.clone()),
        None => {}
    }
    if !post.is_empty() {
        list.push(Object::Ref(
            cos.add(Object::Stream(Stream::flate(Dict::new(), post.as_bytes()))),
        ));
    }
    cos.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
    Ok(())
}

/// A grid ruled on new blank pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridStyle {
    /// Spacing in points (4 to 288).
    pub spacing: f64,
    /// Dots at the crossings instead of lines.
    pub dots: bool,
    /// Grey level of the lines, 0 (black) to 1 (white).
    pub gray: f64,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            spacing: 18.0,
            dots: false,
            gray: 0.75,
        }
    }
}

fn grid_content(w: f64, h: f64, g: &GridStyle) -> String {
    let s = g.spacing;
    let mut c = format!("q {:.3} G {:.3} g 0.25 w\n", g.gray, g.gray);
    if g.dots {
        let mut y = s;
        while y < h {
            let mut x = s;
            while x < w {
                c.push_str(&format!("{:.2} {:.2} 0.8 0.8 re f\n", x - 0.4, y - 0.4));
                x += s;
            }
            y += s;
        }
    } else {
        let mut x = s;
        while x < w {
            c.push_str(&format!("{x:.2} 0 m {x:.2} {h:.2} l S\n"));
            x += s;
        }
        let mut y = s;
        while y < h {
            c.push_str(&format!("0 {y:.2} m {w:.2} {y:.2} l S\n"));
            y += s;
        }
    }
    c.push_str("Q\n");
    c
}

/// How Page Setup places the existing content on the new page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    /// The new page size (points, as displayed).
    pub width: f64,
    pub height: f64,
    /// `None`: scale the content to fit inside the margins; `Some(k)`: this scale (0.05 to 20).
    pub scale: Option<f64>,
    /// Extra offset of the content, points (x right, y up).
    pub offset: (f64, f64),
    /// Turn the content (degrees counter-clockwise) about its centre.
    pub rotation: f64,
    /// Centre the content on the page (else it keeps its lower-left corner at the margin).
    pub center: bool,
    /// Margins left free: left, right, top, bottom (binding margins), points.
    pub margins: [f64; 4],
    /// Draw a border at the margins, this wide (0 = none).
    pub border: f64,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            width: 612.0,
            height: 792.0,
            scale: None,
            offset: (0.0, 0.0),
            rotation: 0.0,
            center: true,
            margins: [0.0; 4],
            border: 0.0,
        }
    }
}

/// An affine map `[a b c d e f]` (x' = a x + c y + e, y' = b x + d y + f).
fn apply(m: [f64; 6], p: Point) -> Point {
    Point::new(m[0] * p.x + m[2] * p.y + m[4], m[1] * p.x + m[3] * p.y + m[5])
}

/// The rotation angle that turns the line `a`-`b` level (Page Setup's rotation "from a drawn
/// line"), counter-clockwise degrees.
pub fn level_angle(a: Point, b: Point) -> f64 {
    -(b.y - a.y).atan2(b.x - a.x).to_degrees()
}

impl Session {
    /// Insert pages from several PDFs in order, each with its own pages (`None` = all), so the
    /// first inserted page becomes page `at`. One undoable step.
    pub fn insert_files(&mut self, at: usize, items: &[(PathBuf, Option<Vec<usize>>)]) -> Result<PageReport> {
        if at > self.page_count() {
            return Err(invalid(format!(
                "position {} is past the end (1 to {})",
                at + 1,
                self.page_count() + 1
            )));
        }
        if items.is_empty() || items.len() > 500 {
            return Err(invalid("give 1 to 500 files"));
        }
        let mut plan = PagePlan::identity(self.page_count());
        let mut pos = at;
        for (path, pages) in items {
            let file = ForeignPdf::open(path)?;
            let n = pages.as_ref().map_or(file.page_count(), Vec::len);
            plan.insert_file(pos, file, pages.as_deref())?;
            pos += n;
        }
        self.apply_page_plan("Insert Pages", &plan)
    }

    /// Insert `count` blank pages at `at`, ruled with `grid`, or copies of the first page of
    /// `template` (its size and content). One undoable step.
    pub fn insert_blank_styled(
        &mut self,
        at: usize,
        count: usize,
        size: Option<(f64, f64)>,
        grid: Option<GridStyle>,
        template: Option<&Path>,
    ) -> Result<PageReport> {
        if at > self.page_count() {
            return Err(invalid(format!(
                "position {} is past the end (1 to {})",
                at + 1,
                self.page_count() + 1
            )));
        }
        if count == 0 || count > 1_000 {
            return Err(invalid("count must be 1 to 1000"));
        }
        if let Some(g) = &grid
            && !(g.spacing.is_finite() && (4.0..=288.0).contains(&g.spacing) && (0.0..=1.0).contains(&g.gray))
        {
            return Err(invalid("grid spacing is 4 to 288 points"));
        }
        if let Some(t) = template {
            let mut plan = PagePlan::identity(self.page_count());
            for i in 0..count {
                plan.insert_file(at + i, ForeignPdf::open(t)?, Some(&[0]))?;
            }
            return self.apply_page_plan("Insert Pages from Template", &plan);
        }
        let report = self.insert_blank_pages(at, count, size)?;
        if let Some(g) = grid {
            let sizes: Vec<(f64, f64)> = (at..at + count)
                .map(|p| {
                    self.doc
                        .pages
                        .get(p)
                        .map_or((612.0, 792.0), |i| (i.media.width(), i.media.height()))
                })
                .collect();
            // the grid joins the insert's undo step
            self.edit("Insert Blank Pages", |s| {
                let objs = page_objs(&s.file.cos)?;
                for (i, (w, h)) in sizes.iter().enumerate() {
                    let Some(page) = objs.get(at + i).copied() else {
                        continue;
                    };
                    wrap_content(&mut s.file.cos, page, "", &grid_content(*w, *h, &g))?;
                }
                Ok(((), true))
            })?;
            // one step: fold the grid into the insert
            if let Some(grid_step) = self.undo.pop_back() {
                self.undo_bytes = self.undo_bytes.saturating_sub(grid_step.bytes);
            }
        }
        Ok(report)
    }

    /// Extract each of `pages` to its own file in `dir`: `<stem>_<n>.pdf`, or the page label
    /// when `by_label` (falling back to the number). Existing files are replaced only with
    /// `overwrite` (else a ` (2)` is added). With `delete`, the pages then leave this document
    /// (one undoable step). Returns the files written.
    pub fn extract_each(
        &mut self,
        pages: &[usize],
        dir: &Path,
        stem: &str,
        by_label: bool,
        overwrite: bool,
        delete: bool,
    ) -> Result<Vec<PathBuf>> {
        if !dir.is_dir() {
            return Err(invalid(format!("{} is not a folder", dir.display())));
        }
        if pages.is_empty() || pages.len() > 5_000 {
            return Err(invalid("give 1 to 5000 pages"));
        }
        let labels = self.page_labels();
        let width = self.page_count().to_string().len();
        let clean = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if "/\\:*?\"<>|".contains(c) || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .collect::<String>()
                .trim()
                .to_string()
        };
        let stem = if stem.trim().is_empty() {
            "page".to_string()
        } else {
            clean(stem)
        };
        let mut out = Vec::new();
        for p in pages {
            self.page(*p)?;
            let label = labels.get(*p).map(|l| clean(l)).unwrap_or_default();
            let base = if by_label && !label.is_empty() {
                label
            } else {
                format!("{stem}_{:0width$}", p + 1)
            };
            let mut path = dir.join(format!("{base}.pdf"));
            let mut k = 2;
            while !overwrite && (path.exists() || out.contains(&path)) && k < 10_000 {
                path = dir.join(format!("{base} ({k}).pdf"));
                k += 1;
            }
            self.extract_pages(&[*p], &path, false)?;
            out.push(path);
        }
        if delete {
            self.delete_pages(pages)?;
        }
        Ok(out)
    }

    /// Page Setup: give `pages` the size in `setup` and place their content (scaled, turned,
    /// moved, centred) on it; markups move with the content. A border can be drawn at the
    /// margins. One undoable step.
    pub fn page_setup(&mut self, pages: &[usize], setup: &PageSetup) -> Result<()> {
        let st = setup;
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        crate::blank::check_size(st.width, st.height)?;
        if let Some(k) = st.scale
            && !(k.is_finite() && (0.05..=20.0).contains(&k))
        {
            return Err(invalid("scale is 5% to 2000%"));
        }
        if !(st.rotation.is_finite() && st.rotation.abs() <= 360.0)
            || !(st.offset.0.is_finite() && st.offset.1.is_finite())
            || !st.margins.iter().all(|m| m.is_finite() && (0.0..=720.0).contains(m))
            || !(st.border.is_finite() && (0.0..=72.0).contains(&st.border))
        {
            return Err(invalid(
                "rotation within 360 degrees, margins 0 to 720 points, border 0 to 72",
            ));
        }
        let [ml, mr, mt, mb] = st.margins;
        let (aw, ah) = (st.width - ml - mr, st.height - mt - mb);
        if aw <= 1.0 || ah <= 1.0 {
            return Err(invalid("the margins leave no room on the page"));
        }
        let mut maps: Vec<(usize, [f64; 6])> = Vec::new();
        for p in pages {
            let c = self.page(*p)?.crop.normalized();
            let (r, s0) = (st.rotation.to_radians(), 1.0);
            let (cs, sn) = (r.cos(), r.sin());
            // the content's turned extent
            let (w, h) = (
                (c.width() * cs).abs() + (c.height() * sn).abs(),
                (c.width() * sn).abs() + (c.height() * cs).abs(),
            );
            let k = st.scale.unwrap_or_else(|| (aw / w).min(ah / h)) * s0;
            // turn and scale about the content's centre, then put that centre in place
            let (ccx, ccy) = ((c.x0 + c.x1) / 2.0, (c.y0 + c.y1) / 2.0);
            let (tx, ty) = if st.center {
                (ml + aw / 2.0, mb + ah / 2.0)
            } else {
                (ml + w * k / 2.0, mb + h * k / 2.0)
            };
            let (tx, ty) = (tx + st.offset.0, ty + st.offset.1);
            let (a, b, cc, d) = (k * cs, k * sn, -k * sn, k * cs);
            let e = tx - (a * ccx + cc * ccy);
            let f = ty - (b * ccx + d * ccy);
            maps.push((*p, [a, b, cc, d, e, f]));
        }
        let border = st.border;
        let (pw, ph) = (st.width, st.height);
        self.edit("Page Setup", |s| {
            for m in s.doc.markups.iter_mut() {
                let Some((_, t)) = maps.iter().find(|(p, _)| *p == m.page) else {
                    continue;
                };
                for p in m.pts.iter_mut() {
                    *p = apply(*t, *p);
                }
                for h in m.holes.iter_mut() {
                    for p in h.iter_mut() {
                        *p = apply(*t, *p);
                    }
                }
                let k = (t[0] * t[3] - t[1] * t[2]).abs().sqrt();
                m.line_width *= k;
                m.text.size = (m.text.size * k).clamp(1.0, 400.0);
                if let Some(bb) = markupcraft_geom::bbox(&m.pts) {
                    m.rect = bb;
                }
                m.dirty = true;
            }
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let objs = page_objs(&s.file.cos)?;
            for (p, t) in &maps {
                let Some(page) = objs.get(*p).copied() else { continue };
                let pre = format!(
                    "q {:.6} {:.6} {:.6} {:.6} {:.4} {:.4} cm\n",
                    t[0], t[1], t[2], t[3], t[4], t[5]
                );
                let mut post = String::from("\nQ\n");
                if border > 0.0 {
                    post.push_str(&format!(
                        "q 0 G {border:.3} w {:.3} {:.3} {:.3} {:.3} re S Q\n",
                        ml + border / 2.0,
                        mb + border / 2.0,
                        aw - border,
                        ah - border
                    ));
                }
                wrap_content(&mut s.file.cos, page, &pre, &post)?;
                s.file.cos.update_dict(page, |d| {
                    d.set(
                        b"MediaBox".to_vec(),
                        Object::Array(vec![Object::Int(0), Object::Int(0), Object::Real(pw), Object::Real(ph)]),
                    );
                    for k in [&b"CropBox"[..], b"BleedBox", b"TrimBox", b"ArtBox", b"Rotate", b"VP"] {
                        d.remove(k);
                    }
                })?;
            }
            s.reload();
            Ok(((), true))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect, text};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mc-finish-pages-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn three() -> Vec<u8> {
        pdf(&[
            SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "ONE")),
            SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "TWO")),
            SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 50.0, 50.0)),
        ])
    }

    #[test]
    fn insert_several_files_grid_template_extract_each_and_setup() {
        let d = tmp("all");
        let a = d.join("a.pdf");
        std::fs::write(&a, three()).unwrap();
        let mut s = Session::from_bytes(three(), d.join("main.pdf")).unwrap();
        let r = s
            .insert_files(1, &[(a.clone(), Some(vec![2])), (a.clone(), Some(vec![0, 1]))])
            .unwrap();
        assert_eq!(r.pages_after, 6);
        s.undo().unwrap();
        assert_eq!(s.page_count(), 3);

        let g = GridStyle {
            spacing: 36.0,
            ..Default::default()
        };
        s.insert_blank_styled(3, 2, Some((300.0, 300.0)), Some(g), None)
            .unwrap();
        assert_eq!(s.page_count(), 5);
        s.undo().unwrap();
        assert_eq!(s.page_count(), 3, "grid and pages are one step");
        s.insert_blank_styled(0, 2, None, None, Some(&a)).unwrap();
        assert_eq!(s.page_count(), 5);
        s.undo().unwrap();

        s.set_page_labels(&[(0, "A-1".into()), (1, "A-2".into()), (2, "A-3".into())])
            .unwrap();
        let out = tmp("each");
        let files = s.extract_each(&[0, 2], &out, "plans", true, false, false).unwrap();
        assert_eq!(files, vec![out.join("A-1.pdf"), out.join("A-3.pdf")]);
        let again = s.extract_each(&[0], &out, "plans", true, false, false).unwrap();
        assert_eq!(again, vec![out.join("A-1 (2).pdf")]);
        let by_num = s.extract_each(&[1], &out, "plans", false, true, true).unwrap();
        assert_eq!(by_num, vec![out.join("plans_2.pdf")]);
        assert_eq!(s.page_count(), 2);

        let id = s
            .add_markup(markupcraft_model::Markup::new(
                markupcraft_model::Kind::Line,
                0,
                vec![Point::new(0.0, 0.0), Point::new(612.0, 792.0)],
            ))
            .unwrap();
        let setup = PageSetup {
            width: 1224.0,
            height: 1584.0,
            scale: None,
            ..Default::default()
        };
        s.page_setup(&[0], &setup).unwrap();
        let m = s.page(0).unwrap().media.normalized();
        assert!((m.width() - 1224.0).abs() < 0.01);
        let line = s.markup(&id).unwrap();
        assert!((line.pts[1].x - 1224.0).abs() < 0.5, "{:?}", line.pts);
        assert!(
            s.page_setup(
                &[0],
                &PageSetup {
                    margins: [700.0; 4],
                    ..setup
                }
            )
            .is_err()
        );
        assert!((level_angle(Point::new(0.0, 0.0), Point::new(10.0, 10.0)) + 45.0).abs() < 1e-9);
    }
}
