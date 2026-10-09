//! Geometry edits on one markup: move, rotate, resize, vertices. Everything that is drawn in
//! page space moves with the shape (cutouts, the popup window, a moved caption).

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup};

use crate::{Result, invalid};

/// Most vertices a markup may have.
pub const MAX_POINTS: usize = 100_000;
/// Coordinates beyond this are refused (PDF implementation limits are far smaller).
pub const MAX_COORD: f64 = 1.0e7;

/// Fewest and most points a kind takes.
pub fn point_limits(kind: Kind) -> (usize, usize) {
    match kind {
        Kind::Length | Kind::Line | Kind::Arrow | Kind::Diameter | Kind::Radius => (2, 2),
        Kind::Angle => (3, 3),
        Kind::Area | Kind::Perimeter | Kind::Polygon | Kind::Cloud | Kind::Volume => (3, MAX_POINTS),
        Kind::Polylength | Kind::Polyline | Kind::Ink | Kind::Highlight => (2, MAX_POINTS),
        Kind::Rectangle | Kind::Ellipse | Kind::Text | Kind::Typewriter | Kind::Stamp | Kind::Snapshot => {
            (2, MAX_POINTS)
        }
        _ => (1, MAX_POINTS),
    }
}

/// Check a point list for `kind`: count and finite, in-range coordinates.
pub fn check_points(kind: Kind, pts: &[Point]) -> Result<()> {
    let (lo, hi) = point_limits(kind);
    if pts.len() < lo || pts.len() > hi {
        let want = if lo == hi {
            format!("exactly {lo}")
        } else if hi == MAX_POINTS {
            format!("at least {lo}")
        } else {
            format!("{lo} to {hi}")
        };
        return Err(invalid(format!(
            "{} needs {want} points (got {})",
            kind.name(),
            pts.len()
        )));
    }
    check_finite(pts)
}

pub fn check_finite(pts: &[Point]) -> Result<()> {
    if pts
        .iter()
        .any(|p| !(p.x.is_finite() && p.y.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD))
    {
        return Err(invalid(format!(
            "coordinates must be finite numbers within ±{MAX_COORD}"
        )));
    }
    Ok(())
}

/// Box-shaped kinds store their outline as the four corners of a box.
pub fn is_box(kind: Kind) -> bool {
    matches!(kind, Kind::Rectangle | Kind::Ellipse)
}

/// Box kinds: the four corners of the points' bounding box.
pub fn normalize(m: &mut Markup) {
    if is_box(m.kind)
        && let Some(b) = bbox(&m.pts)
    {
        m.pts = b.corners().to_vec();
    }
}

fn each_point(m: &mut Markup, mut f: impl FnMut(Point) -> Point) {
    for p in &mut m.pts {
        *p = f(*p);
    }
    for h in &mut m.holes {
        for p in h {
            *p = f(*p);
        }
    }
}

fn map_rect(r: &Rect, f: impl Fn(Point) -> Point) -> Rect {
    let c = r.corners().map(f);
    bbox(&c).unwrap_or(*r)
}

pub fn translate(m: &mut Markup, dx: f64, dy: f64) {
    let d = Point::new(dx, dy);
    each_point(m, |p| p + d);
    m.rect = Rect::new(m.rect.x0 + dx, m.rect.y0 + dy, m.rect.x1 + dx, m.rect.y1 + dy);
    if let Some(r) = &mut m.popup {
        *r = Rect::new(r.x0 + dx, r.y0 + dy, r.x1 + dx, r.y1 + dy);
    }
    m.dirty = true;
}

/// Centre of the markup's points.
pub fn center(m: &Markup) -> Option<Point> {
    let b = bbox(&m.pts)?;
    Some(Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0))
}

/// Rotate counter-clockwise by `degrees` about `c` (PDF space, y up). Box kinds turn only by
/// multiples of 90 degrees.
pub fn rotate(m: &mut Markup, degrees: f64, c: Point) -> Result<()> {
    if !degrees.is_finite() {
        return Err(invalid("degrees must be a number"));
    }
    if is_box(m.kind) && degrees.rem_euclid(90.0).abs() > 1e-9 && (degrees.rem_euclid(90.0) - 90.0).abs() > 1e-9 {
        return Err(invalid(format!(
            "a {} turns only by multiples of 90 degrees",
            m.kind.name()
        )));
    }
    // Quarter turns exactly, so boxes stay on whole numbers.
    let quarter = [
        (0.0, (1.0, 0.0)),
        (90.0, (0.0, 1.0)),
        (180.0, (-1.0, 0.0)),
        (270.0, (0.0, -1.0)),
    ];
    let r = degrees.rem_euclid(360.0);
    let (cs, sn) = quarter
        .iter()
        .find(|(d, _)| (r - d).abs() < 1e-12)
        .map(|(_, cs)| *cs)
        .unwrap_or_else(|| (degrees.to_radians().cos(), degrees.to_radians().sin()));
    let turn = |p: Point| {
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        Point::new(c.x + dx * cs - dy * sn, c.y + dx * sn + dy * cs)
    };
    each_point(m, turn);
    if let Some(o) = &mut m.caption_offset {
        *o = Point::new(o.x * cs - o.y * sn, o.x * sn + o.y * cs);
    }
    m.rect = map_rect(&m.rect, turn);
    normalize(m);
    m.dirty = true;
    Ok(())
}

/// Scale the shape so its points' bounding box becomes `to`.
pub fn resize(m: &mut Markup, to: Rect) -> Result<()> {
    let to = to.normalized();
    check_finite(&to.corners())?;
    let from = bbox(&m.pts).ok_or_else(|| invalid("the markup has no points"))?;
    let sx = if from.width() > 1e-9 {
        to.width() / from.width()
    } else {
        1.0
    };
    let sy = if from.height() > 1e-9 {
        to.height() / from.height()
    } else {
        1.0
    };
    let map = |p: Point| Point::new(to.x0 + (p.x - from.x0) * sx, to.y0 + (p.y - from.y0) * sy);
    each_point(m, map);
    if let Some(o) = &mut m.caption_offset {
        *o = Point::new(o.x * sx, o.y * sy);
    }
    m.rect = map_rect(&m.rect, map);
    normalize(m);
    m.dirty = true;
    Ok(())
}

/// Replace the points.
pub fn set_points(m: &mut Markup, pts: Vec<Point>) -> Result<()> {
    check_points(m.kind, &pts)?;
    if !m.arcs.is_empty() && pts.len() != m.pts.len() {
        return Err(invalid("this markup has arcs; keep the same number of points"));
    }
    if m.kind == Kind::Ink && m.strokes.iter().any(|s| *s >= pts.len()) {
        m.strokes.retain(|s| *s < pts.len());
    }
    m.pts = pts;
    normalize(m);
    m.dirty = true;
    Ok(())
}

/// Move vertex `index` to `p`.
pub fn move_vertex(m: &mut Markup, index: usize, p: Point) -> Result<()> {
    check_finite(&[p])?;
    if is_box(m.kind) {
        // Dragging a corner of a box: the opposite corner stays.
        if index >= 4 {
            return Err(invalid(format!("vertex {} does not exist (a box has 4)", index + 1)));
        }
        let b = bbox(&m.pts).ok_or_else(|| invalid("the markup has no points"))?;
        let opposite = b
            .corners()
            .get((index + 2) % 4)
            .copied()
            .ok_or_else(|| invalid("a box has 4 corners"))?;
        m.pts = vec![opposite, p];
        normalize(m);
        m.dirty = true;
        return Ok(());
    }
    let n = m.pts.len();
    let v = m
        .pts
        .get_mut(index)
        .ok_or_else(|| invalid(format!("vertex {} does not exist (the markup has {n})", index + 1)))?;
    *v = p;
    m.dirty = true;
    Ok(())
}

/// Insert `p` so it becomes vertex `index`.
pub fn insert_vertex(m: &mut Markup, index: usize, p: Point) -> Result<()> {
    check_finite(&[p])?;
    let (_, hi) = point_limits(m.kind);
    if is_box(m.kind) || m.pts.len() >= hi {
        return Err(invalid(format!("a {} cannot take another vertex", m.kind.name())));
    }
    if !m.arcs.is_empty() {
        return Err(invalid(
            "this markup has arcs; its vertices cannot be inserted or removed",
        ));
    }
    if index > m.pts.len() {
        return Err(invalid(format!(
            "position {} is past the end (1 to {})",
            index + 1,
            m.pts.len() + 1
        )));
    }
    m.pts.insert(index, p);
    for s in &mut m.strokes {
        if *s > index {
            *s += 1;
        }
    }
    m.dirty = true;
    Ok(())
}

/// Remove vertex `index`.
pub fn delete_vertex(m: &mut Markup, index: usize) -> Result<()> {
    let (lo, _) = point_limits(m.kind);
    if is_box(m.kind) || m.pts.len() <= lo {
        return Err(invalid(format!(
            "a {} needs at least {lo} vertices; it has {}",
            m.kind.name(),
            m.pts.len()
        )));
    }
    if !m.arcs.is_empty() {
        return Err(invalid(
            "this markup has arcs; its vertices cannot be inserted or removed",
        ));
    }
    if index >= m.pts.len() {
        return Err(invalid(format!(
            "vertex {} does not exist (the markup has {})",
            index + 1,
            m.pts.len()
        )));
    }
    m.pts.remove(index);
    let n = m.pts.len();
    for s in &mut m.strokes {
        if *s > index {
            *s -= 1;
        }
    }
    m.strokes.retain(|s| *s > 0 && *s < n);
    m.strokes.dedup();
    m.dirty = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_quarter_turn_about_centre() {
        let mut m = Markup::new(
            Kind::Polygon,
            0,
            vec![Point::new(0.0, 0.0), Point::new(2.0, 0.0), Point::new(2.0, 1.0)],
        );
        rotate(&mut m, 90.0, Point::new(1.0, 0.5)).unwrap();
        let p = m.pts[0];
        assert!((p.x - 1.5).abs() < 1e-9 && (p.y - -0.5).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn box_rotates_by_quarters_only() {
        let mut m = Markup::new(Kind::Rectangle, 0, Rect::new(0.0, 0.0, 4.0, 2.0).corners().to_vec());
        assert!(rotate(&mut m, 45.0, Point::new(2.0, 1.0)).is_err());
        rotate(&mut m, 90.0, Point::new(2.0, 1.0)).unwrap();
        assert_eq!(bbox(&m.pts).unwrap(), Rect::new(1.0, -1.0, 3.0, 3.0));
    }

    #[test]
    fn resize_maps_box() {
        let mut m = Markup::new(Kind::Area, 0, Rect::new(0.0, 0.0, 10.0, 10.0).corners().to_vec());
        m.holes.push(vec![Point::new(5.0, 5.0)]);
        resize(&mut m, Rect::new(100.0, 100.0, 120.0, 110.0)).unwrap();
        assert_eq!(bbox(&m.pts).unwrap(), Rect::new(100.0, 100.0, 120.0, 110.0));
        assert_eq!(m.holes[0][0], Point::new(110.0, 105.0));
    }
}
