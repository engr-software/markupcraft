//! Snapshot: a rectangular region of a page placed as a markup. Saved the way Revu 21 saves its
//! snapshots: a `/Stamp` with `/IT /StampSnapshot`, `/Rotation`, and an `/AP /N` form XObject
//! holding the page content, whose `/BBox` is the captured region in the source page's space and
//! whose `/Matrix` places it on `/Rect` (with the turn when the pages differ in `/Rotate`).
//!
//! Snapshots read from a file keep their `/AP`. New ones (made here or copied) get theirs from
//! [`SnapshotSource`]: another snapshot's `/AP /N`, or a region of a page captured on save.

use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Kind, Markup, SnapshotSource};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object, Stream};

use super::AnnotKind;
use super::common::box_of;
use crate::ap::{Ap, f3};
use crate::pdf::{self, n, real, rect_arr};

/// Most content bytes one capture copies (decompression and memory guard).
const MAX_CONTENT: usize = 256 << 20;
/// Deepest page-tree inheritance walk for `/Resources`.
const MAX_DEPTH: usize = 64;

fn quarter(degrees: f64) -> i64 {
    let q = if degrees.is_finite() {
        (degrees / 90.0).round() as i64
    } else {
        0
    };
    q.rem_euclid(4) * 90
}

/// A 2-D affine matrix `[a b c d e f]`.
type Matrix = [f64; 6];

fn apply(m: &Matrix, p: Point) -> Point {
    let [a, b, c, d, e, f] = *m;
    Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
}

/// Map box `from` (turned by `rot` degrees counter-clockwise about the origin) onto box `to`.
fn place_matrix(from: Rect, rot: i64, to: Rect) -> Matrix {
    let (r00, r01, r10, r11) = match rot {
        90 => (0.0, -1.0, 1.0, 0.0),
        180 => (-1.0, 0.0, 0.0, -1.0),
        270 => (0.0, 1.0, -1.0, 0.0),
        _ => (1.0, 0.0, 0.0, 1.0),
    };
    let turned: Vec<Point> = from
        .corners()
        .iter()
        .map(|p| Point::new(r00 * p.x + r01 * p.y, r10 * p.x + r11 * p.y))
        .collect();
    let t = bbox(&turned).unwrap_or_default();
    let sx = if t.x1 > t.x0 {
        (to.x1 - to.x0) / (t.x1 - t.x0)
    } else {
        1.0
    };
    let sy = if t.y1 > t.y0 {
        (to.y1 - to.y0) / (t.y1 - t.y0)
    } else {
        1.0
    };
    [
        sx * r00,
        sy * r10,
        sx * r01,
        sy * r11,
        to.x0 - sx * t.x0,
        to.y0 - sy * t.y0,
    ]
}

fn matrix_of(d: &Dict) -> Matrix {
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    if let Some(a) = d.get(b"Matrix").and_then(Object::as_array)
        && a.len() == 6
    {
        for (v, o) in m.iter_mut().zip(a) {
            *v = pdf::num_or(Some(o), *v);
        }
    }
    m
}

fn matrix_arr(m: &Matrix) -> Object {
    Object::Array(m.iter().map(|v| real(*v)).collect())
}

fn page_rotate(cos: &CosDoc, page: Option<ObjRef>) -> i64 {
    let Some(page) = page else { return 0 };
    pdf::pages(cos)
        .iter()
        .find(|p| p.id == page)
        .map_or(0, |p| quarter(p.rotate as f64))
}

fn form(bbox: Rect, resources: Object) -> Dict {
    pdf::dict(&[
        ("Type", n("XObject")),
        ("Subtype", n("Form")),
        ("FormType", Object::Int(1)),
        ("BBox", rect_arr(&bbox)),
        ("Resources", resources),
    ])
}

/// The page's `/Resources`, following inheritance up the page tree.
fn page_resources(cos: &CosDoc, page: &Dict) -> Object {
    let mut d = page.clone();
    for _ in 0..MAX_DEPTH {
        if let Some(r) = d.get(b"Resources") {
            return r.clone();
        }
        let Some(parent) = d.reference(b"Parent") else { break };
        match cos.get(parent).as_dict() {
            Some(p) => d = p.clone(),
            None => break,
        }
    }
    Object::Dict(Dict::new())
}

/// The page's content streams, decoded and joined.
fn page_content(cos: &CosDoc, page: &Dict) -> Vec<u8> {
    let contents = cos.resolve(page.get(b"Contents").unwrap_or(&Object::Null));
    let parts: Vec<Object> = match contents.as_ref() {
        Object::Array(a) => a.clone(),
        other => vec![other.clone()],
    };
    let mut out = Vec::new();
    for p in parts {
        let p = cos.resolve(&p);
        if let Object::Stream(st) = p.as_ref()
            && let Ok(data) = st.decoded_within(MAX_CONTENT.saturating_sub(out.len()))
        {
            out.extend_from_slice(&data);
            out.push(b'\n');
        }
    }
    out
}

/// A copy of a snapshot in this file: draw the original's appearance moved and scaled into `bx`.
fn copy_of_annot(cos: &mut CosDoc, a: &mut Dict, obj: (u32, u16), bx: Rect) -> bool {
    let orig = cos.get(ObjRef::new(obj.0, obj.1));
    let Some(od) = orig.as_dict() else { return false };
    let ap = cos.resolve(od.get(b"AP").unwrap_or(&Object::Null));
    let Some(n_obj) = ap.as_dict().and_then(|d| d.get(b"N")).cloned() else {
        return false;
    };
    let n_val = cos.resolve(&n_obj);
    let Object::Stream(st) = n_val.as_ref() else {
        return false;
    };
    let Some(bb) = pdf::rect(st.dict.get(b"BBox")).map(Rect::normalized) else {
        return false;
    };
    let mtx = matrix_of(&st.dict);
    let shown = bbox(&bb.corners().map(|p| apply(&mtx, p))).unwrap_or_default();
    let place = place_matrix(shown, 0, bx);
    let rotation = od.get(b"Rotation").and_then(Object::as_int).unwrap_or(0);
    let n_ref = match n_obj {
        Object::Ref(r) => r,
        other => cos.add(other),
    };
    let content = format!(
        "q {} 0 0 {} {} {} cm /S0 Do Q\n",
        f3(place[0]),
        f3(place[3]),
        f3(place[4]),
        f3(place[5])
    );
    let res = Object::Dict(pdf::dict(&[(
        "XObject",
        Object::Dict(pdf::dict(&[("S0", Object::Ref(n_ref))])),
    )]));
    let wrapper = cos.add(Object::Stream(Stream::flate(form(bx, res), content.as_bytes())));
    pdf::set(a, "AP", Object::Dict(pdf::dict(&[("N", Object::Ref(wrapper))])));
    pdf::set(a, "Rotation", Object::Int(rotation));
    true
}

/// A region of a page of this file, captured as a form XObject placed into `bx`.
fn capture(cos: &mut CosDoc, a: &mut Dict, page: usize, region: Rect, bx: Rect) -> bool {
    let pages = pdf::pages(cos);
    let Some(src) = pages.get(page) else { return false };
    let region = region.normalized();
    if region.width() <= 0.0 || region.height() <= 0.0 {
        return false;
    }
    let content = page_content(cos, &src.dict);
    let res = page_resources(cos, &src.dict);
    let dest = page_rotate(cos, a.reference(b"P"));
    let rot = quarter((dest - quarter(src.rotate as f64)) as f64);
    let mut d = form(region, res);
    pdf::set(&mut d, "Matrix", matrix_arr(&place_matrix(region, rot, bx)));
    let mut body = Ap::new();
    body.nums(&[region.x0, region.y0, region.width(), region.height()], "re")
        .op("W n\n");
    let mut data = body.bytes();
    data.extend_from_slice(&content);
    let r = cos.add(Object::Stream(Stream::flate(d, &data)));
    pdf::set(a, "AP", Object::Dict(pdf::dict(&[("N", Object::Ref(r))])));
    pdf::set(a, "Rotation", Object::Int(rot));
    true
}

/// `AnnotKind::finish`: `/AP /N` = the content placed in the box, `/Rect`, `/Rotation`.
fn snapshot_finish(cos: &mut CosDoc, a: &mut Dict, m: &mut Markup) {
    let Some(src) = m.snapshot.clone() else { return };
    if m.pts.len() < 4 {
        return;
    }
    let bx = box_of(m);
    let done = match (src.annot, src.page) {
        (Some(obj), _) if copy_of_annot(cos, a, obj, bx) => true,
        (_, Some(page)) => capture(cos, a, page, src.region, bx),
        _ => false,
    };
    if done {
        pdf::set(a, "Rect", rect_arr(&bx));
        m.rect = bx;
    }
}

/// Remember where a read snapshot's content is: its own `/AP`.
fn read_snapshot(_cos: &CosDoc, _a: &Dict, m: &mut Markup) {
    m.snapshot = Some(SnapshotSource {
        annot: m.in_file().then_some(m.obj),
        page: None,
        region: m.rect.normalized(),
    });
}

fn snapshot_geometry(_a: &mut Dict, _m: &Markup) {}

fn draw_nothing(_ap: &mut Ap, _m: &Markup, _extent: &mut Vec<Point>) {}

fn snapshot_rect(m: &Markup) -> Rect {
    box_of(m)
}

pub static SNAPSHOT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_nothing),
    write_geometry: Some(snapshot_geometry),
    rect_of: Some(snapshot_rect),
    read_keys: Some(read_snapshot),
    finish: Some(snapshot_finish),
    ..AnnotKind::new(Kind::Snapshot, "Stamp", Some("StampSnapshot"), 0, false)
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_matrix_maps_corners() {
        let from = Rect::new(100.0, 100.0, 300.0, 200.0);
        let to = Rect::new(0.0, 0.0, 100.0, 50.0);
        let m = place_matrix(from, 0, to);
        assert_eq!(apply(&m, Point::new(100.0, 100.0)), Point::new(0.0, 0.0));
        assert_eq!(apply(&m, Point::new(300.0, 200.0)), Point::new(100.0, 50.0));
        let m = place_matrix(from, 90, Rect::new(0.0, 0.0, 50.0, 100.0));
        let b = bbox(&from.corners().map(|p| apply(&m, p))).unwrap();
        assert!((b.x1 - 50.0).abs() < 1e-9 && (b.y1 - 100.0).abs() < 1e-9 && b.x0.abs() < 1e-9);
        assert_eq!(quarter(-90.0), 270);
        assert_eq!(quarter(f64::NAN), 0);
    }
}
