//! Helpers between the interface and the engine. Every change to a document goes through
//! `markupcraft_engine::Session` (the same session the automation tools drive), so undo, redo
//! and the command table are shared; this module only answers questions the UI asks about
//! markups and turns engine results into status-bar text.

use markupcraft_engine::{MarkupPatch, Session};
use markupcraft_geom::{Point, Rect, bbox};
use markupcraft_model::{Document, Kind, Markup};

/// Whether MarkupCraft can write this markup back after an edit. Kinds without a writer entry
/// (and looks we cannot redraw) are shown from the file and stay read-only for now.
pub fn editable(m: &Markup) -> bool {
    !m.foreign_look && m.kind != Kind::Other && markupcraft_revu::kinds::kind_for(m.kind).kind == m.kind
}

/// Whether the canvas draws this markup from the model (and the renderer hides its `/AP`):
/// everything we can write, plus markups the file gives no appearance (the PDF would show
/// nothing for them).
pub fn drawn_by_us(m: &Markup) -> bool {
    editable(m) || (!m.stored_look && !m.foreign_look && m.kind != Kind::Other)
}

/// Objects of annotations the canvas draws itself (hidden in the renderer).
pub fn drawn_objects(doc: &Document) -> Vec<(u32, u16)> {
    doc.markups
        .iter()
        .filter(|m| m.in_file() && drawn_by_us(m))
        .map(|m| m.obj)
        .collect()
}

/// Kinds whose geometry is a box (their first four points) rather than a vertex list.
pub fn uses_rect(k: Kind) -> bool {
    matches!(
        k,
        Kind::Rectangle
            | Kind::Ellipse
            | Kind::Text
            | Kind::Callout
            | Kind::Typewriter
            | Kind::Stamp
            | Kind::Snapshot
            | Kind::Note
            | Kind::Attachment
            | Kind::Hyperlink
            | Kind::Caret
            | Kind::Other
    )
}

/// The box of a box-shaped markup: its first four points (the writer's box), else `/Rect`.
pub fn box_of(m: &Markup) -> Rect {
    if m.pts.len() >= 4
        && let Some(b) = bbox(m.pts.get(..4).unwrap_or_default())
    {
        return b;
    }
    m.rect.normalized()
}

/// The user-space bounding box of a markup (its points, its box, else its `/Rect`).
pub fn markup_bbox(m: &Markup) -> Rect {
    if uses_rect(m.kind) {
        let mut b = box_of(m);
        if m.kind == Kind::Callout {
            for p in m.pts.iter().skip(4).take(2) {
                b = Rect::new(b.x0.min(p.x), b.y0.min(p.y), b.x1.max(p.x), b.y1.max(p.y));
            }
        }
        return b;
    }
    bbox(&m.pts).unwrap_or_else(|| m.rect.normalized())
}

/// The centre of a markup's bounding box.
pub fn center_of(m: &Markup) -> Point {
    let b = markup_bbox(m);
    Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
}

/// Replace the selection (ids that vanished are dropped).
pub fn select(s: &mut Session, ids: Vec<String>) {
    let keep: Vec<String> = ids.into_iter().filter(|id| s.doc().find(id).is_some()).collect();
    if s.select(&keep).is_err() {
        s.clear_selection();
    }
}

/// Add `id` to the selection, or take it out when it is in.
pub fn toggle_selected(s: &mut Session, id: &str) {
    let mut sel = s.selection().to_vec();
    if let Some(k) = sel.iter().position(|x| x == id) {
        sel.remove(k);
    } else {
        sel.push(id.to_string());
    }
    select(s, sel);
}

/// Status-bar text for an engine result.
pub fn report<T>(r: markupcraft_engine::Result<T>, ok: impl FnOnce(T) -> String) -> String {
    match r {
        Ok(v) => ok(v),
        Err(e) => e.to_string(),
    }
}

/// `n markups` with the right plural.
pub fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// The properties of `template` as a patch (Apply Tool Properties, Format Painter).
pub fn patch_from(template: &Markup) -> MarkupPatch {
    MarkupPatch {
        color: Some(template.color),
        fill: Some(template.fill),
        opacity: Some(template.opacity),
        fill_opacity: Some(template.fill_opacity),
        line_width: Some(template.line_width),
        dash: Some(template.dash.clone()),
        line_start: Some(template.line_start.clone()),
        line_end: Some(template.line_end.clone()),
        cloud: Some(template.cloud),
        font: Some(template.text.font.clone()),
        font_size: Some(template.text.size),
        text_color: Some(template.text.color),
        bold: Some(template.text.bold),
        italic: Some(template.text.italic),
        underline: Some(template.text.underline),
        align: Some(template.text.align),
        subject: Some(template.subject.clone()),
        label: Some(template.label.clone()),
        layer: Some(template.layer.clone()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_come_from_the_first_four_points() {
        let r = Rect::new(10.0, 10.0, 50.0, 30.0);
        let mut m = Markup::new(Kind::Callout, 0, r.corners().to_vec());
        m.rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        m.pts.push(Point::new(-20.0, 0.0));
        m.pts.push(Point::new(0.0, 20.0));
        assert_eq!(box_of(&m), r);
        assert_eq!(markup_bbox(&m), Rect::new(-20.0, 0.0, 50.0, 30.0));
        let mut o = Markup::new(Kind::Other, 0, Vec::new());
        o.rect = r;
        assert!(!editable(&o));
        assert_eq!(markup_bbox(&o), r);
    }

    #[test]
    fn selection_helpers_drop_unknown_ids() {
        let mut s = Session::new_blank("t.pdf", &[(612.0, 792.0)]).unwrap();
        let id = s
            .add_markup(Markup::new(
                Kind::Line,
                0,
                vec![Point::new(0.0, 0.0), Point::new(9.0, 9.0)],
            ))
            .unwrap();
        select(&mut s, vec!["nope".into(), id.clone()]);
        assert_eq!(s.selection(), std::slice::from_ref(&id));
        toggle_selected(&mut s, &id);
        assert!(s.selection().is_empty());
        assert_eq!(plural(2, "markup"), "2 markups");
    }
}
