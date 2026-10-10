//! Free rotation of box markups (rectangle, ellipse, text box, stamp): the points keep the
//! unturned box and `Markup::rotation` turns it about its centre (counter-clockwise degrees).
//! Written as Revu's `/Rotation` plus an `/AP /N /Matrix` that turns the appearance.

use markupcraft_geom::{Point, Rect, bbox};

use crate::{Kind, Markup};

/// Kinds that keep an angle of their own instead of turning their points.
pub fn free_rotates(k: Kind) -> bool {
    matches!(k, Kind::Rectangle | Kind::Ellipse | Kind::Text | Kind::Stamp)
}

/// `d` in `[0, 360)`; anything not finite, or within a hair of a full turn, is 0.
pub fn norm_degrees(d: f64) -> f64 {
    if !d.is_finite() {
        return 0.0;
    }
    let r = d.rem_euclid(360.0);
    if r < 1e-9 || 360.0 - r < 1e-9 { 0.0 } else { r }
}

/// The unturned box: the bounding box of the first four points.
pub fn frame(m: &Markup) -> Option<Rect> {
    bbox(m.pts.get(..4)?)
}

/// The turn of a markup: (centre of its box, radians counter-clockwise), when it has one.
pub fn turn_of(m: &Markup) -> Option<(Point, f64)> {
    if !free_rotates(m.kind) || norm_degrees(m.rotation) == 0.0 {
        return None;
    }
    let b = frame(m)?;
    Some((
        Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
        norm_degrees(m.rotation).to_radians(),
    ))
}

/// `p` turned by `rad` about `c`.
pub fn turn_point(c: Point, rad: f64, p: Point) -> Point {
    let (s, co) = rad.sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    Point::new(c.x + dx * co - dy * s, c.y + dx * s + dy * co)
}

/// The corners of the turned box (`None` when the markup is not turned).
pub fn turned_corners(m: &Markup) -> Option<Vec<Point>> {
    let (c, rad) = turn_of(m)?;
    let b = frame(m)?;
    Some(b.corners().iter().map(|p| turn_point(c, rad, *p)).collect())
}

/// The bounding box of the turned box (`None` when the markup is not turned).
pub fn turned_bounds(m: &Markup) -> Option<Rect> {
    bbox(&turned_corners(m)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_box_has_turned_corners() {
        let mut m = Markup::new(Kind::Rectangle, 0, Rect::new(0.0, 0.0, 20.0, 10.0).corners().to_vec());
        assert!(turn_of(&m).is_none());
        m.rotation = 90.0;
        let b = turned_bounds(&m).expect("turned");
        assert!(
            (b.width() - 10.0).abs() < 1e-9 && (b.height() - 20.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(norm_degrees(-30.0), 330.0);
        assert_eq!(norm_degrees(720.0), 0.0);
        m.kind = Kind::Polygon;
        assert!(turn_of(&m).is_none(), "outlines turn their points instead");
    }
}
