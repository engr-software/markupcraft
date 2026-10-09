//! Hatch fills in the file: MarkupCraft's own key, and the lines drawn into the markup's
//! appearance so every viewer shows them.
//!
//! ```text
//! Annotation /PCHatch << /S /Diagonal /Sp 6 /W 0.5 /C [r g b] >>      /C absent = the line colour
//! ```
//!
//! Revu's own hatch (a `/Pattern` appearance) is kept untouched (`Markup::foreign_look`).

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::hatch::{Hatch, HatchStyle};
use markupcraft_model::{Kind, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, Object, Stream};

use crate::ap::f3;
use crate::kinds;
use crate::pdf::{self, color_arr, n, real};

/// Read `/PCHatch`.
pub fn read(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    let Some(d) = a.get(b"PCHatch").and_then(|o| cos.dict(o)) else {
        return;
    };
    let Some(style) = HatchStyle::from_name(&pdf::name(d.get(b"S"))) else {
        return;
    };
    let h = Hatch {
        style,
        spacing: pdf::num_or(d.get(b"Sp"), 6.0),
        width: pdf::num_or(d.get(b"W"), 0.5),
        color: pdf::color(d.get(b"C")),
    };
    if h.valid() {
        m.hatch = Some(h);
    }
}

/// The outline(s) the hatch is clipped to: the shape, and an Area's cutouts.
fn outlines(m: &Markup) -> Vec<Vec<Point>> {
    let Some(b) = bbox(&m.pts) else { return Vec::new() };
    match m.kind {
        Kind::Ellipse => {
            let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
            let (rx, ry) = (b.width() / 2.0, b.height() / 2.0);
            vec![
                (0..64)
                    .map(|i| {
                        let t = i as f64 / 64.0 * std::f64::consts::TAU;
                        Point::new(cx + rx * t.cos(), cy + ry * t.sin())
                    })
                    .collect(),
            ]
        }
        Kind::Rectangle => vec![Rect::new(b.x0, b.y0, b.x1, b.y1).corners().to_vec()],
        _ => {
            let mut v = vec![m.pts.clone()];
            v.extend(m.holes.iter().filter(|h| h.len() >= 3).cloned());
            v
        }
    }
}

/// Kinds a hatch is drawn on.
pub fn hatchable(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Area | Kind::Polygon | Kind::Rectangle | Kind::Ellipse | Kind::Cloud | Kind::Volume
    )
}

/// The drawing operators of `h` inside `m` (clip, then the lines).
pub fn hatch_ops(m: &Markup, h: &Hatch) -> String {
    let rings = outlines(m);
    let Some(first) = rings.first() else {
        return String::new();
    };
    let Some(b) = bbox(first) else { return String::new() };
    let mut s = String::from("q\n");
    for r in &rings {
        for (i, p) in r.iter().enumerate() {
            s.push_str(&format!("{} {} {}\n", f3(p.x), f3(p.y), if i == 0 { "m" } else { "l" }));
        }
        s.push_str("h\n");
    }
    s.push_str("W* n\n");
    let c = h.color.unwrap_or(m.color);
    s.push_str(&format!(
        "{} {} {} RG {} w 0 J\n",
        f3(c.r),
        f3(c.g),
        f3(c.b),
        f3(h.width)
    ));
    for (a, z) in h.lines(b) {
        s.push_str(&format!("{} {} m {} {} l\n", f3(a.x), f3(a.y), f3(z.x), f3(z.y)));
    }
    s.push_str("S Q\n");
    s
}

/// Write `/PCHatch` and, when MarkupCraft drew the appearance, the hatch lines into it.
pub fn write(cos: &mut CosDoc, a: &mut Dict, m: &Markup) {
    let Some(h) = m.hatch.filter(|h| h.valid() && hatchable(m.kind)) else {
        if a.contains(b"PCHatch") {
            a.remove(b"PCHatch");
        }
        return;
    };
    let mut d = pdf::dict(&[("S", n(h.style.name())), ("Sp", real(h.spacing)), ("W", real(h.width))]);
    if let Some(c) = &h.color {
        pdf::set(&mut d, "C", color_arr(c));
    }
    pdf::set(a, "PCHatch", Object::Dict(d));
    let own = !m.in_file() || kinds::app_draws(&m.subtype, &m.intent, !m.stamp.is_empty(), m.foreign_look);
    if !own {
        return;
    }
    let Some(ap_ref) = a.get(b"AP").and_then(|o| cos.dict(o)).and_then(|ap| ap.reference(b"N")) else {
        return;
    };
    let obj = cos.get(ap_ref);
    let Object::Stream(st) = obj.as_ref() else { return };
    let Ok(mut data) = st.decoded() else { return };
    data.extend_from_slice(hatch_ops(m, &h).as_bytes());
    let dict = st.dict.clone();
    cos.set(ap_ref, Object::Stream(Stream::flate(dict, &data)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hatch_ops_clip_to_the_shape_and_its_cutouts() {
        let mut m = Markup::new(Kind::Area, 0, Rect::new(0.0, 0.0, 100.0, 100.0).corners().to_vec());
        m.holes.push(Rect::new(10.0, 10.0, 20.0, 20.0).corners().to_vec());
        let ops = hatch_ops(&m, &Hatch::default());
        assert!(ops.contains("W* n"));
        assert_eq!(ops.matches(" h\n").count() + ops.matches("\nh\n").count(), 2, "{ops}");
        assert!(ops.ends_with("S Q\n"));
    }
}
