//! Markup shapes: revision clouds, ellipses and line endings.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::path::{Path, Seg};
use crate::{Point, Rect};

/// Bezier quarter-circle constant.
pub const KAPPA: f64 = 0.552_284_749_8;

/// Distance between scallops: Revu's intensity 2 measures about 14.2 pt between scallops.
pub fn scallop_spacing(intensity: f64) -> f64 {
    7.085 * intensity.max(0.25)
}

/// How far the scallops reach outside the outline.
pub fn cloud_bulge(intensity: f64) -> f64 {
    0.6 * scallop_spacing(intensity)
}

/// Circular arc from angle `a0` sweeping `sweep` radians on circle (`c`, `r`), as Beziers of at
/// most 90 degrees each.
fn append_arc(out: &mut Path, c: Point, r: f64, a0: f64, sweep: f64) {
    let n = ((sweep.abs() / FRAC_PI_2 - 1e-9).ceil() as i64).clamp(1, 8);
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for i in 0..n {
        let t0 = a0 + step * i as f64;
        let t1 = t0 + step;
        let p0 = Point::new(c.x + r * t0.cos(), c.y + r * t0.sin());
        let p1 = Point::new(c.x + r * t1.cos(), c.y + r * t1.sin());
        out.push(Seg::Curve(
            Point::new(p0.x - k * r * t0.sin(), p0.y + k * r * t0.cos()),
            Point::new(p1.x + k * r * t1.sin(), p1.y - k * r * t1.cos()),
            p1,
        ));
    }
}

/// Most scallops one cloud draws (a guard against absurd outlines in untrusted files).
const MAX_SCALLOPS: usize = 20_000;

/// Revision cloud along the closed polygon `pts`: circles of radius `cloud_bulge` centred along
/// the outline every `scallop_spacing` points (every vertex is a centre); each draws its outer
/// arc between its outer crossings with its neighbours.
pub fn cloud_path(pts: &[Point], intensity: f64) -> Path {
    let mut out = Path::new();
    let n = pts.len();
    if n < 2 {
        return out;
    }
    let ring = || pts.iter().zip(pts.iter().cycle().skip(1));
    let area2: f64 = ring().map(|(a, b)| a.x * b.y - b.x * a.y).sum();
    // counter-clockwise (y up): the inside is to the left
    let sign = if area2 >= 0.0 { 1.0 } else { -1.0 };
    let spacing = scallop_spacing(intensity);
    let r = cloud_bulge(intensity);
    let mut c: Vec<Point> = Vec::new();
    for (a, b) in ring() {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if !len.is_finite() || len < 1e-6 {
            continue;
        }
        let k = ((len / spacing).round() as usize).max(1);
        if c.len() + k > MAX_SCALLOPS {
            break;
        }
        for j in 0..k {
            let t = j as f64 / k as f64;
            c.push(Point::new(a.x + dx * t, a.y + dy * t));
        }
    }
    let m = c.len();
    if m < 2 {
        return out;
    }
    // Outer crossing of two neighbouring circles.
    let crossing = |p: Point, q: Point| {
        let (dx, dy) = (q.x - p.x, q.y - p.y);
        let d = dx.hypot(dy);
        let mid = p.mid(q);
        if d < 1e-9 || d >= 2.0 * r {
            return mid;
        }
        let h = (r * r - d * d / 4.0).sqrt();
        Point::new(mid.x + dy / d * h * sign, mid.y - dx / d * h * sign)
    };
    // x[i] = crossing of circle i and i + 1
    let x: Vec<Point> = c
        .iter()
        .zip(c.iter().cycle().skip(1))
        .map(|(p, q)| crossing(*p, *q))
        .collect();
    let Some(last) = x.last() else { return out };
    out.push(Seg::Move(*last));
    let prev = std::iter::once(*last).chain(x.iter().copied());
    for ((ci, from), to) in c.iter().zip(prev).zip(x.iter()) {
        let a0 = (from.y - ci.y).atan2(from.x - ci.x);
        let a1 = (to.y - ci.y).atan2(to.x - ci.x);
        // the long way round, through the outside
        let mut sweep = a1 - a0;
        if sign > 0.0 {
            while sweep <= 0.0 {
                sweep += TAU;
            }
        } else {
            while sweep >= 0.0 {
                sweep -= TAU;
            }
        }
        append_arc(&mut out, *ci, r, a0, sweep);
    }
    out.push(Seg::Close);
    out
}

/// An ellipse inscribed in `b`, four Bezier quarters from the right-hand point, closed.
pub fn ellipse_path(b: Rect) -> Path {
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let (rx, ry) = ((b.x1 - b.x0) / 2.0, (b.y1 - b.y0) / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    vec![
        Seg::Move(p(cx + rx, cy)),
        Seg::Curve(p(cx + rx, cy + ky), p(cx + kx, cy + ry), p(cx, cy + ry)),
        Seg::Curve(p(cx - kx, cy + ry), p(cx - rx, cy + ky), p(cx - rx, cy)),
        Seg::Curve(p(cx - rx, cy - ky), p(cx - kx, cy - ry), p(cx, cy - ry)),
        Seg::Curve(p(cx + kx, cy - ry), p(cx + rx, cy - ky), p(cx + rx, cy)),
        Seg::Close,
    ]
}

/// Every `/LE` line ending style (ISO 32000-1 Table 176).
pub const LINE_ENDINGS: &[&str] = &[
    "None",
    "Square",
    "Circle",
    "Diamond",
    "OpenArrow",
    "ClosedArrow",
    "Butt",
    "ROpenArrow",
    "RClosedArrow",
    "Slash",
];

/// Furthest any ending reaches from its tip (the 7.8 x 4.5 arrow reaches 9.005 x width).
pub fn line_ending_reach(w: f64) -> f64 {
    9.1 * w.max(1.0)
}

/// A line ending's outline; `filled` = painted with the stroke colour (Revu fills closed shapes).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ending {
    pub path: Path,
    pub filled: bool,
}

/// One line ending at `tip` for a line arriving from `from`, for line width `w`. Unknown styles
/// and `None` give an empty path.
pub fn line_ending(from: Point, tip: Point, style: &str, w: f64) -> Ending {
    let mut e = Ending::default();
    let (dx, dy) = (tip.x - from.x, tip.y - from.y);
    let len = dx.hypot(dy);
    if !len.is_finite() || len < 1e-9 {
        return e;
    }
    let (ux, uy) = (dx / len, dy / len); // pointing at the tip
    let s = w.max(1.0);
    // Revu's arrows: barbs 7.8 x line width back along the line and 4.5 x width to each side.
    let (back, side) = (7.8 * s, 4.5 * s);
    let h = 3.0 * s; // half size of the symbol endings
    let p = Point::new;
    let arrow = |t: Point, dir: f64, closed: bool| {
        let (bx, by) = (-ux * dir, -uy * dir);
        let a1 = p(t.x + back * bx - side * by, t.y + back * by + side * bx);
        let a2 = p(t.x + back * bx + side * by, t.y + back * by - side * bx);
        let mut path = vec![Seg::Move(a1), Seg::Line(t), Seg::Line(a2)];
        if closed {
            path.push(Seg::Close);
        }
        Ending { path, filled: closed }
    };
    let behind = p(tip.x - ux * back, tip.y - uy * back);
    match style {
        "OpenArrow" => e = arrow(tip, 1.0, false),
        "ClosedArrow" => e = arrow(tip, 1.0, true),
        "ROpenArrow" => e = arrow(behind, -1.0, false),
        "RClosedArrow" => e = arrow(behind, -1.0, true),
        "Circle" => {
            e.path = ellipse_path(Rect::new(tip.x - h, tip.y - h, tip.x + h, tip.y + h));
            e.filled = true;
        }
        "Square" => {
            e.path = vec![
                Seg::Move(p(tip.x - h * ux + h * uy, tip.y - h * uy - h * ux)),
                Seg::Line(p(tip.x + h * ux + h * uy, tip.y + h * uy - h * ux)),
                Seg::Line(p(tip.x + h * ux - h * uy, tip.y + h * uy + h * ux)),
                Seg::Line(p(tip.x - h * ux - h * uy, tip.y - h * uy + h * ux)),
                Seg::Close,
            ];
            e.filled = true;
        }
        "Diamond" => {
            let d = h * 1.4;
            e.path = vec![
                Seg::Move(p(tip.x + d * ux, tip.y + d * uy)),
                Seg::Line(p(tip.x - d * uy, tip.y + d * ux)),
                Seg::Line(p(tip.x - d * ux, tip.y - d * uy)),
                Seg::Line(p(tip.x + d * uy, tip.y - d * ux)),
                Seg::Close,
            ];
            e.filled = true;
        }
        "Butt" => {
            e.path = vec![
                Seg::Move(p(tip.x - uy * h * 1.5, tip.y + ux * h * 1.5)),
                Seg::Line(p(tip.x + uy * h * 1.5, tip.y - ux * h * 1.5)),
            ];
        }
        "Slash" => {
            // 30 degrees off the perpendicular
            let (c30, s30) = (3.0_f64.sqrt() / 2.0, 0.5);
            let (ax, ay) = (-uy * c30 + ux * s30, ux * c30 + uy * s30);
            e.path = vec![
                Seg::Move(p(tip.x - ax * h * 1.5, tip.y - ay * h * 1.5)),
                Seg::Line(p(tip.x + ax * h * 1.5, tip.y + ay * h * 1.5)),
            ];
        }
        _ => {}
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_arrow_matches_revu() {
        // Revu Arrow, width 1.5: tip-to-barb 11.7 along the line, 6.75 to each side.
        let e = line_ending(
            Point::new(796.7693, 435.7891),
            Point::new(724.0405, 435.7891),
            "ClosedArrow",
            1.5,
        );
        assert!(e.filled);
        assert_eq!(e.path.len(), 4);
        let Some(Seg::Move(a)) = e.path.first() else {
            panic!("arrow starts with a move")
        };
        assert!((a.x - 735.7405).abs() < 1e-3);
        assert!(((a.y - 435.7891).abs() - 6.75).abs() < 1e-3);
    }

    #[test]
    fn every_ending_style_draws() {
        for s in LINE_ENDINGS {
            let e = line_ending(Point::new(0.0, 0.0), Point::new(50.0, 0.0), s, 1.0);
            assert_eq!(e.path.is_empty(), *s == "None", "{s}");
            let reach = line_ending_reach(1.0);
            for q in crate::path::path_points(&e.path) {
                assert!(q.dist(Point::new(50.0, 0.0)) <= reach + 1e-9, "{s} reaches too far");
            }
        }
        assert!(
            line_ending(Point::new(1.0, 1.0), Point::new(1.0, 1.0), "ClosedArrow", 1.0)
                .path
                .is_empty()
        );
    }

    #[test]
    fn cloud_is_closed_and_within_its_bulge() {
        let sq = Rect::new(0.0, 0.0, 100.0, 60.0).corners();
        let cl = cloud_path(&sq, 2.0);
        assert!(matches!(cl.first(), Some(Seg::Move(_))));
        assert_eq!(cl.last(), Some(&Seg::Close));
        let mut reach: f64 = 0.0;
        for s in &cl {
            if let Seg::Curve(_, _, c) = s {
                reach = reach.max((-c.y).max(c.y - 60.0));
            }
        }
        assert!(reach > 4.0 && reach <= cloud_bulge(2.0) + 1e-6);
        // clockwise outlines bulge outwards too
        let mut cw = sq.to_vec();
        cw.reverse();
        assert!(!cloud_path(&cw, 2.0).is_empty());
        assert!(cloud_path(&[Point::new(0.0, 0.0)], 2.0).is_empty());
    }

    #[test]
    fn ellipse_touches_its_box() {
        let e = ellipse_path(Rect::new(0.0, 0.0, 20.0, 10.0));
        assert_eq!(e.first(), Some(&Seg::Move(Point::new(20.0, 5.0))));
        assert_eq!(e.len(), 6);
    }
}
