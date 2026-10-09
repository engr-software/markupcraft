//! Visual Search: box a symbol and find every graphically similar instance.
//!
//! The selected region and the searched pages are rendered to grayscale. Matching is
//! normalized cross-correlation (NCC, insensitive to brightness and contrast) of the region
//! against every position: first on images shrunk so the symbol is about 16 pixels across,
//! then refined at full resolution around each promising spot. The symbol can also be
//! searched turned by 90, 180 and 270 degrees. Overlapping hits keep the best one.
//!
//! Hits can become one Count measurement per page (a point at each hit) or highlight
//! rectangles, as one undoable step.

use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Color, Kind, Markup};

use crate::raster::{Gray, PageImage};
use crate::{Result, Session, invalid};

/// Most hits one search returns.
pub const MAX_HITS: usize = 10_000;
/// The longest side a searched page is rendered at.
const MAX_SIDE: f32 = 5_000.0;
/// The symbol's longer side on the coarse images, in pixels.
const COARSE_SIDE: usize = 16;

/// What to do with the hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualAction {
    /// Only report them.
    None,
    /// One Count measurement per page, a point at each hit.
    Count,
    /// A translucent rectangle on each hit.
    Highlight,
}

impl VisualAction {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "none" | "" => VisualAction::None,
            "count" => VisualAction::Count,
            "highlight" => VisualAction::Highlight,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualSearchOptions {
    /// The page the symbol is on (0-based) and its box (user space).
    pub page: usize,
    pub region: Rect,
    /// Pages to search (0-based); empty: every page.
    pub pages: Vec<usize>,
    /// 0 (exact matches only) to 1 (loose).
    pub sensitivity: f64,
    /// Also search the symbol turned by 90, 180 and 270 degrees.
    pub rotations: bool,
    /// With `rotations`: in 45-degree steps (eight turns) instead of quarter turns.
    pub fine_rotations: bool,
    /// Only hits whose ink is the symbol's colour (a colour histogram refine).
    pub color_filter: bool,
    /// Ignore linework that runs outside the selection box (only what lies wholly inside it is
    /// the symbol).
    pub limit_to_selection: bool,
    /// Rendering resolution.
    pub dpi: f64,
    pub max_hits: usize,
    pub action: VisualAction,
    pub color: Color,
    pub subject: String,
}

impl Default for VisualSearchOptions {
    fn default() -> Self {
        Self {
            page: 0,
            region: Rect::default(),
            pages: Vec::new(),
            sensitivity: 0.5,
            rotations: true,
            fine_rotations: false,
            color_filter: false,
            limit_to_selection: false,
            dpi: 100.0,
            max_hits: 1_000,
            action: VisualAction::None,
            color: Color::rgb(0.0, 0.45, 1.0),
            subject: "Visual Search".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualHit {
    pub page: usize,
    pub rect: Rect,
    /// NCC, -1 to 1.
    pub score: f64,
    /// Clockwise degrees the symbol is turned at this hit.
    pub rotation: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VisualReport {
    pub hits: Vec<VisualHit>,
    /// Markups made from the hits.
    pub markups: Vec<String>,
}

/// Summed-area tables of an image and of its squares.
struct Integral {
    w: usize,
    s: Vec<f64>,
    s2: Vec<f64>,
}

impl Integral {
    fn new(g: &Gray) -> Self {
        let w = g.w + 1;
        let mut s = vec![0.0; w * (g.h + 1)];
        let mut s2 = vec![0.0; w * (g.h + 1)];
        for y in 0..g.h {
            let (mut row, mut row2) = (0.0, 0.0);
            for x in 0..g.w {
                let v = g.get(x, y) as f64;
                row += v;
                row2 += v * v;
                let above = s.get(y * w + x + 1).copied().unwrap_or(0.0);
                let above2 = s2.get(y * w + x + 1).copied().unwrap_or(0.0);
                if let Some(c) = s.get_mut((y + 1) * w + x + 1) {
                    *c = above + row;
                }
                if let Some(c) = s2.get_mut((y + 1) * w + x + 1) {
                    *c = above2 + row2;
                }
            }
        }
        Self { w, s, s2 }
    }

    fn sum(t: &[f64], w: usize, x: usize, y: usize, tw: usize, th: usize) -> f64 {
        let at = |xx: usize, yy: usize| t.get(yy * w + xx).copied().unwrap_or(0.0);
        at(x + tw, y + th) - at(x, y + th) - at(x + tw, y) + at(x, y)
    }

    /// Sum and sum of squares of the window at `(x, y)` of size `tw x th`.
    fn window(&self, x: usize, y: usize, tw: usize, th: usize) -> (f64, f64) {
        (
            Self::sum(&self.s, self.w, x, y, tw, th),
            Self::sum(&self.s2, self.w, x, y, tw, th),
        )
    }
}

/// A template, mean-centred, with its norm.
struct Template {
    w: usize,
    h: usize,
    t: Vec<f64>,
    norm: f64,
}

impl Template {
    fn new(g: &Gray) -> Option<Self> {
        let n = (g.w * g.h) as f64;
        if n < 4.0 {
            return None;
        }
        let mean = g.px.iter().map(|v| *v as f64).sum::<f64>() / n;
        let t: Vec<f64> = g.px.iter().map(|v| *v as f64 - mean).collect();
        let norm = t.iter().map(|v| v * v).sum::<f64>().sqrt();
        // A blank (or nearly uniform) region matches everything.
        (norm / n.sqrt() > 4.0).then_some(Self {
            w: g.w,
            h: g.h,
            t,
            norm,
        })
    }

    /// NCC of the window of `img` at `(x, y)`.
    fn score(&self, img: &Gray, ii: &Integral, x: usize, y: usize) -> f64 {
        let n = (self.w * self.h) as f64;
        let (s, s2) = ii.window(x, y, self.w, self.h);
        let var = s2 - s * s / n;
        if var <= 1e-6 {
            return 0.0;
        }
        let mut dot = 0.0;
        for ty in 0..self.h {
            let row = (y + ty) * img.w + x;
            let (Some(src), Some(tr)) = (
                img.px.get(row..row + self.w),
                self.t.get(ty * self.w..(ty + 1) * self.w),
            ) else {
                return 0.0;
            };
            dot += src.iter().zip(tr).map(|(a, b)| *a as f64 * b).sum::<f64>();
        }
        dot / (var.sqrt() * self.norm)
    }
}

/// Hits of `tmpl` (and its turns) in `img`: `(x, y, w, h, score, quarter turns)` in `img`
/// pixels.
/// `g` turned clockwise by `deg` degrees about its centre (nearest pixel; white outside).
fn rotate_any(g: &Gray, deg: u32) -> Gray {
    if deg.is_multiple_of(90) {
        return g.rotated(deg / 90);
    }
    let (s, c) = (deg as f64).to_radians().sin_cos();
    let (w, h) = (g.w as f64, g.h as f64);
    let nw = (w * c.abs() + h * s.abs()).ceil().max(1.0) as usize;
    let nh = (w * s.abs() + h * c.abs()).ceil().max(1.0) as usize;
    let mut out = Gray::new(nw, nh, 255);
    let (cx, cy, ncx, ncy) = (w / 2.0, h / 2.0, nw as f64 / 2.0, nh as f64 / 2.0);
    for y in 0..nh {
        for x in 0..nw {
            // Inverse turn (y down, so clockwise is the usual positive angle).
            let (dx, dy) = (x as f64 + 0.5 - ncx, y as f64 + 0.5 - ncy);
            let sx = c * dx + s * dy + cx;
            let sy = -s * dx + c * dy + cy;
            if sx >= 0.0
                && sy >= 0.0
                && sx < w
                && sy < h
                && let Some(px) = out.px.get_mut(y * nw + x)
            {
                *px = g.get(sx as usize, sy as usize);
            }
        }
    }
    out
}

/// Clear ink that touches the border of `g` (linework running out of the selection).
fn keep_inside(g: &mut Gray) {
    let (w, h) = (g.w, g.h);
    let dark = |g: &Gray, x: usize, y: usize| g.get(x, y) < 200;
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0));
        stack.push((x, h.saturating_sub(1)));
    }
    for y in 0..h {
        stack.push((0, y));
        stack.push((w.saturating_sub(1), y));
    }
    while let Some((x, y)) = stack.pop() {
        if x >= w || y >= h || !dark(g, x, y) {
            continue;
        }
        if let Some(px) = g.px.get_mut(y * w + x) {
            *px = 255;
        }
        for (dx, dy) in [
            (-1i64, -1i64),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx >= 0 && ny >= 0 {
                stack.push((nx as usize, ny as usize));
            }
        }
    }
}

/// The mean colour of the ink (pixels darker than the paper) inside a user-space rectangle.
fn ink_color(img: &crate::raster::RgbImage, r: Rect) -> Option<[f64; 3]> {
    let s = img.scale as f64;
    let a = img.geom.user_to_view(r.x0 as f32, r.y0 as f32);
    let b = img.geom.user_to_view(r.x1 as f32, r.y1 as f32);
    let (x0, x1) = (
        (a[0].min(b[0]) as f64 * s) as usize,
        (a[0].max(b[0]) as f64 * s).ceil() as usize,
    );
    let (y0, y1) = (
        (a[1].min(b[1]) as f64 * s) as usize,
        (a[1].max(b[1]) as f64 * s).ceil() as usize,
    );
    let (mut sum, mut n) = ([0.0f64; 3], 0usize);
    for y in y0..y1.min(img.h) {
        for x in x0..x1.min(img.w) {
            let i = (y * img.w + x) * 4;
            let Some(p) = img.rgba.get(i..i + 4) else { continue };
            let wh = 255 - p[3] as u32;
            let c = [
                (p[0] as u32 + wh).min(255) as f64,
                (p[1] as u32 + wh).min(255) as f64,
                (p[2] as u32 + wh).min(255) as f64,
            ];
            if c.iter().sum::<f64>() / 3.0 < 200.0 {
                for k in 0..3 {
                    sum[k] += c[k];
                }
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum.map(|v| v / n as f64))
}

fn match_image(
    img: &Gray,
    tmpl: &Gray,
    angles: &[u32],
    threshold: f64,
    cap: usize,
) -> Vec<(usize, usize, usize, usize, f64, u32)> {
    let f = (tmpl.w.max(tmpl.h) / COARSE_SIDE).max(1);
    let coarse = img.downsample(f);
    let ci = Integral::new(&coarse);
    let fi = Integral::new(img);
    let mut out = Vec::new();
    for &q in angles {
        let full_t = rotate_any(tmpl, q);
        let (Some(ft), Some(ct)) = (Template::new(&full_t), Template::new(&full_t.downsample(f))) else {
            continue;
        };
        if ct.w > coarse.w || ct.h > coarse.h || ft.w > img.w || ft.h > img.h {
            continue;
        }
        // Coarse scores, then local maxima above a looser threshold.
        let (cw, ch) = (coarse.w - ct.w + 1, coarse.h - ct.h + 1);
        let mut scores = vec![0.0f64; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                if let Some(s) = scores.get_mut(y * cw + x) {
                    *s = ct.score(&coarse, &ci, x, y);
                }
            }
        }
        let loose = threshold - 0.25;
        for y in 0..ch {
            for x in 0..cw {
                let s = scores.get(y * cw + x).copied().unwrap_or(0.0);
                if s < loose {
                    continue;
                }
                let mut is_max = true;
                'n: for yy in y.saturating_sub(1)..=(y + 1).min(ch - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(cw - 1) {
                        let o = scores.get(yy * cw + xx).copied().unwrap_or(0.0);
                        if o > s || (o == s && (yy, xx) < (y, x)) {
                            is_max = false;
                            break 'n;
                        }
                    }
                }
                if !is_max {
                    continue;
                }
                // Refine at full resolution around the coarse spot.
                let (x0, y0) = ((x * f).saturating_sub(f), (y * f).saturating_sub(f));
                let (x1, y1) = ((x * f + f).min(img.w - ft.w), (y * f + f).min(img.h - ft.h));
                let mut best = (0usize, 0usize, f64::MIN);
                for yy in y0..=y1 {
                    for xx in x0..=x1 {
                        let s = ft.score(img, &fi, xx, yy);
                        if s > best.2 {
                            best = (xx, yy, s);
                        }
                    }
                }
                if best.2 >= threshold {
                    out.push((best.0, best.1, ft.w, ft.h, best.2, q));
                    if out.len() >= cap.saturating_mul(4).max(64) {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let inter = w * h;
    let small = (a.width() * a.height()).min(b.width() * b.height());
    if small <= 0.0 { 0.0 } else { inter / small }
}

impl Session {
    /// Find every instance of the symbol in `opts.region` of `opts.page`; optionally mark
    /// them (one undoable step).
    pub fn visual_search(&mut self, opts: &VisualSearchOptions) -> Result<VisualReport> {
        self.page(opts.page)?;
        if !(opts.sensitivity.is_finite() && (0.0..=1.0).contains(&opts.sensitivity)) {
            return Err(invalid("sensitivity must be from 0 to 1"));
        }
        if !(opts.dpi.is_finite() && (18.0..=600.0).contains(&opts.dpi)) {
            return Err(invalid("dpi must be from 18 to 600"));
        }
        let r = opts.region;
        if !(r.width().is_finite() && r.height().is_finite() && r.width() > 0.5 && r.height() > 0.5) {
            return Err(invalid(
                "the region must be a rectangle at least half a point on each side",
            ));
        }
        let pages: Vec<usize> = if opts.pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            opts.pages.clone()
        };
        for p in &pages {
            self.page(*p)?;
        }
        let cap = opts.max_hits.clamp(1, MAX_HITS);
        let threshold = 0.92 - 0.3 * opts.sensitivity;
        let doc = self.renderable(true)?;
        let scale = (opts.dpi / 72.0) as f32;
        let src: PageImage = doc.render(opts.page, scale, MAX_SIDE)?;
        let [x0, y0, x1, y1] = src.rect_to_px(r.normalized());
        let (x0, y0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
        let (x1, y1) = (x1.ceil().max(0.0) as usize, y1.ceil().max(0.0) as usize);
        let mut tmpl = src.gray.crop(x0, y0, x1, y1);
        if opts.limit_to_selection {
            keep_inside(&mut tmpl);
        }
        if tmpl.w < 3 || tmpl.h < 3 {
            return Err(invalid("the region is too small to search for (or off the page)"));
        }
        if Template::new(&tmpl).is_none() {
            return Err(invalid("the region is blank; box a symbol to search for"));
        }
        let angles: Vec<u32> = match (opts.rotations, opts.fine_rotations) {
            (false, _) => vec![0],
            (true, false) => vec![0, 90, 180, 270],
            (true, true) => (0..8).map(|k| k * 45).collect(),
        };
        let color_doc = if opts.color_filter {
            Some(self.renderable(true)?)
        } else {
            None
        };
        let want = match &color_doc {
            Some(d) => ink_color(&d.render_rgba(opts.page, src.scale)?, r.normalized()),
            None => None,
        };
        let mut hits: Vec<VisualHit> = Vec::new();
        for &p in &pages {
            let img = if p == opts.page {
                src.clone()
            } else {
                doc.render(p, src.scale, MAX_SIDE)?
            };
            if (img.scale - src.scale).abs() > 1e-4 {
                // A page too large to render at the symbol's resolution: skip it rather than
                // compare at a different scale.
                continue;
            }
            let mut found: Vec<VisualHit> = match_image(&img.gray, &tmpl, &angles, threshold, cap)
                .into_iter()
                .map(|(x, y, w, h, score, q)| VisualHit {
                    page: p,
                    rect: img.rect_to_user([x as f64, y as f64, (x + w) as f64, (y + h) as f64]),
                    score,
                    rotation: q,
                })
                .collect();
            if let (Some(d), Some(want)) = (&color_doc, want) {
                let rgb = d.render_rgba(p, img.scale)?;
                found
                    .retain(|h| ink_color(&rgb, h.rect).is_some_and(|c| (0..3).all(|k| (c[k] - want[k]).abs() < 70.0)));
            }
            found.sort_by(|a, b| b.score.total_cmp(&a.score));
            let mut kept: Vec<VisualHit> = Vec::new();
            for h in found {
                if kept.iter().all(|k| overlap(&k.rect, &h.rect) < 0.3) {
                    kept.push(h);
                }
            }
            kept.sort_by(|a, b| b.rect.y1.total_cmp(&a.rect.y1).then(a.rect.x0.total_cmp(&b.rect.x0)));
            hits.extend(kept);
            if hits.len() >= cap {
                hits.truncate(cap);
                break;
            }
        }
        let mut markups = Vec::new();
        match opts.action {
            VisualAction::None => {}
            VisualAction::Count => {
                let mut list = Vec::new();
                for &p in &pages {
                    let pts: Vec<Point> = hits
                        .iter()
                        .filter(|h| h.page == p)
                        .map(|h| Point::new((h.rect.x0 + h.rect.x1) / 2.0, (h.rect.y0 + h.rect.y1) / 2.0))
                        .collect();
                    if pts.is_empty() {
                        continue;
                    }
                    let mut m = Markup::new(Kind::Count, p, pts);
                    m.color = opts.color;
                    m.subject = opts.subject.clone();
                    list.push(m);
                }
                markups = self.add_new_markups("Visual Search Count", list)?;
            }
            VisualAction::Highlight => {
                let list: Vec<Markup> = hits
                    .iter()
                    .map(|h| {
                        let mut m = Markup::new(Kind::Rectangle, h.page, h.rect.padded(1.0).corners().to_vec());
                        m.color = opts.color;
                        m.fill = Some(opts.color);
                        m.fill_opacity = 0.3;
                        m.opacity = 0.6;
                        m.line_width = 1.0;
                        m.subject = opts.subject.clone();
                        m
                    })
                    .collect();
                markups = self.add_new_markups("Visual Search Highlight", list)?;
            }
        }
        Ok(VisualReport { hits, markups })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf, rect};

    /// An "L"-shaped symbol at (x, y), turned by `q` quarter turns about its corner.
    pub(crate) fn symbol(x: f64, y: f64, q: u32) -> String {
        let (a, b) = match q % 4 {
            0 => ((30.0, 6.0), (6.0, 18.0)),
            1 => ((6.0, 30.0), (18.0, 6.0)),
            2 => ((30.0, 6.0), (6.0, 18.0)),
            _ => ((6.0, 30.0), (18.0, 6.0)),
        };
        match q % 4 {
            0 => format!("{}{}", rect(x, y, a.0, a.1), rect(x, y, b.0, b.1)),
            1 => format!("{}{}", rect(x, y, a.0, a.1), rect(x, y + 24.0, b.0, b.1)),
            2 => format!("{}{}", rect(x, y + 12.0, a.0, a.1), rect(x + 24.0, y, b.0, b.1)),
            _ => format!("{}{}", rect(x + 12.0, y, a.0, a.1), rect(x, y, b.0, b.1)),
        }
    }

    pub(crate) fn sheet() -> Vec<u8> {
        let mut c = String::new();
        c.push_str(&symbol(100.0, 600.0, 0));
        c.push_str(&symbol(400.0, 600.0, 0));
        c.push_str(&symbol(250.0, 300.0, 1)); // turned
        c.push_str(&rect(450.0, 200.0, 30.0, 30.0)); // a square: not the symbol
        c.push_str(&line(50.0, 100.0, 550.0, 100.0, 2.0));
        let p2 = format!("{}{}", symbol(300.0, 400.0, 0), symbol(60.0, 60.0, 2));
        pdf(&[
            SyntheticPage::new(612.0, 792.0, c),
            SyntheticPage::new(612.0, 792.0, p2),
        ])
    }

    #[test]
    fn eighth_turns_colour_filter_and_limit_to_selection() {
        // The L symbol upright at (100, 600), turned 45 degrees at (400, 600), and in red at
        // (100, 300); a long line runs through the selection box at (100, 450).
        let turned = format!(
            "q 0.7071 -0.7071 0.7071 0.7071 400 600 cm {} Q
",
            symbol(0.0, 0.0, 0)
        );
        let red = format!("1 0 0 rg {}", symbol(100.0, 300.0, 0).replace("0 g ", ""));
        let crossed = format!("{}{}", symbol(300.0, 450.0, 0), line(50.0, 473.0, 560.0, 473.0, 2.0));
        let c = format!("{}{turned}{red}0 g {crossed}", symbol(100.0, 600.0, 0));
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, c)]), "v.pdf").unwrap();
        let base = VisualSearchOptions {
            region: Rect::new(97.0, 597.0, 133.0, 621.0),
            sensitivity: 0.35,
            ..Default::default()
        };
        let quarter = s.visual_search(&base).unwrap();
        let at = |r: &VisualReport, x: f64, y: f64| r.hits.iter().any(|h| h.rect.contains(Point::new(x, y)));
        assert!(!at(&quarter, 400.0, 610.0), "a 45-degree turn is not a quarter turn");
        let fine = s
            .visual_search(&VisualSearchOptions {
                fine_rotations: true,
                ..base.clone()
            })
            .unwrap();
        assert!(fine.hits.iter().any(|h| h.rotation % 90 == 45), "{:?}", fine.hits);
        // Colour: the red copy matches by shape, not by colour.
        assert!(at(&quarter, 110.0, 305.0), "{:?}", quarter.hits);
        let colour = s
            .visual_search(&VisualSearchOptions {
                color_filter: true,
                ..base.clone()
            })
            .unwrap();
        assert!(
            !at(&colour, 110.0, 305.0) && at(&colour, 110.0, 605.0),
            "{:?}",
            colour.hits
        );
        // A box around the crossed symbol: with the limit, the line through it is ignored and
        // the plain symbols are found.
        let boxed = VisualSearchOptions {
            region: Rect::new(295.0, 445.0, 335.0, 476.0),
            rotations: false,
            sensitivity: 0.1,
            ..base
        };
        let loose = s.visual_search(&boxed).unwrap();
        let limited = s
            .visual_search(&VisualSearchOptions {
                limit_to_selection: true,
                ..boxed
            })
            .unwrap();
        assert!(
            limited.hits.len() > loose.hits.len(),
            "{} vs {}",
            limited.hits.len(),
            loose.hits.len()
        );
    }

    #[test]
    fn finds_the_symbol_turned_and_on_other_pages() {
        let mut s = Session::from_bytes(sheet(), "sheet.pdf").unwrap();
        let opts = VisualSearchOptions {
            region: Rect::new(97.0, 597.0, 133.0, 621.0),
            ..Default::default()
        };
        let r = s.visual_search(&opts).unwrap();
        let on = |p: usize| r.hits.iter().filter(|h| h.page == p).count();
        assert_eq!((on(0), on(1)), (3, 2), "{:#?}", r.hits);
        assert!(r.hits.iter().any(|h| h.rotation == 90 || h.rotation == 270));
        assert!(
            r.hits.iter().all(|h| !h.rect.contains(Point::new(465.0, 215.0))),
            "the square is not a hit"
        );
        assert!(r.markups.is_empty() && s.doc().markups.is_empty());

        // Upright only, page 1 only, then counted.
        let upright = VisualSearchOptions {
            rotations: false,
            pages: vec![0],
            action: VisualAction::Count,
            ..opts.clone()
        };
        let r = s.visual_search(&upright).unwrap();
        assert_eq!(r.hits.len(), 2);
        assert_eq!(r.markups.len(), 1);
        let m = s.markup(&r.markups[0]).unwrap();
        assert_eq!((m.kind, m.pts.len()), (Kind::Count, 2));
        let hl = VisualSearchOptions {
            action: VisualAction::Highlight,
            ..opts.clone()
        };
        assert_eq!(s.visual_search(&hl).unwrap().markups.len(), 5);

        // Errors.
        let blank = VisualSearchOptions {
            region: Rect::new(500.0, 700.0, 540.0, 740.0),
            ..Default::default()
        };
        assert!(s.visual_search(&blank).is_err());
        let off = VisualSearchOptions {
            region: Rect::new(5000.0, 5000.0, 5040.0, 5040.0),
            ..Default::default()
        };
        assert!(s.visual_search(&off).is_err());
        let bad_page = VisualSearchOptions { page: 9, ..opts };
        assert!(s.visual_search(&bad_page).is_err());
    }
}
