//! Draws markups from the model onto the canvas (so edits show at once, without a re-render),
//! plus selection outlines and handles. Geometry is PDF user space; [`Xf`] maps it to the
//! screen through the page's crop box and `/Rotate`. Text is laid out with the same wrapping
//! the PDF writer uses (`markupcraft_geom::text`), so what is typed lands where it saves.

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use markupcraft_geom::path::Seg;
use markupcraft_geom::text::stamp_lines;
use markupcraft_geom::{Point, shapes};
use markupcraft_model::{Kind, Markup, caption, measure_extras};
use markupcraft_render::PageGeom;
use markupcraft_revu::kinds::common::font_of;

use crate::actions::{box_of, markup_bbox, uses_rect};
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

    /// A user-space rectangle on screen.
    pub fn rect_of(&self, r: markupcraft_geom::Rect) -> Rect {
        Rect::from_two_pos(
            self.to_screen(Point::new(r.x0, r.y0)),
            self.to_screen(Point::new(r.x1, r.y1)),
        )
    }
}

/// The handle positions of a markup (user space): its vertices, or its box corners (a
/// Callout adds its leader tip and knee as handles 4 and 5).
pub fn handles(m: &Markup) -> Vec<Point> {
    if uses_rect(m.kind) {
        let mut h = box_of(m).corners().to_vec();
        if m.kind == Kind::Callout {
            h.extend(m.pts.iter().skip(4).take(2));
        }
        return h;
    }
    match m.kind {
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly | Kind::Ink | Kind::Highlight => {
            Vec::new()
        }
        _ if m.pts.len() <= 500 => {
            // the cutouts' vertices follow the outline's (engine vertex numbering)
            let mut h = m.pts.clone();
            h.extend(m.holes.iter().flatten().take(500));
            h
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

/// A PDF path flattened onto the screen (one list of points per subpath).
fn flatten(xf: &Xf, path: &[Seg]) -> Vec<Vec<Pos2>> {
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut cur: Vec<Pos2> = Vec::new();
    let mut last = Point::default();
    for s in path {
        match *s {
            Seg::Move(p) => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                cur.push(xf.to_screen(p));
                last = p;
            }
            Seg::Line(p) => {
                cur.push(xf.to_screen(p));
                last = p;
            }
            Seg::Curve(a, b, c) => {
                let n = 8;
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    let q = Point::new(
                        u * u * u * last.x + 3.0 * u * u * t * a.x + 3.0 * u * t * t * b.x + t * t * t * c.x,
                        u * u * u * last.y + 3.0 * u * u * t * a.y + 3.0 * u * t * t * b.y + t * t * t * c.y,
                    );
                    cur.push(xf.to_screen(q));
                }
                last = c;
            }
            Seg::Close => {
                if let Some(f) = cur.first().copied() {
                    cur.push(f);
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
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
                if m.holes.is_empty() {
                    fill_polygon(p, &pts, f);
                } else {
                    let mut rings = vec![pts.clone()];
                    rings.extend(m.holes.iter().map(|h| screen(h)));
                    fill_even_odd(p, &rings, f);
                }
            }
            if m.kind == Kind::Cloud || m.cloud > 0.0 {
                for sub in flatten(xf, &shapes::cloud_path(&m.pts, m.cloud.clamp(0.0, 4.0))) {
                    stroke_path(p, &sub, false, stroke, &m.dash, xf);
                }
            } else {
                stroke_path(p, &pts, true, stroke, &m.dash, xf);
            }
            for h in &m.holes {
                stroke_path(p, &screen(h), true, stroke, &m.dash, xf);
            }
        }
        Kind::Polyline | Kind::Polylength | Kind::Diameter | Kind::Radius | Kind::Angle => {
            let pts = screen(&m.pts);
            if let Some((c, r)) = measure_extras::circle_of(m) {
                let ring = screen(&measure_extras::ellipse_ring(c, r, r, 72));
                if let Some(f) = fill {
                    fill_polygon(p, &ring, f);
                }
                stroke_path(p, &ring, true, stroke, &m.dash, xf);
            }
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
                Stroke::new(stroke.width.max(1.0), color32(&m.color, m.opacity.min(0.45)))
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
        Kind::Attachment => crate::shapes_more::paint_attachment(p, xf, m),
        Kind::Rectangle | Kind::Hyperlink | Kind::Caret => {
            let pts = screen(&box_of(m).corners());
            if let Some(f) = fill {
                fill_polygon(p, &pts, f);
            }
            stroke_path(p, &pts, true, stroke, &m.dash, xf);
        }
        Kind::Stamp => paint_stamp(p, xf, m, stroke, fill),
        Kind::Snapshot => {
            let r = xf.rect_of(box_of(m));
            p.extend(Shape::dashed_line(
                &[
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ],
                Stroke::new(1.0, Color32::from_gray(90)),
                5.0,
                3.0,
            ));
        }
        Kind::Ellipse => {
            let r = xf.rect_of(box_of(m));
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
                let w = (tl.distance(bl) * 0.08).clamp(1.0, 4.0);
                match m.kind {
                    Kind::TextHighlight => {
                        p.add(Shape::convex_polygon(
                            vec![tl, tr, br, bl],
                            color32(&m.color, 0.4 * m.opacity),
                            Stroke::NONE,
                        ));
                    }
                    Kind::Underline => {
                        p.line_segment([bl, br], Stroke::new(w, stroke.color));
                    }
                    Kind::Strikeout => {
                        p.line_segment([tl.lerp(bl, 0.55), tr.lerp(br, 0.55)], Stroke::new(w, stroke.color));
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
                    markupcraft_model::CountSymbol::Custom if !m.symbol_paths.is_empty() => {
                        for path in measure_extras::symbol_paths_at(m, *q).iter().take(200) {
                            let s: Vec<Pos2> = path.iter().map(|p| xf.to_screen(*p)).collect();
                            p.add(Shape::line(s, Stroke::new(1.5, col)));
                        }
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
        Kind::Note if m.icon == crate::shapes_more::FLAG_ICON => crate::shapes_more::paint_flag(p, xf, m),
        Kind::Note => {
            let b = box_of(m);
            let at = xf.to_screen(Point::new(b.x0, b.y1));
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
        Kind::Dimension => crate::shapes_more::paint_dimension(p, xf, m, stroke),
        Kind::Arc => {
            let pts = screen(&markupcraft_revu::kinds::more::arc_polyline(m, 48));
            stroke_path(p, &pts, false, stroke, &m.dash, xf);
            line_endings(p, &pts, m, stroke);
        }
        Kind::Other => {}
    }
    if m.show_centroid
        && let Some(c) = measure_extras::centroid(m)
    {
        crate::shapes_more::paint_centroid(p, xf.to_screen(c), stroke.color);
    }
    if let Some(h) = m.hatch.filter(|h| h.valid()) {
        paint_hatch(p, xf, m, &h);
    }
    if m.kind.is_measurement() {
        if m.segment_values {
            paint_segment_values(p, xf, m);
        }
        paint_caption(p, xf, m);
    }
}

/// The outline a hatch fills (user space).
fn hatch_ring(m: &Markup) -> Vec<Point> {
    match m.kind {
        Kind::Rectangle => box_of(m).corners().to_vec(),
        Kind::Ellipse => {
            let b = box_of(m);
            measure_extras::ellipse_ring(
                Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
                b.width() / 2.0,
                b.height() / 2.0,
                72,
            )
        }
        _ => m.pts.clone(),
    }
}

/// The parts of segment `a`-`b` inside the rings (even-odd), as parameter spans.
pub fn clip_segment(a: Point, b: Point, rings: &[Vec<Point>]) -> Vec<(f64, f64)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut ts: Vec<f64> = Vec::new();
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (Some(p), Some(q)) = (ring.get(i), ring.get((i + 1) % n)) else {
                continue;
            };
            let (ex, ey) = (q.x - p.x, q.y - p.y);
            let den = dx * ey - dy * ex;
            if den.abs() < 1e-12 {
                continue;
            }
            let t = ((p.x - a.x) * ey - (p.y - a.y) * ex) / den;
            let u = ((p.x - a.x) * dy - (p.y - a.y) * dx) / den;
            if (0.0..=1.0).contains(&t) && (0.0..1.0).contains(&u) {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.chunks(2)
        .filter_map(|c| match c {
            [s, e] if e - s > 1e-9 => Some((*s, *e)),
            _ => None,
        })
        .collect()
}

fn paint_hatch(p: &Painter, xf: &Xf, m: &Markup, h: &markupcraft_model::hatch::Hatch) {
    if !markupcraft_revu::hatch::hatchable(m.kind) {
        return;
    }
    let mut rings = vec![hatch_ring(m)];
    rings.extend(m.holes.iter().cloned());
    let Some(b) = markupcraft_geom::bbox(rings.first().map_or(&[][..], |r| r.as_slice())) else {
        return;
    };
    let col = color32(&h.color.unwrap_or(m.color), m.opacity);
    let st = Stroke::new(xf.len(h.width).max(0.6), col);
    for (a, e) in h.lines(b).into_iter().take(4000) {
        for (t0, t1) in clip_segment(a, e, &rings) {
            let at = |t: f64| Point::new(a.x + (e.x - a.x) * t, a.y + (e.y - a.y) * t);
            p.line_segment([xf.to_screen(at(t0)), xf.to_screen(at(t1))], st);
        }
    }
}

fn family(m: &Markup) -> FontFamily {
    if m.text.font.to_ascii_lowercase().contains("courier") {
        FontFamily::Monospace
    } else {
        FontFamily::Proportional
    }
}

/// One line of text with its baseline at `base` (screen).
#[allow(clippy::too_many_arguments)]
fn text_line(
    p: &Painter,
    base: Pos2,
    text: &str,
    size: f32,
    color: Color32,
    fam: FontFamily,
    italic: bool,
    underline: bool,
) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::new(size, fam),
            color,
            italics: italic,
            underline: if underline {
                Stroke::new((size / 14.0).max(1.0), color)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        },
    );
    let galley = p.layout_job(job);
    // egui places text by its top; the baseline sits about 0.78 of the size below it.
    let top = base - vec2(0.0, size * 0.78 + size * 0.1);
    p.galley(top, galley, color);
}

fn paint_text_box(p: &Painter, xf: &Xf, m: &Markup, stroke: Stroke, fill: Option<Color32>) {
    let b = box_of(m);
    let r = xf.rect_of(b);
    if m.kind == Kind::Callout
        && m.pts.len() >= 6
        && let (Some(tip), Some(knee)) = (m.pts.get(4).copied(), m.pts.get(5).copied())
    {
        let at = markupcraft_revu::kinds::text::callout_attach(m);
        let lead = vec![xf.to_screen(at), xf.to_screen(knee), xf.to_screen(tip)];
        let st = Stroke::new(stroke.width.max(1.0), stroke.color);
        p.add(Shape::line(lead.clone(), st));
        let reversed: Vec<Pos2> = lead.iter().rev().copied().collect();
        let mut e = m.clone();
        e.line_start = m.line_end.clone();
        e.line_end = "None".into();
        line_endings(p, &reversed, &e, st);
    }
    if let Some(f) = fill {
        p.rect_filled(r, 0.0, f);
    }
    if m.kind != Kind::Typewriter && stroke.width > 0.0 {
        p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
    }
    paint_text_lines(p, xf, m, b);
}

/// The markup's text laid out in box `b` like the writer does.
pub fn paint_text_lines(p: &Painter, xf: &Xf, m: &Markup, b: markupcraft_geom::Rect) {
    let size = xf.len(m.text.size.clamp(1.0, 400.0));
    if size < 3.0 || m.contents.is_empty() {
        return;
    }
    let col = color32(&m.text.color, m.opacity);
    let clip = p.clip_rect().intersect(xf.rect_of(b).expand(2.0));
    let cp = p.with_clip_rect(clip);
    let lines = markupcraft_revu::kinds::text::markup_lines(m, b);
    if !m.rich.is_empty() {
        let starts = markupcraft_revu::kinds::text::line_starts(&m.contents, &lines);
        for (l, s0) in lines.iter().zip(&starts).take(400) {
            rich_line(&cp, xf, m, l, *s0, size);
        }
        return;
    }
    for l in lines.iter().take(400) {
        text_line(
            &cp,
            xf.to_screen(Point::new(l.x, l.y)),
            &l.text,
            size,
            col,
            family(m),
            m.text.italic,
            m.text.underline,
        );
    }
}

/// One laid-out line of a markup with rich runs: each segment in its own style, placed at the
/// writer's x positions (bold is drawn twice, a hair apart, since the UI font has no bold face).
fn rich_line(p: &Painter, xf: &Xf, m: &Markup, l: &markupcraft_geom::text::TextLine, s0: usize, size: f32) {
    let chars: Vec<char> = l.text.chars().collect();
    let n = chars.len();
    let mut x = l.x;
    for (s, e, cs) in markupcraft_model::rich::segments(&m.text, &m.rich, s0, s0 + n) {
        let t: String = chars.get(s - s0..e - s0).unwrap_or_default().iter().collect();
        let f = font_of(&markupcraft_model::TextStyle {
            bold: cs.bold,
            italic: cs.italic,
            ..m.text.clone()
        });
        let col = color32(&cs.color, m.opacity);
        let at = xf.to_screen(Point::new(x, l.y));
        text_line(p, at, &t, size, col, family(m), cs.italic, cs.underline);
        if cs.bold {
            text_line(
                p,
                at + vec2((size / 24.0).max(0.5), 0.0),
                &t,
                size,
                col,
                family(m),
                cs.italic,
                false,
            );
        }
        x += markupcraft_geom::text::text_width(&t, &f);
    }
}

fn paint_stamp(p: &Painter, xf: &Xf, m: &Markup, stroke: Stroke, fill: Option<Color32>) {
    let b = box_of(m);
    let r = xf.rect_of(b);
    let rad = (r.width().min(r.height()) * 0.12).min(255.0) as u8;
    if let Some(f) = fill {
        p.rect_filled(r, rad, f);
    }
    if stroke.width > 0.0 {
        p.rect_stroke(r, rad, stroke, egui::StrokeKind::Inside);
        let inset = xf.len(m.line_width + 2.5);
        p.rect_stroke(
            r.shrink(inset),
            rad.saturating_sub(inset as u8),
            Stroke::new((stroke.width / 3.0).max(0.5), stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    let text = if m.contents.is_empty() {
        m.subject.clone()
    } else {
        m.contents.clone()
    };
    let font = font_of(&m.text);
    let col = color32(&m.text.color, m.opacity);
    for (l, size) in stamp_lines(b, &text, &font, m.line_width).iter().take(20) {
        let s = xf.len(*size);
        if s >= 3.0 {
            text_line(
                p,
                xf.to_screen(Point::new(l.x, l.y)),
                &l.text,
                s,
                col,
                family(m),
                m.text.italic,
                false,
            );
        }
    }
}

/// The measured value at the caption anchor.
fn paint_caption(p: &Painter, xf: &Xf, m: &Markup) {
    if m.hide_caption {
        return;
    }
    let text = match caption::caption_text(m) {
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
    let r = caption_rect(p, xf, m, &text, size);
    let col = color32(&m.color, 1.0);
    if m.caption_leader && m.caption_offset.is_some() {
        let from = xf.to_screen(caption::default_caption_anchor(m));
        let to = r.center();
        // from the caption's nearest edge
        let edge = pos2(from.x.clamp(r.left(), r.right()), from.y.clamp(r.top(), r.bottom()));
        if edge.distance(from) > 2.0 {
            p.line_segment([if r.contains(from) { to } else { edge }, from], Stroke::new(1.0, col));
            p.circle_filled(from, 2.0, col);
        }
    }
    p.rect_filled(r.expand(1.0), 1.0, Color32::from_white_alpha(170));
    let galley = p.layout_job(caption_job(m, text, size, col));
    p.galley(r.min, galley, col);
}

/// A caption laid out in its style: italic, underline, strike-through, super / subscript
/// (smaller and raised or lowered); bold is not a separate face in the UI font.
fn caption_job(m: &Markup, text: String, size: f32, col: Color32) -> LayoutJob {
    let t = &m.text;
    let deco = |on: bool| {
        if on {
            Stroke::new((size / 14.0).max(1.0), col)
        } else {
            Stroke::NONE
        }
    };
    let (sz, valign) = match t.script {
        1 => (size * 0.7, egui::Align::TOP),
        -1 => (size * 0.7, egui::Align::BOTTOM),
        _ => (size, egui::Align::BOTTOM),
    };
    let mut job = LayoutJob::default();
    job.append(
        &text,
        0.0,
        TextFormat {
            font_id: FontId::proportional(sz),
            color: col,
            italics: t.italic,
            underline: deco(t.underline),
            strikethrough: deco(t.strike),
            valign,
            ..Default::default()
        },
    );
    job
}

/// Where the caption of `m` is on screen (for drawing and for Shift-dragging it).
pub fn caption_rect(p: &Painter, xf: &Xf, m: &Markup, text: &str, size: f32) -> Rect {
    let at = xf.to_screen(caption::caption_anchor(m));
    let galley = p.layout_job(caption_job(m, text.to_string(), size, Color32::BLACK));
    Align2::CENTER_CENTER.anchor_size(at, galley.size())
}

/// The screen rectangle of a measurement's caption, `None` when it has none.
pub fn caption_hit_rect(p: &Painter, xf: &Xf, m: &Markup) -> Option<Rect> {
    if !m.kind.is_measurement() || m.kind == Kind::Count || m.hide_caption {
        return None;
    }
    let text = caption::caption_text(m);
    if text.is_empty() {
        return None;
    }
    let size = xf.len(m.text.size.clamp(4.0, 200.0));
    (size >= 4.0).then(|| caption_rect(p, xf, m, &text, size).expand(2.0))
}

fn paint_segment_values(p: &Painter, xf: &Xf, m: &Markup) {
    let size = xf.len((m.text.size * 0.8).clamp(4.0, 200.0));
    if size < 5.0 {
        return;
    }
    let col = color32(&m.color, 1.0);
    for v in measure_extras::segment_values(m).iter().take(2000) {
        let at = xf.to_screen(v.at);
        let galley = p.layout_no_wrap(v.text.clone(), FontId::proportional(size), col);
        let r = Align2::CENTER_BOTTOM.anchor_size(at - vec2(0.0, 2.0), galley.size());
        p.rect_filled(r, 1.0, Color32::from_white_alpha(150));
        p.galley(r.min, galley, col);
    }
}

/// Stroke a path, dashed when `dash` is set (PDF units).
pub(crate) fn stroke_path(p: &Painter, pts: &[Pos2], closed: bool, stroke: Stroke, dash: &[f64], xf: &Xf) {
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

/// Fill rings with the even-odd rule (an area with cutouts), one screen row at a time.
pub fn fill_even_odd(p: &Painter, rings: &[Vec<Pos2>], color: Color32) {
    let all: Vec<Pos2> = rings.iter().flatten().copied().collect();
    if all.len() < 3 {
        return;
    }
    let bounds = Rect::from_points(&all).intersect(p.clip_rect());
    if !bounds.is_positive() {
        return;
    }
    let mut mesh = Mesh::default();
    let mut y = bounds.top().floor() + 0.5;
    let mut xs: Vec<f32> = Vec::new();
    let mut rows = 0;
    while y < bounds.bottom() && rows < 4000 {
        xs.clear();
        for ring in rings {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f32::total_cmp);
        for span in xs.as_chunks::<2>().0 {
            let r = Rect::from_min_max(pos2(span[0], y - 0.5), pos2(span[1], y + 0.5));
            mesh.add_colored_rect(r, color);
        }
        y += 1.0;
        rows += 1;
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

fn tick(p: &Painter, pts: &[Pos2], at: Pos2, stroke: Stroke) {
    let (Some(a), Some(b)) = (pts.first(), pts.last()) else {
        return;
    };
    let d = (*b - *a).normalized();
    let n = vec2(-d.y, d.x) * 5.0;
    p.line_segment([at - n, at + n], stroke);
}

/// PDF line endings (`/LE`) at both ends of an open path.
pub(crate) fn line_endings(p: &Painter, pts: &[Pos2], m: &Markup, stroke: Stroke) {
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
    let r = xf.rect_of(b).expand(3.0);
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
    let arcs = !m.arcs.is_empty() && !uses_rect(m.kind);
    for (i, h) in handles(m).into_iter().enumerate() {
        if arcs && !measure_extras::is_editable_vertex(m, i) {
            // arc interior points: only the arc's middle (its handle) is drawn
            continue;
        }
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
        Kind::Arc => near_path(&markupcraft_revu::kinds::more::arc_polyline(m, 48), false),
        Kind::Dimension => match markupcraft_revu::kinds::more::dimension_lines(m) {
            Some((a, b, ea, eb)) => near_path(&[a, b], false) || near_path(&ea, false) || near_path(&eb, false),
            None => near_path(&m.pts, false),
        },
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly => m
            .pts
            .as_chunks::<4>()
            .0
            .iter()
            .any(|q| point_in_polygon(at, &[q[0], q[1], q[3], q[2]])),
        Kind::Callout => {
            box_of(m).padded(tol).contains(at)
                || m.pts.get(4..6).is_some_and(|l| {
                    let a = markupcraft_revu::kinds::text::callout_attach(m);
                    dist_to_segment(at, a, l[1]) <= tol + 1.0 || dist_to_segment(at, l[1], l[0]) <= tol + 1.0
                })
        }
        Kind::Note => {
            let b = box_of(m);
            markupcraft_geom::Rect::new(b.x0, b.y1 - 20.0, b.x0 + 20.0, b.y1)
                .padded(tol)
                .contains(at)
                || b.padded(tol).contains(at)
        }
        _ => box_of(m).padded(tol).contains(at),
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
    fn hatch_lines_clip_to_the_shape() {
        let sq = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let spans = clip_segment(Point::new(-5.0, 5.0), Point::new(15.0, 5.0), std::slice::from_ref(&sq));
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0 - 0.25).abs() < 1e-9 && (spans[0].1 - 0.75).abs() < 1e-9);
        let hole = vec![
            Point::new(4.0, 4.0),
            Point::new(6.0, 4.0),
            Point::new(6.0, 6.0),
            Point::new(4.0, 6.0),
        ];
        assert_eq!(
            clip_segment(Point::new(-5.0, 5.0), Point::new(15.0, 5.0), &[sq, hole]).len(),
            2
        );
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

    #[test]
    fn callouts_have_leader_handles() {
        let m = crate::tools::new_markup(
            Kind::Callout,
            0,
            &[Point::new(0.0, 0.0), Point::new(100.0, 100.0), Point::new(200.0, 150.0)],
        )
        .unwrap();
        let h = handles(&m);
        assert_eq!(h.len(), 6);
        assert_eq!(h[4], Point::new(0.0, 0.0));
        assert!(hit(&m, Point::new(150.0, 120.0), 1.0));
    }
}
