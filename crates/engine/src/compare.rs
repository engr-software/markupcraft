//! Compare Documents: find what changed between an older and a newer revision of a drawing
//! and mark every change with a cloud on the newer document (subject "Compare"), so the
//! Markups List becomes the list of changes.
//!
//! - **Graphics**: both pages are rendered to grayscale at the same resolution. A pixel of the
//!   old page is *removed* ink when its ink (255 - gray) exceeds the darkest new ink within
//!   `tolerance_px` by more than the colour threshold; *added* is the same the other way round
//!   (tolerant of anti-aliasing and one-pixel shifts). Changed pixels are counted on a grid of
//!   `cell_px` cells; a cell with at least `min_pixels` changed pixels is changed, and changed
//!   cells closer than `merge_px` join one region.
//! - **Text**: the words of the two pages are diffed (PdfCraft's `pdfcraft-compare`, Myers'
//!   algorithm); inserted and replaced words are marked where they are on the new page, and
//!   deleted words next to where they were.
//!
//! Pages are paired in order (old page *i* with new page *i*) unless pairs are given. The old
//! document is only read.

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Color, Kind, Markup};

use crate::raster::{Gray, PageImage, Renderable, words};
use crate::{Result, Session, geometry, invalid, props};

/// Most regions one comparison marks.
pub const MAX_REGIONS: usize = 5_000;
/// The longest side a compared page is rendered at, in pixels.
pub const MAX_SIDE: f32 = 6_000.0;

/// What to compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    Text,
    Graphics,
    Both,
}

impl CompareMode {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "text" => CompareMode::Text,
            "graphics" | "graphic" | "raster" => CompareMode::Graphics,
            "both" | "all" => CompareMode::Both,
            _ => return None,
        })
    }

    fn text(self) -> bool {
        matches!(self, CompareMode::Text | CompareMode::Both)
    }

    fn graphics(self) -> bool {
        matches!(self, CompareMode::Graphics | CompareMode::Both)
    }
}

/// Raster difference settings (pixel units are at `dpi`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffOptions {
    /// Ink difference (0-255) that counts as a change.
    pub threshold: u8,
    /// Anti-aliasing / shift tolerance in pixels.
    pub tolerance_px: usize,
    /// Grid cell size in pixels.
    pub cell_px: usize,
    /// Changed pixels a cell needs to count.
    pub min_pixels: usize,
    /// Changed cells closer than this join one region.
    pub merge_px: usize,
    /// Ignore a band this wide around the page edge.
    pub margin_px: usize,
    /// Regions are grown by this much.
    pub pad_px: usize,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            threshold: 64,
            tolerance_px: 1,
            cell_px: 8,
            min_pixels: 4,
            merge_px: 24,
            margin_px: 0,
            pad_px: 6,
        }
    }
}

/// A changed region of a raster difference, in the first image's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRegion {
    /// `[x0, y0, x1, y1]`, x1/y1 exclusive.
    pub rect: [usize; 4],
    /// Changed pixels with ink only in the second image.
    pub added: usize,
    /// Changed pixels with ink only in the first image.
    pub removed: usize,
}

impl PixelRegion {
    pub fn what(&self) -> &'static str {
        change_kind(self.added, self.removed)
    }
}

fn change_kind(added: usize, removed: usize) -> &'static str {
    if added > 0 && removed > 0 {
        let r = added as f64 / (added + removed) as f64;
        if r > 0.85 {
            "added"
        } else if r < 0.15 {
            "removed"
        } else {
            "changed"
        }
    } else if added > 0 {
        "added"
    } else {
        "removed"
    }
}

/// Ink (255 - gray), dilated: the darkest ink within `r` pixels (square window).
fn dilated_ink(g: &Gray, w: usize, h: usize, r: usize) -> Vec<u8> {
    let mut ink = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            if let Some(v) = ink.get_mut(y * w + x) {
                *v = 255 - g.get(x, y);
            }
        }
    }
    if r == 0 {
        return ink;
    }
    let mut tmp = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut m = 0u8;
            for k in x.saturating_sub(r)..=(x + r).min(w - 1) {
                m = m.max(ink.get(y * w + k).copied().unwrap_or(0));
            }
            if let Some(v) = tmp.get_mut(y * w + x) {
                *v = m;
            }
        }
    }
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut m = 0u8;
            for k in y.saturating_sub(r)..=(y + r).min(h - 1) {
                m = m.max(tmp.get(k * w + x).copied().unwrap_or(0));
            }
            if let Some(v) = out.get_mut(y * w + x) {
                *v = m;
            }
        }
    }
    out
}

/// The per-pixel change mask over the common size (0 none, 1 removed, 2 added).
pub fn change_mask(a: &Gray, b: &Gray, opt: &DiffOptions) -> (usize, usize, Vec<u8>) {
    let (w, h) = (a.w.min(b.w), a.h.min(b.h));
    let mut mask = vec![0u8; w * h];
    if w == 0 || h == 0 {
        return (w, h, mask);
    }
    let da = dilated_ink(a, w, h, opt.tolerance_px);
    let db = dilated_ink(b, w, h, opt.tolerance_px);
    let m = opt.margin_px;
    let t = opt.threshold as i32;
    for y in m..h.saturating_sub(m) {
        for x in m..w.saturating_sub(m) {
            let i = y * w + x;
            let ink_a = 255 - a.get(x, y) as i32;
            let ink_b = 255 - b.get(x, y) as i32;
            let (dai, dbi) = (
                da.get(i).copied().unwrap_or(0) as i32,
                db.get(i).copied().unwrap_or(0) as i32,
            );
            if let Some(v) = mask.get_mut(i) {
                if ink_a - dbi > t {
                    *v = 1;
                } else if ink_b - dai > t {
                    *v = 2;
                }
            }
        }
    }
    (w, h, mask)
}

#[derive(Clone, Copy)]
struct Cell {
    n: usize,
    added: usize,
    removed: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            n: 0,
            added: 0,
            removed: 0,
            x0: usize::MAX,
            y0: usize::MAX,
            x1: 0,
            y1: 0,
        }
    }
}

/// Compare `b` against `a` pixel for pixel over their common size; the changed regions in
/// reading order (bands of 64 pixels, then left to right).
pub fn diff(a: &Gray, b: &Gray, opt: &DiffOptions) -> Vec<PixelRegion> {
    let (w, h, mask) = change_mask(a, b, opt);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let cs = opt.cell_px.max(1);
    let (gw, gh) = (w.div_ceil(cs), h.div_ceil(cs));
    let mut cells = vec![Cell::default(); gw * gh];
    for y in 0..h {
        for x in 0..w {
            let v = mask.get(y * w + x).copied().unwrap_or(0);
            if v == 0 {
                continue;
            }
            let Some(c) = cells.get_mut((y / cs) * gw + x / cs) else {
                continue;
            };
            c.n += 1;
            if v == 2 {
                c.added += 1;
            } else {
                c.removed += 1;
            }
            c.x0 = c.x0.min(x);
            c.y0 = c.y0.min(y);
            c.x1 = c.x1.max(x + 1);
            c.y1 = c.y1.max(y + 1);
        }
    }
    let on: Vec<bool> = cells.iter().map(|c| c.n >= opt.min_pixels.max(1)).collect();
    let k = opt.merge_px.div_ceil(cs).max(1);
    let mut label = vec![false; cells.len()];
    let mut regions: Vec<PixelRegion> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..cells.len() {
        if !on.get(start).copied().unwrap_or(false) || label.get(start).copied().unwrap_or(true) {
            continue;
        }
        if regions.len() >= MAX_REGIONS {
            break;
        }
        let mut r = PixelRegion {
            rect: [usize::MAX, usize::MAX, 0, 0],
            added: 0,
            removed: 0,
        };
        if let Some(l) = label.get_mut(start) {
            *l = true;
        }
        stack.clear();
        stack.push(start);
        while let Some(ci) = stack.pop() {
            let Some(c) = cells.get(ci).copied() else { continue };
            r.rect = [
                r.rect[0].min(c.x0),
                r.rect[1].min(c.y0),
                r.rect[2].max(c.x1),
                r.rect[3].max(c.y1),
            ];
            r.added += c.added;
            r.removed += c.removed;
            let (cx, cy) = (ci % gw, ci / gw);
            for yy in cy.saturating_sub(k)..=(cy + k).min(gh - 1) {
                for xx in cx.saturating_sub(k)..=(cx + k).min(gw - 1) {
                    let j = yy * gw + xx;
                    if on.get(j).copied().unwrap_or(false) && !label.get(j).copied().unwrap_or(true) {
                        if let Some(l) = label.get_mut(j) {
                            *l = true;
                        }
                        stack.push(j);
                    }
                }
            }
        }
        regions.push(r);
    }
    for r in &mut regions {
        r.rect = [
            r.rect[0].saturating_sub(opt.pad_px),
            r.rect[1].saturating_sub(opt.pad_px),
            (r.rect[2] + opt.pad_px).min(w),
            (r.rect[3] + opt.pad_px).min(h),
        ];
    }
    merge_overlapping(&mut regions);
    regions.sort_by_key(|r| (r.rect[1] / 64, r.rect[0], r.rect[1]));
    regions
}

fn merge_overlapping(regions: &mut Vec<PixelRegion>) {
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..regions.len() {
            for j in i + 1..regions.len() {
                let (Some(p), Some(q)) = (regions.get(i).copied(), regions.get(j).copied()) else {
                    continue;
                };
                if p.rect[0] < q.rect[2] && q.rect[0] < p.rect[2] && p.rect[1] < q.rect[3] && q.rect[1] < p.rect[3] {
                    if let Some(p) = regions.get_mut(i) {
                        p.rect = [
                            p.rect[0].min(q.rect[0]),
                            p.rect[1].min(q.rect[1]),
                            p.rect[2].max(q.rect[2]),
                            p.rect[3].max(q.rect[3]),
                        ];
                        p.added += q.added;
                        p.removed += q.removed;
                    }
                    regions.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
}

/// How to compare and how to mark the changes.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareOptions {
    pub mode: CompareMode,
    /// `(old page, new page)` pairs, 0-based; empty pairs every page in order.
    pub pairs: Vec<(usize, usize)>,
    /// 0 (only strong changes) to 1 (the faintest difference counts); sets the colour threshold.
    pub sensitivity: f64,
    /// Rasterization resolution.
    pub dpi: f64,
    /// Grid cell size and the changed pixels a cell needs (pixel density), at `dpi`.
    pub cell_px: usize,
    pub min_pixels: usize,
    /// Changed areas closer than this many points join one cloud.
    pub merge_pt: f64,
    /// Ignore a band this wide (points) around each page.
    pub margin_pt: f64,
    /// Only this rectangle of the new page (user space).
    pub window: Option<Rect>,
    /// Include existing markups in the graphics comparison.
    pub include_markups: bool,
    /// The clouds.
    pub color: Color,
    pub subject: String,
    pub line_width: f64,
    /// Cloud intensity (0 = plain rectangles).
    pub cloud: f64,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            mode: CompareMode::Both,
            pairs: Vec::new(),
            sensitivity: 0.5,
            dpi: 100.0,
            cell_px: 8,
            min_pixels: 4,
            merge_pt: 18.0,
            margin_pt: 0.0,
            window: None,
            include_markups: false,
            color: Color::rgb(1.0, 0.5, 0.0),
            subject: "Compare".into(),
            line_width: 1.5,
            cloud: 1.0,
        }
    }
}

/// One change found.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareRegion {
    /// Page of the new document (0-based).
    pub page: usize,
    /// Old page it was compared with.
    pub old_page: usize,
    pub rect: Rect,
    /// "added", "removed" or "changed".
    pub kind: String,
    /// "text", "graphics" or "text+graphics".
    pub source: String,
    /// The words involved (text changes).
    pub text: String,
    /// The cloud that marks it.
    pub markup: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompareReport {
    pub pairs: Vec<(usize, usize)>,
    pub regions: Vec<CompareRegion>,
    pub text_changes: usize,
    pub graphics_changes: usize,
}

/// Compare an old revision (as PDF bytes) with a new one; the regions on the new pages.
pub fn compare_bytes(old: &Renderable, new: &Renderable, opts: &CompareOptions) -> Result<CompareReport> {
    if !(opts.sensitivity.is_finite() && (0.0..=1.0).contains(&opts.sensitivity)) {
        return Err(invalid("sensitivity must be from 0 to 1"));
    }
    if !(opts.dpi.is_finite() && (18.0..=600.0).contains(&opts.dpi)) {
        return Err(invalid("dpi must be from 18 to 600"));
    }
    if !(opts.merge_pt.is_finite() && opts.merge_pt >= 0.0 && opts.margin_pt.is_finite() && opts.margin_pt >= 0.0) {
        return Err(invalid("merge and margin must be zero or more points"));
    }
    let pairs: Vec<(usize, usize)> = if opts.pairs.is_empty() {
        (0..old.page_count().min(new.page_count())).map(|i| (i, i)).collect()
    } else {
        opts.pairs.clone()
    };
    for &(o, n) in &pairs {
        old.geom(o)?;
        new.geom(n)?;
    }
    if pairs.is_empty() {
        return Err(invalid("no pages to compare"));
    }
    let scale = (opts.dpi / 72.0) as f32;
    let mut regions = Vec::new();
    let (mut text_n, mut gfx_n) = (0, 0);
    for &(o, n) in &pairs {
        let mut page_regions: Vec<CompareRegion> = Vec::new();
        if opts.mode.graphics() {
            let a = old.render(o, scale, MAX_SIDE)?;
            let b = new.render(n, scale, MAX_SIDE)?;
            // Both pages at one resolution (the cap may have lowered one of them).
            let (a, b) = if (a.scale - b.scale).abs() > 1e-4 {
                let s = a.scale.min(b.scale);
                (old.render(o, s, MAX_SIDE)?, new.render(n, s, MAX_SIDE)?)
            } else {
                (a, b)
            };
            let px = |pt: f64| (pt * b.scale as f64).round().max(0.0) as usize;
            let d = DiffOptions {
                threshold: (8.0 + (1.0 - opts.sensitivity) * 112.0).round().clamp(1.0, 254.0) as u8,
                tolerance_px: 1,
                cell_px: opts.cell_px.clamp(1, 512),
                min_pixels: opts.min_pixels.clamp(1, 512 * 512),
                merge_px: px(opts.merge_pt),
                margin_px: px(opts.margin_pt),
                pad_px: px(4.0),
            };
            for r in diff(&a.gray, &b.gray, &d) {
                let rect = b.rect_to_user(r.rect.map(|v| v as f64));
                gfx_n += 1;
                page_regions.push(CompareRegion {
                    page: n,
                    old_page: o,
                    rect,
                    kind: r.what().into(),
                    source: "graphics".into(),
                    text: String::new(),
                    markup: String::new(),
                });
            }
        }
        if opts.mode.text() {
            for r in text_regions(old, new, o, n)? {
                text_n += 1;
                page_regions.push(r);
            }
        }
        if let Some(w) = opts.window {
            page_regions.retain(|r| overlaps(&r.rect, &w));
            for r in &mut page_regions {
                r.rect = Rect::new(
                    r.rect.x0.max(w.x0),
                    r.rect.y0.max(w.y0),
                    r.rect.x1.min(w.x1),
                    r.rect.y1.min(w.y1),
                );
            }
        }
        merge_regions(&mut page_regions);
        regions.extend(page_regions);
        if regions.len() > MAX_REGIONS {
            regions.truncate(MAX_REGIONS);
            break;
        }
    }
    Ok(CompareReport {
        pairs,
        regions,
        text_changes: text_n,
        graphics_changes: gfx_n,
    })
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// Regions of one page whose boxes overlap become one.
fn merge_regions(v: &mut Vec<CompareRegion>) {
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..v.len() {
            for j in i + 1..v.len() {
                let (Some(p), Some(q)) = (v.get(i), v.get(j)) else {
                    continue;
                };
                if !overlaps(&p.rect, &q.rect) {
                    continue;
                }
                let q = v.remove(j);
                if let Some(p) = v.get_mut(i) {
                    p.rect = Rect::new(
                        p.rect.x0.min(q.rect.x0),
                        p.rect.y0.min(q.rect.y0),
                        p.rect.x1.max(q.rect.x1),
                        p.rect.y1.max(q.rect.y1),
                    );
                    if p.kind != q.kind {
                        p.kind = "changed".into();
                    }
                    if p.source != q.source {
                        p.source = "text+graphics".into();
                    }
                    if !q.text.is_empty() {
                        if !p.text.is_empty() {
                            p.text.push_str(" / ");
                        }
                        p.text.push_str(&q.text);
                    }
                }
                merged = true;
                break 'outer;
            }
        }
    }
}

/// Text changes between old page `o` and new page `n`.
fn text_regions(old: &Renderable, new: &Renderable, o: usize, n: usize) -> Result<Vec<CompareRegion>> {
    let to_words = |r: &Renderable, page: usize| -> Result<Vec<pdfcraft_compare::Word>> {
        let geom = r.geom(page)?.clone();
        Ok(r.text(page)
            .map(|t| words(&t, &geom))
            .unwrap_or_default()
            .into_iter()
            .map(|w| pdfcraft_compare::Word {
                text: w.text,
                page,
                rect: [w.rect.x0, w.rect.y0, w.rect.x1, w.rect.y1],
            })
            .collect())
    };
    let a = to_words(old, o)?;
    let b = to_words(new, n)?;
    let c = pdfcraft_compare::compare(&a, &b);
    let mut out = Vec::new();
    for ch in c.changes {
        let rects: Vec<[f64; 4]> = if ch.new.rects.is_empty() {
            ch.new.near.into_iter().collect()
        } else {
            ch.new.rects.clone()
        };
        let pts: Vec<Point> = rects
            .iter()
            .flat_map(|r| [Point::new(r[0], r[1]), Point::new(r[2], r[3])])
            .collect();
        let Some(rect) = bbox(&pts) else { continue };
        let (kind, text) = match ch.kind {
            pdfcraft_compare::Kind::Inserted => ("added", ch.new.text.clone()),
            pdfcraft_compare::Kind::Deleted => ("removed", ch.old.text.clone()),
            pdfcraft_compare::Kind::Replaced => ("changed", format!("{} -> {}", ch.old.text, ch.new.text)),
        };
        out.push(CompareRegion {
            page: n,
            old_page: o,
            rect: rect.padded(2.0),
            kind: kind.into(),
            source: "text".into(),
            text,
            markup: String::new(),
        });
    }
    Ok(out)
}

impl Session {
    /// Compare this (newer) document with an older revision and cloud every change on this
    /// one, as one undoable step. `old` is the older document's PDF bytes.
    pub fn compare_with(&mut self, old: std::sync::Arc<Vec<u8>>, opts: &CompareOptions) -> Result<CompareReport> {
        if opts.subject.chars().count() > 256 {
            return Err(invalid("subject is too long"));
        }
        if !(opts.line_width.is_finite() && (0.0..=72.0).contains(&opts.line_width)) {
            return Err(invalid("line width must be from 0 to 72 points"));
        }
        if !(opts.cloud.is_finite() && (0.0..=2.0).contains(&opts.cloud)) {
            return Err(invalid("cloud must be from 0 to 2"));
        }
        let old = Renderable::new(old, !opts.include_markups)?;
        let new = self.renderable(!opts.include_markups)?;
        let mut report = compare_bytes(&old, &new, opts)?;
        let markups: Vec<Markup> = report
            .regions
            .iter()
            .map(|r| {
                let mut m = Markup::new(
                    if opts.cloud > 0.0 { Kind::Cloud } else { Kind::Polygon },
                    r.page,
                    r.rect.padded(2.0).corners().to_vec(),
                );
                m.color = opts.color;
                m.fill = None;
                m.line_width = opts.line_width;
                m.cloud = opts.cloud;
                m.subject = opts.subject.clone();
                let what = capitalize(&r.kind);
                m.contents = if r.text.is_empty() {
                    what
                } else {
                    format!("{what}: {}", truncate(&r.text, 2000))
                };
                m
            })
            .collect();
        let ids = self.add_new_markups("Compare Documents", markups)?;
        for (r, id) in report.regions.iter_mut().zip(ids) {
            r.markup = id;
        }
        Ok(report)
    }

    /// Add markups as one undoable step (`label`), filling ids, authors and subjects as
    /// [`Session::add_markup`] does. Returns their ids; they become the selection.
    pub fn add_new_markups(&mut self, label: &str, list: Vec<Markup>) -> Result<Vec<String>> {
        if list.is_empty() {
            return Ok(Vec::new());
        }
        let mut used: std::collections::HashSet<String> = self.doc.markups.iter().map(|m| m.id.clone()).collect();
        let mut ready = Vec::with_capacity(list.len());
        for mut m in list {
            self.page(m.page)?;
            if !props::can_create(m.kind) {
                return Err(invalid(format!("{} markups cannot be created yet", m.kind.name())));
            }
            geometry::check_points(m.kind, &m.pts)?;
            geometry::normalize(&mut m);
            m.id = loop {
                let id = markupcraft_revu::new_markup_id();
                if used.insert(id.clone()) {
                    break id;
                }
            };
            m.obj = (0, 0);
            m.annot_index = None;
            m.dirty = true;
            m.stored_look = false;
            if m.author.is_empty() {
                m.author = self.author.clone();
            }
            if m.subject.is_empty() {
                m.subject = props::default_subject(m.kind);
            }
            if m.rect == Rect::default()
                && let Some(b) = bbox(&m.pts)
            {
                m.rect = b.padded(m.line_width + 1.0);
            }
            ready.push(m);
        }
        let ids: Vec<String> = ready.iter().map(|m| m.id.clone()).collect();
        self.edit(label, move |s| {
            s.doc.markups.extend(ready);
            Ok(((), true))
        })?;
        self.selection = ids.clone();
        Ok(ids)
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// A page image's ink box (for tests and callers that need to know where the content is).
pub fn ink_box(img: &PageImage) -> Option<Rect> {
    let g = &img.gray;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..g.h {
        for x in 0..g.w {
            if g.get(x, y) < 128 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 < x1).then(|| img.rect_to_user([x0 as f64, y0 as f64, x1 as f64, y1 as f64]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(w: usize, h: usize) -> Gray {
        Gray::new(w, h, 255)
    }

    fn fill(g: &mut Gray, x0: usize, y0: usize, x1: usize, y1: usize) {
        for y in y0..y1 {
            for x in x0..x1 {
                g.px[y * g.w + x] = 0;
            }
        }
    }

    #[test]
    fn raster_diff_finds_added_and_removed_ink() {
        let mut a = canvas(200, 200);
        let mut b = canvas(200, 200);
        fill(&mut a, 10, 10, 60, 14); // common line
        fill(&mut b, 10, 10, 60, 14);
        fill(&mut a, 150, 150, 180, 180); // removed box
        fill(&mut b, 20, 120, 40, 140); // added box
        let r = diff(&a, &b, &DiffOptions::default());
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0].what(), "added");
        assert_eq!(r[1].what(), "removed");
        assert!(r[0].rect[0] <= 20 && r[0].rect[2] >= 40);
    }

    #[test]
    fn one_pixel_shift_is_tolerated() {
        let mut a = canvas(100, 100);
        let mut b = canvas(100, 100);
        fill(&mut a, 10, 10, 90, 12);
        fill(&mut b, 10, 11, 90, 13);
        assert!(diff(&a, &b, &DiffOptions::default()).is_empty());
        let strict = DiffOptions {
            tolerance_px: 0,
            ..Default::default()
        };
        assert!(!diff(&a, &b, &strict).is_empty());
    }

    fn revisions() -> (Vec<u8>, Vec<u8>) {
        use crate::synthetic::{SyntheticPage, line, pdf, rect, text};
        let common = line(40.0, 40.0, 560.0, 40.0, 2.0);
        let old = format!(
            "{common}{}{}",
            text(50.0, 700.0, 14.0, "DOOR 101 TYPE A"),
            rect(300.0, 300.0, 50.0, 50.0)
        );
        let new = format!(
            "{common}{}{}",
            text(50.0, 700.0, 14.0, "DOOR 102 TYPE A"),
            rect(100.0, 100.0, 50.0, 50.0)
        );
        (
            pdf(&[SyntheticPage::new(612.0, 792.0, old)]),
            pdf(&[SyntheticPage::new(612.0, 792.0, new)]),
        )
    }

    #[test]
    fn compare_documents_clouds_text_and_graphic_changes() {
        let (old, new) = revisions();
        let mut s = Session::from_bytes(new, "new.pdf").unwrap();
        let r = s
            .compare_with(std::sync::Arc::new(old), &CompareOptions::default())
            .unwrap();
        assert_eq!(r.pairs, vec![(0, 0)]);
        // The edited word, the box that left and the box that arrived.
        assert_eq!(r.regions.len(), 3, "{:#?}", r.regions);
        let near = |x: f64, y: f64| r.regions.iter().find(|g| g.rect.contains(Point::new(x, y)));
        let word = near(112.0, 705.0).expect("text change");
        assert!(
            word.source.contains("text") && word.text.contains("101 -> 102"),
            "{word:?}"
        );
        assert_eq!(near(325.0, 325.0).unwrap().kind, "removed");
        assert_eq!(near(125.0, 125.0).unwrap().kind, "added");
        assert!(near(300.0, 40.0).is_none(), "the common line is unchanged");
        let clouds: Vec<&Markup> = s.doc().markups.iter().filter(|m| m.subject == "Compare").collect();
        assert_eq!(clouds.len(), 3);
        assert!(clouds.iter().all(|m| m.kind == Kind::Cloud));
        assert_eq!(s.undo_label(), Some("Compare Documents"));
        s.undo().unwrap();
        assert!(s.doc().markups.is_empty());

        // Text only: just the word; graphics only: the boxes and the word's pixels.
        let text_only = CompareOptions {
            mode: CompareMode::Text,
            ..Default::default()
        };
        let (old, _) = revisions();
        assert_eq!(
            s.compare_with(std::sync::Arc::new(old.clone()), &text_only)
                .unwrap()
                .regions
                .len(),
            1
        );
        // A window around the boxes only.
        let window = CompareOptions {
            window: Some(Rect::new(80.0, 80.0, 400.0, 400.0)),
            mode: CompareMode::Graphics,
            ..Default::default()
        };
        assert_eq!(
            s.compare_with(std::sync::Arc::new(old.clone()), &window)
                .unwrap()
                .regions
                .len(),
            2
        );
        // Identical documents: nothing.
        let same = s.current_bytes().unwrap();
        let before = s.doc().markups.len();
        let r = s
            .compare_with(
                same,
                &CompareOptions {
                    include_markups: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(r.regions.is_empty());
        assert_eq!(s.doc().markups.len(), before);
        // Bad input is an error, not a crash.
        assert!(
            s.compare_with(std::sync::Arc::new(b"junk".to_vec()), &CompareOptions::default())
                .is_err()
        );
        let bad = CompareOptions {
            pairs: vec![(0, 5)],
            ..Default::default()
        };
        assert!(s.compare_with(std::sync::Arc::new(old), &bad).is_err());
    }

    #[test]
    fn nearby_changes_merge_and_margins_are_ignored() {
        let mut a = canvas(200, 200);
        let b = {
            let mut b = canvas(200, 200);
            fill(&mut b, 50, 50, 60, 60);
            fill(&mut b, 70, 50, 80, 60);
            fill(&mut b, 0, 0, 3, 200); // in the margin
            b
        };
        fill(&mut a, 199, 199, 200, 200);
        let opt = DiffOptions {
            margin_px: 5,
            ..Default::default()
        };
        let r = diff(&a, &b, &opt);
        assert_eq!(r.len(), 1, "{r:?}");
        let empty = diff(&canvas(0, 0), &b, &opt);
        assert!(empty.is_empty());
    }
}
