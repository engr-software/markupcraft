//! Draws markups from the model onto the canvas (so edits show at once, without a re-render),
//! plus selection outlines and handles. Geometry is PDF user space; [`Xf`] maps it to the
//! screen through the page's crop box and `/Rotate`.

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup, caption};
use markupcraft_render::PageGeom;

use crate::actions::{markup_bbox, uses_rect};
use crate::theme::{Tokens, color32};

/// One page's mapping between PDF user space and the screen.
#[derive(Clone, Debug)]
pub struct Xf {
    /// The page on screen.
    pub rect: Rect,
    pub geom: PageGeom,
    /// Screen points per PDF point.
    pub k: f32,
}

impl Xf {
    pub fn to_screen(&self, p: Point) -> Pos2 {
        let v = self.geom.user_to_view(p.x as f32, p.y as f32);
        self.rect.min + vec2(v[0] * self.k, v[1] * self.k)
    }

    pub fn to_user(&self, s: Pos2) -> Point {
        let k = self.k.max(1e-6);
        let u = self
            .geom
            .view_to_user((s.x - self.rect.min.x) / k, (s.y - self.rect.min.y) / k);
        Point::new(f64::from(u[0]), f64::from(u[1]))
    }

    /// A length in PDF units on screen.
    pub fn len(&self, pt: f64) -> f32 {
        pt as f32 * self.k
    }
}

/// The handle positions of a markup (user space): its vertices, or its box corners.
pub fn handles(m: &Markup) -> Vec<Point> {
    if uses_rect(m.kind) {
        return m.rect.normalized().corners().to_vec();
    }
    match m.kind {
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly | Kind::Ink | Kind::Highlight => {
            Vec::new()
        }
        _ => m.pts.iter().take(500).copied().collect(),
    }
}

fn stroke_of(xf: &Xf, m: &Markup) -> Stroke {
    let w = if m.line_width <= 0.0 {
        0.0
    } else {
        xf.len(m.line_width).max(1.0)
    };
    Stroke::new(w, color32(&m.color, m.opacity))
}

fn fill_of(m: &Markup) -> Option<Color32> {
    m.fill.map(|f| color32(&f, m.fill_opacity * m.opacity))
}

/// Draw one markup.
pub fn paint_markup(p: &Painter, xf: &Xf, m: &Markup) {
    let stroke = stroke_of(xf, m);
    let fill = fill_of(m);
    let screen = |pts: &[Point]| -> Vec<Pos2> { pts.iter().map(|q| xf.to_screen(*q)).collect() };
    match m.kind {
        Kind::Area | Kind::Perimeter | Kind::Polygon | Kind::Volume | Kind::Cloud => {
            let pts = screen(&m.pts);
            if let Some(f) = fill.filter(|_| m.kind != Kind::Perimeter) {
                fill_polygon(p, &pts, f);
            }
            if m.kind == Kind::Cloud || m.cloud > 0.0 {
                let bumps = cloud_path(&pts, xf.len(6.0 + 4.0 * m.cloud.clamp(0.0, 4.0)));
                stroke_path(p, &bumps, true, stroke, &m.dash, xf);
            } else {
                stroke_path(p, &pts, true, stroke, &m.dash, xf);
            }
            for h in &m.holes {
                stroke_path(p, &screen(h), true, stroke, &m.dash, xf);
            }
        }
        Kind::Polyline | Kind::Polylength | Kind::Diameter | Kind::Radius | Kind::Angle => {
            let pts = screen(&m.pts);
            stroke_path(p, &pts, false, stroke, &m.dash, xf);
            line_endings(p, &pts, m, stroke);
        }
        Kind::Line | Kind::Length | Kind::Arrow => {
            let pts = screen(&m.pts);
            stroke_path(p, &pts, false, stroke, &m.dash, xf);
            line_endings(p, &pts, m, stroke);
            if m.kind == Kind::Length {
                for e in &pts {
                    tick(p, &pts, *e, stroke);
                }
            }
        }
        Kind::Ink | Kind::Highlight => {
            let st = if m.kind == Kind::Highlight {
                Stroke::new(stroke.width.max(xf.len(8.0)), color32(&m.color, m.opacity.min(0.45)))
            } else {
                stroke
            };
            let mut starts = vec![0];
            starts.extend(m.strokes.iter().copied().filter(|s| *s < m.pts.len()));
            starts.push(m.pts.len());
            for w in starts.windows(2) {
                if let (Some(a), Some(b)) = (w.first(), w.get(1))
                    && let Some(seg) = m.pts.get(*a..*b)
                {
                    stroke_path(p, &screen(seg), false, st, &[], xf);
                }
            }
        }
        Kind::Rectangle | Kind::Stamp | Kind::Snapshot | Kind::Attachment | Kind::Hyperlink | Kind::Caret => {
            let pts = screen(&m.rect.normalized().corners());
            if let Some(f) = fill {
                fill_polygon(p, &pts, f);
            }
            stroke_path(p, &pts, true, stroke, &m.dash, xf);
            if m.kind == Kind::Stamp && !m.subject.is_empty() {
                let r = Rect::from_points(&pts);
                p.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &m.subject,
                    FontId::proportional((r.height() * 0.4).clamp(6.0, 48.0)),
                    stroke.color,
                );
            }
        }
        Kind::Ellipse => {
            let r = Rect::from_points(&screen(&m.rect.normalized().corners()));
            let n = 72;
            let pts: Vec<Pos2> = (0..n)
                .map(|i| {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    r.center() + vec2(a.cos() * r.width() / 2.0, a.sin() * r.height() / 2.0)
                })
                .collect();
            if let Some(f) = fill {
                p.add(Shape::convex_polygon(pts.clone(), f, Stroke::NONE));
            }
            stroke_path(p, &pts, true, stroke, &m.dash, xf);
        }
        Kind::Text | Kind::Callout | Kind::Typewriter => paint_text_box(p, xf, m, stroke, fill),
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly => {
            for q in m.pts.as_chunks::<4>().0 {
                let s = screen(q);
                // QuadPoints order: top-left, top-right, bottom-left, bottom-right.
                let [tl, tr, bl, br] = [s[0], s[1], s[2], s[3]];
                match m.kind {
                    Kind::TextHighlight => {
                        p.add(Shape::convex_polygon(
                            vec![tl, tr, br, bl],
                            color32(&m.color, 0.4 * m.opacity),
                            Stroke::NONE,
                        ));
                    }
                    Kind::Underline => {
                        p.line_segment([bl, br], Stroke::new((stroke.width).max(1.0), stroke.color));
                    }
                    Kind::Strikeout => {
                        p.line_segment(
                            [tl.lerp(bl, 0.55), tr.lerp(br, 0.55)],
                            Stroke::new(stroke.width.max(1.0), stroke.color),
                        );
                    }
                    _ => {
                        let len = bl.distance(br);
                        let n = (len / 4.0).ceil().clamp(1.0, 2000.0) as usize;
                        let pts: Vec<Pos2> = (0..=n)
                            .map(|i| {
                                let t = i as f32 / n as f32;
                                let up = if i % 2 == 0 { 0.0 } else { -2.0 };
                                bl.lerp(br, t) + vec2(0.0, up)
                            })
                            .collect();
                        p.add(Shape::line(pts, Stroke::new(1.0, stroke.color)));
                    }
                }
            }
        }
        Kind::Count => {
            let r = xf.len(5.0 * m.symbol_scale.clamp(0.1, 20.0)).max(3.0);
            let pts = if m.pts.is_empty() {
                vec![markupcraft_geom::vertex_mean(&m.rect.corners())]
            } else {
                m.pts.clone()
            };
            let col = color32(&m.color, m.opacity);
            for q in pts.iter().take(10_000) {
                let c = xf.to_screen(*q);
                match m.count_symbol {
                    markupcraft_model::CountSymbol::Square => {
                        p.rect_filled(Rect::from_center_size(c, Vec2::splat(2.0 * r)), 0.0, col);
                    }
                    markupcraft_model::CountSymbol::Cross => {
                        let st = Stroke::new(2.0, col);
                        p.line_segment([c - vec2(r, r), c + vec2(r, r)], st);
                        p.line_segment([c - vec2(r, -r), c + vec2(r, -r)], st);
                    }
                    markupcraft_model::CountSymbol::Check => {
                        p.add(Shape::line(
                            vec![c + vec2(-r, 0.0), c + vec2(-r * 0.3, r * 0.7), c + vec2(r, -r)],
                            Stroke::new(2.0, col),
                        ));
                    }
                    _ => {
                        p.circle_filled(c, r, col);
                    }
                }
            }
        }
        Kind::Note => {
            let r = m.rect.normalized();
            let at = xf.to_screen(Point::new(r.x0, r.y1));
            let s = xf.len(20.0).clamp(10.0, 40.0);
            let box_ = Rect::from_min_size(at, Vec2::splat(s));
            p.rect_filled(box_, 2.0, color32(&m.color, m.opacity));
            p.rect_stroke(
                box_,
                2.0,
                Stroke::new(1.0, Color32::from_black_alpha(140)),
                egui::StrokeKind::Inside,
            );
            for i in 1..4 {
                let y = box_.top() + box_.height() * i as f32 / 4.0;
                p.line_segment(
                    [pos2(box_.left() + s * 0.2, y), pos2(box_.right() - s * 0.2, y)],
                    Stroke::new(1.0, Color32::from_black_alpha(120)),
                );
            }
        }
        Kind::Other => {}
    }
    if m.kind.is_measurement() {
        paint_caption(p, xf, m);
    }
}

fn paint_text_box(p: &Painter, xf: &Xf, m: &Markup, stroke: Stroke, fill: Option<Color32>) {
    let corners = m.rect.normalized().corners();
    let pts: Vec<Pos2> = corners.iter().map(|q| xf.to_screen(*q)).collect();
    let r = Rect::from_points(&pts);
    if let Some(f) = fill {
        p.rect_filled(r, 0.0, f);
    }
    if m.kind != Kind::Typewriter && stroke.width > 0.0 {
        p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
    }
    if m.kind == Kind::Callout && m.pts.len() >= 2 {
        let lead: Vec<Pos2> = m.pts.iter().map(|q| xf.to_screen(*q)).collect();
        p.add(Shape::line(lead.clone(), stroke));
        line_endings(p, &lead, m, stroke);
    }
    let size = xf.len(m.text.size.clamp(1.0, 400.0));
    if size < 3.0 || m.contents.is_empty() {
        return;
    }
    let galley = p.layout(
        m.contents.clone(),
        FontId::proportional(size),
        color32(&m.text.color, m.opacity),
        (r.width() - 4.0).max(10.0),
    );
    let clip = p.clip_rect().intersect(r.expand(1.0));
    p.with_clip_rect(clip)
        .galley(r.min + vec2(2.0, 2.0), galley, Color32::BLACK);
}

/// The measured value at the caption anchor.
fn paint_caption(p: &Painter, xf: &Xf, m: &Markup) {
    let text = match m.quantity_text() {
        t if !t.is_empty() => t,
        _ => m.contents.lines().next().unwrap_or_default().to_string(),
    };
    if text.is_empty() {
        return;
    }
    let size = xf.len(m.text.size.clamp(4.0, 200.0));
    if size < 4.0 {
        return;
    }
    let at = xf.to_screen(caption::caption_anchor(m));
    let col = color32(&m.color, 1.0);
    let galley = p.layout_no_wrap(text, FontId::proportional(size), col);
    let r = Align2::CENTER_CENTER.anchor_size(at, galley.size());
    p.rect_filled(r.expand(1.0), 1.0, Color32::from_white_alpha(170));
    p.galley(r.min, galley, col);
}

/// Stroke a path, dashed when `dash` is set (PDF units).
fn stroke_path(p: &Painter, pts: &[Pos2], closed: bool, stroke: Stroke, dash: &[f64], xf: &Xf) {
    if pts.len() < 2 || stroke.width <= 0.0 {
        return;
    }
    if let Some(d) = dash.first().copied().filter(|d| *d > 0.0) {
        let on = xf.len(d).max(1.0);
        let off = xf.len(dash.get(1).copied().unwrap_or(d)).max(1.0);
        let mut path = pts.to_vec();
        if closed && let Some(f) = pts.first() {
            path.push(*f);
        }
        p.extend(Shape::dashed_line(&path, stroke, on, off));
    } else if closed {
        p.add(PathShape::closed_line(pts.to_vec(), stroke));
    } else {
        p.add(PathShape::line(pts.to_vec(), PathStroke::from(stroke)));
    }
}

/// Fill any simple polygon (concave too) by ear clipping; convex fan for very large ones.
pub fn fill_polygon(p: &Painter, pts: &[Pos2], color: Color32) {
    if pts.len() < 3 {
        return;
    }
    let tris = triangulate(pts);
    let mut mesh = Mesh::default();
    if tris.is_empty() {
        p.add(Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
        return;
    }
    for q in pts {
        mesh.colored_vertex(*q, color);
    }
    for [a, b, c] in tris {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    p.add(Shape::mesh(mesh));
}

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn signed_area(pts: &[Pos2]) -> f32 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        / 2.0
}

/// Ear-clipping triangulation of a simple polygon; empty when it gives up (self-intersecting,
/// degenerate, or more than 2000 vertices).
pub fn triangulate(pts: &[Pos2]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if !(3..=2000).contains(&n) {
        return Vec::new();
    }
    let ccw = signed_area(pts) > 0.0;
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    let inside = |p: Pos2, a: Pos2, b: Pos2, c: Pos2| {
        let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            let turn = cross(a, b, c);
            if (ccw && turn <= 0.0) || (!ccw && turn >= 0.0) {
                continue;
            }
            let blocked = idx
                .iter()
                .any(|&j| j != ia && j != ib && j != ic && inside(pts[j], a, b, c));
            if blocked {
                continue;
            }
            out.push([ia, ib, ic]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            return Vec::new();
        }
    }
    if let [a, b, c] = idx[..] {
        out.push([a, b, c]);
    }
    out
}

/// A revision-cloud outline: arcs of about `chord` along each edge, bulging outward.
fn cloud_path(pts: &[Pos2], chord: f32) -> Vec<Pos2> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let chord = chord.max(3.0);
    // Screen y points down, so a positive signed area is clockwise on screen.
    let outward = if signed_area(pts) > 0.0 { -1.0 } else { 1.0 };
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = a.distance(b);
        let arcs = (len / chord).ceil().clamp(1.0, 400.0) as usize;
        for j in 0..arcs {
            let (q0, q1) = (
                a.lerp(b, j as f32 / arcs as f32),
                a.lerp(b, (j + 1) as f32 / arcs as f32),
            );
            let mid = q0.lerp(q1, 0.5);
            let half = (q1 - q0) / 2.0;
            let normal = vec2(-half.y, half.x) * outward;
            for s in 0..=8 {
                let t = s as f32 / 8.0 * std::f32::consts::PI;
                out.push(mid - half * t.cos() + normal * t.sin());
            }
        }
    }
    out
}

fn tick(p: &Painter, pts: &[Pos2], at: Pos2, stroke: Stroke) {
    let (Some(a), Some(b)) = (pts.first(), pts.last()) else {
        return;
    };
    let d = (*b - *a).normalized();
    let n = vec2(-d.y, d.x) * 5.0;
    p.line_segment([at - n, at + n], stroke);
}

/// PDF line endings (`/LE`) at both ends of an open path.
fn line_endings(p: &Painter, pts: &[Pos2], m: &Markup, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    let n = pts.len();
    let ends = [
        (pts[0], pts[1], m.line_start.as_str()),
        (pts[n - 1], pts[n - 2], m.line_end.as_str()),
    ];
    let size = (stroke.width * 3.0 + 6.0).min(60.0);
    for (tip, from, name) in ends {
        let d = (tip - from).normalized();
        if !d.x.is_finite() {
            continue;
        }
        let nrm = vec2(-d.y, d.x);
        match name {
            "OpenArrow" | "ROpenArrow" | "ClosedArrow" | "RClosedArrow" => {
                let dir = if name.starts_with('R') { -d } else { d };
                let base = tip - dir * size;
                let (l, r) = (base + nrm * size * 0.5, base - nrm * size * 0.5);
                if name.contains("Closed") {
                    p.add(Shape::convex_polygon(vec![tip, l, r], stroke.color, stroke));
                } else {
                    p.add(Shape::line(vec![l, tip, r], stroke));
                }
            }
            "Circle" => {
                p.circle_stroke(tip, size * 0.4, stroke);
            }
            "Square" => {
                p.rect_stroke(
                    Rect::from_center_size(tip, Vec2::splat(size * 0.7)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            "Diamond" => {
                let s = size * 0.45;
                p.add(PathShape::closed_line(
                    vec![tip + d * s, tip + nrm * s, tip - d * s, tip - nrm * s],
                    stroke,
                ));
            }
            "Butt" => {
                p.line_segment([tip + nrm * size * 0.5, tip - nrm * size * 0.5], stroke);
            }
            "Slash" => {
                let s = (d + nrm).normalized() * size * 0.5;
                p.line_segment([tip + s, tip - s], stroke);
            }
            _ => {}
        }
    }
}

/// Selection outline and handles for a selected markup.
pub fn paint_selection(p: &Painter, xf: &Xf, m: &Markup, t: &Tokens, editable: bool) {
    let b = markup_bbox(m);
    let r = Rect::from_points(&[
        xf.to_screen(Point::new(b.x0, b.y0)),
        xf.to_screen(Point::new(b.x1, b.y1)),
    ])
    .expand(3.0);
    let col = if editable { t.select } else { t.text_faint };
    p.extend(Shape::dashed_line(
        &[
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ],
        Stroke::new(1.0, col),
        4.0,
        3.0,
    ));
    if !editable || m.locked() {
        return;
    }
    for h in handles(m) {
        let c = xf.to_screen(h);
        let hr = Rect::from_center_size(c, Vec2::splat(7.0));
        p.rect_filled(hr, 1.0, Color32::WHITE);
        p.rect_stroke(hr, 1.0, Stroke::new(1.2, col), egui::StrokeKind::Middle);
    }
}

/// Hit test in user space; `tol` in PDF units.
pub fn hit(m: &Markup, at: Point, tol: f64) -> bool {
    use markupcraft_geom::{dist_to_segment, point_in_polygon};
    let near_path = |pts: &[Point], closed: bool| {
        let w = m.line_width / 2.0 + tol;
        pts.windows(2).any(|s| dist_to_segment(at, s[0], s[1]) <= w)
            || (closed
                && pts.len() > 2
                && pts
                    .first()
                    .zip(pts.last())
                    .is_some_and(|(f, l)| dist_to_segment(at, *l, *f) <= w))
    };
    match m.kind {
        Kind::Area | Kind::Polygon | Kind::Volume | Kind::Cloud => {
            point_in_polygon(at, &m.pts) || near_path(&m.pts, true)
        }
        Kind::Perimeter => near_path(&m.pts, true) || (m.fill.is_some() && point_in_polygon(at, &m.pts)),
        Kind::Polyline
        | Kind::Polylength
        | Kind::Line
        | Kind::Length
        | Kind::Arrow
        | Kind::Ink
        | Kind::Highlight
        | Kind::Diameter
        | Kind::Radius
        | Kind::Angle => near_path(&m.pts, false),
        Kind::Count => m.pts.iter().any(|q| q.dist(at) <= 6.0 * m.symbol_scale.max(0.1) + tol),
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly => m
            .pts
            .as_chunks::<4>()
            .0
            .iter()
            .any(|q| point_in_polygon(at, &[q[0], q[1], q[3], q[2]])),
        _ => m.rect.normalized().padded(tol).contains(at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_an_l_shape() {
        let l = [
            pos2(0.0, 0.0),
            pos2(4.0, 0.0),
            pos2(4.0, 1.0),
            pos2(1.0, 1.0),
            pos2(1.0, 4.0),
            pos2(0.0, 4.0),
        ];
        let t = triangulate(&l);
        assert_eq!(t.len(), 4);
        let area: f32 = t.iter().map(|[a, b, c]| cross(l[*a], l[*b], l[*c]).abs() / 2.0).sum();
        assert!((area - 7.0).abs() < 1e-4);
        assert!(triangulate(&l[..2]).is_empty());
    }

    #[test]
    fn transform_round_trips_through_rotation() {
        let geom = PageGeom {
            width: 792.0,
            height: 612.0,
            crop: [0.0, 0.0, 612.0, 792.0],
            rotation: 90,
            label: "1".into(),
        };
        let xf = Xf {
            rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(792.0 * 2.0, 612.0 * 2.0)),
            geom,
            k: 2.0,
        };
        let p = Point::new(100.0, 200.0);
        let back = xf.to_user(xf.to_screen(p));
        assert!((back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3, "{back:?}");
    }

    #[test]
    fn hits_lines_and_boxes() {
        let mut l = Markup::new(Kind::Line, 0, vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]);
        l.line_width = 2.0;
        assert!(hit(&l, Point::new(50.0, 2.0), 2.0));
        assert!(!hit(&l, Point::new(50.0, 10.0), 2.0));
        let mut r = Markup::new(Kind::Rectangle, 0, Vec::new());
        r.rect = markupcraft_geom::Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(hit(&r, Point::new(5.0, 5.0), 1.0));
    }
}
