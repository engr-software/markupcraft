//! Freehand and stamps: Pen (`/Ink` + `/InkList`), Highlight (`/Ink` + `/BM /Multiply`) and our
//! text Stamp (`/Stamp` + `/PCStamp`). Other apps' stamps keep their own `/AP`.

use markupcraft_geom::Point;
use markupcraft_geom::text::stamp_lines;
use markupcraft_model::{Color, Kind, Markup};
use pdfcraft_cos::{Dict, Document as CosDoc, Object};

use super::AnnotKind;
use super::common::{box_of, font_of};
use super::text::{parse_text_css, text_css};
use crate::ap::Ap;
use crate::pdf::{self, n, points_arr, s};

// -------------------------------------------------------------------------------- ink ---

/// The strokes of an Ink markup: `pts` split at `strokes`.
pub fn strokes_of(m: &Markup) -> Vec<&[Point]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for b in m.strokes.iter().copied().chain(std::iter::once(m.pts.len())) {
        let end = b.min(m.pts.len());
        if end > start
            && let Some(s) = m.pts.get(start..end)
        {
            out.push(s);
        }
        start = start.max(end);
    }
    out
}

fn draw_ink(ap: &mut Ap, m: &Markup, _extent: &mut Vec<Point>) {
    for st in strokes_of(m) {
        let Some(first) = st.first() else { continue };
        ap.move_to(*first);
        if st.len() == 1 {
            ap.line_to(Point::new(first.x + 0.01, first.y));
        }
        for p in st.iter().skip(1) {
            ap.line_to(*p);
        }
        ap.op("S\n");
    }
}

fn write_ink(a: &mut Dict, m: &Markup) {
    let list = strokes_of(m).into_iter().map(points_arr).collect();
    pdf::set(a, "InkList", Object::Array(list));
    if m.multiply {
        pdf::set(a, "BM", n("Multiply"));
    } else {
        pdf::remove(a, "BM");
    }
}

fn read_ink(cos: &CosDoc, a: &Dict, m: &mut Markup) {
    let list = cos.resolve(a.get(b"InkList").unwrap_or(&Object::Null));
    let Some(list) = list.as_array() else { return };
    m.pts.clear();
    m.strokes.clear();
    for st in list {
        let st = cos.resolve(st);
        if st.as_array().is_none() {
            continue;
        }
        if !m.pts.is_empty() {
            m.strokes.push(m.pts.len());
        }
        m.pts.extend(pdf::points(Some(&st)));
    }
}

// ------------------------------------------------------------------------------ stamp ---

/// One of our own text stamp designs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StampDesign {
    /// stored in `/PCStamp`
    pub id: &'static str,
    /// the big line
    pub text: &'static str,
    pub color: Color,
    /// `/Name` (a standard ISO 32000 stamp name where one fits)
    pub pdf_name: &'static str,
}

const GREEN: Color = Color::rgb(0.0, 0.5, 0.15);
const BLUE: Color = Color::rgb(0.05, 0.3, 0.8);
const RED: Color = Color::rgb(0.8, 0.0, 0.0);

pub static STAMP_DESIGNS: &[StampDesign] = &[
    StampDesign {
        id: "Approved",
        text: "APPROVED",
        color: GREEN,
        pdf_name: "Approved",
    },
    StampDesign {
        id: "ApprovedAsNoted",
        text: "APPROVED AS NOTED",
        color: GREEN,
        pdf_name: "Approved",
    },
    StampDesign {
        id: "Reviewed",
        text: "REVIEWED",
        color: BLUE,
        pdf_name: "ForComment",
    },
    StampDesign {
        id: "ReviseResubmit",
        text: "REVISE AND RESUBMIT",
        color: Color::rgb(0.85, 0.4, 0.0),
        pdf_name: "NotApproved",
    },
    StampDesign {
        id: "Rejected",
        text: "REJECTED",
        color: RED,
        pdf_name: "NotApproved",
    },
    StampDesign {
        id: "Draft",
        text: "DRAFT",
        color: Color::rgb(0.35, 0.35, 0.4),
        pdf_name: "Draft",
    },
    StampDesign {
        id: "Void",
        text: "VOID",
        color: RED,
        pdf_name: "Expired",
    },
    StampDesign {
        id: "ForConstruction",
        text: "FOR CONSTRUCTION",
        color: GREEN,
        pdf_name: "Final",
    },
    StampDesign {
        id: "NotForConstruction",
        text: "NOT FOR CONSTRUCTION",
        color: RED,
        pdf_name: "NotForPublicRelease",
    },
    StampDesign {
        id: "InfoOnly",
        text: "FOR INFORMATION ONLY",
        color: BLUE,
        pdf_name: "ForPublicRelease",
    },
];

pub fn find_stamp(id: &str) -> Option<&'static StampDesign> {
    STAMP_DESIGNS.iter().find(|d| d.id == id)
}

fn round_rect(ap: &mut Ap, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) {
    let k = 0.5523 * r;
    let p = Point::new;
    ap.move_to(p(x0 + r, y0))
        .line_to(p(x1 - r, y0))
        .curve_to(p(x1 - r + k, y0), p(x1, y0 + r - k), p(x1, y0 + r))
        .line_to(p(x1, y1 - r))
        .curve_to(p(x1, y1 - r + k), p(x1 - r + k, y1), p(x1 - r, y1))
        .line_to(p(x0 + r, y1))
        .curve_to(p(x0 + r - k, y1), p(x0, y1 - r + k), p(x0, y1 - r))
        .line_to(p(x0, y0 + r))
        .curve_to(p(x0, y0 + r - k), p(x0 + r - k, y0), p(x0 + r, y0))
        .op("h ");
}

fn draw_stamp(ap: &mut Ap, m: &Markup, extent: &mut Vec<Point>) {
    let b = box_of(m);
    *extent = vec![Point::new(b.x0, b.y0), Point::new(b.x1, b.y1)];
    let w = m.line_width;
    let rad = b.width().min(b.height()) * 0.12;
    let h = w / 2.0;
    if m.fill.is_some() {
        round_rect(ap, b.x0 + h, b.y0 + h, b.x1 - h, b.y1 - h, rad);
        ap.op("f\n");
    }
    if w > 0.0 {
        round_rect(ap, b.x0 + h, b.y0 + h, b.x1 - h, b.y1 - h, rad);
        ap.op("S\n");
        let inset = w + 2.5;
        ap.op("q ").nums(&[(w / 3.0).max(0.5)], "w");
        round_rect(
            ap,
            b.x0 + inset,
            b.y0 + inset,
            b.x1 - inset,
            b.y1 - inset,
            (rad - inset + h).max(0.0),
        );
        ap.op("S Q\n");
    }
    let font = font_of(&m.text);
    ap.op("BT ").fill_rgb(&m.text.color).newline();
    for (line, size) in stamp_lines(b, &m.contents, &font, w) {
        if line.text.is_empty() {
            continue;
        }
        ap.op("/").op(font.res_name()).op(" ").nums(&[size], "Tf");
        ap.op("1 0 0 1 ").nums(&[line.x, line.y], "Tm");
        let t = markupcraft_geom::text::to_win_ansi(&line.text);
        ap.op(&format!("({}) Tj\n", Ap::text_literal(&t)));
    }
    ap.op("ET\n");
}

fn stamp_create_keys(a: &mut Dict, m: &Markup) {
    let name = find_stamp(&m.stamp).map_or("Draft", |d| d.pdf_name);
    pdf::set(a, "Name", n(name));
    pdf::set(a, "PCStamp", s(&m.stamp));
}

fn write_stamp(a: &mut Dict, m: &Markup) {
    if !m.stamp.is_empty() {
        pdf::set(a, "PCStamp", s(&m.stamp));
        // our stamps keep their font in /DS (the same CSS as text boxes)
        pdf::set(a, "DS", s(&text_css(&m.text)));
    }
}

fn read_stamp(_cos: &CosDoc, a: &Dict, m: &mut Markup) {
    if m.stamp.is_empty() {
        return;
    }
    if let Some(Object::String(ds)) = a.get(b"DS") {
        parse_text_css(&ds.to_text(), &mut m.text);
    }
}

fn stamp_rect(m: &Markup) -> markupcraft_geom::Rect {
    box_of(m)
}

// ------------------------------------------------------------------------------ table ---

pub static INK: AnnotKind = AnnotKind {
    draw_shape: Some(draw_ink),
    write_geometry: Some(write_ink),
    read_keys: Some(read_ink),
    ..AnnotKind::new(Kind::Ink, "Ink", None, 0, false)
};
pub static HIGHLIGHT: AnnotKind = AnnotKind {
    draw_shape: Some(draw_ink),
    write_geometry: Some(write_ink),
    read_keys: Some(read_ink),
    ..AnnotKind::new(Kind::Highlight, "Ink", None, 0, false)
};
pub static STAMP: AnnotKind = AnnotKind {
    create_keys: Some(stamp_create_keys),
    draw_shape: Some(draw_stamp),
    write_geometry: Some(write_stamp),
    rect_of: Some(stamp_rect),
    read_keys: Some(read_stamp),
    ..AnnotKind::new(Kind::Stamp, "Stamp", None, 0, false)
};
