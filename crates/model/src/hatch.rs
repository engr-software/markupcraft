//! Hatch patterns for closed markups (Area, Polygon, Rectangle, Ellipse, Cloud, Volume): lines
//! at a fixed spacing drawn inside the shape, over or instead of its fill. Our own design.

use markupcraft_geom::{Point, Rect};
use serde::{Deserialize, Serialize};

use crate::Color;

/// Most hatch lines drawn in one direction.
pub const MAX_LINES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HatchStyle {
    /// lines rising to the right (45°)
    Diagonal,
    /// lines falling to the right (135°)
    BackDiagonal,
    Horizontal,
    Vertical,
    /// horizontal and vertical
    Cross,
    /// both diagonals
    DiagonalCross,
}

impl HatchStyle {
    pub const ALL: [HatchStyle; 6] = [
        HatchStyle::Diagonal,
        HatchStyle::BackDiagonal,
        HatchStyle::Horizontal,
        HatchStyle::Vertical,
        HatchStyle::Cross,
        HatchStyle::DiagonalCross,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HatchStyle::Diagonal => "Diagonal",
            HatchStyle::BackDiagonal => "BackDiagonal",
            HatchStyle::Horizontal => "Horizontal",
            HatchStyle::Vertical => "Vertical",
            HatchStyle::Cross => "Cross",
            HatchStyle::DiagonalCross => "DiagonalCross",
        }
    }

    /// By name, ignoring case, spaces, `-` and `_`.
    pub fn from_name(s: &str) -> Option<Self> {
        let squash = |t: &str| {
            t.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        };
        let want = squash(s);
        Self::ALL.into_iter().find(|h| squash(h.name()) == want)
    }

    /// Line directions in degrees.
    pub fn angles(self) -> &'static [f64] {
        match self {
            HatchStyle::Diagonal => &[45.0],
            HatchStyle::BackDiagonal => &[135.0],
            HatchStyle::Horizontal => &[0.0],
            HatchStyle::Vertical => &[90.0],
            HatchStyle::Cross => &[0.0, 90.0],
            HatchStyle::DiagonalCross => &[45.0, 135.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Hatch {
    pub style: HatchStyle,
    /// distance between lines, PDF points
    pub spacing: f64,
    /// line width, PDF points
    pub width: f64,
    /// `None` = the markup's line colour
    pub color: Option<Color>,
}

impl Default for Hatch {
    fn default() -> Self {
        Self {
            style: HatchStyle::Diagonal,
            spacing: 6.0,
            width: 0.5,
            color: None,
        }
    }
}

impl Hatch {
    /// Spacing and width are positive and finite.
    pub fn valid(&self) -> bool {
        self.spacing.is_finite() && self.spacing >= 0.5 && self.width.is_finite() && self.width >= 0.0
    }

    /// The hatch lines covering box `b` (to be clipped to the shape).
    pub fn lines(&self, b: Rect) -> Vec<(Point, Point)> {
        let b = b.normalized();
        let mut out = Vec::new();
        if !self.valid() || !(b.width().is_finite() && b.height().is_finite()) {
            return out;
        }
        let c = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        let r = (b.width().hypot(b.height()) / 2.0).max(1e-9);
        let n = ((r / self.spacing).ceil() as usize).min(MAX_LINES / 2);
        for deg in self.style.angles() {
            let t = deg.to_radians();
            let (dx, dy) = (t.cos(), t.sin());
            let (nx, ny) = (-dy, dx);
            // lines on a grid anchored at the page origin, so neighbouring shapes line up
            let off0 = (c.x * nx + c.y * ny) / self.spacing;
            let base = off0.round();
            for k in 0..=(2 * n) {
                let j = base + k as f64 - n as f64;
                let d = j * self.spacing - (c.x * nx + c.y * ny);
                let m = Point::new(c.x + nx * d, c.y + ny * d);
                out.push((
                    Point::new(m.x - dx * r, m.y - dy * r),
                    Point::new(m.x + dx * r, m.y + dy * r),
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hatch_lines_cover_the_box() {
        let h = Hatch {
            style: HatchStyle::Cross,
            spacing: 10.0,
            ..Default::default()
        };
        let l = h.lines(Rect::new(0.0, 0.0, 100.0, 50.0));
        // both directions, each spanning the box's diagonal
        assert!(l.len() >= 2 * 10);
        assert!(
            l.iter()
                .any(|(a, b)| (a.y - b.y).abs() < 1e-9 && (a.y - 20.0).abs() < 1e-9)
        );
        assert_eq!(HatchStyle::from_name("diagonal-cross"), Some(HatchStyle::DiagonalCross));
        assert!(
            Hatch {
                spacing: 0.0,
                ..Default::default()
            }
            .lines(Rect::new(0.0, 0.0, 1.0, 1.0))
            .is_empty()
        );
    }
}
