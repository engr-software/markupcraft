//! Where a measurement's value caption sits, and Revu's `/CO` caption offset.
//!
//! Area / Perimeter: the vertex mean (Revu anchors there; `/CO` is an offset from it, seen on
//! 37 captions of the reference set). Length: `/CO` is `[along, perpendicular]` to the line
//! (ISO 32000-1 §12.5.6.7). With Show Segment Values the running total sits beside the last
//! segment, on the side away from its segment value; with arcs, at the middle control segment.
//! Angle: inside the angle on its bisector. Others: the middle of the middle segment.

use markupcraft_geom::{Point, vertex_mean};

use crate::measure_extras::{arc_handle, control_segments};
use crate::{Kind, Markup};

/// How far the Angle caption sits from the vertex, and the total from its segment (PDF units).
const ANGLE_CAPTION: f64 = 20.0;
const TOTAL_OFFSET: f64 = 10.0;

/// The caption anchor before the user moved it.
pub fn default_caption_anchor(m: &Markup) -> Point {
    let Some(first) = m.pts.first() else {
        return Point::default();
    };
    if m.caption_last_segment
        && matches!(m.kind, Kind::Area | Kind::Perimeter | Kind::Volume | Kind::Polylength)
        && let Some(at) = last_segment_anchor(m)
    {
        return at;
    }
    if matches!(m.kind, Kind::Area | Kind::Perimeter | Kind::Volume) {
        return vertex_mean(&m.pts);
    }
    if m.kind == Kind::Angle
        && let [a, v, b, ..] = m.pts.as_slice()
    {
        let unit = |p: &Point| {
            let l = p.dist(*v);
            if l > 0.0 {
                Point::new((p.x - v.x) / l, (p.y - v.y) / l)
            } else {
                Point::default()
            }
        };
        let (ua, ub) = (unit(a), unit(b));
        let (bx, by) = (ua.x + ub.x, ua.y + ub.y);
        let l = bx.hypot(by);
        let dir = if l > 1e-9 {
            Point::new(bx / l, by / l)
        } else {
            Point::new(-ua.y, ua.x)
        };
        return Point::new(v.x + dir.x * ANGLE_CAPTION, v.y + dir.y * ANGLE_CAPTION);
    }
    if m.segment_values || !m.arcs.is_empty() {
        let segs = control_segments(m);
        let seg = if m.segment_values {
            segs.last()
        } else {
            segs.get(segs.len() / 2)
        };
        if let Some(s) = seg
            && let (Some(a), Some(b)) = (
                s.path.first().and_then(|&i| m.pts.get(i)),
                s.path.last().and_then(|&i| m.pts.get(i)),
            )
        {
            let at = s
                .arc
                .and_then(|j| arc_handle(m, j))
                .and_then(|h| m.pts.get(h).copied())
                .unwrap_or_else(|| a.mid(*b));
            if !m.segment_values {
                return at;
            }
            let l = a.dist(*b);
            let mut n = if l > 0.0 {
                Point::new(-(b.y - a.y) / l, (b.x - a.x) / l)
            } else {
                Point::new(0.0, 1.0)
            };
            // segment values are drawn on the upper side
            if n.y > 0.0 || (n.y == 0.0 && n.x > 0.0) {
                n = Point::new(-n.x, -n.y);
            }
            return Point::new(at.x + n.x * TOTAL_OFFSET, at.y + n.y * TOTAL_OFFSET);
        }
    }
    if m.pts.len() >= 2 {
        let k = m.pts.len() / 2;
        if let (Some(a), Some(b)) = (m.pts.get(k - 1), m.pts.get(k)) {
            return a.mid(*b);
        }
    }
    *first
}

/// Beside the middle of the last drawn segment (the one ending at the last vertex): outside a
/// closed shape (away from its vertex mean), below an open line.
fn last_segment_anchor(m: &Markup) -> Option<Point> {
    let n = m.pts.len();
    if n < 2 {
        return None;
    }
    let (a, b) = (*m.pts.get(n - 2)?, *m.pts.get(n - 1)?);
    let l = a.dist(b);
    if l <= 0.0 {
        return Some(a);
    }
    let mid = a.mid(b);
    let mut nrm = Point::new(-(b.y - a.y) / l, (b.x - a.x) / l);
    if matches!(m.kind, Kind::Polylength) {
        if nrm.y > 0.0 || (nrm.y == 0.0 && nrm.x > 0.0) {
            nrm = Point::new(-nrm.x, -nrm.y);
        }
    } else {
        let c = vertex_mean(&m.pts);
        if (mid.x - c.x) * nrm.x + (mid.y - c.y) * nrm.y < 0.0 {
            nrm = Point::new(-nrm.x, -nrm.y);
        }
    }
    Some(Point::new(mid.x + nrm.x * TOTAL_OFFSET, mid.y + nrm.y * TOTAL_OFFSET))
}

/// Where the caption is drawn: the default anchor plus the user's offset.
pub fn caption_anchor(m: &Markup) -> Point {
    let a = default_caption_anchor(m);
    match m.caption_offset {
        Some(o) => a + o,
        None => a,
    }
}

fn line_frame(m: &Markup) -> Option<(Point, Point)> {
    let (a, b) = (*m.pts.first()?, *m.pts.last()?);
    let l = a.dist(b);
    if l <= 0.0 {
        return None;
    }
    let u = Point::new((b.x - a.x) / l, (b.y - a.y) / l);
    Some((u, Point::new(-u.y, u.x)))
}

/// A Length's `/CO [along perp]` to a page-space offset.
pub fn line_co_to_offset(m: &Markup, along: f64, perp: f64) -> Point {
    match line_frame(m) {
        Some((u, n)) => Point::new(u.x * along + n.x * perp, u.y * along + n.y * perp),
        None => Point::new(along, perp),
    }
}

/// A page-space offset to a Length's `/CO [along perp]`.
pub fn offset_to_line_co(m: &Markup, o: Point) -> (f64, f64) {
    match line_frame(m) {
        Some((u, n)) => (o.x * u.x + o.y * u.y, o.x * n.x + o.y * n.y),
        None => (o.x, o.y),
    }
}

/// The fields a caption template can show, as `{name}` (Properties > Caption > Edit).
pub const CAPTION_FIELDS: &[(&str, &str)] = &[
    ("value", "Measurement"),
    ("all", "All measurements (P / A / WA / V)"),
    ("subject", "Subject"),
    ("label", "Label"),
    ("author", "Author"),
    ("layer", "Layer"),
    ("status", "Status"),
    ("length", "Length / perimeter"),
    ("area", "Area"),
    ("wall area", "Wall area"),
    ("volume", "Volume"),
    ("depth", "Depth"),
    ("slope", "Slope"),
    ("count", "Count"),
];

/// Longest caption a template expands to (characters).
const MAX_CAPTION: usize = 2000;

/// One field's text for `m` (`None` = not a field; `Some("")` = no value).
pub fn caption_field(m: &Markup, name: &str) -> Option<String> {
    use crate::measure_extras::{net_area, volume_of, wall_area};
    let s = m.scale.as_ref().filter(|s| s.valid());
    let fmt = |v: Option<f64>, area: bool| -> String {
        match (v, s) {
            (Some(v), Some(s)) => crate::format_value(v, if area { &s.area } else { &s.dist }),
            _ => String::new(),
        }
    };
    let closed = crate::measure_extras::closed_shape(m.kind);
    let length = || -> Option<f64> {
        let sc = s?;
        match m.kind {
            Kind::Area | Kind::Volume | Kind::Perimeter => Some(sc.length_of(&m.pts, closed) * m.slope_factor()),
            Kind::Length | Kind::Polylength => m.quantity(),
            _ => None,
        }
    };
    let area = || -> Option<f64> {
        match m.kind {
            Kind::Area | Kind::Volume => net_area(m).map(|a| a * m.slope_factor()),
            _ => None,
        }
    };
    Some(match name.trim() {
        "value" => m.quantity_text(),
        "subject" => m.subject.clone(),
        "label" => m.label.clone(),
        "author" => m.author.clone(),
        "layer" => m.layer.clone(),
        "status" => m.status.clone(),
        "length" | "perimeter" => fmt(length(), false),
        "area" => fmt(area(), true),
        "wall area" => fmt(wall_area(m), true),
        "volume" => match (volume_of(m).filter(|_| m.depth != 0.0), s) {
            (Some(v), Some(sc)) if !sc.volume.is_empty() => crate::format_value(v, &sc.volume),
            _ => String::new(),
        },
        "depth" if m.depth != 0.0 => fmt(Some(m.depth), false),
        "depth" => String::new(),
        "slope" => m.slope_of().label(),
        "count" if m.kind == Kind::Count => m.quantity_text(),
        "count" => String::new(),
        "all" => {
            let mut parts = Vec::new();
            for (tag, key) in [("P", "length"), ("A", "area"), ("WA", "wall area"), ("V", "volume")] {
                let v = caption_field(m, key).unwrap_or_default();
                if !v.is_empty() {
                    parts.push(format!("{tag}: {v}"));
                }
            }
            if parts.is_empty() {
                m.quantity_text()
            } else {
                parts.join(" / ")
            }
        }
        other => {
            let id = other.strip_prefix("c:")?;
            m.column_data.get(id).cloned().unwrap_or_default()
        }
    })
}

/// The caption a measurement shows: its template with every `{field}` filled in, or just the
/// value without one. Unknown `{names}` stay as typed.
pub fn caption_text(m: &Markup) -> String {
    if m.caption_template.trim().is_empty() {
        return m.quantity_text();
    }
    let mut out = String::new();
    let mut rest = m.caption_template.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(rest.get(..open).unwrap_or_default());
        let after = rest.get(open + 1..).unwrap_or_default();
        match after.find('}') {
            Some(close) => {
                let name = after.get(..close).unwrap_or_default();
                match caption_field(m, name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = after.get(close + 1..).unwrap_or_default();
            }
            None => {
                out.push_str(rest.get(open..).unwrap_or_default());
                rest = "";
            }
        }
        if out.len() > MAX_CAPTION {
            break;
        }
    }
    out.push_str(rest);
    out.chars().take(MAX_CAPTION).collect::<String>().trim().to_string()
}

/// Kinds whose moved caption is stored in Revu's `/CO`.
pub fn uses_co(k: Kind) -> bool {
    matches!(k, Kind::Area | Kind::Perimeter | Kind::Length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn caption_along_the_last_segment() {
        let sq = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
            Point::new(0.0, 100.0),
        ];
        let mut m = Markup::new(Kind::Perimeter, 0, sq);
        assert_eq!(default_caption_anchor(&m), Point::new(50.0, 50.0));
        m.caption_last_segment = true;
        // the last segment runs (100,100) -> (0,100): the caption sits above it, outside
        assert_eq!(default_caption_anchor(&m), Point::new(50.0, 100.0 + TOTAL_OFFSET));
    }

    #[test]
    fn caption_anchors() {
        let mut a = Markup {
            kind: Kind::Area,
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)],
            ..Default::default()
        };
        assert_eq!(default_caption_anchor(&a), p(50.0, 25.0));
        a.caption_offset = Some(p(10.0, -5.0));
        assert_eq!(caption_anchor(&a), p(60.0, 20.0));

        let l = Markup {
            kind: Kind::Length,
            pts: vec![p(100.0, 0.0), p(0.0, 0.0)],
            ..Default::default()
        };
        let o = line_co_to_offset(&l, 10.0, 5.0);
        assert!((o.x + 10.0).abs() < 1e-9 && (o.y + 5.0).abs() < 1e-9);
        let (along, perp) = offset_to_line_co(&l, o);
        assert!((along - 10.0).abs() < 1e-9 && (perp - 5.0).abs() < 1e-9);

        // With Show Segment Values the total sits under the last segment.
        let mut pl = Markup {
            kind: Kind::Polylength,
            pts: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(200.0, 100.0)],
            segment_values: true,
            ..Default::default()
        };
        assert_eq!(default_caption_anchor(&pl), p(150.0, 90.0));
        pl.segment_values = false;
        assert_eq!(default_caption_anchor(&pl), p(100.0, 50.0));
        // With an arc: the middle control segment's handle.
        crate::measure_extras::convert_to_arc(&mut pl, 1, 0.0);
        let h = default_caption_anchor(&pl);
        assert!((h.y - 50.0).abs() < 1e-9 && (h.x - 75.0).abs() < 1e-9, "{h:?}");

        let ang = Markup {
            kind: Kind::Angle,
            pts: vec![p(10.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)],
            ..Default::default()
        };
        let c = default_caption_anchor(&ang);
        assert!((c.x - c.y).abs() < 1e-9 && (c.x - 20.0 / 2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn caption_templates_fill_in_fields() {
        let mut a = Markup {
            kind: Kind::Area,
            subject: "Slab".into(),
            pts: vec![p(0.0, 0.0), p(90.0, 0.0), p(90.0, 90.0), p(0.0, 90.0)],
            scale: Some(crate::Scale::architectural(0.125, 1.0)),
            depth: 0.5,
            ..Default::default()
        };
        assert_eq!(caption_text(&a), "100 sf");
        a.caption_template = "{subject}\n{value} {nope}".into();
        assert_eq!(caption_text(&a), "Slab\n100 sf {nope}");
        a.caption_template = "{all}".into();
        assert_eq!(caption_text(&a), "P: 40'-0\" / A: 100 sf / WA: 20 sf / V: 50 cu ft");
        a.column_data.insert("cost".into(), "$12".into());
        a.caption_template = "{c:cost} {unclosed".into();
        assert_eq!(caption_text(&a), "$12 {unclosed");
        a.caption_template = "{slope}".into();
        a.slope_type = 2;
        a.slope = 30.0;
        assert_eq!(caption_text(&a), "30\u{b0}");
        let c = crate::measure_extras::centroid(&a).unwrap();
        assert!((c.x - 45.0).abs() < 1e-9 && (c.y - 45.0).abs() < 1e-9);
        a.holes
            .push(vec![p(0.0, 0.0), p(45.0, 0.0), p(45.0, 90.0), p(0.0, 90.0)]);
        let c = crate::measure_extras::centroid(&a).unwrap();
        assert!((c.x - 67.5).abs() < 1e-9, "{c:?}");
    }
}
