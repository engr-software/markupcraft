//! Vector paths: move / line / cubic Bezier / close, in PDF user space.

use crate::Point;

/// One path segment. `Curve(c1, c2, end)` is a cubic Bezier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}

pub type Path = Vec<Seg>;

/// A polyline through `pts`, optionally closed.
pub fn polyline(pts: &[Point], closed: bool) -> Path {
    let mut p: Path = pts
        .iter()
        .enumerate()
        .map(|(i, q)| if i == 0 { Seg::Move(*q) } else { Seg::Line(*q) })
        .collect();
    if closed && !pts.is_empty() {
        p.push(Seg::Close);
    }
    p
}

/// Every point the path names (end points and control points), for bounding boxes.
pub fn path_points(p: &Path) -> Vec<Point> {
    let mut out = Vec::with_capacity(p.len() * 3);
    for s in p {
        match *s {
            Seg::Move(a) | Seg::Line(a) => out.push(a),
            Seg::Curve(a, b, c) => out.extend([a, b, c]),
            Seg::Close => {}
        }
    }
    out
}
