//! Text markups over page text (`/Highlight`, `/Underline`, `/StrikeOut`, `/Squiggly` with
//! `/QuadPoints`), the insert-text `/Caret` and the sticky Note (`/Text` with a `/Popup`).
//!
//! These follow ISO 32000-1 12.5.6.10 / .4 / .11: four `/QuadPoints` per run (upper-left,
//! upper-right, lower-left, lower-right); `/Caret` with `/RD` and `/Sy`; `/Text` with `/Name`,
//! `/Open` and a `/Popup`.

use markupcraft_geom::marks::{Mark, QuadStyle, caret_mark, note_icon, quad_marks, quad_pad};
use markupcraft_geom::path::path_points;
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Color, Kind, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, ObjRef, Object};

use super::AnnotKind;
use super::common::{box_from_rd, box_of, write_rd};
use crate::ap::Ap;
use crate::pdf::{self, n, points_arr, rect_arr};

fn quad_style(k: Kind) -> Option<QuadStyle> {
    match k {
        Kind::TextHighlight => Some(QuadStyle::Highlight),
        Kind::Underline => Some(QuadStyle::Underline),
        Kind::Strikeout => Some(QuadStyle::Strikeout),
        Kind::Squiggly => Some(QuadStyle::Squiggly),
        _ => None,
    }
}

/// New markups' default colours, as Revu / Acrobat draw them.
pub fn default_color(k: Kind) -> Color {
    match k {
        Kind::TextHighlight => Color::rgb(1.0, 1.0, 0.0),
        Kind::Note => Color::rgb(1.0, 0.82, 0.1),
        Kind::Underline | Kind::Squiggly => Color::rgb(0.0, 0.4, 1.0),
        _ => Color::RED,
    }
}

fn marks_of(m: &Markup) -> Vec<Mark> {
    if let Some(st) = quad_style(m.kind) {
        return quad_marks(st, &m.pts);
    }
    if m.pts.len() < 4 {
        return Vec::new();
    }
    match m.kind {
        Kind::Caret => vec![caret_mark(box_of(m))],
        Kind::Note => note_icon(box_of(m), &m.icon),
        _ => Vec::new(),
    }
}

/// Icon outlines: near-black.
const INK: Color = Color::rgb(0.1, 0.1, 0.1);

fn draw_marks(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let c = m.color;
    let quad = quad_style(m.kind).is_some();
    for k in marks_of(m) {
        if k.path.is_empty() {
            continue;
        }
        ap.op("q ");
        ap.fill_rgb(if k.fill_white { &Color::WHITE } else { &c });
        ap.stroke_rgb(if k.stroke_ink { &INK } else { &c });
        ap.op("[] 0 d ")
            .nums(&[k.width], "w")
            .op(if quad { "0 J 0 j\n" } else { "1 J 1 j\n" });
        ap.segs(&k.path);
        ap.op(match (k.fill, k.stroke) {
            (true, true) => "B",
            (true, false) => "f",
            _ => "S",
        })
        .op(" Q\n");
        extent.extend(path_points(&k.path));
    }
}

/// `/Rect`: the quads plus room for strokes, or the box.
pub fn mark_rect(m: &Markup) -> Rect {
    if quad_style(m.kind).is_some() {
        return bbox(&m.pts).unwrap_or_default().padded(quad_pad(&m.pts));
    }
    if m.pts.len() >= 4 {
        return box_of(m);
    }
    bbox(&m.pts).unwrap_or_default()
}

// ------------------------------------------------------------------------- quad kinds ---

fn write_quads(a: &mut Dict, m: &Markup) {
    pdf::set(a, "QuadPoints", points_arr(&m.pts));
}

fn read_quads(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    let mut pts = pdf::points(Some(&cos.resolve(a.get(b"QuadPoints").unwrap_or(&Object::Null))));
    pts.truncate(pts.len() / 4 * 4);
    if pts.is_empty() {
        // no quads: the whole /Rect
        let r = m.rect;
        pts = vec![
            Point::new(r.x0, r.y1),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
        ];
    }
    m.pts = pts;
    if m.kind == Kind::TextHighlight {
        m.multiply = true; // a highlight never hides its text
    }
}

// ------------------------------------------------------------------------------ caret ---

fn caret_create(a: &mut Dict, _m: &Markup) {
    pdf::set(a, "Sy", n("None"));
}

fn caret_geometry(a: &mut Dict, m: &Markup) {
    let r = mark_rect(m);
    write_rd(a, r, r); // the caret fills its /Rect
}

fn read_box_rd(_cos: &CosDoc, a: &Dict, m: &mut Markup) {
    box_from_rd(a, m);
}

// ------------------------------------------------------------------------------- note ---

fn note_geometry(a: &mut Dict, m: &Markup) {
    pdf::set(a, "Name", n(if m.icon.is_empty() { "Comment" } else { &m.icon }));
    pdf::set(a, "Open", Object::Bool(m.popup_open));
}

fn read_note(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    m.icon = pdf::name(a.get(b"Name"));
    m.popup_open = pdf::boolean(a.get(b"Open")).unwrap_or(false);
    let pop = cos.resolve(a.get(b"Popup").unwrap_or(&Object::Null));
    if let Some(pop) = pop.as_dict() {
        m.popup = pdf::rect(pop.get(b"Rect")).map(Rect::normalized);
        if let Some(open) = pdf::boolean(pop.get(b"Open")) {
            m.popup_open = open;
        }
    }
}

/// The note's `/Popup`: created once (and added to the page's `/Annots`), then kept in step.
/// A new note's `/Parent` link is set by the writer once the note has an object number.
fn note_popup(cos: &mut CosDoc, a: &mut Dict, m: &mut Markup) {
    let b = mark_rect(m);
    let popup = *m
        .popup
        .get_or_insert(Rect::new(b.x1 + 6.0, b.y1 - 120.0, b.x1 + 226.0, b.y1));
    let existing = a.reference(b"Popup").filter(|r| {
        cos.get(*r)
            .as_dict()
            .is_some_and(|d| d.name(b"Subtype") == Some(b"Popup"))
    });
    let pop = match existing {
        Some(r) => r,
        None => {
            let mut d = pdf::dict(&[("Type", n("Annot")), ("Subtype", n("Popup"))]);
            let page = a.reference(b"P");
            if let Some(p) = page {
                pdf::set(&mut d, "P", Object::Ref(p));
            }
            let r = cos.add(Object::Dict(d));
            if let Some(p) = page {
                let (mut arr, holder) = crate::write::annots_of(cos, p);
                arr.push(Object::Ref(r));
                crate::write::set_annots(cos, p, arr, holder);
            }
            pdf::set(a, "Popup", Object::Ref(r));
            r
        }
    };
    let parent = m.in_file().then(|| ObjRef::new(m.obj.0, m.obj.1));
    let open = m.popup_open;
    let _ = cos.update_dict(pop, |d| {
        if let Some(p) = parent {
            d.set(b"Parent".to_vec(), Object::Ref(p));
        }
        d.set(b"Rect".to_vec(), rect_arr(&popup));
        d.set(b"Open".to_vec(), Object::Bool(open));
        // Print, NoZoom, NoRotate (as Acrobat writes popups)
        d.set(b"F".to_vec(), Object::Int(28));
    });
}

// ------------------------------------------------------------------------------ table ---

pub static TEXT_HIGHLIGHT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_marks),
    write_geometry: Some(write_quads),
    rect_of: Some(mark_rect),
    read_keys: Some(read_quads),
    ..AnnotKind::new(Kind::TextHighlight, "Highlight", None, 0, false)
};
pub static UNDERLINE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_marks),
    write_geometry: Some(write_quads),
    rect_of: Some(mark_rect),
    read_keys: Some(read_quads),
    ..AnnotKind::new(Kind::Underline, "Underline", None, 0, false)
};
pub static STRIKEOUT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_marks),
    write_geometry: Some(write_quads),
    rect_of: Some(mark_rect),
    read_keys: Some(read_quads),
    ..AnnotKind::new(Kind::Strikeout, "StrikeOut", None, 0, false)
};
pub static SQUIGGLY: AnnotKind = AnnotKind {
    draw_shape: Some(draw_marks),
    write_geometry: Some(write_quads),
    rect_of: Some(mark_rect),
    read_keys: Some(read_quads),
    ..AnnotKind::new(Kind::Squiggly, "Squiggly", None, 0, false)
};
pub static CARET: AnnotKind = AnnotKind {
    create_keys: Some(caret_create),
    draw_shape: Some(draw_marks),
    write_geometry: Some(caret_geometry),
    rect_of: Some(mark_rect),
    read_keys: Some(read_box_rd),
    ..AnnotKind::new(Kind::Caret, "Caret", None, 0, false)
};
pub static NOTE: AnnotKind = AnnotKind {
    draw_shape: Some(draw_marks),
    write_geometry: Some(note_geometry),
    rect_of: Some(mark_rect),
    read_keys: Some(read_note),
    finish: Some(note_popup),
    ..AnnotKind::new(Kind::Note, "Text", None, 0, false)
};
