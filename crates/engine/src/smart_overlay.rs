//! Smart Overlay: whole sets overlaid sheet by sheet, each sheet registered by matching its
//! drawing (not its page box), with a match score per sheet and per discipline.
//!
//! Registration: both sheets are rendered small and their ink found; a first guess fits the
//! ink extents (scale and centre), then the scale and offset are refined by a search that
//! maximizes the ink of the revised sheet landing on ink of the current one (with a one-pixel
//! tolerance). The match score is the share of ink that agrees after registration, averaged
//! both ways: 1.0 for the same drawing, near 0 for unrelated sheets.
//!
//! Disciplines come from the sheet number's letter prefix (A, S, M, E, P, FP...).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_geom::Point;
use markupcraft_model::Color;
use serde::Serialize;

use crate::batch_compare::{BatchJob, SheetPair, SheetRef, match_sheets};
use crate::overlay::{OverlayAlign, OverlayLayer, overlay_pages_shaded};
use crate::raster::{PageImage, Renderable, read_pdf};
use crate::{Result, invalid};

/// Ink: darker than this grey.
const INK: u8 = 160;
/// Most ink pixels sampled per sheet.
const SAMPLES: usize = 6_000;

/// `[a b c d e f]`: user space of the revised sheet to user space of the current one.
pub type Matrix = [f64; 6];

fn apply(m: &Matrix, p: Point) -> Point {
    Point::new(m[0] * p.x + m[2] * p.y + m[4], m[1] * p.x + m[3] * p.y + m[5])
}

/// A rendered sheet: its ink in user space, and a lookup of ink with a one-pixel tolerance.
struct Ink {
    img: PageImage,
    near: Vec<bool>,
    samples: Vec<Point>,
}

impl Ink {
    fn new(img: PageImage) -> Option<Self> {
        let g = &img.gray;
        let (w, h) = (g.w, g.h);
        if w == 0 || h == 0 {
            return None;
        }
        let mut near = vec![false; w.saturating_mul(h)];
        let mut all = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if g.get(x, y) < INK {
                    all.push((x, y));
                    for (dx, dy) in [(0i64, 0i64), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx >= 0
                            && ny >= 0
                            && (nx as usize) < w
                            && (ny as usize) < h
                            && let Some(v) = near.get_mut(ny as usize * w + nx as usize)
                        {
                            *v = true;
                        }
                    }
                }
            }
        }
        if all.is_empty() {
            return None;
        }
        let step = (all.len() / SAMPLES).max(1);
        let samples = all
            .iter()
            .step_by(step)
            .map(|(x, y)| img.to_user(*x as f64 + 0.5, *y as f64 + 0.5))
            .collect();
        Some(Self { img, near, samples })
    }

    /// Whether user-space point `p` is on (or next to) ink.
    fn hit(&self, p: Point) -> bool {
        let [x0, y0, _, _] = self.img.rect_to_px(markupcraft_geom::Rect::new(p.x, p.y, p.x, p.y));
        if !(x0.is_finite() && y0.is_finite()) || x0 < 0.0 || y0 < 0.0 {
            return false;
        }
        let (x, y) = (x0 as usize, y0 as usize);
        x < self.img.gray.w
            && self
                .near
                .get(y.saturating_mul(self.img.gray.w).saturating_add(x))
                .copied()
                .unwrap_or(false)
    }

    fn extents(&self) -> markupcraft_geom::Rect {
        let mut r = markupcraft_geom::Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &self.samples {
            r = markupcraft_geom::Rect::new(r.x0.min(p.x), r.y0.min(p.y), r.x1.max(p.x), r.y1.max(p.y));
        }
        r
    }
}

/// The share of `from`'s ink that lands on `to`'s ink through `m`.
fn agreement(from: &Ink, to: &Ink, m: &Matrix) -> f64 {
    let n = from.samples.len().max(1);
    let hits = from.samples.iter().filter(|p| to.hit(apply(m, **p))).count();
    hits as f64 / n as f64
}

fn invert(m: &Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if !(det.is_finite() && det.abs() > 1e-12) {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// Register `layer` onto `base` by their content. Returns the map (layer user space to base
/// user space) and the match score (0 to 1).
fn register(base: &Ink, layer: &Ink) -> Option<(Matrix, f64)> {
    let (a, b) = (base.extents(), layer.extents());
    if b.width() < 1.0 || b.height() < 1.0 || a.width() < 1.0 || a.height() < 1.0 {
        return None;
    }
    let s0 = ((a.width() / b.width()) + (a.height() / b.height())) / 2.0;
    let (acx, acy) = ((a.x0 + a.x1) / 2.0, (a.y0 + a.y1) / 2.0);
    let (bcx, bcy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let at = |s: f64, dx: f64, dy: f64| -> Matrix { [s, 0.0, 0.0, s, acx - s * bcx + dx, acy - s * bcy + dy] };
    // One base pixel in points: the search step.
    let px = 1.0 / f64::from(base.img.scale.max(1e-3));
    let mut best = (at(s0, 0.0, 0.0), agreement(layer, base, &at(s0, 0.0, 0.0)));
    // Coarse to fine: scale tweaks, then offsets within 16 pixels, then single pixels.
    for k in [0.97, 0.98, 0.99, 1.01, 1.02, 1.03] {
        let m = at(s0 * k, 0.0, 0.0);
        let v = agreement(layer, base, &m);
        if v > best.1 {
            best = (m, v);
        }
    }
    for step in [4.0, 1.0] {
        let reach = if step > 1.0 { 4 } else { 3 };
        let centre = best.0;
        for iy in -reach..=reach {
            for ix in -reach..=reach {
                let mut m = centre;
                m[4] += f64::from(ix) * step * px;
                m[5] += f64::from(iy) * step * px;
                let v = agreement(layer, base, &m);
                if v > best.1 {
                    best = (m, v);
                }
            }
        }
    }
    let back = invert(&best.0)?;
    let score = (best.1 + agreement(base, layer, &back)) / 2.0;
    Some((best.0, score))
}

/// Register page `page_b` of `b` onto page `page_a` of `a` by content: the map and the score.
pub fn content_align(a: &Arc<Vec<u8>>, page_a: usize, b: &Arc<Vec<u8>>, page_b: usize) -> Result<(Matrix, f64)> {
    let render = |bytes: &Arc<Vec<u8>>, p: usize| -> Result<Ink> {
        let img = Renderable::new(bytes.clone(), true)?.render(p, 0.5, 1200.0)?;
        Ink::new(img).ok_or_else(|| invalid("Smart Overlay needs linework on both sheets"))
    };
    let (base, layer) = (render(a, page_a)?, render(b, page_b)?);
    register(&base, &layer).ok_or_else(|| invalid("the sheets could not be registered"))
}

/// The discipline of a sheet number ("A-101" is Architectural).
pub fn discipline(sheet: &str) -> &'static str {
    let prefix: String = sheet
        .trim()
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_uppercase();
    match prefix.as_str() {
        "G" => "General",
        "C" => "Civil",
        "L" => "Landscape",
        "S" => "Structural",
        "A" => "Architectural",
        "I" | "ID" => "Interiors",
        "FP" | "F" => "Fire Protection",
        "P" => "Plumbing",
        "M" | "H" => "Mechanical",
        "E" => "Electrical",
        "T" => "Telecommunications",
        _ => "Other",
    }
}

/// One sheet of a Smart Overlay.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SmartSheet {
    pub current: SheetRef,
    pub revised: SheetRef,
    pub sheet: String,
    pub discipline: String,
    /// 0 to 1.
    pub score: f64,
    pub output: PathBuf,
    pub error: String,
}

/// A Smart Overlay's report: every sheet, and each discipline's sheet count and mean score.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct SmartReport {
    pub sheets: Vec<SmartSheet>,
    pub disciplines: Vec<(String, usize, f64)>,
    pub unmatched_current: Vec<SheetRef>,
    pub unmatched_revised: Vec<SheetRef>,
}

fn sheet_name(r: &SheetRef) -> String {
    // File-and-page keys are `<NAME>#<page>`.
    let key = r.key.split('#').next().unwrap_or_default().trim();
    if key.is_empty() {
        r.file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        key.to_string()
    }
}

/// Overlay one pair registered by content.
fn overlay_pair(p: &SheetPair, out: &Path, colors: [Color; 2], advanced: bool) -> Result<f64> {
    let a = read_pdf(&p.current.file)?;
    let b = read_pdf(&p.revised.file)?;
    let (m, score) = content_align(&a, p.current.page, &b, p.revised.page)?;
    let from = [Point::new(0.0, 0.0), Point::new(1000.0, 0.0), Point::new(0.0, 1000.0)];
    let to = from.map(|q| apply(&m, q));
    let layer = |bytes: Arc<Vec<u8>>, page: usize, color: Color, name: &str, align: OverlayAlign| OverlayLayer {
        pages: vec![page],
        color,
        name: name.to_string(),
        align,
        ..OverlayLayer::new(bytes)
    };
    overlay_pages_shaded(
        &[
            layer(a, p.current.page, colors[0], "Current", OverlayAlign::Page),
            layer(
                b,
                p.revised.page,
                colors[1],
                "Revised",
                OverlayAlign::Three { from, to },
            ),
        ],
        out,
        advanced,
    )?;
    Ok(score)
}

/// Smart Overlay of a batch job: sheets paired as the job says, each overlay written to
/// `out_dir` (`<sheet> overlay.pdf`). A sheet that fails is reported, not fatal.
pub fn smart_overlay(job: &BatchJob, out_dir: &Path, advanced_shading: bool) -> Result<SmartReport> {
    if !out_dir.is_dir() {
        return Err(invalid(format!("{} is not a folder", out_dir.display())));
    }
    let (pairs, left_c, left_r) = match_sheets(job)?;
    if pairs.is_empty() {
        return Err(invalid("no sheets were paired: check the matching"));
    }
    let colors = crate::batch_compare::batch_overlay_colors();
    let mut report = SmartReport {
        unmatched_current: left_c,
        unmatched_revised: left_r,
        ..Default::default()
    };
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    for p in pairs.iter().take(crate::batch_compare::MAX_PAIRS) {
        let sheet = sheet_name(&p.revised);
        let safe: String = sheet
            .chars()
            .map(|c| {
                if "/\\:*?\"<>|".contains(c) || c.is_control() {
                    '_'
                } else {
                    c
                }
            })
            .take(100)
            .collect();
        let n = used.entry(safe.clone()).or_default();
        *n += 1;
        let file = if *n > 1 {
            format!("{safe} overlay {n}.pdf")
        } else {
            format!("{safe} overlay.pdf")
        };
        let output = out_dir.join(file);
        let (score, error) = match overlay_pair(p, &output, colors, advanced_shading) {
            Ok(s) => (s, String::new()),
            Err(e) => (0.0, e.to_string()),
        };
        report.sheets.push(SmartSheet {
            current: p.current.clone(),
            revised: p.revised.clone(),
            discipline: discipline(&sheet).to_string(),
            sheet,
            score,
            output,
            error,
        });
    }
    let mut by: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for s in report.sheets.iter().filter(|s| s.error.is_empty()) {
        let e = by.entry(s.discipline.clone()).or_default();
        e.0 += 1;
        e.1 += s.score;
    }
    report.disciplines = by
        .into_iter()
        .map(|(d, (n, sum))| (d, n, sum / n.max(1) as f64))
        .collect();
    Ok(report)
}

/// The report as CSV: one row per sheet, then one per discipline.
pub fn smart_report_csv(r: &SmartReport) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("Sheet,Discipline,Match %,Current,Revised,Overlay,Error\n");
    for s in &r.sheets {
        out.push_str(&format!(
            "{},{},{:.1},{},{},{},{}\n",
            q(&s.sheet),
            q(&s.discipline),
            s.score * 100.0,
            q(&format!("{} p{}", s.current.file.display(), s.current.page + 1)),
            q(&format!("{} p{}", s.revised.file.display(), s.revised.page + 1)),
            q(&s.output.display().to_string()),
            q(&s.error)
        ));
    }
    out.push_str("\nDiscipline,Sheets,Mean match %\n");
    for (d, n, m) in &r.disciplines {
        out.push_str(&format!("{},{n},{:.1}\n", q(d), m * 100.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf, rect};

    fn drawing(k: f64, dx: f64, dy: f64, extra: bool) -> String {
        let mut c = String::new();
        for (x0, y0, x1, y1) in [
            (100.0, 100.0, 500.0, 100.0),
            (500.0, 100.0, 500.0, 400.0),
            (500.0, 400.0, 100.0, 400.0),
            (100.0, 400.0, 100.0, 100.0),
            (100.0, 250.0, 300.0, 250.0),
            (300.0, 100.0, 300.0, 330.0),
            (380.0, 180.0, 460.0, 360.0),
        ] {
            c.push_str(&line(x0 * k + dx, y0 * k + dy, x1 * k + dx, y1 * k + dy, 2.0));
        }
        c.push_str(&rect(150.0 * k + dx, 300.0 * k + dy, 40.0 * k, 40.0 * k));
        if extra {
            c.push_str(&rect(400.0 * k + dx, 120.0 * k + dy, 30.0 * k, 30.0 * k));
        }
        c
    }

    #[test]
    fn sheets_register_by_content_and_score() {
        let d = std::env::temp_dir().join(format!("markupcraft-smart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cur")).unwrap();
        std::fs::create_dir_all(d.join("rev")).unwrap();
        std::fs::create_dir_all(d.join("out")).unwrap();
        // A-101: the revised sheet is the same plan at 70% and moved, with one change.
        std::fs::write(
            d.join("cur/A-101.pdf"),
            pdf(&[SyntheticPage::new(612.0, 500.0, drawing(1.0, 0.0, 0.0, false))]),
        )
        .unwrap();
        std::fs::write(
            d.join("rev/A-101.pdf"),
            pdf(&[SyntheticPage::new(612.0, 500.0, drawing(0.7, 60.0, 30.0, true))]),
        )
        .unwrap();
        // M-201: unrelated drawings.
        std::fs::write(
            d.join("cur/M-201.pdf"),
            pdf(&[SyntheticPage::new(612.0, 500.0, drawing(1.0, 0.0, 0.0, false))]),
        )
        .unwrap();
        std::fs::write(
            d.join("rev/M-201.pdf"),
            pdf(&[SyntheticPage::new(
                612.0,
                500.0,
                format!(
                    "{}{}",
                    line(50.0, 450.0, 560.0, 60.0, 2.0),
                    rect(80.0, 80.0, 20.0, 300.0)
                ),
            )]),
        )
        .unwrap();
        let job = BatchJob {
            current: vec![d.join("cur/A-101.pdf"), d.join("cur/M-201.pdf")],
            revised: vec![d.join("rev/A-101.pdf"), d.join("rev/M-201.pdf")],
            ..Default::default()
        };
        let r = smart_overlay(&job, &d.join("out"), true).unwrap();
        assert_eq!(r.sheets.len(), 2, "{r:?}");
        let a = r.sheets.iter().find(|s| s.sheet.starts_with("A-101")).unwrap();
        let m = r.sheets.iter().find(|s| s.sheet.starts_with("M-201")).unwrap();
        assert!(a.error.is_empty() && a.output.is_file(), "{a:?}");
        assert!(a.score > 0.8, "same plan rescaled scores high: {}", a.score);
        assert!(m.score < 0.5, "unrelated sheets score low: {}", m.score);
        assert_eq!(a.discipline, "Architectural");
        assert_eq!(m.discipline, "Mechanical");
        assert_eq!(r.disciplines.len(), 2);
        let csv = smart_report_csv(&r);
        assert!(csv.contains("Architectural") && csv.contains("Match %"));
        // The registration found the scale (1 / 0.7).
        let ba = read_pdf(&d.join("cur/A-101.pdf")).unwrap();
        let bb = read_pdf(&d.join("rev/A-101.pdf")).unwrap();
        let (mx, _) = content_align(&ba, 0, &bb, 0).unwrap();
        assert!((mx[0] - 1.0 / 0.7).abs() < 0.08, "{mx:?}");
    }
}
