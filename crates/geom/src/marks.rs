//! How text markups (highlight, underline, strikethrough, squiggly over `/QuadPoints`), the
//! insert caret and the sticky-note icons look. Shared by the canvas and the appearance writer.

use crate::path::{Path, polyline};
use crate::shapes::ellipse_path;
use crate::{Point, Rect, bbox};

/// One run of text as `/QuadPoints` stores it: upper-left, upper-right, lower-left, lower-right.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Quad {
    pub ul: Point,
    pub ur: Point,
    pub ll: Point,
    pub lr: Point,
}

impl Quad {
    pub fn height(&self) -> f64 {
        self.ll.dist(self.ul)
    }
    /// In `/QuadPoints` order.
    pub fn points(&self) -> [Point; 4] {
        [self.ul, self.ur, self.ll, self.lr]
    }
}

/// Quads from `/QuadPoints` order points (a trailing partial quad is dropped).
pub fn quads_from_points(pts: &[Point]) -> Vec<Quad> {
    pts.as_chunks::<4>()
        .0
        .iter()
        .map(|[ul, ur, ll, lr]| Quad {
            ul: *ul,
            ur: *ur,
            ll: *ll,
            lr: *lr,
        })
        .collect()
}

/// Which mark a quad markup draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuadStyle {
    Highlight,
    Underline,
    Strikeout,
    Squiggly,
}

/// One painted part of a markup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mark {
    pub path: Path,
    pub fill: bool,
    pub stroke: bool,
    /// fill white instead of the markup colour (icon details)
    pub fill_white: bool,
    /// stroke near-black instead of the markup colour (icon outline)
    pub stroke_ink: bool,
    /// stroke width, points
    pub width: f64,
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// A line across quad `q` at fraction `t` of its height from the bottom.
fn across(q: &Quad, t: f64) -> Path {
    polyline(&[lerp(q.ll, q.ul, t), lerp(q.lr, q.ur, t)], false)
}

const MAX_SQUIGGLE_STEPS: usize = 20_000;

fn squiggle(q: &Quad) -> Path {
    let h = q.height();
    let len = q.ll.dist(q.lr);
    let base0 = lerp(q.ll, q.ul, 0.04);
    let base1 = lerp(q.lr, q.ur, 0.04);
    let hh = h.max(1e-9);
    let up = Point::new((q.ul.x - q.ll.x) / hh, (q.ul.y - q.ll.y) / hh);
    let amp = h / 9.0;
    let step = (h / 8.0).max(0.5);
    let n = ((len / step).ceil() as usize).clamp(2, MAX_SQUIGGLE_STEPS);
    let pts: Vec<Point> = (0..=n)
        .map(|i| {
            let p = lerp(base0, base1, i as f64 / n as f64);
            let off = if i % 2 == 1 { amp } else { 0.0 };
            Point::new(p.x + up.x * off, p.y + up.y * off)
        })
        .collect();
    polyline(&pts, false)
}

/// The marks of a text markup over `pts` (`/QuadPoints` order).
pub fn quad_marks(style: QuadStyle, pts: &[Point]) -> Vec<Mark> {
    quads_from_points(pts)
        .iter()
        .map(|q| {
            let h = q.height();
            let mut k = Mark {
                width: (h / 14.0).max(0.5),
                ..Default::default()
            };
            match style {
                QuadStyle::Highlight => {
                    k.path = polyline(&[q.ul, q.ur, q.lr, q.ll], true);
                    k.fill = true;
                }
                QuadStyle::Underline => {
                    k.path = across(q, 0.1);
                    k.stroke = true;
                }
                QuadStyle::Strikeout => {
                    k.path = across(q, 0.48);
                    k.stroke = true;
                }
                QuadStyle::Squiggly => {
                    k.path = squiggle(q);
                    k.stroke = true;
                    k.width = (h / 18.0).max(0.4);
                }
            }
            k
        })
        .collect()
}

/// Room a quad markup needs around its quads for its strokes.
pub fn quad_pad(pts: &[Point]) -> f64 {
    quads_from_points(pts)
        .iter()
        .map(|q| q.height() / 12.0)
        .fold(1.0, f64::max)
}

/// The insert caret filling box `b`.
pub fn caret_mark(b: Rect) -> Mark {
    let b = b.normalized();
    let xm = (b.x0 + b.x1) / 2.0;
    let h = b.y1 - b.y0;
    Mark {
        path: polyline(
            &[
                Point::new(b.x0, b.y0),
                Point::new(xm, b.y1),
                Point::new(b.x1, b.y0),
                Point::new(xm, b.y0 + 0.32 * h),
            ],
            true,
        ),
        fill: true,
        ..Default::default()
    }
}

/// The caret's box for an insertion point at the start (`at_end` false) or end of quad `q`,
/// sized to the text height.
pub fn caret_box(q: &Quad, at_end: bool) -> [Point; 4] {
    let h = q.height().max(2.0);
    let base = if at_end { q.lr } else { q.ll };
    let w = (0.4 * h).max(3.0);
    Rect::new(base.x - w / 2.0, base.y - 0.15 * h, base.x + w / 2.0, base.y + 0.4 * h).corners()
}

/// `/Name` values the Note icon offers (ISO 32000-1 12.5.6.4 plus Acrobat's Comment, Key ...).
pub const NOTE_ICONS: &[&str] = &["Comment", "Note", "Help", "Insert", "Key", "Paragraph", "NewParagraph"];

/// A note icon's default size, points (square).
pub const NOTE_SIZE: f64 = 24.0;
/// The caret's default width, points (height = 1.3 x width).
pub const CARET_WIDTH: f64 = 10.0;

/// Icon shapes on a unit square (0..1, y up), placed into the box.
struct Unit {
    x0: f64,
    y0: f64,
    s: f64,
}

impl Unit {
    fn at(&self, u: f64, v: f64) -> Point {
        Point::new(self.x0 + u * self.s, self.y0 + v * self.s)
    }
    fn poly(&self, uv: &[(f64, f64)], closed: bool) -> Path {
        let pts: Vec<Point> = uv.iter().map(|(u, v)| self.at(*u, *v)).collect();
        polyline(&pts, closed)
    }
    fn circle(&self, cu: f64, cv: f64, r: f64) -> Path {
        let c = self.at(cu, cv);
        let rr = r * self.s;
        ellipse_path(Rect::new(c.x - rr, c.y - rr, c.x + rr, c.y + rr))
    }
    fn ink(&self, path: Path, width: f64) -> Mark {
        Mark {
            path,
            stroke: true,
            stroke_ink: true,
            width,
            ..Default::default()
        }
    }
    fn text_lines(&self, out: &mut Vec<Mark>, vs: &[f64], from: f64, to: f64) {
        for v in vs {
            out.push(self.ink(self.poly(&[(from, *v), (to, *v)], false), self.s / 20.0));
        }
    }
}

/// The sticky-note icon `icon` (a `/Name`; unknown names draw the Comment bubble) in box `b`.
pub fn note_icon(b: Rect, icon: &str) -> Vec<Mark> {
    let b = b.normalized();
    let s = (b.x1 - b.x0).min(b.y1 - b.y0);
    let u = Unit {
        x0: (b.x0 + b.x1 - s) / 2.0,
        y0: (b.y0 + b.y1 - s) / 2.0,
        s,
    };
    let body = |path: Path| Mark {
        path,
        fill: true,
        stroke: true,
        stroke_ink: true,
        width: s / 24.0,
        ..Default::default()
    };
    let mut out = Vec::new();
    match icon {
        "Note" => {
            out.push(body(u.poly(
                &[(0.18, 0.06), (0.18, 0.94), (0.82, 0.94), (0.82, 0.3), (0.58, 0.06)],
                true,
            )));
            u.text_lines(&mut out, &[0.74, 0.6, 0.46], 0.3, 0.7);
            out.push(u.ink(u.poly(&[(0.58, 0.06), (0.58, 0.3), (0.82, 0.3)], false), s / 24.0));
        }
        "Help" => {
            out.push(body(u.circle(0.5, 0.5, 0.44)));
            let q = u.poly(
                &[
                    (0.36, 0.64),
                    (0.4, 0.74),
                    (0.5, 0.78),
                    (0.6, 0.74),
                    (0.63, 0.64),
                    (0.58, 0.56),
                    (0.5, 0.5),
                    (0.5, 0.38),
                ],
                false,
            );
            out.push(u.ink(q, s / 11.0));
            out.push(Mark {
                fill: true,
                ..u.ink(u.circle(0.5, 0.24, 0.05), s / 24.0)
            });
        }
        "Insert" => out.push(body(u.poly(&[(0.1, 0.12), (0.5, 0.9), (0.9, 0.12), (0.5, 0.42)], true))),
        "Key" => {
            out.push(body(u.circle(0.32, 0.66, 0.22)));
            let shaft = u.poly(
                &[(0.46, 0.5), (0.86, 0.1), (0.86, 0.26), (0.76, 0.26), (0.76, 0.2)],
                false,
            );
            out.push(u.ink(shaft, s / 12.0));
            out.push(Mark {
                path: u.circle(0.26, 0.72, 0.07),
                fill: true,
                fill_white: true,
                ..Default::default()
            });
        }
        "NewParagraph" => {
            out.push(body(u.poly(&[(0.5, 0.94), (0.86, 0.5), (0.14, 0.5)], true)));
            let n = u.poly(&[(0.3, 0.08), (0.3, 0.4), (0.5, 0.08), (0.5, 0.4)], false);
            out.push(u.ink(n, s / 14.0));
            let p = u.poly(
                &[(0.62, 0.08), (0.62, 0.4), (0.8, 0.4), (0.8, 0.26), (0.62, 0.26)],
                false,
            );
            out.push(u.ink(p, s / 14.0));
        }
        "PushPin" => {
            out.push(u.ink(u.poly(&[(0.5, 0.5), (0.24, 0.06)], false), s / 10.0));
            out.push(body(u.circle(0.58, 0.68, 0.26)));
        }
        "Paperclip" => {
            let clip = u.poly(
                &[
                    (0.42, 0.3),
                    (0.42, 0.78),
                    (0.52, 0.9),
                    (0.62, 0.78),
                    (0.62, 0.2),
                    (0.47, 0.06),
                    (0.32, 0.2),
                    (0.32, 0.86),
                ],
                false,
            );
            out.push(u.ink(clip, s / 12.0));
        }
        "Graph" => {
            out.push(body(
                u.poly(&[(0.08, 0.08), (0.92, 0.08), (0.92, 0.92), (0.08, 0.92)], true),
            ));
            out.push(u.ink(u.poly(&[(0.28, 0.2), (0.28, 0.5)], false), s / 9.0));
            out.push(u.ink(u.poly(&[(0.5, 0.2), (0.5, 0.75)], false), s / 9.0));
            out.push(u.ink(u.poly(&[(0.72, 0.2), (0.72, 0.6)], false), s / 9.0));
        }
        "Tag" => {
            out.push(body(u.poly(
                &[(0.06, 0.5), (0.36, 0.82), (0.94, 0.82), (0.94, 0.18), (0.36, 0.18)],
                true,
            )));
            out.push(Mark {
                path: u.circle(0.3, 0.5, 0.07),
                fill: true,
                fill_white: true,
                ..Default::default()
            });
        }
        "Flag" => {
            // a pennant on a pole (the Flag markup)
            out.push(u.ink(u.poly(&[(0.2, 0.04), (0.2, 0.96)], false), s / 10.0));
            out.push(body(u.poly(&[(0.2, 0.96), (0.9, 0.78), (0.2, 0.56)], true)));
        }
        "Paragraph" => {
            out.push(body(u.circle(0.42, 0.66, 0.24)));
            let mut stems = u.poly(&[(0.42, 0.9), (0.78, 0.9)], false);
            stems.extend(u.poly(&[(0.56, 0.9), (0.56, 0.08)], false));
            stems.extend(u.poly(&[(0.74, 0.9), (0.74, 0.08)], false));
            out.push(u.ink(stems, s / 12.0));
        }
        _ => {
            // Comment: a speech bubble with three lines of text
            out.push(body(u.poly(
                &[
                    (0.06, 0.3),
                    (0.06, 0.92),
                    (0.94, 0.92),
                    (0.94, 0.3),
                    (0.46, 0.3),
                    (0.22, 0.06),
                    (0.28, 0.3),
                ],
                true,
            )));
            u.text_lines(&mut out, &[0.78, 0.62, 0.46], 0.2, 0.8);
        }
    }
    out
}

/// Bounding box of every mark.
pub fn marks_bbox(marks: &[Mark]) -> Option<Rect> {
    let pts: Vec<Point> = marks.iter().flat_map(|m| crate::path::path_points(&m.path)).collect();
    bbox(&pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_marks_per_style() {
        let q = [
            Point::new(100.0, 512.0),
            Point::new(166.0, 512.0),
            Point::new(100.0, 500.0),
            Point::new(166.0, 500.0),
        ];
        for st in [
            QuadStyle::Highlight,
            QuadStyle::Underline,
            QuadStyle::Strikeout,
            QuadStyle::Squiggly,
        ] {
            let m = quad_marks(st, &q);
            assert_eq!(m.len(), 1);
            let bb = marks_bbox(&m).unwrap();
            assert!(bb.x0 >= 100.0 - 1e-9 && bb.x1 <= 166.0 + 1e-9 && bb.y0 >= 500.0 - 1e-9 && bb.y1 <= 512.0);
        }
        assert!(quad_marks(QuadStyle::Highlight, &q[..3]).is_empty());
        assert!((quad_pad(&q) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn icons_stay_in_their_box() {
        let b = Rect::new(10.0, 10.0, 34.0, 34.0);
        for icon in NOTE_ICONS.iter().chain(&["Unknown"]) {
            let m = note_icon(b, icon);
            assert!(!m.is_empty());
            let bb = marks_bbox(&m).unwrap();
            assert!(bb.x0 >= 10.0 - 1e-9 && bb.x1 <= 34.0 + 1e-9, "{icon}");
        }
        let c = caret_mark(Rect::new(0.0, 0.0, 10.0, 13.0));
        assert!(c.fill && c.path.len() == 5);
        let q = Quad {
            ll: Point::new(0.0, 0.0),
            ul: Point::new(0.0, 10.0),
            ..Default::default()
        };
        assert_eq!(caret_box(&q, false)[0], Point::new(-2.0, -1.5));
    }
}
