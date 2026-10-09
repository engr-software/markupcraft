//! The gestures of the special tools (`tools::more::Special`): the Eraser, Lasso selection,
//! Select Text, Insert / Replace Text and the Flag. Each change goes through the engine.

use egui::{Color32, Pos2, Stroke};
use markupcraft_geom::{Point, Rect as URect};
use markupcraft_model::{Kind, Markup};

use crate::DocTab;
use crate::actions;
use crate::canvas::{CanvasCx, CanvasOut};
use crate::interact::{Draft, Input, Stage, add_markup, open_new_editor, styled, text_quads};
use crate::tools::{self, more::Special};

/// The eraser's reach, screen points.
pub const ERASER_PX: f32 = 8.0;

/// press, click, release, current pointer
pub type Frame = (Option<Pos2>, Option<Pos2>, Option<Pos2>, Option<Pos2>);

/// Run the special tool `s` for this frame.
pub fn run(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut, s: Special, frame: Frame) {
    match s {
        Special::Eraser => eraser(ix, doc, out, frame),
        Special::Lasso => lasso(ix, doc, out, frame),
        Special::SelectText => select_text(ix, doc, out, frame),
        Special::InsertText => insert_text(ix, doc, cx, out, frame),
        Special::Flag => flag(ix, doc, cx, out, frame),
        Special::Attach => attach(ix, doc, out, frame),
    }
}

/// The key of a File Attachment click waiting for its file: (document, page, x, y).
pub const ATTACH_AT: &str = "markupcraft-attach-at";

fn attach(ix: &mut Input<'_>, doc: &mut DocTab, out: &mut CanvasOut, frame: Frame) {
    let (_, click, _, _) = frame;
    let Some(s) = click else { return };
    let Some((page, xf)) = ix.page_at(s) else { return };
    let at = xf.to_user(s);
    let uid = doc.uid;
    ix.ui
        .ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(ATTACH_AT), (uid, page, at.x, at.y)));
    out.status = Some("File Attachment: choose the file to attach".into());
}

/// A dimension drawn on a page with a scale shows its scaled length until the user types text.
pub fn prepare(doc: &DocTab, m: &mut Markup) {
    if m.kind == Kind::Dimension
        && m.contents.is_empty()
        && let (Some(a), Some(b)) = (m.pts.first(), m.pts.get(1))
        && let Some(sc) = doc.session.doc().pages.get(m.page).and_then(|p| p.scale_at(*a))
    {
        let v = sc.length_of(&[*a, *b], false);
        m.contents = markupcraft_model::format_value(v, &sc.dist);
    }
}

/// A press starts a freehand path; moves extend it. Returns the path so far and whether it
/// was released this frame.
fn freehand(ix: &Input<'_>, doc: &mut DocTab, tool: &'static str, frame: Frame) -> Option<(usize, Vec<Point>, bool)> {
    let (press, _click, release, cur) = frame;
    if let Some(s) = press
        && let Some((page, xf)) = ix.page_at(s)
    {
        doc.view.draft = Some(Draft {
            tool,
            page,
            pts: vec![xf.to_user(s)],
            stage: Stage::Dragging,
        });
    }
    let d = doc.view.draft.as_mut()?;
    let xf = ix.xf(d.page)?;
    if let Some(c) = release.or(cur) {
        let far = d.pts.last().is_none_or(|l| xf.to_screen(*l).distance(c) >= 1.5);
        if far && d.pts.len() < markupcraft_engine::markup_ops::MAX_PATH {
            d.pts.push(xf.to_user(c));
        }
    }
    let (page, pts) = (d.page, d.pts.clone());
    if release.is_some() {
        doc.view.draft = None;
    }
    Some((page, pts, release.is_some()))
}

fn eraser(ix: &mut Input<'_>, doc: &mut DocTab, out: &mut CanvasOut, frame: Frame) {
    let (_, click, _, cur) = frame;
    if let Some(c) = cur.or(ix.resp.hover_pos())
        && ix.page_at(c).is_some()
    {
        ix.painter
            .circle_stroke(c, ERASER_PX, Stroke::new(1.2, Color32::from_rgb(200, 40, 120)));
    }
    let single = click.and_then(|s| ix.page_at(s).map(|(p, xf)| (p, vec![xf.to_user(s)], true)));
    let Some((page, path, done)) = single.or_else(|| freehand(ix, doc, "eraser", frame)) else {
        return;
    };
    let Some(xf) = ix.xf(page) else { return };
    let radius = f64::from(ERASER_PX / xf.k.max(1e-6));
    if !done {
        let line: Vec<Pos2> = path.iter().map(|p| xf.to_screen(*p)).collect();
        ix.painter.add(egui::Shape::line(
            line,
            Stroke::new(ERASER_PX * 2.0, Color32::from_rgba_unmultiplied(200, 40, 120, 40)),
        ));
        return;
    }
    let r = doc.session.erase_ink(page, &path, radius);
    out.status = Some(actions::report(r, |n| match n {
        0 => "Eraser: drag across Pen or Highlight strokes".to_string(),
        n => format!("Erased from {}", actions::plural(n, "markup")),
    }));
}

fn lasso(ix: &mut Input<'_>, doc: &mut DocTab, out: &mut CanvasOut, frame: Frame) {
    let add = ix.ui.input(|i| i.modifiers.shift || i.modifiers.command);
    let Some((page, ring, done)) = freehand(ix, doc, "lasso", frame) else {
        return;
    };
    let Some(xf) = ix.xf(page) else { return };
    if !done {
        let mut line: Vec<Pos2> = ring.iter().map(|p| xf.to_screen(*p)).collect();
        if let Some(f) = line.first().copied() {
            line.push(f);
        }
        ix.painter.extend(egui::Shape::dashed_line(
            &line,
            Stroke::new(1.2, ix.tokens.select),
            5.0,
            3.0,
        ));
        return;
    }
    let r = doc.session.select_lasso(page, &ring, add);
    out.status = Some(actions::report(r, |n| {
        format!("Lasso: {} selected", actions::plural(n, "markup"))
    }));
}

/// The glyphs of the page text between `a` and `b` (user space), as an index range.
fn text_range(doc: &mut DocTab, xf: &crate::painter::Xf, page: usize, a: Point, b: Point) -> Option<String> {
    let t = doc.page_text(page).filter(|t| !t.is_empty())?;
    let to_view = |p: Point| {
        let v = xf.geom.user_to_view(p.x as f32, p.y as f32);
        (v[0], v[1])
    };
    let ((ax, ay), (bx, by)) = (to_view(a), to_view(b));
    let (i, j) = (t.nearest(ax, ay)?, t.nearest(bx, by)?);
    let (lo, hi) = (i.min(j), i.max(j));
    Some(t.text_of(lo..hi + 1))
}

fn select_text(ix: &mut Input<'_>, doc: &mut DocTab, out: &mut CanvasOut, frame: Frame) {
    if let Some(s) = ix.resp.hover_pos()
        && ix.page_at(s).is_some()
    {
        ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let Some((page, path, done)) = freehand(ix, doc, "selecttext", frame) else {
        return;
    };
    let (Some(a), Some(b)) = (path.first().copied(), path.last().copied()) else {
        return;
    };
    let Some(xf) = ix.xf(page).cloned() else { return };
    let quads = text_quads(doc, &xf, page, a, b, false);
    for q in quads.as_chunks::<4>().0 {
        let pts: Vec<Pos2> = [q[0], q[1], q[3], q[2]].iter().map(|p| xf.to_screen(*p)).collect();
        ix.painter.add(egui::Shape::convex_polygon(
            pts,
            ix.tokens.select.gamma_multiply(0.25),
            Stroke::NONE,
        ));
    }
    if !done {
        return;
    }
    match text_range(doc, &xf, page, a, b).filter(|t| !t.trim().is_empty()) {
        Some(text) => {
            let n = text.chars().count();
            ix.ui.ctx().copy_text(text);
            out.status = Some(format!("Copied {} of page text", actions::plural(n, "character")));
        }
        None => out.status = Some("No text here to select".into()),
    }
}

/// A caret's box at `at` (user space).
fn caret_at(page: usize, at: Point, size: f64) -> Markup {
    let mut m = tools::default_look(Kind::Caret);
    m.page = page;
    m.subject = "Inserted Text".into();
    let r = URect::new(
        at.x - size / 2.0,
        at.y - size * 0.15,
        at.x + size / 2.0,
        at.y + size * 0.85,
    );
    m.pts = r.corners().to_vec();
    m.rect = r;
    m
}

fn insert_text(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut, frame: Frame) {
    let (press, click, release, cur) = frame;
    let tool = cx.tool;
    if let Some(s) = click
        && let Some((page, xf)) = ix.page_at(s)
    {
        let caret = styled(caret_at(page, xf.to_user(s), 10.0), cx);
        open_new_editor(doc, tool, caret, None);
        out.status = Some("Type the text to insert; click away to finish".into());
        return;
    }
    if let Some(s) = press
        && let Some((page, xf)) = ix.page_at(s)
    {
        doc.view.draft = Some(Draft {
            tool: tool.id,
            page,
            pts: vec![xf.to_user(s)],
            stage: Stage::Dragging,
        });
    }
    let Some(d) = doc.view.draft.clone() else { return };
    let (Some(xf), Some(c)) = (ix.xf(d.page).cloned(), release.or(cur)) else {
        return;
    };
    let a = d.pts.first().copied().unwrap_or_default();
    let quads = text_quads(doc, &xf, d.page, a, xf.to_user(c), false);
    let strike = tools::new_markup(Kind::Strikeout, d.page, &quads).map(|mut m| {
        m.subject = "Replace Text".into();
        m.color = markupcraft_model::Color::rgb(0.0, 0.4, 1.0);
        m
    });
    if release.is_none() {
        if let Some(m) = &strike {
            crate::interact::preview(ix, d.page, m);
        }
        return;
    }
    doc.view.draft = None;
    let Some(m) = strike else {
        out.status = Some("Replace: drag across the words to replace".into());
        return;
    };
    // the caret after the struck words: the right end of the last line
    let end = quads
        .as_chunks::<4>()
        .0
        .last()
        .map(|q| Point::new(q[3].x, q[3].y))
        .unwrap_or(a);
    let height = quads
        .as_chunks::<4>()
        .0
        .last()
        .map_or(10.0, |q| q[0].dist(q[2]).clamp(6.0, 40.0));
    if let Some(id) = add_markup(doc, tool, m, None, out) {
        out.created = None;
        let caret = caret_at(d.page, end, height);
        open_new_editor(doc, tool, caret, Some(id));
        out.status = Some("Type the replacement text; click away to finish".into());
    }
}

fn flag(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut, frame: Frame) {
    let (_, click, _, _) = frame;
    let Some(s) = click else { return };
    let Some((page, xf)) = ix.page_at(s) else { return };
    let Some(mut m) = tools::new_markup(Kind::Note, page, &[xf.to_user(s)]) else {
        return;
    };
    m.icon = crate::shapes_more::FLAG_ICON.into();
    m.color = markupcraft_model::Color::RED;
    m.subject = "Flag".into();
    let mut m = styled(m, cx);
    m.icon = crate::shapes_more::FLAG_ICON.into();
    add_markup(doc, cx.tool, m, None, out);
}
