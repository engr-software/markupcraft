//! The features' canvas layer: highlights over the pages (search hits, compare changes, spaces,
//! links, redaction marks) and picks (a point, a box or an outline) that run instead of the
//! active tool while a feature waits for one.
//!
//! The app state and the canvas do not reach each other directly: [`publish`] leaves this
//! frame's layer in egui's temporary memory, the canvas draws it (and leaves what was picked)
//! in [`layer`], and [`take_picked`] collects the picks on the next pass.

use egui::{Color32, Id, Pos2, Stroke};
use markupcraft_geom::Point;

use crate::AppState;
use crate::painter::Xf;

/// How a pick is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    /// one click
    Point,
    /// a drag
    Rect,
    /// a click (the point) or a drag (the box)
    PointOrRect,
    /// clicks; double-click or Enter closes it (at least 3 points)
    Polygon,
}

/// A highlight: a closed outline on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub page: usize,
    pub pts: Vec<Point>,
    pub fill: Color32,
    pub stroke: Color32,
}

impl Mark {
    pub fn rect(page: usize, r: markupcraft_geom::Rect, fill: Color32, stroke: Color32) -> Self {
        Self {
            page,
            pts: r.corners().to_vec(),
            fill,
            stroke,
        }
    }
}

/// What the canvas shows for the features this frame.
#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub doc: u64,
    pub pick: Option<PickKind>,
    pub marks: Vec<Mark>,
    /// Markup layers turned off in the Layers panel (their markups are not drawn).
    pub hidden: Vec<String>,
}

/// One finished pick.
#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
    pub doc: u64,
    pub page: usize,
    pub pts: Vec<Point>,
}

/// A pick in progress (points so far).
#[derive(Debug, Clone, Default)]
struct Draft {
    page: usize,
    pts: Vec<Point>,
}

fn layer_id() -> Id {
    Id::new("markupcraft-features-layer")
}
fn picked_id() -> Id {
    Id::new("markupcraft-features-picked")
}
fn draft_id() -> Id {
    Id::new("markupcraft-features-draft")
}

pub const HIT_FILL: Color32 = Color32::from_rgba_premultiplied(60, 50, 0, 70);
pub const HIT_STROKE: Color32 = Color32::from_rgb(230, 170, 0);
pub const CURRENT_FILL: Color32 = Color32::from_rgba_premultiplied(0, 50, 90, 80);
pub const CURRENT_STROKE: Color32 = Color32::from_rgb(0, 120, 215);

/// Leave this frame's layer for the canvas of the active document.
pub fn publish(app: &AppState, ctx: &egui::Context) {
    let Some(d) = app.doc() else {
        ctx.data_mut(|m| m.remove::<Layer>(layer_id()));
        return;
    };
    let f = &app.features;
    let mut marks = Vec::new();
    f.search.marks(d, &mut marks);
    f.compare.marks(d, &mut marks);
    f.spaces.marks(d, &mut marks);
    f.links.marks(d, &mut marks);
    // Redaction marks are read from the file: only while redacting.
    if f.redact.confirm || matches!(f.pick, Some((_, super::Pick::Redact))) {
        f.redact.marks(d, &mut marks);
    }
    f.signatures.marks(d, &mut marks);
    super::more6::prefs::form_marks(app, d, &mut marks);
    f.forms.more.marks(d, &mut marks);
    let pick = f.pick.filter(|(uid, _)| *uid == d.uid).map(|(_, p)| p.kind());
    if pick.is_none() {
        ctx.data_mut(|m| m.remove::<Draft>(draft_id()));
    }
    let hidden = d
        .session
        .layers()
        .into_iter()
        .filter(|l| !l.visible)
        .map(|l| l.name)
        .collect();
    ctx.data_mut(|m| {
        m.insert_temp(
            layer_id(),
            Layer {
                doc: d.uid,
                pick,
                marks,
                hidden,
            },
        )
    });
}

/// Markup layers hidden in document `doc` (the canvas skips their markups).
pub fn hidden_layers(ctx: &egui::Context, doc: u64) -> Vec<String> {
    ctx.data(|m| m.get_temp::<Layer>(layer_id()))
        .filter(|l| l.doc == doc)
        .map(|l| l.hidden)
        .unwrap_or_default()
}

/// Picks made since the last call.
pub fn take_picked(ctx: &egui::Context) -> Vec<Picked> {
    ctx.data_mut(|m| m.remove_temp::<Vec<Picked>>(picked_id()))
        .unwrap_or_default()
}

fn push(ui: &egui::Ui, p: Picked) {
    ui.ctx()
        .data_mut(|m| m.get_temp_mut_or_default::<Vec<Picked>>(picked_id()).push(p));
}

fn poly(xf: &Xf, pts: &[Point]) -> Vec<Pos2> {
    pts.iter().map(|p| xf.to_screen(*p)).collect()
}

/// Draw the layer on the visible pages and run a pick. True when a pick took the pointer (the
/// active tool does not run this frame).
pub fn layer(ui: &mut egui::Ui, resp: &egui::Response, painter: &egui::Painter, xfs: &[(usize, Xf)], doc: u64) -> bool {
    let Some(l) = ui.ctx().data(|m| m.get_temp::<Layer>(layer_id())) else {
        return false;
    };
    if l.doc != doc {
        return false;
    }
    for m in &l.marks {
        let Some((_, xf)) = xfs.iter().find(|(i, _)| *i == m.page) else {
            continue;
        };
        let pts = poly(xf, &m.pts);
        if pts.len() >= 3 {
            // Concave outlines (some spaces) are not filled; their outline carries the
            // highlight.
            if is_convex(&pts) {
                painter.add(egui::Shape::convex_polygon(pts.clone(), m.fill, Stroke::NONE));
            }
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, m.stroke)));
        }
    }
    if l.pick.is_none()
        && resp.clicked()
        && ui.input(|i| i.modifiers.command)
        && let Some(s) = resp.interact_pointer_pos()
        && let Some((page, xf)) = xfs.iter().find(|(_, xf)| xf.rect.contains(s))
    {
        super::partials_more2::push_ctrl_click(ui.ctx(), doc, *page, xf.to_user(s));
    }
    let Some(kind) = l.pick else { return false };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    let pointer = resp.hover_pos().or_else(|| resp.interact_pointer_pos());
    let page_at = |s: Pos2| {
        xfs.iter()
            .find(|(_, xf)| xf.rect.expand(2.0).contains(s))
            .map(|(i, xf)| (*i, xf.to_user(s)))
    };
    let draft: Option<Draft> = ui.ctx().data(|m| m.get_temp::<Draft>(draft_id()));
    let set_draft = |ui: &egui::Ui, d: Option<Draft>| {
        ui.ctx().data_mut(|m| match d {
            Some(d) => {
                m.insert_temp(draft_id(), d);
            }
            None => {
                m.remove::<Draft>(draft_id());
            }
        });
    };
    let accent = Stroke::new(1.5, CURRENT_STROKE);
    match kind {
        PickKind::Point | PickKind::Rect | PickKind::PointOrRect => {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && kind != PickKind::Point
                && let Some((page, at)) = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| resp.interact_pointer_pos())
                    .and_then(page_at)
            {
                set_draft(ui, Some(Draft { page, pts: vec![at] }));
            }
            if let (Some(d), Some(s)) = (&draft, pointer)
                && let Some((_, xf)) = xfs.iter().find(|(i, _)| *i == d.page)
                && let Some(a) = d.pts.first()
            {
                let r = egui::Rect::from_two_pos(xf.to_screen(*a), s);
                painter.rect_filled(r, 0.0, CURRENT_FILL);
                painter.rect_stroke(r, 0.0, accent, egui::StrokeKind::Middle);
                if resp.drag_stopped() {
                    let b = xf.to_user(s);
                    push(
                        ui,
                        Picked {
                            doc,
                            page: d.page,
                            pts: vec![*a, b],
                        },
                    );
                    set_draft(ui, None);
                }
            } else if resp.clicked()
                && kind != PickKind::Rect
                && let Some((page, at)) = resp.interact_pointer_pos().and_then(page_at)
            {
                push(
                    ui,
                    Picked {
                        doc,
                        page,
                        pts: vec![at],
                    },
                );
            }
        }
        PickKind::Polygon => {
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let mut d = draft.clone();
            if resp.clicked()
                && let Some((page, at)) = resp.interact_pointer_pos().and_then(page_at)
            {
                match &mut d {
                    Some(dd) if dd.page == page => dd.pts.push(at),
                    _ => d = Some(Draft { page, pts: vec![at] }),
                }
            }
            if let Some(dd) = &d
                && let Some((_, xf)) = xfs.iter().find(|(i, _)| *i == dd.page)
            {
                let mut pts = poly(xf, &dd.pts);
                if let Some(s) = pointer {
                    pts.push(s);
                }
                painter.add(egui::Shape::line(pts, accent));
                for p in &dd.pts {
                    painter.circle_filled(xf.to_screen(*p), 3.0, CURRENT_STROKE);
                }
            }
            let finish = resp.double_clicked() || enter;
            match d {
                Some(dd) if finish && dd.pts.len() >= 3 => {
                    let mut pts = dd.pts;
                    // The double-click's second click added a point on top of the last one.
                    while pts.len() > 3
                        && let [.., a, b] = pts.as_slice()
                        && a.dist(*b) < 0.5
                    {
                        pts.pop();
                    }
                    push(
                        ui,
                        Picked {
                            doc,
                            page: dd.page,
                            pts,
                        },
                    );
                    set_draft(ui, None);
                }
                other => set_draft(ui, other),
            }
        }
    }
    true
}

fn is_convex(pts: &[Pos2]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..n {
        let (Some(a), Some(b), Some(c)) = (pts.get(i), pts.get((i + 1) % n), pts.get((i + 2) % n)) else {
            return false;
        };
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() > 1e-3 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convexity() {
        let sq = [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ];
        assert!(is_convex(&sq));
        let l = [
            Pos2::new(0.0, 0.0),
            Pos2::new(2.0, 0.0),
            Pos2::new(2.0, 1.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(1.0, 2.0),
            Pos2::new(0.0, 2.0),
        ];
        assert!(!is_convex(&l));
    }
}
