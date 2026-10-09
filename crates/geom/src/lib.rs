//! Points, rectangles and path geometry in PDF user space (bottom-left origin, points).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod marks;
pub mod path;
pub mod shapes;
pub mod snap;
pub mod text;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dist(self, o: Point) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }
    pub fn mid(self, o: Point) -> Point {
        Point::new((self.x + o.x) / 2.0, (self.y + o.y) / 2.0)
    }
}

impl std::ops::Add for Point {
    type Output = Point;
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Point {
    type Output = Point;
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}

/// An axis-aligned rectangle `[x0 y0 x1 y1]` as PDF writes it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }
    /// The same rectangle with x0 <= x1 and y0 <= y1.
    pub fn normalized(self) -> Rect {
        Rect::new(
            self.x0.min(self.x1),
            self.y0.min(self.y1),
            self.x0.max(self.x1),
            self.y0.max(self.y1),
        )
    }
    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).abs()
    }
    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).abs()
    }
    pub fn contains(&self, p: Point) -> bool {
        let r = self.normalized();
        p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1
    }
    pub fn as_array(&self) -> [f64; 4] {
        [self.x0, self.y0, self.x1, self.y1]
    }
    pub fn from_array(a: [f64; 4]) -> Rect {
        Rect::new(a[0], a[1], a[2], a[3])
    }
    /// The four corners counter-clockwise from the lower left.
    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.x0, self.y0),
            Point::new(self.x1, self.y0),
            Point::new(self.x1, self.y1),
            Point::new(self.x0, self.y1),
        ]
    }
    pub fn padded(self, pad: f64) -> Rect {
        Rect::new(self.x0 - pad, self.y0 - pad, self.x1 + pad, self.y1 + pad)
    }
}

/// Bounding box of `pts`, `None` when empty.
pub fn bbox(pts: &[Point]) -> Option<Rect> {
    let first = pts.first()?;
    let mut r = Rect::new(first.x, first.y, first.x, first.y);
    for p in pts {
        r.x0 = r.x0.min(p.x);
        r.y0 = r.y0.min(p.y);
        r.x1 = r.x1.max(p.x);
        r.y1 = r.y1.max(p.y);
    }
    Some(r)
}

/// Shoelace area, absolute.
pub fn polygon_area(p: &[Point]) -> f64 {
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        let a = p[i];
        let b = p[(i + 1) % n];
        s += a.x * b.y - b.x * a.y;
    }
    s.abs() / 2.0
}

/// Length along `p`; `closed` adds the closing segment (only with 3+ points).
pub fn polyline_length(p: &[Point], closed: bool) -> f64 {
    let mut s: f64 = p.windows(2).map(|w| w[0].dist(w[1])).sum();
    if closed
        && p.len() > 2
        && let (Some(f), Some(l)) = (p.first(), p.last())
    {
        s += f.dist(*l);
    }
    s
}

/// Mean of the vertices (Revu anchors an area's caption here).
pub fn vertex_mean(p: &[Point]) -> Point {
    if p.is_empty() {
        return Point::default();
    }
    let n = p.len() as f64;
    let (sx, sy) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q.x, y + q.y));
    Point::new(sx / n, sy / n)
}

/// Distance from `p` to segment `a`-`b`.
pub fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 <= f64::EPSILON {
        return p.dist(a);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0);
    p.dist(Point::new(a.x + t * d.x, a.y + t * d.y))
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_area_and_perimeter() {
        let sq = Rect::new(0.0, 0.0, 10.0, 10.0).corners();
        assert_eq!(polygon_area(&sq), 100.0);
        assert_eq!(polyline_length(&sq, true), 40.0);
        assert_eq!(polyline_length(&sq, false), 30.0);
        assert!(point_in_polygon(Point::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(15.0, 5.0), &sq));
    }
}
