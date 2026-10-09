//! Dynamic Fill by detection on a rendered image of the page, for scans and drawings whose
//! linework is not vector: the page is rendered at a chosen DPI (markups hidden or not), dark
//! pixels (by an edge sensitivity) are walls, small gaps are closed by growing the walls, the
//! region around the seed is flood filled, and its outline (with islands as cutouts) is traced
//! back to page coordinates.

use std::collections::{HashMap, VecDeque};

use markupcraft_geom::Point;

use crate::fill::FillRegion;
use crate::{Result, Session, invalid};

/// Detection settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterFill {
    /// Detection resolution, 36 to 300 dots per inch.
    pub dpi: f64,
    /// Edge sensitivity, 1 to 254: pixels darker than this grey level are walls (higher finds
    /// fainter lines).
    pub sensitivity: u8,
    /// Gaps up to this many points are closed.
    pub gap: f64,
    /// Leave markups out of the image (they are not walls).
    pub hide_markups: bool,
    /// Islands inside the region become cutouts.
    pub cutouts: bool,
}

impl Default for RasterFill {
    fn default() -> Self {
        Self {
            dpi: 100.0,
            sensitivity: 160,
            gap: 1.0,
            hide_markups: true,
            cutouts: true,
        }
    }
}

/// Most pixels a filled region may cover.
const MAX_FILL: usize = 40_000_000;

/// Douglas-Peucker simplification of a closed loop of pixel corners.
fn simplify(pts: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    fn rec(p: &[(f64, f64)], a: usize, b: usize, tol: f64, keep: &mut Vec<bool>) {
        let (Some(pa), Some(pb)) = (p.get(a), p.get(b)) else {
            return;
        };
        let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let mut best = (0.0, a);
        for i in a + 1..b {
            let Some(q) = p.get(i) else { continue };
            let d = ((q.0 - pa.0) * dy - (q.1 - pa.1) * dx).abs() / len;
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            if let Some(k) = keep.get_mut(best.1) {
                *k = true;
            }
            rec(p, a, best.1, tol, keep);
            rec(p, best.1, b, tol, keep);
        }
    }
    let n = pts.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n / 2] = true;
    rec(pts, 0, n / 2, tol, &mut keep);
    let mut tail: Vec<(f64, f64)> = pts[n / 2..].to_vec();
    tail.push(pts[0]);
    let mut keep2 = vec![false; tail.len()];
    rec(&tail, 0, tail.len() - 1, tol, &mut keep2);
    for (i, k) in keep2.iter().enumerate() {
        if *k && let Some(slot) = keep.get_mut(n / 2 + i) {
            *slot = true;
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

fn shoelace(p: &[Point]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// The loops of the boundary between filled and empty pixels (pixel-corner coordinates),
/// each closed, the filled side on the right.
fn boundary_loops(mask: &[bool], w: usize, h: usize) -> Vec<Vec<(f64, f64)>> {
    let at = |x: i64, y: i64| -> bool {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && mask[y as usize * w + x as usize]
    };
    // directed edges start -> end, around each filled pixel's empty sides (clockwise in image
    // coordinates, y down)
    let mut next: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !at(x, y) {
                continue;
            }
            if !at(x, y - 1) {
                next.entry((x, y)).or_default().push((x + 1, y));
            }
            if !at(x + 1, y) {
                next.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !at(x, y + 1) {
                next.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !at(x - 1, y) {
                next.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut loops = Vec::new();
    let starts: Vec<(i64, i64)> = next.keys().copied().collect();
    for s in starts {
        while let Some(first) = next.get_mut(&s).and_then(Vec::pop) {
            let mut lp = vec![(s.0 as f64, s.1 as f64)];
            let mut cur = first;
            let mut guard = 0usize;
            while cur != s && guard < 50_000_000 {
                lp.push((cur.0 as f64, cur.1 as f64));
                match next.get_mut(&cur).and_then(Vec::pop) {
                    Some(n) => cur = n,
                    None => break,
                }
                guard += 1;
            }
            if lp.len() >= 4 {
                loops.push(lp);
            }
        }
    }
    loops
}

impl Session {
    /// Dynamic Fill on the rendered page image around `seed` (page 0-based, user space).
    pub fn dynamic_fill_raster(&self, page: usize, seed: Point, o: &RasterFill) -> Result<FillRegion> {
        if !(o.dpi.is_finite() && (36.0..=300.0).contains(&o.dpi)) {
            return Err(invalid("detection DPI is 36 to 300"));
        }
        if !(1..=254).contains(&o.sensitivity) || !(o.gap.is_finite() && (0.0..=36.0).contains(&o.gap)) {
            return Err(invalid("sensitivity is 1 to 254 and the gap 0 to 36 points"));
        }
        let doc = self.renderable(o.hide_markups)?;
        let img = doc.render(page, (o.dpi / 72.0) as f32, 8_000.0)?;
        let (w, h) = (img.gray.w, img.gray.h);
        if w < 3 || h < 3 {
            return Err(invalid("the page is too small to fill"));
        }
        let mut wall: Vec<bool> = (0..w * h).map(|i| img.gray.get(i % w, i / w) < o.sensitivity).collect();
        // close gaps: grow the walls by the gap's half in pixels
        let r = ((o.gap * f64::from(img.scale)) / 2.0).ceil() as i64;
        if r > 0 {
            let src = wall.clone();
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if src[y as usize * w + x as usize] {
                        continue;
                    }
                    'near: for dy in -r..=r {
                        for dx in -r..=r {
                            let (nx, ny) = (x + dx, y + dy);
                            if nx >= 0
                                && ny >= 0
                                && (nx as usize) < w
                                && (ny as usize) < h
                                && src[ny as usize * w + nx as usize]
                            {
                                wall[y as usize * w + x as usize] = true;
                                break 'near;
                            }
                        }
                    }
                }
            }
        }
        let px = img.rect_to_px(markupcraft_geom::Rect::new(seed.x, seed.y, seed.x, seed.y));
        let (sx, sy) = (px[0].floor(), px[1].floor());
        if !(sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h) {
            return Err(invalid("the point is off the page"));
        }
        let (sx, sy) = (sx as usize, sy as usize);
        if wall[sy * w + sx] {
            return Err(invalid("the point is on a line: click inside a room"));
        }
        let mut fill = vec![false; w * h];
        let mut q = VecDeque::from([(sx, sy)]);
        fill[sy * w + sx] = true;
        let mut n = 0usize;
        while let Some((x, y)) = q.pop_front() {
            n += 1;
            if n > MAX_FILL {
                return Err(invalid("the region is too large"));
            }
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                return Err(invalid(
                    "no closed region around the point (it runs off the page; raise the gap or the sensitivity)",
                ));
            }
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                let i = ny * w + nx;
                if !fill[i] && !wall[i] {
                    fill[i] = true;
                    q.push_back((nx, ny));
                }
            }
        }
        let loops = boundary_loops(&fill, w, h);
        let to_user =
            |lp: &[(f64, f64)]| -> Vec<Point> { simplify(lp, 0.75).iter().map(|(x, y)| img.to_user(*x, *y)).collect() };
        let mut polys: Vec<Vec<Point>> = loops.iter().map(|l| to_user(l)).filter(|p| p.len() >= 3).collect();
        polys.sort_by(|a, b| shoelace(b).abs().total_cmp(&shoelace(a).abs()));
        let Some(mut outer) = polys.first().cloned() else {
            return Err(invalid("no region found"));
        };
        if shoelace(&outer) < 0.0 {
            outer.reverse();
        }
        let min_hole = (2.0 * 72.0 / o.dpi).powi(2) * 4.0;
        let mut holes: Vec<Vec<Point>> = if o.cutouts {
            polys
                .iter()
                .skip(1)
                .filter(|p| shoelace(p).abs() > min_hole)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        for hl in &mut holes {
            if shoelace(hl) > 0.0 {
                hl.reverse();
            }
        }
        let area = shoelace(&outer).abs() - holes.iter().map(|h| shoelace(h).abs()).sum::<f64>();
        Ok(FillRegion {
            outer,
            holes,
            area,
            segments: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn a_room_on_the_page_image_with_a_gap_closed() {
        // a 200 x 100 point room drawn as four thick lines, with a 2 point gap in its top wall
        let content = "3 w 100 100 m 300 100 l S 300 100 m 300 200 l S 100 200 m 199 200 l S 201 200 m 300 200 l S 100 200 m 100 100 l S\n".to_string();
        let s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, content)]), "r.pdf").unwrap();
        let o = RasterFill {
            dpi: 72.0,
            gap: 4.0,
            ..Default::default()
        };
        let r = s.dynamic_fill_raster(0, Point::new(200.0, 150.0), &o).unwrap();
        // walls are thick and grown: the area is a bit under 200 x 100
        assert!(r.area > 15_000.0 && r.area < 20_000.0, "{}", r.area);
        assert!(r.outer.len() >= 4);
        let open = RasterFill {
            dpi: 72.0,
            gap: 0.0,
            ..Default::default()
        };
        assert!(
            s.dynamic_fill_raster(0, Point::new(200.0, 150.0), &open).is_err(),
            "the gap leaks"
        );
        assert!(
            s.dynamic_fill_raster(0, Point::new(200.0, 150.0), &RasterFill { dpi: 5.0, ..o })
                .is_err()
        );
    }
}
