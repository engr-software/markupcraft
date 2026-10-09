//! Rulers (Ctrl+R) and the full-screen crosshair, drawn over the canvas. The rulers run along
//! the top and left of the document pane, zero at the current page's top-left corner, in
//! inches, centimetres, millimetres, points or picas (right-click a ruler); they shade the
//! page, the selected markups' extent, and mark the pointer.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use markupcraft_geom::Point;

use super::RulerUnit;
use crate::AppState;
use crate::canvas::PT;
use crate::theme::Tokens;

pub const RULER: f32 = 18.0;

/// The canvas rect inside `rect` (smaller by the rulers when they show).
pub fn rulers_frame(app: &AppState, _ui: &egui::Ui, rect: Rect) -> Rect {
    if app.shell.ui.rulers && app.has_doc() {
        Rect::from_min_max(rect.min + vec2(RULER, RULER), rect.max)
    } else {
        rect
    }
}

/// A tick spacing (in units) that puts labelled ticks at least `min_px` apart.
pub fn tick_step(px_per_unit: f32, min_px: f32) -> f32 {
    let mut step = 1.0 / 64.0;
    for _ in 0..40 {
        if step * px_per_unit >= min_px {
            return step;
        }
        let s = step;
        step = if (s * 64.0).fract() == 0.0 && s < 1.0 {
            s * 2.0
        } else {
            let mag = 10f32.powf(s.log10().floor());
            let m = s / mag;
            if m < 1.5 {
                2.0 * mag
            } else if m < 3.5 {
                5.0 * mag
            } else {
                10.0 * mag
            }
        };
    }
    step
}

/// The current page's rect on screen and the zoom (screen points per PDF point).
fn page_on_screen(app: &AppState) -> Option<(Rect, f32, usize)> {
    let d = app.doc()?;
    let r = d.render.as_ref()?;
    let page = d.view.current;
    let g = r.page(page)?;
    let [x0, y0, x1, y1] = g.crop;
    let a = d
        .view
        .user_to_screen(page, Point::new(f64::from(x0), f64::from(y0)), r.pages())?;
    let b = d
        .view
        .user_to_screen(page, Point::new(f64::from(x1), f64::from(y1)), r.pages())?;
    Some((Rect::from_two_pos(a, b), d.view.zoom * PT, page))
}

/// Selected markups' extent on screen.
fn selection_on_screen(app: &AppState) -> Option<Rect> {
    let d = app.doc()?;
    let r = d.render.as_ref()?;
    let mut out: Option<Rect> = None;
    for id in d.selection() {
        let Some(m) = d.session.doc().find(id) else { continue };
        let b = crate::actions::box_of(m);
        let a = d.view.user_to_screen(m.page, Point::new(b.x0, b.y0), r.pages())?;
        let c = d.view.user_to_screen(m.page, Point::new(b.x1, b.y1), r.pages())?;
        let s = Rect::from_two_pos(a, c);
        out = Some(out.map_or(s, |o| o.union(s)));
    }
    out
}

/// Rulers and crosshair over the pane `rect` (after the canvas drew).
pub fn paint(app: &mut AppState, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
    let canvas = rulers_frame(app, ui, rect);
    super::workspace::paint_replies(app, ui, canvas);
    super::workspace::scrollbars(app, ui);
    if app.shell.ui.crosshair
        && let Some(p) = pointer.filter(|p| canvas.contains(*p))
    {
        let painter = ui.painter_at(canvas);
        let c = app.shell.ui.extra.snap_color.map_or(t.select, |_| {
            crate::snapping::indicator_color(app.shell.ui.extra.snap_color)
        });
        painter.hline(canvas.x_range(), p.y, Stroke::new(1.0, c));
        painter.vline(p.x, canvas.y_range(), Stroke::new(1.0, c));
    }
    if !(app.shell.ui.rulers && app.has_doc()) {
        return;
    }
    let unit = app.shell.ui.ruler_unit;
    let Some((page, k, _)) = page_on_screen(app) else {
        return;
    };
    let sel = selection_on_screen(app);
    let top = Rect::from_min_max(pos2(canvas.left(), rect.top()), pos2(canvas.right(), canvas.top()));
    let left = Rect::from_min_max(pos2(rect.left(), canvas.top()), pos2(canvas.left(), canvas.bottom()));
    let corner = Rect::from_min_max(rect.min, canvas.min);
    let p = ui.painter_at(rect);
    p.rect_filled(corner, 0.0, t.chrome);
    p.text(
        corner.center(),
        Align2::CENTER_CENTER,
        unit.short(),
        FontId::proportional(9.0),
        t.text_muted,
    );
    let px_per_unit = unit.points() * k;
    let step = tick_step(px_per_unit, 48.0);
    for (r, horizontal) in [(top, true), (left, false)] {
        p.rect_filled(r, 0.0, t.panel);
        let pr = p.with_clip_rect(r);
        // The page's span and the selection's.
        let span = |a: f32, b: f32| {
            if horizontal {
                Rect::from_min_max(pos2(a, r.top()), pos2(b, r.bottom()))
            } else {
                Rect::from_min_max(pos2(r.left(), a), pos2(r.right(), b))
            }
        };
        let (p0, p1) = if horizontal {
            (page.left(), page.right())
        } else {
            (page.top(), page.bottom())
        };
        pr.rect_filled(span(p0, p1), 0.0, Color32::WHITE);
        if let Some(s) = sel {
            let (a, b) = if horizontal {
                (s.left(), s.right())
            } else {
                (s.top(), s.bottom())
            };
            pr.rect_filled(span(a, b), 0.0, t.accent_soft);
        }
        // Ticks from the page origin, both ways.
        let (lo, hi) = if horizontal {
            (r.left(), r.right())
        } else {
            (r.top(), r.bottom())
        };
        let minor = step / 4.0;
        let first = ((lo - p0) / (minor * px_per_unit)).floor() as i64;
        let last = ((hi - p0) / (minor * px_per_unit)).ceil() as i64;
        if last.saturating_sub(first) > 4000 || !px_per_unit.is_finite() || px_per_unit <= 0.0 {
            continue;
        }
        for i in first..=last {
            let v = p0 + i as f32 * minor * px_per_unit;
            let major = i.rem_euclid(4) == 0;
            let len = if major { RULER * 0.6 } else { RULER * 0.25 };
            let (a, b) = if horizontal {
                (pos2(v, r.bottom() - len), pos2(v, r.bottom()))
            } else {
                (pos2(r.right() - len, v), pos2(r.right(), v))
            };
            pr.line_segment([a, b], Stroke::new(1.0, t.text_faint));
            if major {
                let value = i as f32 * minor;
                let text = if (value - value.round()).abs() < 1e-4 {
                    format!("{}", value.round() as i64)
                } else {
                    format!("{value:.2}")
                };
                let at = if horizontal {
                    pos2(v + 2.0, r.top() + 1.0)
                } else {
                    pos2(r.left() + 1.0, v + 2.0)
                };
                pr.text(at, Align2::LEFT_TOP, text, FontId::proportional(9.0), t.text_muted);
            }
        }
        if let Some(ptr) = pointer {
            let v = if horizontal { ptr.x } else { ptr.y };
            let (a, b) = if horizontal {
                (pos2(v, r.top()), pos2(v, r.bottom()))
            } else {
                (pos2(r.left(), v), pos2(r.right(), v))
            };
            pr.line_segment([a, b], Stroke::new(1.0, t.accent));
        }
        pr.line_segment(
            if horizontal {
                [pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())]
            } else {
                [pos2(r.right(), r.top()), pos2(r.right(), r.bottom())]
            },
            Stroke::new(1.0, t.border),
        );
        // Right-click: units.
        let resp = ui.interact(r, egui::Id::new(("ruler", horizontal)), Sense::click());
        resp.context_menu(|ui| {
            for u in RulerUnit::ALL {
                if ui.radio(app.shell.ui.ruler_unit == u, u.label()).clicked() {
                    app.shell.ui.ruler_unit = u;
                    app.shell.save_ui();
                    ui.close();
                }
            }
        });
    }
}

/// The pointer's position on the current page in ruler units (for the status bar).
pub fn pointer_readout(app: &AppState) -> Option<String> {
    let d = app.doc()?;
    let (page, at) = d.view.pointer?;
    let g = d.render.as_ref()?.page(page)?;
    let unit = app.shell.ui.ruler_unit;
    let v = d.view.view_geom(g).user_to_view(at.x as f32, at.y as f32);
    let k = unit.points();
    Some(format!("{:.2}, {:.2} {}", v[0] / k, v[1] / k, unit.short()))
}

/// The current page's size in inches (status bar).
pub fn page_size_readout(app: &AppState) -> Option<String> {
    let d = app.doc()?;
    let g = d.render.as_ref()?.page(d.view.current)?;
    Some(format!("{:.2} x {:.2} in", g.width / 72.0, g.height / 72.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_ticks_stay_readable() {
        // 1 in at 100 % is 96 screen points: whole inches are far enough apart.
        assert_eq!(tick_step(96.0, 48.0), 0.5);
        assert!(tick_step(5.0, 48.0) >= 10.0);
        assert!(tick_step(10_000.0, 48.0) <= 1.0 / 64.0 + 1e-6);
    }
}
