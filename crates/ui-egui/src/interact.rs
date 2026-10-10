//! What the pointer and the keys do on the canvas with the active tool, Revu style:
//!
//! - Select: click, Shift/Ctrl+click, box select (left or right button), drag to move (Shift
//!   keeps it straight, Ctrl drops a copy, Ctrl+Shift copies in a straight line), drag a handle to reshape, Shift+drag a measurement's caption to move it
//!   alone (saved as `/CO`), double-click a text markup to edit its text, right-click for the
//!   markup menu.
//! - Point tools: click to place points; Enter, a double-click or a right-click finishes,
//!   Backspace removes the last point, Esc cancels (Count keeps what it counted). Two-point
//!   tools also take a press-drag-release. Shift constrains to 0/45/90 degrees from the last
//!   point. The value being measured follows the cursor.
//! - Box, freehand, text, stamp, note and text-markup tools as described in `tools/`.
//!
//! Points snap to content, markups and the grid when those toggles are on. Every change is an
//! engine call, so it is one undo step; gestures show live geometry (a preview) and commit on
//! release.

use egui::{Color32, FontId, Pos2, Rect, Stroke, vec2};
use markupcraft_geom::text::text_width;
use markupcraft_geom::{Point, Rect as URect, bbox};
use markupcraft_model::{Kind, Markup, measure_extras};
use markupcraft_revu::kinds::common::font_of;

use crate::DocTab;
use crate::actions::{self, box_of, editable, uses_rect};
use crate::canvas::{CanvasCx, CanvasOut};
use crate::painter::{self, Xf};
use crate::snapping::{self, Snapped};
use crate::theme::Tokens;
use crate::tools::{self, DragShape, Role, ToolDef, ToolKind};

/// The pointer within this many screen points of a handle grabs it.
const HANDLE_REACH: f32 = 6.0;
/// Click tolerance for picking a markup, screen points.
const PICK: f32 = 5.0;
/// Default text box size (points) when the Text Box tool is clicked rather than dragged.
const TEXT_BOX: (f64, f64) = (180.0, 40.0);
/// Default callout box size.
const CALLOUT_BOX: (f64, f64) = (144.0, 36.0);

/// A Select-tool gesture in progress.
#[derive(Debug, Clone)]
pub enum Gesture {
    Move {
        page: usize,
        start: Point,
        /// Ctrl: drop a copy instead of moving.
        copy: bool,
        /// Shift: along the horizontal or vertical only.
        straight: bool,
    },
    Handle {
        id: String,
        index: usize,
        page: usize,
    },
    Caption {
        id: String,
        page: usize,
        start: Point,
        from: Point,
    },
    Box {
        page: usize,
        start: Pos2,
    },
    /// Turning the selected markup with its rotation handle about `pivot`.
    Rotate {
        id: String,
        page: usize,
        pivot: Point,
        /// the pointer's angle about the pivot when the drag began (radians)
        start: f64,
    },
}

/// Where a drawing tool is.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// placing points
    Points,
    /// a press is being dragged; `pts[0]` is where it started
    Dragging,
    /// Callout: the tip is placed (`pts[0]`); the next click places the box
    CalloutTip,
    /// Cloud+: the cloud was added; the next click places its callout
    CloudPlus { cloud: String },
    /// Sketch to Scale placed the last point: finish on the next frame
    Typed,
}

/// A markup being drawn.
#[derive(Debug, Clone)]
pub struct Draft {
    pub tool: &'static str,
    pub page: usize,
    pub pts: Vec<Point>,
    pub stage: Stage,
}

/// What the text editor edits.
#[derive(Debug, Clone)]
pub enum EditTarget {
    /// A markup not added yet: it is added with the text when the editor closes.
    New {
        markup: Box<Markup>,
        tool: &'static str,
        /// Cloud+: group the new callout with this cloud
        group_with: Option<String>,
    },
    Existing(String),
}

/// The text editor shown over the page.
#[derive(Debug, Clone)]
pub struct TextEditor {
    pub target: EditTarget,
    pub page: usize,
    pub text: String,
    /// rich text runs of the text being typed (char offsets into `text`)
    pub rich: Vec<markupcraft_model::rich::TextRun>,
    /// Ctrl+B / I / U with nothing selected: the style for the text typed next
    pub pending: crate::richedit::Pending,
    /// focus was requested
    pub opened: bool,
}

impl TextEditor {
    pub fn existing_id(&self) -> Option<String> {
        match &self.target {
            EditTarget::Existing(id) => Some(id.clone()),
            EditTarget::New { .. } => None,
        }
    }
}

/// What the context menu was opened on.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextTarget {
    pub page: usize,
    pub at: Point,
    pub markup: Option<String>,
    /// the vertex under the pointer (of `markup`)
    pub vertex: Option<usize>,
    /// the segment under the pointer: insert a vertex after this index, at this point
    pub segment: Option<(usize, Point)>,
    /// the cutout under the pointer
    pub hole: Option<usize>,
}

/// One frame's input and drawing surfaces.
pub struct Input<'a> {
    pub ui: &'a egui::Ui,
    pub resp: &'a egui::Response,
    pub painter: &'a egui::Painter,
    pub xfs: &'a [(usize, Xf)],
    pub tokens: &'a Tokens,
    /// space or middle button panning: tools ignore the pointer
    pub panning: bool,
}

impl Input<'_> {
    pub(crate) fn xf(&self, page: usize) -> Option<&Xf> {
        self.xfs.iter().find(|(i, _)| *i == page).map(|(_, x)| x)
    }

    pub(crate) fn page_at(&self, s: Pos2) -> Option<(usize, &Xf)> {
        self.xfs
            .iter()
            .find(|(_, xf)| xf.rect.expand(4.0).contains(s))
            .map(|(i, xf)| (*i, xf))
    }
}

/// Keys the canvas handles itself (when no text field has the keyboard).
struct Keys {
    enter: bool,
    backspace: bool,
}

fn keys(ui: &egui::Ui, draft_active: bool) -> Keys {
    if ui.ctx().egui_wants_keyboard_input() || !draft_active {
        return Keys {
            enter: false,
            backspace: false,
        };
    }
    ui.input_mut(|i| Keys {
        enter: i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        backspace: i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
    })
}

/// Handle this frame's input for the active tool.
pub fn run(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) {
    // While the text editor is open (and in the frame it closes) clicks belong to it.
    let had_editor = doc.view.editor.is_some();
    if had_editor {
        editor_ui(ix, doc, out);
    }
    let drafting = doc.view.draft.is_some();
    if !ix.panning && !had_editor {
        match cx.tool.kind {
            ToolKind::Pan => {}
            ToolKind::Select => select_tool(ix, doc, cx, out),
            _ => draw_tool(ix, doc, cx, out),
        }
    }
    if !drafting && !had_editor {
        context_menu(ix, doc, cx, out);
    }
    if !ix.resp.dragged() && !ix.resp.drag_stopped() && doc.view.gesture.is_some() {
        // A gesture whose release we missed (focus loss) is dropped.
        doc.view.gesture = None;
        doc.view.preview.clear();
    }
    if let Some(g) = crate::snapping_more::current(ix.ui.ctx())
        && let Some(xf) = ix.xf(g.page)
        && (doc.view.draft.is_some() || doc.view.gesture.is_some() || cx.tool.draws())
    {
        crate::snapping_more::paint(ix.painter, xf, g.page, snapping::indicator_color(cx.snaps.color));
    }
    if let Some((page, s)) = doc.view.snapped
        && let (Some(g), Some(xf)) = (s.glyph, ix.xf(page))
        && (doc.view.draft.is_some() || doc.view.gesture.is_some() || cx.tool.draws())
    {
        snapping::paint_glyph(
            ix.painter,
            xf.to_screen(s.pt),
            g,
            snapping::indicator_color(cx.snaps.color),
        );
    }
}

// ---- snapping --------------------------------------------------------------------------------

/// `raw` snapped with the status-bar toggles; `exclude` is the markup being reshaped.
fn snap_at(
    ix: &Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    page: usize,
    raw: Point,
    exclude: Option<&str>,
) -> Snapped {
    let Some(xf) = ix.xf(page) else {
        return Snapped::raw(raw);
    };
    let s = cx.snaps;
    // Holding Ctrl places the point exactly where the pointer is (Revu: Ctrl overrides snap).
    let ctrl = ix.ui.input(|i| i.modifiers.command);
    if ctrl || !(s.grid || s.content || s.markup) {
        doc.view.snapped = None;
        crate::snapping_more::store(ix.ui.ctx(), None);
        return Snapped::raw(raw);
    }
    let reach = f64::from(snapping::reach() / xf.k.max(1e-6));
    let content = if s.content {
        let bytes = doc.bytes.clone();
        doc.snaps.index(page, &bytes)
    } else {
        None
    };
    let doc_ref = doc.session.doc();
    let markups: Vec<&Markup> = if s.markup {
        doc_ref
            .markups_on(page)
            .filter(|m| exclude != Some(m.id.as_str()))
            .take(20_000)
            .collect()
    } else {
        Vec::new()
    };
    let mut r = snapping::snap_point(raw, s, reach, content.as_deref(), &markups);
    // Snap to Markup's alignment guides when nothing nearer caught the point
    let guides = if s.markup && r.glyph.is_none() {
        crate::snapping_more::find(page, raw, reach, &markups)
    } else {
        None
    };
    if let Some(g) = &guides {
        r.pt = g.at;
    }
    crate::snapping_more::store(ix.ui.ctx(), guides);
    doc.view.snapped = Some((page, r));
    r
}

/// The pointer position for a drawing tool: snapped, or Shift-constrained from `from`.
fn tool_point(
    ix: &Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    page: usize,
    raw: Point,
    from: Option<Point>,
    square: bool,
) -> Point {
    let shift = ix.ui.input(|i| i.modifiers.shift);
    match from {
        Some(f) if shift => {
            doc.view.snapped = None;
            if square {
                tools::square(f, raw)
            } else {
                tools::constrain(f, raw)
            }
        }
        _ => snap_at(ix, doc, cx, page, raw, None).pt,
    }
}

// ---- drawing tools ---------------------------------------------------------------------------

pub(crate) fn pointer_frame(ix: &Input<'_>) -> (Option<Pos2>, Option<Pos2>, Option<Pos2>, bool, bool, Option<Pos2>) {
    let resp = ix.resp;
    let press = resp
        .drag_started_by(egui::PointerButton::Primary)
        .then(|| {
            ix.ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
        })
        .flatten();
    let click = resp
        .interact_pointer_pos()
        .filter(|_| resp.clicked_by(egui::PointerButton::Primary));
    let release = resp
        .drag_stopped_by(egui::PointerButton::Primary)
        .then(|| ix.ui.input(|i| i.pointer.latest_pos()))
        .flatten();
    // A quick double-click right after placing a point reads as a triple click.
    let dbl =
        resp.double_clicked_by(egui::PointerButton::Primary) || resp.triple_clicked_by(egui::PointerButton::Primary);
    let right = resp.secondary_clicked();
    let cur = ix.ui.input(|i| i.pointer.latest_pos());
    (press, click, release, dbl, right, cur)
}

fn draw_tool(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) {
    let tool = cx.tool;
    let (press, click, release, dbl, right, cur) = pointer_frame(ix);
    if let Some(s) = ix.resp.hover_pos()
        && ix.page_at(s).is_some()
    {
        ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    // A draft from another tool (the tool changed) is dropped.
    if doc.view.draft.as_ref().is_some_and(|d| d.tool != tool.id) {
        doc.view.draft = None;
    }
    if doc.view.draft.as_ref().is_some_and(|d| d.stage == Stage::Typed) {
        finish_typed(doc, cx, out);
        return;
    }
    let k = keys(ix.ui, doc.view.draft.is_some());
    // Show where the first point would snap.
    if doc.view.draft.is_none() {
        match cur.and_then(|c| ix.page_at(c).map(|(p, xf)| (p, xf.to_user(c)))) {
            Some((page, at)) => {
                snap_at(ix, doc, cx, page, at, None);
            }
            None => doc.view.snapped = None,
        }
    }

    // Drawing mode: a click places a copy of the Tool Chest item.
    if cx.drawing_mode
        && let Some(t) = cx.template
    {
        if let Some(s) = click
            && let Some((page, xf)) = ix.page_at(s)
        {
            let at = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
            let mut m = crate::chest::place_copy(t, page, at);
            if let Some(set_scale) = &cx.edit.item_scale
                && let Some(page_scale) = doc.session.doc().pages.get(page).and_then(|p| p.scale_at(at))
            {
                crate::chest_sets::rescale_copy(&mut m, set_scale, page_scale);
            }
            add_markup(doc, tool, m, None, out);
        }
        return;
    }

    match tool.kind {
        ToolKind::Points {
            kind,
            closed,
            finish_at,
            role,
        } => points_tool(
            ix,
            doc,
            cx,
            out,
            (kind, closed, finish_at, role),
            (press, click, release, dbl, right, cur),
            &k,
        ),
        ToolKind::Drag { kind, shape, role } => {
            drag_tool(ix, doc, cx, out, (kind, shape, role), (press, click, release, cur));
        }
        ToolKind::Box(kind) => {
            if two_click_anchor(ix, doc, cx, click) {
                return;
            }
            if let Some(s) = press
                && let Some((page, xf)) = ix.page_at(s)
            {
                let a = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
                doc.view.draft = Some(Draft {
                    tool: tool.id,
                    page,
                    pts: vec![a],
                    stage: Stage::Dragging,
                });
            }
            let Some(d) = doc.view.draft.clone() else { return };
            let Some(xf) = ix.xf(d.page) else { return };
            let Some(cur) = release.or(cur) else { return };
            let a = d.pts.first().copied().unwrap_or_default();
            let b = tool_point(ix, doc, cx, d.page, xf.to_user(cur), Some(a), true);
            // Alt: the first point is the centre.
            let (a, b) = crate::modkeys::from_center(ix.ui.input(|i| i.modifiers.alt), a, b);
            let m = tools::new_markup(kind, d.page, &[a, b]);
            if release.is_some() {
                doc.view.draft = None;
                if let Some(m) = m {
                    if kind == Kind::Snapshot {
                        snapshot_to_clipboard(doc, m, out);
                    } else {
                        add_markup(doc, tool, styled(m, cx), None, out);
                    }
                }
            } else if let Some(m) = m {
                preview(ix, d.page, &styled(m, cx));
            }
        }
        ToolKind::Freehand(kind) => {
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
            let Some(d) = doc.view.draft.as_mut() else { return };
            let Some(xf) = ix.xf(d.page) else { return };
            if let Some(c) = release.or(cur) {
                let p = xf.to_user(c);
                let far = d.pts.last().is_none_or(|l| xf.to_screen(*l).distance(c) >= 1.5);
                if far && d.pts.len() < 100_000 {
                    d.pts.push(p);
                }
            }
            let d = d.clone();
            let m = tools::new_markup(kind, d.page, &d.pts).map(|m| styled(m, cx));
            if release.is_some() {
                doc.view.draft = None;
                if let Some(m) = m {
                    add_markup(doc, tool, m, None, out);
                }
            } else if let Some(m) = m {
                preview(ix, d.page, &m);
            }
        }
        ToolKind::Text(kind) => text_tool(ix, doc, cx, out, kind, (press, click, release, cur)),
        ToolKind::Stamp => {
            let place = |doc: &mut DocTab, page: usize, r: URect, out: &mut CanvasOut| {
                if let Some(m) = stamp_markup(cx, page, r) {
                    add_markup(doc, tool, m, None, out);
                }
            };
            if let Some(s) = click
                && let Some((page, xf)) = ix.page_at(s)
            {
                let at = xf.to_user(s);
                let (w, h) = stamp_size(cx);
                place(
                    doc,
                    page,
                    URect::new(at.x - w / 2.0, at.y - h / 2.0, at.x + w / 2.0, at.y + h / 2.0),
                    out,
                );
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
            let (Some(xf), Some(c)) = (ix.xf(d.page), release.or(cur)) else {
                return;
            };
            let a = d.pts.first().copied().unwrap_or_default();
            let b = xf.to_user(c);
            let r = URect::new(a.x, a.y, b.x, b.y).normalized();
            if release.is_some() {
                doc.view.draft = None;
                if r.width() > 4.0 && r.height() > 4.0 {
                    place(doc, d.page, r, out);
                }
            } else if let Some(m) = stamp_markup(cx, d.page, r) {
                preview(ix, d.page, &m);
            }
        }
        ToolKind::Note => {
            if let Some(s) = click
                && let Some((page, xf)) = ix.page_at(s)
                && let Some(m) = tools::new_markup(Kind::Note, page, &[xf.to_user(s)])
            {
                open_new_editor(doc, tool, styled(m, cx), None);
            }
        }
        ToolKind::TextMarkup(kind) => {
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
            if let Some(s) = click
                && let Some((page, xf)) = ix.page_at(s)
            {
                let at = xf.to_user(s);
                let quads = text_quads(doc, xf, page, at, at, true);
                if let Some(m) = tools::new_markup(kind, page, &quads) {
                    add_markup(doc, tool, styled(m, cx), None, out);
                } else {
                    out.status = Some("No text here to mark".into());
                }
            }
            let Some(d) = doc.view.draft.clone() else { return };
            let (Some(xf), Some(c)) = (ix.xf(d.page), release.or(cur)) else {
                return;
            };
            let a = d.pts.first().copied().unwrap_or_default();
            let b = xf.to_user(c);
            let quads = text_quads(doc, xf, d.page, a, b, false);
            let m = tools::new_markup(kind, d.page, &quads).map(|m| styled(m, cx));
            if release.is_some() {
                doc.view.draft = None;
                if let Some(m) = m {
                    add_markup(doc, tool, m, None, out);
                }
            } else if let Some(m) = m {
                preview(ix, d.page, &m);
            }
        }
        ToolKind::Special(s) => crate::gestures::run(ix, doc, cx, out, s, (press, click, release, cur)),
        ToolKind::Select | ToolKind::Pan => {}
    }
}

/// The markup with the active template's look (Tool Chest item or the tool's default).
pub(crate) fn styled(mut m: Markup, cx: &CanvasCx<'_>) -> Markup {
    if let Some(t) = cx.template
        && t.kind == m.kind
    {
        tools::apply_look(t, &mut m);
    }
    crate::more::keep_subject(&mut m, cx);
    m
}

pub(crate) fn preview(ix: &Input<'_>, page: usize, m: &Markup) {
    if let Some(xf) = ix.xf(page) {
        painter::paint_markup(ix.painter, xf, m);
    }
}

type Frame = (Option<Pos2>, Option<Pos2>, Option<Pos2>, bool, bool, Option<Pos2>);

fn points_tool(
    ix: &mut Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    out: &mut CanvasOut,
    spec: (Kind, bool, usize, Role),
    frame: Frame,
    k: &Keys,
) {
    let (kind, closed, finish_at, role) = spec;
    let (press, click, release, dbl, right, cur) = frame;
    let tool = cx.tool;

    // Cloud+: after the cloud, a click places the callout.
    if let Some(Draft {
        stage: Stage::CloudPlus { cloud },
        page,
        ..
    }) = doc.view.draft.clone()
    {
        if let Some(s) = click.or(press)
            && let Some(xf) = ix.xf(page)
        {
            let at = xf.to_user(s);
            let tip = doc
                .session
                .doc()
                .find(&cloud)
                .and_then(|c| measure_extras::nearest_segment(&c.pts, true, at, f64::MAX).map(|(_, p)| p))
                .unwrap_or(at);
            let (w, h) = CALLOUT_BOX;
            let a = Point::new(at.x - w / 2.0, at.y + h / 2.0);
            let b = Point::new(at.x + w / 2.0, at.y - h / 2.0);
            doc.view.draft = None;
            if let Some(m) = tools::new_markup(Kind::Callout, page, &[tip, a, b]) {
                open_new_editor(doc, tool, styled(m, cx), Some(cloud));
            }
        } else if let (Some(c), Some(xf)) = (cur, ix.xf(page)) {
            ix.painter.text(
                c + vec2(14.0, 14.0),
                egui::Align2::LEFT_TOP,
                "Click where the callout text goes",
                FontId::proportional(12.0),
                Color32::BLACK,
            );
            let _ = xf;
        }
        return;
    }

    // Cloud: a drag draws a rectangle cloud, clicks a polygon one.
    if kind == Kind::Cloud && role == Role::Markup && crate::more::cloud_drag(ix, doc, cx, out, (press, release, cur)) {
        return;
    }
    // Two-point tools: press, drag, release.
    if finish_at == 2
        && let Some(s) = press
        && doc.view.draft.as_ref().is_none_or(|d| d.pts.is_empty())
        && let Some((page, xf)) = ix.page_at(s)
    {
        let a = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
        doc.view.draft = Some(Draft {
            tool: tool.id,
            page,
            pts: vec![a],
            stage: Stage::Dragging,
        });
    } else if let Some(s) = press.filter(|_| finish_at != 2) {
        // A drag on a many-point tool places a point where it started.
        add_point(ix, doc, cx, tool, s);
    }
    // Clicking the first point again closes a shape.
    if let Some(s) = click
        && closed
        && let Some(d) = doc.view.draft.as_ref()
        && d.pts.len() >= 3
        && let (Some(xf), Some(first)) = (ix.xf(d.page), d.pts.first())
        && xf.to_screen(*first).distance(s) <= 8.0
    {
        finish(doc, cx, out, spec);
        return;
    }
    if let Some(s) = click {
        let before = doc.view.draft.as_ref().map_or(0, |d| d.pts.len());
        if before == 0 {
            doc.view.arc_through.clear();
        }
        add_point(ix, doc, cx, tool, s);
        let after = doc.view.draft.as_ref().map_or(0, |d| d.pts.len());
        if after > before && finish_at == 0 && ix.ui.input(|i| i.modifiers.alt) {
            // Alt+click: an arc passes through this point to the next one.
            doc.view.arc_through.push(after - 1);
        }
        if finish_at > 0 && after >= finish_at && after > before {
            finish(doc, cx, out, spec);
            return;
        }
    }
    // The release of a drag places the second point of a two-point tool (also after a click
    // placed the first).
    if let Some(d) = doc.view.draft.as_ref()
        && (d.stage == Stage::Dragging || (finish_at == 2 && d.pts.len() == 1))
        && let Some(r) = release
        && let Some(xf) = ix.xf(d.page)
    {
        let (page, a) = (d.page, d.pts.first().copied().unwrap_or_default());
        let b = tool_point(ix, doc, cx, page, xf.to_user(r), Some(a), false);
        if let Some(d) = doc.view.draft.as_mut() {
            d.pts.push(b);
            d.stage = Stage::Points;
        }
        finish(doc, cx, out, spec);
        return;
    }
    if k.backspace
        && let Some(d) = doc.view.draft.as_mut()
    {
        d.pts.pop();
        if d.pts.is_empty() {
            doc.view.draft = None;
        }
    }
    if (k.enter || dbl || right) && doc.view.draft.is_some() {
        finish(doc, cx, out, spec);
        return;
    }

    // The draft so far, plus the cursor, with the live value.
    let Some(d) = doc.view.draft.clone() else { return };
    let Some(xf) = ix.xf(d.page) else { return };
    let cursor = cur
        .filter(|c| xf.rect.expand(40.0).contains(*c))
        .map(|c| tool_point(ix, doc, cx, d.page, xf.to_user(c), d.pts.last().copied(), false));
    let mut pts = d.pts.clone();
    if let Some(c) = cursor
        && kind != Kind::Count
    {
        pts.push(c);
    }
    let shown = if role == Role::Calibrate || role == Role::Cutout {
        let mut m = Markup::new(
            if role == Role::Cutout {
                Kind::Polygon
            } else {
                Kind::Line
            },
            d.page,
            pts.clone(),
        );
        m.color = markupcraft_model::Color::rgb(0.88, 0.06, 0.75);
        m.dash = vec![4.0, 3.0];
        Some(m)
    } else {
        tools::new_markup(kind, d.page, &tools::role_points(role, &pts)).map(|mut m| {
            m = styled(m, cx);
            if kind.is_measurement() && kind != Kind::Count {
                let first = pts.first().copied().unwrap_or_default();
                m.scale = doc
                    .session
                    .doc()
                    .pages
                    .get(d.page)
                    .and_then(|p| p.scale_at(first))
                    .cloned();
            }
            m
        })
    };
    if let Some(m) = &shown {
        painter::paint_markup(ix.painter, xf, m);
    } else if pts.len() >= 2 {
        let line: Vec<Pos2> = pts.iter().map(|p| xf.to_screen(*p)).collect();
        ix.painter.add(egui::Shape::line(line, Stroke::new(1.5, Color32::RED)));
    }
    for p in &d.pts {
        ix.painter
            .circle_stroke(xf.to_screen(*p), 3.0, Stroke::new(1.5, ix.tokens.select));
    }
    if kind == Kind::Count {
        crate::features::partials_more2::count_readout(
            ix.painter.ctx(),
            ix.resp.rect,
            d.pts.len(),
            cx.edit.more.resume_count.is_some(),
        );
    }
    let readout = live_readout(doc, d.page, kind, role, &tools::role_points(role, &pts));
    if let (Some(text), Some(c)) = (readout, cur) {
        let galley = ix
            .painter
            .layout_no_wrap(text.clone(), FontId::proportional(12.0), Color32::BLACK);
        let r = Rect::from_min_size(c + vec2(16.0, 16.0), galley.size()).expand2(vec2(4.0, 2.0));
        ix.painter.rect_filled(r, 2.0, Color32::from_rgb(255, 255, 220));
        ix.painter.rect_stroke(
            r,
            2.0,
            Stroke::new(1.0, Color32::from_gray(150)),
            egui::StrokeKind::Inside,
        );
        ix.painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::BLACK);
        out.status = Some(text);
    }
}

/// The value under construction: the measurement, or the paper length while calibrating.
fn live_readout(doc: &DocTab, page: usize, kind: Kind, role: Role, pts: &[Point]) -> Option<String> {
    if role == Role::Calibrate {
        let (a, b) = (pts.first()?, pts.get(1)?);
        let inches = a.dist(*b) / 72.0;
        return Some(format!("{inches:.3} in on the sheet"));
    }
    if !kind.is_measurement() {
        return None;
    }
    if kind == Kind::Count {
        return Some(format!("Count: {}", pts.len()));
    }
    let first = *pts.first()?;
    let scale = doc.session.doc().pages.get(page)?.scale_at(first).cloned();
    let Some(scale) = scale else {
        return Some("No scale: calibrate this page (Measure > Calibrate)".into());
    };
    let mut m = Markup::new(kind, page, pts.to_vec());
    m.scale = Some(scale);
    let q = m.quantity_text();
    if q.is_empty() {
        return None;
    }
    let what = match role {
        Role::Cutout => "Cutout",
        _ => kind.name(),
    };
    Some(format!("{what}: {q}"))
}

fn add_point(ix: &Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, tool: &'static ToolDef, s: Pos2) {
    let page = match doc.view.draft.as_ref() {
        Some(d) => d.page,
        None => match ix.page_at(s) {
            Some((p, _)) => p,
            None => return,
        },
    };
    let Some(xf) = ix.xf(page) else { return };
    let last = doc.view.draft.as_ref().and_then(|d| d.pts.last().copied());
    let p = tool_point(ix, doc, cx, page, xf.to_user(s), last, false);
    match doc.view.draft.as_mut() {
        Some(d) if d.pts.len() < markupcraft_engine::geometry::MAX_POINTS => d.pts.push(p),
        Some(_) => {}
        None => {
            doc.view.draft = Some(Draft {
                tool: tool.id,
                page,
                pts: vec![p],
                stage: Stage::Points,
            });
        }
    }
}

/// Finish a points draft.
fn finish(doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut, spec: (Kind, bool, usize, Role)) {
    let (kind, _, _, role) = spec;
    let Some(d) = doc.view.draft.take() else { return };
    let tool = cx.tool;
    if crate::more::finish_resumed(doc, cx, (kind, d.page), &d.pts, out) {
        return;
    }
    match role {
        Role::Calibrate => {
            if let (Some(a), Some(b)) = (d.pts.first(), d.pts.get(1))
                && a.dist(*b) > 0.5
            {
                out.calibrate = Some((d.page, *a, *b));
            } else {
                out.status = Some("Calibrate: click two points that are apart".into());
            }
            out.done = true;
        }
        Role::Cutout => {
            let ring: Vec<Point> = tools::new_markup(Kind::Polygon, d.page, &d.pts)
                .map(|m| m.pts)
                .unwrap_or_default();
            add_cutout_ring(doc, d.page, ring, out);
        }
        Role::CloudPlus => {
            if let Some(m) = tools::new_markup(kind, d.page, &d.pts)
                && let Some(id) = add_markup(doc, tool, styled(m, cx), None, out)
            {
                // Cloud+ goes on to its callout.
                out.created = None;
                doc.view.draft = Some(Draft {
                    tool: tool.id,
                    page: d.page,
                    pts: Vec::new(),
                    stage: Stage::CloudPlus { cloud: id },
                });
                out.status = Some("Click where the callout text goes".into());
            }
        }
        Role::Viewport => {}
        Role::Markup | Role::Radius3 | Role::Arc3 => {
            let through = std::mem::take(&mut doc.view.arc_through);
            match tools::new_markup(kind, d.page, &tools::role_points(role, &d.pts)) {
                Some(mut m) => {
                    if role == Role::Markup && m.pts.len() == d.pts.len() {
                        markupcraft_engine::extras6::arcs_through(&mut m, &through);
                    }
                    add_markup(doc, tool, styled(m, cx), None, out);
                }
                None => out.status = Some(format!("{}: not enough points", tool.label)),
            }
        }
    }
}

/// Cut `ring` out of the Area (or Volume) on `page` its first point is inside (the selected
/// one first).
fn add_cutout_ring(doc: &mut DocTab, page: usize, ring: Vec<Point>, out: &mut CanvasOut) {
    let Some(first) = ring.first().copied().filter(|_| ring.len() >= 3) else {
        out.status = Some("A cutout needs at least three points".into());
        return;
    };
    let sel = doc.session.selection().to_vec();
    let area = doc
        .session
        .doc()
        .markups_on(page)
        .filter(|m| measure_extras::can_have_cutouts(m.kind))
        .filter(|m| markupcraft_geom::point_in_polygon(first, &m.pts))
        .max_by_key(|m| sel.contains(&m.id))
        .map(|m| m.id.clone());
    out.status = Some(match area {
        Some(id) => actions::report(doc.session.add_cutout(&id, ring), |_| "Added a cutout".into()),
        None => "Draw the cutout inside an Area measurement".into(),
    });
}

/// Box and drag tools: a click (no drag) sets the first corner, the next click (or a Sketch to
/// Scale size) the opposite one. Returns whether the click was taken.
fn two_click_anchor(ix: &Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, click: Option<Pos2>) -> bool {
    let Some(s) = click else { return false };
    match doc.view.draft.clone() {
        Some(d) if d.stage == Stage::Points && d.pts.len() == 1 => {
            if let Some(xf) = ix.xf(d.page) {
                let a = d.pts.first().copied().unwrap_or_default();
                let b = tool_point(ix, doc, cx, d.page, xf.to_user(s), Some(a), true);
                if let Some(dr) = doc.view.draft.as_mut() {
                    dr.pts.push(b);
                    dr.stage = Stage::Typed;
                }
            }
            true
        }
        None => {
            let Some((page, xf)) = ix.page_at(s) else { return false };
            let a = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
            doc.view.draft = Some(Draft {
                tool: cx.tool.id,
                page,
                pts: vec![a],
                stage: Stage::Points,
            });
            true
        }
        Some(_) => false,
    }
}

/// Area by rectangle, Ellipse Cutout, Add Viewport: press, drag, release (or two clicks).
fn drag_tool(
    ix: &mut Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    out: &mut CanvasOut,
    spec: (Kind, DragShape, Role),
    frame: (Option<Pos2>, Option<Pos2>, Option<Pos2>, Option<Pos2>),
) {
    let (kind, shape, role) = spec;
    let (press, click, release, cur) = frame;
    if two_click_anchor(ix, doc, cx, click) {
        return;
    }
    if let Some(s) = press
        && let Some((page, xf)) = ix.page_at(s)
    {
        let a = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
        doc.view.draft = Some(Draft {
            tool: cx.tool.id,
            page,
            pts: vec![a],
            stage: Stage::Dragging,
        });
    }
    let Some(d) = doc.view.draft.clone() else { return };
    let (Some(xf), Some(c)) = (ix.xf(d.page), release.or(cur)) else {
        return;
    };
    let a = d.pts.first().copied().unwrap_or_default();
    let b = tool_point(ix, doc, cx, d.page, xf.to_user(c), Some(a), true);
    if release.is_some() && d.stage == Stage::Dragging {
        if let Some(dr) = doc.view.draft.as_mut() {
            dr.pts = vec![a, b];
        }
        finish_drag(doc, cx, out, spec);
        return;
    }
    let Some(ring) = tools::drag_ring(shape, a, b) else {
        return;
    };
    let mut m = Markup::new(
        if role == Role::Markup { kind } else { Kind::Polygon },
        d.page,
        ring.clone(),
    );
    if role == Role::Markup {
        if let Some(n) = tools::new_markup(kind, d.page, &ring) {
            m = styled(n, cx);
            m.scale = doc.session.doc().pages.get(d.page).and_then(|p| p.scale_at(a)).cloned();
        }
    } else {
        m.color = markupcraft_model::Color::rgb(0.88, 0.06, 0.75);
        m.dash = vec![4.0, 3.0];
    }
    preview(ix, d.page, &m);
    if let Some(text) = live_readout(doc, d.page, kind, role, &ring) {
        out.status = Some(text);
    }
}

/// Finish a drag tool whose draft holds its two corners.
fn finish_drag(doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut, spec: (Kind, DragShape, Role)) {
    let (kind, shape, role) = spec;
    let Some(d) = doc.view.draft.take() else { return };
    let (Some(a), Some(b)) = (d.pts.first().copied(), d.pts.get(1).copied()) else {
        return;
    };
    let Some(ring) = tools::drag_ring(shape, a, b) else {
        out.status = Some(format!("{}: drag a larger box", cx.tool.label));
        return;
    };
    match role {
        Role::Cutout => add_cutout_ring(doc, d.page, ring, out),
        Role::Viewport => {
            let r = URect::new(a.x, a.y, b.x, b.y).normalized();
            out.actions.push(crate::canvas::CanvasAction::NewViewport(d.page, r));
            out.done = true;
        }
        _ => match tools::new_markup(kind, d.page, &ring) {
            Some(m) => {
                add_markup(doc, cx.tool, styled(m, cx), None, out);
            }
            None => out.status = Some(format!("{}: drag a larger box", cx.tool.label)),
        },
    }
}

/// A draft Sketch to Scale completed: finish it like the tool would.
fn finish_typed(doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) {
    match cx.tool.kind {
        ToolKind::Points {
            kind,
            closed,
            finish_at,
            role,
        } => finish(doc, cx, out, (kind, closed, finish_at, role)),
        ToolKind::Drag { kind, shape, role } => finish_drag(doc, cx, out, (kind, shape, role)),
        ToolKind::Box(kind) => {
            let Some(d) = doc.view.draft.take() else { return };
            match tools::new_markup(kind, d.page, &d.pts) {
                Some(m) if kind == Kind::Snapshot => snapshot_to_clipboard(doc, m, out),
                Some(m) => {
                    add_markup(doc, cx.tool, styled(m, cx), None, out);
                }
                None => out.status = Some(format!("{}: the size is too small", cx.tool.label)),
            }
        }
        _ => doc.view.draft = None,
    }
}

/// Esc while drawing: Count keeps what it counted; other drafts are dropped. Returns whether
/// there was a draft or an editor.
pub fn escape(doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) -> bool {
    if doc.view.editor.is_some() {
        commit_editor(doc, out);
        return true;
    }
    let Some(d) = doc.view.draft.as_ref() else { return false };
    if let ToolKind::Points {
        kind: Kind::Count,
        closed,
        finish_at,
        role,
    } = cx.tool.kind
        && !d.pts.is_empty()
    {
        finish(doc, cx, out, (Kind::Count, closed, finish_at, role));
    }
    doc.view.draft = None;
    true
}

/// Add a markup drawn with `tool`; returns its id.
pub(crate) fn add_markup(
    doc: &mut DocTab,
    tool: &'static ToolDef,
    mut m: Markup,
    group_with: Option<String>,
    out: &mut CanvasOut,
) -> Option<String> {
    if m.kind.is_text() {
        markupcraft_revu::kinds::text::autosize_text_box(&mut m);
    }
    crate::gestures::prepare(doc, &mut m);
    match doc.session.add_markup(m.clone()) {
        Ok(id) => {
            if let Some(g) = group_with {
                let _ = doc.session.group(&[g, id.clone()]);
                actions::select(&mut doc.session, vec![id.clone()]);
            }
            out.status = Some(format!("Added {}", tool.label));
            out.created = Some((tool.id, Markup { id: id.clone(), ..m }));
            Some(id)
        }
        Err(e) => {
            out.status = Some(e.to_string());
            None
        }
    }
}

fn snapshot_to_clipboard(doc: &mut DocTab, m: Markup, out: &mut CanvasOut) {
    doc.session.set_clipboard(vec![m]);
    out.status = Some("Snapshot copied: Ctrl+V pastes it where the pointer is".into());
    out.done = true;
}

// ---- stamps ----------------------------------------------------------------------------------

fn design(cx: &CanvasCx<'_>) -> &'static markupcraft_revu::kinds::draw::StampDesign {
    markupcraft_revu::kinds::draw::find_stamp(cx.stamp)
        .or(markupcraft_revu::kinds::draw::STAMP_DESIGNS.first())
        .unwrap_or(&FALLBACK_STAMP)
}

static FALLBACK_STAMP: markupcraft_revu::kinds::draw::StampDesign = markupcraft_revu::kinds::draw::StampDesign {
    id: "Draft",
    text: "DRAFT",
    color: markupcraft_model::Color::rgb(0.35, 0.35, 0.4),
    pdf_name: "Draft",
};

fn stamp_size(cx: &CanvasCx<'_>) -> (f64, f64) {
    let d = design(cx);
    let mut style = markupcraft_model::TextStyle {
        bold: true,
        size: 20.0,
        ..Default::default()
    };
    style.font = "Helvetica".into();
    let w = text_width(d.text, &font_of(&style)) + 36.0;
    (w.max(120.0), 54.0)
}

/// `D:20261009...` -> `2026-10-09`.
pub fn today() -> String {
    let d = markupcraft_revu::pdf_date_now();
    let s = d.strip_prefix("D:").unwrap_or(&d);
    match (s.get(0..4), s.get(4..6), s.get(6..8)) {
        (Some(y), Some(m), Some(dd)) => format!("{y}-{m}-{dd}"),
        _ => String::new(),
    }
}

fn stamp_markup(cx: &CanvasCx<'_>, page: usize, r: URect) -> Option<Markup> {
    let d = design(cx);
    let mut m = tools::new_markup(Kind::Stamp, page, &[Point::new(r.x0, r.y0), Point::new(r.x1, r.y1)])?;
    m = styled(m, cx);
    m.stamp = d.id.into();
    m.subject = "Stamp".into();
    m.color = d.color;
    m.text.color = d.color;
    m.text.bold = true;
    m.contents = format!("{}\r{}  {}", d.text, cx.author, today());
    Some(m)
}

// ---- text tools ------------------------------------------------------------------------------

fn text_tool(
    ix: &mut Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    out: &mut CanvasOut,
    kind: Kind,
    frame: (Option<Pos2>, Option<Pos2>, Option<Pos2>, Option<Pos2>),
) {
    let (press, click, release, cur) = frame;
    let tool = cx.tool;
    let _ = out;
    let box_at = |at: Point, (w, h): (f64, f64)| [Point::new(at.x, at.y), Point::new(at.x + w, at.y - h)];
    if kind == Kind::Callout {
        // The tip first (a click, or where a drag starts), then where the box goes.
        let tip_stage = doc
            .view
            .draft
            .as_ref()
            .filter(|d| d.stage == Stage::CalloutTip)
            .cloned();
        if let Some(d) = tip_stage {
            if let Some(s) = click
                && let Some(xf) = ix.xf(d.page)
            {
                let at = xf.to_user(s);
                place_callout(doc, cx, d.page, d.pts.first().copied().unwrap_or(at), at);
            } else if let (Some(c), Some(xf)) = (cur, ix.xf(d.page))
                && let Some(tip) = d.pts.first()
            {
                ix.painter
                    .line_segment([xf.to_screen(*tip), c], Stroke::new(1.5, Color32::RED));
            }
            return;
        }
        if let Some(s) = click
            && let Some((page, xf)) = ix.page_at(s)
        {
            let tip = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
            doc.view.draft = Some(Draft {
                tool: tool.id,
                page,
                pts: vec![tip],
                stage: Stage::CalloutTip,
            });
        }
        if let Some(s) = press
            && let Some((page, xf)) = ix.page_at(s)
        {
            let tip = snap_at(ix, doc, cx, page, xf.to_user(s), None).pt;
            doc.view.draft = Some(Draft {
                tool: tool.id,
                page,
                pts: vec![tip],
                stage: Stage::Dragging,
            });
        }
        if let Some(d) = doc.view.draft.clone().filter(|d| d.stage == Stage::Dragging)
            && let Some(xf) = ix.xf(d.page)
            && let Some(c) = release.or(cur)
            && let Some(tip) = d.pts.first().copied()
        {
            if release.is_some() {
                place_callout(doc, cx, d.page, tip, xf.to_user(c));
            } else {
                ix.painter
                    .line_segment([xf.to_screen(tip), c], Stroke::new(1.5, Color32::RED));
            }
        }
        return;
    }
    if let Some(s) = click
        && let Some((page, xf)) = ix.page_at(s)
    {
        let at = xf.to_user(s);
        let size = if kind == Kind::Typewriter {
            (60.0, 20.0)
        } else {
            TEXT_BOX
        };
        if let Some(m) = tools::new_markup(kind, page, &box_at(at, size)) {
            open_new_editor(doc, tool, styled(m, cx), None);
        }
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
    let (Some(xf), Some(c)) = (ix.xf(d.page), release.or(cur)) else {
        return;
    };
    let a = d.pts.first().copied().unwrap_or_default();
    let b = xf.to_user(c);
    let m = tools::new_markup(kind, d.page, &[a, b]).map(|m| styled(m, cx));
    if release.is_some() {
        doc.view.draft = None;
        if let Some(m) = m {
            open_new_editor(doc, tool, m, None);
        }
    } else if let Some(m) = m {
        preview(ix, d.page, &m);
    }
}

fn place_callout(doc: &mut DocTab, cx: &CanvasCx<'_>, page: usize, tip: Point, at: Point) {
    let (w, h) = CALLOUT_BOX;
    let a = Point::new(at.x - w / 2.0, at.y + h / 2.0);
    let b = Point::new(at.x + w / 2.0, at.y - h / 2.0);
    doc.view.draft = None;
    if let Some(m) = tools::new_markup(Kind::Callout, page, &[tip, a, b]) {
        open_new_editor(doc, cx.tool, styled(m, cx), None);
    }
}

pub(crate) fn open_new_editor(doc: &mut DocTab, tool: &'static ToolDef, markup: Markup, group_with: Option<String>) {
    let page = markup.page;
    doc.view.draft = None;
    doc.view.editor = Some(TextEditor {
        text: markup.contents.clone(),
        rich: markup.rich.clone(),
        target: EditTarget::New {
            markup: Box::new(markup),
            tool: tool.id,
            group_with,
        },
        page,
        pending: None,
        opened: false,
    });
}

/// Open the editor on an existing text markup or note.
pub fn edit_existing(doc: &mut DocTab, id: &str) -> bool {
    let Some(m) = doc.session.doc().find(id) else {
        return false;
    };
    if !(m.kind.is_text() || matches!(m.kind, Kind::Note | Kind::Caret)) || !editable(m) || m.locked() {
        return false;
    }
    doc.view.editor = Some(TextEditor {
        text: m.contents.replace('\r', "\n"),
        rich: m.rich.clone(),
        target: EditTarget::Existing(id.to_string()),
        page: m.page,
        pending: None,
        opened: false,
    });
    true
}

/// Where the editor sits (user space box) and the text style it shows.
fn editor_box(doc: &DocTab, ed: &TextEditor) -> Option<(URect, Markup)> {
    let m = match &ed.target {
        EditTarget::New { markup, .. } => (**markup).clone(),
        EditTarget::Existing(id) => doc.session.doc().find(id)?.clone(),
    };
    let b = if matches!(m.kind, Kind::Note | Kind::Caret) {
        let r = box_of(&m);
        URect::new(r.x1 + 4.0, r.y1 - 100.0, r.x1 + 224.0, r.y1)
    } else {
        box_of(&m)
    };
    Some((b, m))
}

fn editor_ui(ix: &mut Input<'_>, doc: &mut DocTab, out: &mut CanvasOut) {
    let Some(ed) = doc.view.editor.clone() else { return };
    let Some((b, m)) = editor_box(doc, &ed) else {
        doc.view.editor = None;
        return;
    };
    let Some(xf) = ix.xf(ed.page) else {
        // The page left the view: keep what was typed.
        commit_editor(doc, out);
        return;
    };
    let r = xf.rect_of(b);
    let size = if matches!(m.kind, Kind::Note | Kind::Caret) {
        13.0
    } else {
        xf.len(m.text.size).clamp(7.0, 72.0)
    };
    let color = if matches!(m.kind, Kind::Note | Kind::Caret) {
        Color32::BLACK
    } else {
        crate::theme::color32(&m.text.color, 1.0)
    };
    let mut text = ed.text.clone();
    let mut runs = ed.rich.clone();
    let mut pending = ed.pending;
    let id = egui::Id::new("markupcraft-text-editor");
    let edit_id = id.with("edit");
    let ctx = ix.ui.ctx().clone();
    crate::richedit::load_dictionary();
    let rich_ok = m.kind.is_text();
    // Ctrl+B / Ctrl+I / Ctrl+U style the selection, or with none the text typed next.
    if rich_ok {
        use markupcraft_model::rich::StyleChange;
        for (k, change) in [
            (egui::Key::B, StyleChange::Bold),
            (egui::Key::I, StyleChange::Italic),
            (egui::Key::U, StyleChange::Underline),
        ] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, k)) {
                runs = crate::richedit::restyle_or_pend(&ctx, edit_id, &text, &m.text, &runs, change, &mut pending);
            }
        }
    }
    // The toolbar above the editor: styles, colour, spelling suggestions.
    let mut keep_open = false;
    let bar = egui::Area::new(id.with("bar"))
        .fixed_pos(r.min - vec2(0.0, 30.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        use markupcraft_model::rich::StyleChange;
                        if rich_ok {
                            for (letter, change, tip) in [
                                ("B", StyleChange::Bold, "Bold (Ctrl+B)"),
                                ("I", StyleChange::Italic, "Italic (Ctrl+I)"),
                                ("U", StyleChange::Underline, "Underline (Ctrl+U)"),
                            ] {
                                let rt = match letter {
                                    "B" => egui::RichText::new(letter).strong(),
                                    "I" => egui::RichText::new(letter).italics(),
                                    _ => egui::RichText::new(letter).underline(),
                                };
                                if ui.button(rt).on_hover_text(tip).clicked() {
                                    runs = crate::richedit::restyle_or_pend(
                                        &ctx,
                                        edit_id,
                                        &text,
                                        &m.text,
                                        &runs,
                                        change,
                                        &mut pending,
                                    );
                                    keep_open = true;
                                }
                            }
                            for (name, c) in [
                                ("Red", markupcraft_model::Color::RED),
                                ("Black", markupcraft_model::Color::BLACK),
                                ("Blue", markupcraft_model::Color::rgb(0.0, 0.0, 1.0)),
                            ] {
                                let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::click());
                                ui.painter()
                                    .rect_filled(rect.shrink(2.0), 2.0, crate::theme::color32(&c, 1.0));
                                let resp = resp.on_hover_text(format!("{name} text"));
                                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
                                if resp.clicked() {
                                    runs = crate::richedit::restyle_or_pend(
                                        &ctx,
                                        edit_id,
                                        &text,
                                        &m.text,
                                        &runs,
                                        StyleChange::Color(c),
                                        &mut pending,
                                    );
                                    keep_open = true;
                                }
                            }
                        }
                        // Auto-complete from the managed list (General > Spelling).
                        let at = crate::richedit::cursor(&ctx, edit_id).unwrap_or(0);
                        if let Some((s, e, w)) = crate::spell_prefs::word_before(&text, at) {
                            let list = crate::spell_prefs::completions(&w);
                            if !list.is_empty() {
                                ui.separator();
                                for c in list {
                                    if ui.small_button(&c).on_hover_text("Complete").clicked() {
                                        let (t, r) = crate::richedit::replace(&text, &runs, s, e, &c);
                                        text = t;
                                        runs = r;
                                        crate::richedit::set_cursor(&ctx, edit_id, s + c.chars().count());
                                        keep_open = true;
                                    }
                                }
                            }
                        }
                        // Spelling: suggestions for the misspelled word at the cursor.
                        if let Some((s, e, word)) = crate::richedit::word_at(&text, at) {
                            ui.separator();
                            ui.label(egui::RichText::new(&word).color(Color32::from_rgb(200, 30, 30)));
                            for sug in crate::richedit::suggestions(&word) {
                                if ui.small_button(&sug).clicked() {
                                    let (t, r) = crate::richedit::replace(&text, &runs, s, e, &sug);
                                    text = t;
                                    runs = r;
                                    crate::richedit::set_cursor(&ctx, edit_id, s + sug.chars().count());
                                    keep_open = true;
                                }
                            }
                        }
                    });
                });
        });
    let bar_hovered = bar.response.contains_pointer()
        || ctx.layer_id_at(ctx.pointer_hover_pos().unwrap_or_default()) == Some(bar.response.layer_id);
    let before = text.clone();
    let base = m.text.clone();
    let layout_runs = runs.clone();
    let wrap = (r.width() - 6.0).max(40.0);
    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
        let job = crate::richedit::layout_job(buf.as_str(), &base, &layout_runs, size, wrap);
        let mut job = job;
        if !rich_ok {
            for s in &mut job.sections {
                s.format.color = color;
            }
        }
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let area = egui::Area::new(id)
        .fixed_pos(r.min)
        .order(egui::Order::Foreground)
        .show(ix.ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.5, ix.tokens.accent))
                .inner_margin(egui::Margin::same(2))
                .show(ui, |ui| {
                    ui.set_min_size(r.size().max(vec2(40.0, 20.0)));
                    egui::TextEdit::multiline(&mut text)
                        .id(edit_id)
                        .font(FontId::proportional(size))
                        .text_color(color)
                        .desired_width(wrap)
                        .desired_rows(1)
                        .frame(egui::Frame::NONE)
                        .layouter(&mut layouter)
                        .show(ui)
                        .response
                })
                .inner
        });
    let resp = area.inner;
    if text != before {
        runs = markupcraft_model::rich::rebase(&runs, &before, &text);
    }
    let cursor_now = crate::richedit::cursor(&ctx, edit_id);
    runs = crate::richedit::follow_pending(&before, &text, cursor_now, &m.text, runs, &mut pending);
    if keep_open || bar_hovered {
        ctx.memory_mut(|mem| mem.request_focus(edit_id));
    }
    if let Some(e) = doc.view.editor.as_mut() {
        e.text = text;
        e.rich = runs;
        e.pending = pending;
        if !e.opened {
            resp.request_focus();
            e.opened = true;
            // Typing continues the text.
            let edit_id = id.with("edit");
            if let Some(mut state) = egui::TextEdit::load_state(ix.ui.ctx(), edit_id) {
                let end = egui::text::CCursor::new(e.text.chars().count());
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                state.store(ix.ui.ctx(), edit_id);
            }
            return;
        }
    }
    if !(keep_open || bar_hovered) && (resp.lost_focus() || (!resp.has_focus() && !resp.gained_focus())) {
        commit_editor(doc, out);
    }
}

/// Close the editor, keeping its text (a new markup is added; an existing one changed).
pub fn commit_editor(doc: &mut DocTab, out: &mut CanvasOut) {
    let Some(ed) = doc.view.editor.take() else { return };
    let text = ed.text.trim_end_matches('\n').replace('\n', "\r");
    match ed.target {
        EditTarget::New {
            mut markup,
            tool,
            group_with,
        } => {
            if text.trim().is_empty() && markup.kind != Kind::Note {
                out.status = Some("Nothing was typed; no markup added".into());
                out.done = true;
                return;
            }
            let n = text.chars().count();
            markup.rich = markupcraft_model::rich::normalize(&markup.text, &ed.rich, n);
            markup.contents = text;
            if let Some(t) = tools::find(tool) {
                add_markup(doc, t, *markup, group_with, out);
            }
        }
        EditTarget::Existing(id) => {
            let Some(m) = doc.session.doc().find(&id).cloned() else {
                return;
            };
            let rich = markupcraft_model::rich::normalize(&m.text, &ed.rich, text.chars().count());
            if m.contents == text && m.rich == rich {
                return;
            }
            let mut sized = m.clone();
            sized.contents = text.clone();
            if sized.kind.is_text() {
                markupcraft_revu::kinds::text::autosize_text_box(&mut sized);
            }
            doc.session.set_merge_key(Some("edit-text"));
            let patch = markupcraft_engine::MarkupPatch {
                contents: Some(text),
                rich: m.kind.is_text().then_some(rich),
                ..Default::default()
            };
            let mut r = doc
                .session
                .set_properties(std::slice::from_ref(&id), &patch)
                .map(|_| ());
            if r.is_ok() && sized.pts != m.pts {
                r = doc.session.set_points(&id, sized.pts);
            }
            doc.session.set_merge_key(None);
            doc.session.seal();
            out.status = Some(actions::report(r, |_| "Text changed".into()));
        }
    }
}

// ---- text markups ----------------------------------------------------------------------------

/// QuadPoints over the page text between `a` and `b` (a click: the word at `a`). Without text
/// near the pointer, the dragged rectangle itself.
pub(crate) fn text_quads(doc: &mut DocTab, xf: &Xf, page: usize, a: Point, b: Point, word: bool) -> Vec<Point> {
    let to_view = |p: Point| {
        let v = xf.geom.user_to_view(p.x as f32, p.y as f32);
        (v[0], v[1])
    };
    let view_to_user = |x: f32, y: f32| {
        let u = xf.geom.view_to_user(x, y);
        Point::new(f64::from(u[0]), f64::from(u[1]))
    };
    let quad = |r: [f32; 4]| {
        let [x0, y0, x1, y1] = r;
        vec![
            view_to_user(x0, y0),
            view_to_user(x1, y0),
            view_to_user(x0, y1),
            view_to_user(x1, y1),
        ]
    };
    let text = doc.page_text(page);
    if let Some(t) = text.filter(|t| !t.is_empty()) {
        let (ax, ay) = to_view(a);
        let (bx, by) = to_view(b);
        let near = |x: f32, y: f32, i: usize| {
            t.glyphs.get(i).is_some_and(|g| {
                let dx = (g.rect[0] - x).max(x - g.rect[2]).max(0.0);
                let dy = (g.rect[1] - y).max(y - g.rect[3]).max(0.0);
                dx.hypot(dy) <= 12.0
            })
        };
        if let (Some(i), Some(j)) = (t.nearest(ax, ay), t.nearest(bx, by))
            && (near(ax, ay, i) || near(bx, by, j))
        {
            let (mut lo, mut hi) = (i.min(j), i.max(j));
            if word && let Some((w0, w1)) = t.word_at(i) {
                (lo, hi) = (w0, w1);
            }
            return t.line_rects(lo..hi + 1).into_iter().flat_map(quad).collect();
        }
    }
    if word {
        return Vec::new();
    }
    // No text: mark the dragged rectangle.
    let r = URect::new(a.x, a.y, b.x, b.y).normalized();
    if r.width() < 2.0 || r.height() < 2.0 {
        return Vec::new();
    }
    vec![
        Point::new(r.x0, r.y1),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
    ]
}

// ---- select tool -----------------------------------------------------------------------------

/// The topmost markup at `at` on `page`.
/// A click on a grouped markup selects its whole group (Revu); others select themselves.
fn group_of(doc: &DocTab, id: &str) -> Vec<String> {
    let d = doc.session.doc();
    match d.find(id).filter(|m| !m.group.is_empty()) {
        Some(m) => d
            .markups
            .iter()
            .filter(|o| o.page == m.page && o.group == m.group)
            .map(|o| o.id.clone())
            .collect(),
        None => vec![id.to_string()],
    }
}

pub fn top_hit(doc: &DocTab, page: usize, at: Point, tol: f64) -> Option<String> {
    doc.session
        .doc()
        .markups
        .iter()
        .rev()
        .filter(|m| m.page == page)
        .find(|m| painter::hit(doc.view.preview.get(&m.id).unwrap_or(m), at, tol))
        .map(|m| m.id.clone())
}

/// The measurement whose caption is under `s` (screen) on `page`.
fn caption_at(ix: &Input<'_>, doc: &DocTab, page: usize, s: Pos2) -> Option<String> {
    let xf = ix.xf(page)?;
    doc.session
        .doc()
        .markups
        .iter()
        .rev()
        .filter(|m| m.page == page && editable(m) && !m.locked())
        .find(|m| painter::caption_hit_rect(ix.painter, xf, m).is_some_and(|r| r.contains(s)))
        .map(|m| m.id.clone())
}

/// New geometry when handle `index` of `m` is dragged to `to`.
pub fn reshape(m: &Markup, index: usize, to: Point) -> Vec<Point> {
    if uses_rect(m.kind) && index < 4 {
        let old = box_of(m);
        let fixed = old
            .corners()
            .get((index + 2) % 4)
            .copied()
            .unwrap_or(Point::new(old.x0, old.y0));
        let new = URect::new(fixed.x, fixed.y, to.x, to.y).normalized();
        let mut pts = new.corners().to_vec();
        pts.extend(m.pts.iter().skip(4).copied());
        return pts;
    }
    // vertices move as the engine moves them (arcs bend through their handles)
    let mut c = m.clone();
    if markupcraft_engine::geometry::move_vertex(&mut c, index, to).is_ok() {
        return c.pts;
    }
    let mut pts = m.pts.clone();
    if let Some(p) = pts.get_mut(index) {
        *p = to;
    }
    pts
}

fn with_points(m: &Markup, pts: Vec<Point>) -> Markup {
    let mut c = m.clone();
    c.pts = pts;
    if let Some(b) = bbox(c.pts.get(..4.min(c.pts.len())).unwrap_or_default())
        && uses_rect(c.kind)
    {
        c.rect = b;
    }
    c
}

fn movable(m: &Markup) -> bool {
    editable(m) && !m.locked() && markupcraft_engine::props::geometry_editable(m)
}

fn select_tool(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) {
    let (press, click, release, dbl, _right, cur) = pointer_frame(ix);
    let mods = ix.ui.input(|i| i.modifiers);
    let add = mods.shift || mods.command;
    doc.view.draft = None;
    // A right-button drag draws a selection box too.
    if ix.resp.drag_started_by(egui::PointerButton::Secondary)
        && let Some(s) = ix.ui.input(|i| i.pointer.press_origin())
        && let Some((page, _)) = ix.page_at(s)
    {
        if !add {
            doc.session.clear_selection();
        }
        doc.view.gesture = Some(Gesture::Box { page, start: s });
    }
    let release = release.or_else(|| {
        ix.resp
            .drag_stopped_by(egui::PointerButton::Secondary)
            .then(|| ix.ui.input(|i| i.pointer.latest_pos()))
            .flatten()
    });

    // Format Painter: a click copies the look onto the markup under the pointer.
    if let Some(template) = cx.edit.painter.as_ref() {
        if let Some(s) = ix.resp.hover_pos()
            && ix.page_at(s).is_some()
        {
            ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Cell);
        }
        if let Some(s) = click.or(press)
            && let Some((page, xf)) = ix.page_at(s)
        {
            let tol = f64::from(PICK / xf.k.max(1e-6));
            if let Some(id) = top_hit(doc, page, xf.to_user(s), tol) {
                crate::editing::paint_format(doc, template, &id, out);
            }
        }
        return;
    }

    // The rotation handle of a single selected markup.
    if let Some(s) = press
        && let Some((id, page, pivot)) = rotate_handle_at(ix, doc, s)
        && let Some(xf) = ix.xf(page)
    {
        let at = xf.to_user(s);
        doc.view.gesture = Some(Gesture::Rotate {
            id,
            page,
            pivot,
            start: (at.y - pivot.y).atan2(at.x - pivot.x),
        });
        doc.view.preview.clear();
    }
    let rotating = matches!(doc.view.gesture, Some(Gesture::Rotate { .. }));
    let press = press.filter(|_| !rotating);

    // Press: a caption (Shift), a handle of the selection, a markup, else a box.
    if let Some(s) = press {
        doc.view.gesture = None;
        doc.view.preview.clear();
        match ix.page_at(s) {
            Some((page, xf)) => {
                let at = xf.to_user(s);
                let tol = f64::from(PICK / xf.k.max(1e-6));
                let selection = doc.session.selection().to_vec();
                let caption = if mods.shift { caption_at(ix, doc, page, s) } else { None };
                let handle = doc
                    .session
                    .doc()
                    .markups_on(page)
                    .filter(|m| selection.contains(&m.id) && movable(m))
                    .find_map(|m| {
                        painter::handles(m)
                            .iter()
                            .position(|h| xf.to_screen(*h).distance(s) <= HANDLE_REACH + 1.0)
                            .map(|i| (m.id.clone(), i))
                    });
                if let Some(id) = caption {
                    let from = doc
                        .session
                        .doc()
                        .find(&id)
                        .and_then(|m| m.caption_offset)
                        .unwrap_or_default();
                    doc.view.gesture = Some(Gesture::Caption {
                        id,
                        page,
                        start: at,
                        from,
                    });
                } else if let Some((id, _)) = handle
                    .as_ref()
                    .filter(|_| mods.alt)
                    .filter(|(id, _)| doc.session.doc().find(id).is_some_and(|m| m.kind == Kind::Callout))
                {
                    // Alt+drag a callout: it moves as a whole (box, leader and tip).
                    let id = id.clone();
                    if !selection.contains(&id) {
                        actions::select(&mut doc.session, vec![id]);
                    }
                    doc.view.gesture = Some(Gesture::Move {
                        page,
                        start: at,
                        copy: false,
                        straight: mods.shift,
                    });
                } else if let Some((id, index)) = handle {
                    doc.view.gesture = Some(Gesture::Handle { id, index, page });
                } else if mods.command
                    && !mods.shift
                    && let Some((id, index)) = crate::more::ctrl_curve(doc, page, at, tol)
                {
                    // Ctrl+drag a segment of the selected markup: it curves into an arc.
                    doc.view.gesture = Some(Gesture::Handle { id, index, page });
                } else if let Some(id) = top_hit(doc, page, at, tol) {
                    if !selection.contains(&id) {
                        let mut sel = if add { selection } else { Vec::new() };
                        sel.extend(group_of(doc, &id));
                        actions::select(&mut doc.session, sel);
                    }
                    doc.view.gesture = Some(Gesture::Move {
                        page,
                        start: at,
                        copy: mods.command,
                        straight: mods.shift,
                    });
                } else {
                    if !add {
                        doc.session.clear_selection();
                    }
                    doc.view.gesture = Some(Gesture::Box { page, start: s });
                }
            }
            None if !add => doc.session.clear_selection(),
            None => {}
        }
    }

    // Shift+click a vertex (delete) or a segment (add a vertex), Ctrl+click a vertex (arc or
    // straight) of the selected markup.
    let click = match click {
        Some(s) if doc.view.gesture.is_none() && (mods.shift || mods.command) => match ix.page_at(s) {
            Some((page, xf)) => {
                let reach = f64::from((HANDLE_REACH + 2.0) / xf.k.max(1e-6));
                match crate::modkeys::vertex_click(doc, page, xf.to_user(s), reach, mods.shift) {
                    Some(msg) => {
                        out.status = Some(msg);
                        None
                    }
                    None => click,
                }
            }
            None => click,
        },
        c => c,
    };
    // Click (no drag): select, add or toggle.
    if let Some(s) = click
        && doc.view.gesture.is_none()
    {
        match ix.page_at(s) {
            Some((page, xf)) => {
                let tol = f64::from(PICK / xf.k.max(1e-6));
                match top_hit(doc, page, xf.to_user(s), tol) {
                    Some(id) if add => actions::toggle_selected(&mut doc.session, &id),
                    Some(id) => {
                        let ids = group_of(doc, &id);
                        actions::select(&mut doc.session, ids);
                    }
                    None if !add => doc.session.clear_selection(),
                    None => {}
                }
            }
            None if !add => doc.session.clear_selection(),
            None => {}
        }
    }

    // Double-click a text markup or note: edit its text.
    if dbl
        && let Some(s) = ix.resp.interact_pointer_pos()
        && let Some((page, xf)) = ix.page_at(s)
    {
        let tol = f64::from(PICK / xf.k.max(1e-6));
        if let Some(id) = top_hit(doc, page, xf.to_user(s), tol)
            && edit_existing(doc, &id)
        {
            actions::select(&mut doc.session, vec![id]);
            return;
        }
    }

    // Drag in progress: live geometry.
    let gesture = doc.view.gesture.clone();
    let end = release.or(cur);
    match gesture {
        Some(Gesture::Move {
            page,
            start,
            copy,
            straight,
        }) => {
            if let (Some(xf), Some(c)) = (ix.xf(page), end) {
                let now = xf.to_user(c);
                let (mut dx, mut dy) = (now.x - start.x, now.y - start.y);
                if straight {
                    // Move (or copy) in a straight line.
                    if dx.abs() >= dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let ids: Vec<String> = doc
                    .session
                    .doc()
                    .markups
                    .iter()
                    .filter(|m| doc.session.selection().contains(&m.id) && (copy || movable(m)))
                    .map(|m| m.id.clone())
                    .collect();
                // Dragged off the canvas (onto the Tool Chest, say): the markups stay put.
                let off_canvas = release.is_some_and(|r| !ix.resp.rect.contains(r));
                if release.is_none() && !ids.is_empty() {
                    crate::chest_more::offer_drag(ix.ui.ctx(), doc.uid, &ids);
                }
                if release.is_some() {
                    doc.view.preview.clear();
                    if (dx != 0.0 || dy != 0.0) && !ids.is_empty() && !off_canvas {
                        let r = if copy {
                            doc.session.duplicate_markups(&ids, dx, dy).map(|v| v.len())
                        } else {
                            doc.session.move_markups(&ids, dx, dy)
                        };
                        out.status = Some(actions::report(r, |n| {
                            format!(
                                "{} {}",
                                if copy { "Copied" } else { "Moved" },
                                actions::plural(n, "markup")
                            )
                        }));
                    }
                } else {
                    doc.view.preview.clear();
                    for id in &ids {
                        if let Some(m) = doc.session.doc().find(id) {
                            let mut c = m.clone();
                            markupcraft_engine::geometry::translate(&mut c, dx, dy);
                            if copy {
                                painter::paint_markup(ix.painter, xf, &c);
                            } else {
                                doc.view.preview.insert(id.clone(), c);
                            }
                        }
                    }
                    ix.ui.ctx().set_cursor_icon(if copy {
                        egui::CursorIcon::Copy
                    } else {
                        egui::CursorIcon::Move
                    });
                }
            }
        }
        Some(Gesture::Handle { id, index, page }) => 'handle: {
            if let (Some(xf), Some(c), Some(m)) = (ix.xf(page), end, doc.session.doc().find(&id).cloned()) {
                let raw = xf.to_user(c);
                let anchor = if uses_rect(m.kind) {
                    None
                } else {
                    index.checked_sub(1).and_then(|i| m.pts.get(i).copied())
                };
                let shift = ix.ui.input(|i| i.modifiers.shift);
                if let Some(k) = crate::modkeys::corner_of(&m, index) {
                    // A corner of a polyline's or polygon's box: the whole shape scales, in
                    // proportion unless Shift is held.
                    let to = snap_at(ix, doc, cx, page, raw, Some(&id)).pt;
                    if let Some(r) = crate::modkeys::corner_box(&m, k, to, shift) {
                        if release.is_some() {
                            doc.view.preview.clear();
                            let res = doc.session.resize_markup(&id, r);
                            out.status = Some(actions::report(res, |_| "Resized".into()));
                        } else {
                            let mut c = m.clone();
                            if markupcraft_engine::geometry::resize(&mut c, r).is_ok() {
                                doc.view.preview.insert(id.clone(), c);
                            }
                        }
                    } else if release.is_some() {
                        doc.view.preview.clear();
                    }
                    break 'handle;
                }
                let to = match anchor {
                    Some(a) if shift => tools::constrain(a, raw),
                    _ => snap_at(ix, doc, cx, page, raw, Some(&id)).pt,
                };
                let mut pts = reshape(&m, index, to);
                if crate::modkeys::keeps_aspect(&m) && !shift {
                    // Images resize in proportion; Shift breaks the aspect ratio.
                    pts = crate::modkeys::keep_aspect(&m.pts, &pts);
                }
                if release.is_some() {
                    doc.view.preview.clear();
                    let r = if uses_rect(m.kind) {
                        doc.session.set_points(&id, pts)
                    } else {
                        doc.session.move_vertex(&id, index, to)
                    };
                    out.status = Some(actions::report(r, |_| "Reshaped".into()));
                } else if index >= m.pts.len() && !uses_rect(m.kind) {
                    // a cutout's vertex
                    let mut c = m.clone();
                    let _ = markupcraft_engine::geometry::move_vertex(&mut c, index, to);
                    doc.view.preview.insert(id.clone(), c);
                } else {
                    doc.view.preview.insert(id.clone(), with_points(&m, pts));
                }
            }
        }
        Some(Gesture::Caption { id, page, start, from }) => {
            if let (Some(xf), Some(c), Some(m)) = (ix.xf(page), end, doc.session.doc().find(&id).cloned()) {
                let now = xf.to_user(c);
                let off = Point::new(from.x + now.x - start.x, from.y + now.y - start.y);
                if release.is_some() {
                    doc.view.preview.clear();
                    let patch = markupcraft_engine::MarkupPatch {
                        caption_offset: Some(Some(off)),
                        ..Default::default()
                    };
                    let r = doc.session.set_properties(&[id], &patch);
                    out.status = Some(actions::report(r, |_| "Caption moved".into()));
                } else {
                    let mut p = m.clone();
                    p.caption_offset = Some(off);
                    doc.view.preview.insert(id, p);
                    ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                }
            }
        }
        Some(Gesture::Rotate { id, page, pivot, start }) => {
            if let (Some(xf), Some(c), Some(m)) = (ix.xf(page), end, doc.session.doc().find(&id).cloned()) {
                let now = xf.to_user(c);
                let deg = rotation_degrees(&m, start, (now.y - pivot.y).atan2(now.x - pivot.x), mods.shift);
                if release.is_some() {
                    doc.view.preview.clear();
                    if deg.abs() > 1e-9 {
                        let r = doc.session.rotate_markups(std::slice::from_ref(&id), deg, Some(pivot));
                        out.status = Some(actions::report(r, |_| format!("Rotated {deg:.0}\u{b0}")));
                    }
                } else {
                    let mut p = m.clone();
                    if markupcraft_engine::geometry::rotate(&mut p, deg, pivot).is_ok() {
                        doc.view.preview.insert(id, p);
                    }
                    out.status = Some(format!("Rotate {deg:.1}\u{b0}"));
                    ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
            }
        }
        Some(Gesture::Box { page, start }) => {
            if let Some(c) = end {
                let r = Rect::from_two_pos(start, c);
                ix.painter.rect_filled(r, 0.0, ix.tokens.select.gamma_multiply(0.12));
                ix.painter
                    .rect_stroke(r, 0.0, Stroke::new(1.0, ix.tokens.select), egui::StrokeKind::Middle);
                if release.is_some()
                    && let Some(xf) = ix.xf(page)
                {
                    let (a, b) = (xf.to_user(r.min), xf.to_user(r.max));
                    let ur = URect::new(a.x, a.y, b.x, b.y).normalized();
                    let mut sel = doc.session.selection().to_vec();
                    for m in doc.session.doc().markups_on(page) {
                        let mb = actions::markup_bbox(m);
                        let inside = mb.x0 >= ur.x0 && mb.x1 <= ur.x1 && mb.y0 >= ur.y0 && mb.y1 <= ur.y1;
                        if inside && !sel.contains(&m.id) {
                            sel.push(m.id.clone());
                        }
                    }
                    actions::select(&mut doc.session, sel);
                }
            }
        }
        None => {}
    }
    if release.is_some() {
        doc.view.gesture = None;
        doc.view.snapped = None;
    }

    paint_rotate_handle(ix, doc);

    // Hover cursor over markups and captions.
    if doc.view.gesture.is_none()
        && let Some(s) = ix.resp.hover_pos()
        && let Some((page, xf)) = ix.page_at(s)
    {
        let tol = f64::from(PICK / xf.k.max(1e-6));
        if mods.shift && caption_at(ix, doc, page, s).is_some() {
            ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        } else if let Some(id) = top_hit(doc, page, xf.to_user(s), tol) {
            ix.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if let Some(m) = doc.session.doc().find(&id) {
                crate::more::rollover(ix.painter, s, m, cx);
            }
        }
    }
}

// ---- rotation handle -------------------------------------------------------------------------

/// Screen distance of the rotation handle above the selection's top edge.
const ROTATE_GAP: f32 = 22.0;

/// Kinds turned with the handle: outlines turn their points; rectangles, ellipses, text boxes
/// and stamps keep an angle of their own.
fn rotatable(m: &Markup) -> bool {
    movable(m)
        && !m.pts.is_empty()
        && (!uses_rect(m.kind) || markupcraft_model::turn::free_rotates(m.kind))
        && !matches!(
            m.kind,
            Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly | Kind::Count
        )
}

/// The single selected markup with a rotation handle: (id, page, pivot, top, handle).
fn rotate_handle(ix: &Input<'_>, doc: &DocTab) -> Option<(String, usize, Point, Pos2, Pos2)> {
    let [id] = doc.session.selection() else { return None };
    let m = doc.session.doc().find(id)?;
    if !rotatable(m) {
        return None;
    }
    let xf = ix.xf(m.page)?;
    let shown = doc.view.preview.get(id).unwrap_or(m);
    let r = xf.rect_of(actions::markup_bbox(shown));
    let top = egui::pos2(r.center().x, r.top() - 3.0);
    let handle = egui::pos2(r.center().x, r.top() - ROTATE_GAP);
    Some((id.clone(), m.page, actions::center_of(m), top, handle))
}

fn rotate_handle_at(ix: &Input<'_>, doc: &DocTab, s: Pos2) -> Option<(String, usize, Point)> {
    let (id, page, pivot, _, h) = rotate_handle(ix, doc)?;
    (h.distance(s) <= HANDLE_REACH + 1.0).then_some((id, page, pivot))
}

fn paint_rotate_handle(ix: &Input<'_>, doc: &DocTab) {
    let Some((_, _, _, top, h)) = rotate_handle(ix, doc) else {
        return;
    };
    let st = Stroke::new(1.2, ix.tokens.select);
    ix.painter.line_segment([top, h], st);
    ix.painter.circle_filled(h, 4.5, Color32::WHITE);
    ix.painter.circle_stroke(h, 4.5, st);
    if ix.resp.hover_pos().is_some_and(|p| p.distance(h) <= HANDLE_REACH + 1.0) {
        ix.ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}

/// Degrees to turn for a drag from angle `a0` to `a1` (radians, counter-clockwise): steps of
/// 15 degrees (Revu), Shift frees it to whole degrees.
pub fn rotation_degrees(m: &Markup, a0: f64, a1: f64, shift: bool) -> f64 {
    let mut d = (a1 - a0).to_degrees();
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    let step = if m.kind.is_measurement() {
        // measurements snap to 15 degrees; Shift turns them by single degrees (Revu)
        if shift { 1.0 } else { 15.0 }
    } else if shift {
        1.0
    } else {
        15.0
    };
    if step > 0.0 { (d / step).round() * step } else { d }
}

// ---- context menu ----------------------------------------------------------------------------

fn context_menu(ix: &mut Input<'_>, doc: &mut DocTab, cx: &CanvasCx<'_>, out: &mut CanvasOut) {
    if ix.resp.secondary_clicked()
        && doc.view.draft.is_none()
        && let Some(s) = ix.resp.interact_pointer_pos()
        && let Some((page, xf)) = ix.page_at(s)
    {
        let at = xf.to_user(s);
        let tol = f64::from(PICK / xf.k.max(1e-6));
        let id = top_hit(doc, page, at, tol);
        let mut target = ContextTarget {
            page,
            at,
            markup: id.clone(),
            vertex: None,
            segment: None,
            hole: None,
        };
        if let Some(id) = &id {
            if !doc.session.selection().contains(id) {
                actions::select(&mut doc.session, vec![id.clone()]);
            }
            if let Some(m) = doc.session.doc().find(id)
                && !uses_rect(m.kind)
            {
                target.vertex = m
                    .pts
                    .iter()
                    .position(|p| xf.to_screen(*p).distance(s) <= HANDLE_REACH + 2.0);
                let closed = measure_extras::closed_shape(m.kind) || matches!(m.kind, Kind::Polygon | Kind::Cloud);
                if target.vertex.is_none() && measure_extras::can_add_vertices(m.kind) {
                    target.segment = measure_extras::nearest_segment(&m.pts, closed, at, tol * 2.0);
                }
                target.hole = measure_extras::hole_at(m, at, tol, true);
            }
        }
        doc.view.context = Some(target);
    }
    if !matches!(cx.tool.kind, ToolKind::Select | ToolKind::Pan) && doc.view.context.is_none() {
        return;
    }
    let resp = ix.resp.clone();
    resp.context_menu(|ui| {
        crate::context_menu::show(ui, doc, out);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_a_corner_resizes_from_the_opposite_one() {
        let r = URect::new(0.0, 0.0, 10.0, 10.0);
        let mut m = Markup::new(Kind::Rectangle, 0, r.corners().to_vec());
        m.rect = r;
        let pts = reshape(&m, 2, Point::new(20.0, 30.0));
        assert_eq!(bbox(&pts), Some(URect::new(0.0, 0.0, 20.0, 30.0)));
        // A callout keeps its leader when its box is resized; its tip is handle 4.
        let c = tools::new_markup(
            Kind::Callout,
            0,
            &[Point::new(-50.0, -50.0), Point::new(0.0, 10.0), Point::new(10.0, 0.0)],
        )
        .unwrap();
        let pts = reshape(&c, 2, Point::new(40.0, 40.0));
        assert_eq!(pts.len(), 6);
        assert_eq!(pts[4], Point::new(-50.0, -50.0));
        let pts = reshape(&c, 4, Point::new(-60.0, -60.0));
        assert_eq!(pts[4], Point::new(-60.0, -60.0));
    }

    #[test]
    fn today_is_a_date() {
        let t = today();
        assert_eq!(t.len(), 10, "{t}");
        assert_eq!(t.as_bytes()[4], b'-');
    }
}
