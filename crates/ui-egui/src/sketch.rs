//! Sketch to Scale: while a line, polyline, polygon or measurement is being drawn, type the
//! next segment's length (in the page's measurement unit, feet-inches accepted) and angle and
//! press Enter to place the point; a Rectangle or Ellipse (or Area by rectangle) takes a typed
//! width and height from its first click. The bar shows over the canvas while such a draft is
//! open. Angle mode: absolute (0 = right, counter-clockwise) or relative to the last segment.

use markupcraft_geom::Point;
use markupcraft_model::Scale;

use crate::AppState;
use crate::interact::Stage;
use crate::tools::{Role, ToolKind};

/// What the bar offers for the active tool and draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// length and angle of the next segment (`finish_at` 0 = until Finish)
    Segment { finish_at: usize },
    /// width and height from the first corner
    Size,
}

/// The bar's offer for `app`'s active tool, when a draft of it is open.
pub fn offer(app: &AppState) -> Option<Offer> {
    let d = app.doc()?;
    let draft = d.view.draft.as_ref()?;
    let tool = crate::tools::find(app.tool)?;
    if draft.tool != tool.id {
        return None;
    }
    match tool.kind {
        ToolKind::Points {
            kind,
            finish_at,
            role: Role::Markup | Role::Cutout | Role::CloudPlus,
            ..
        } if kind != markupcraft_model::Kind::Count && draft.stage == Stage::Points => {
            Some(Offer::Segment { finish_at })
        }
        ToolKind::Box(_) | ToolKind::Drag { .. } if draft.stage == Stage::Points && draft.pts.len() == 1 => {
            Some(Offer::Size)
        }
        _ => None,
    }
}

/// Apply the bar: add the typed segment (or size). Returns the status text.
pub fn apply(app: &mut AppState, finish: bool) -> String {
    let Some(offer) = offer(app) else {
        return "Sketch to Scale: start drawing first".into();
    };
    let st = app.edit.sketch.clone();
    let Some(d) = app.doc_mut() else { return String::new() };
    let Some(draft) = d.view.draft.clone() else {
        return String::new();
    };
    let Some(last) = draft.pts.last().copied() else {
        return "Sketch to Scale: click the first point".into();
    };
    let info = d.session.doc().pages.get(draft.page).cloned();
    let scale = info.as_ref().and_then(|p| p.scale_at(last)).cloned();
    match offer {
        Offer::Segment { finish_at } => {
            if finish && st.length.trim().is_empty() {
                if draft.pts.len() >= 2
                    && let Some(dr) = d.view.draft.as_mut()
                {
                    dr.stage = Stage::Typed;
                }
                return "Finished".into();
            }
            let Some(len) = to_points(&st.length, scale.as_ref()) else {
                return "Sketch to Scale: type a length like 12'-6\" or 3.5".into();
            };
            let Some(angle) = parse_angle(&st.angle) else {
                return "Sketch to Scale: the angle is a number of degrees".into();
            };
            let prev = draft.pts.len().checked_sub(2).and_then(|i| draft.pts.get(i)).copied();
            let p = next_point(prev, last, len, angle, st.relative);
            if let Some(dr) = d.view.draft.as_mut() {
                dr.pts.push(p);
                if (finish_at > 0 && dr.pts.len() >= finish_at) || finish {
                    dr.stage = Stage::Typed;
                }
            }
            format!("Placed a point {} away", st.length.trim())
        }
        Offer::Size => {
            let (Some(w), Some(h)) = (
                to_points(&st.width, scale.as_ref()),
                to_points(&st.height, scale.as_ref()),
            ) else {
                return "Sketch to Scale: type the width and the height".into();
            };
            if let Some(dr) = d.view.draft.as_mut() {
                dr.pts = vec![last, Point::new(last.x + w, last.y - h)];
                dr.stage = Stage::Typed;
            }
            format!("Placed {} x {}", st.width.trim(), st.height.trim())
        }
    }
}

/// The Sketch to Scale bar, shown at the bottom left of the window while drawing.
pub fn bar(app: &mut AppState, ctx: &egui::Context) {
    let Some(offer) = offer(app) else { return };
    let mut st = app.edit.sketch.clone();
    let (mut go, mut finish) = (false, false);
    egui::Window::new("Sketch to Scale")
        .id(egui::Id::new("sketch-to-scale"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -40.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| match offer {
                Offer::Segment { finish_at } => {
                    ui.label("Length");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.length)
                            .id(egui::Id::new("sketch-length"))
                            .desired_width(70.0)
                            .hint_text("12'-6\""),
                    );
                    go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.label("Angle");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.angle)
                            .id(egui::Id::new("sketch-angle"))
                            .desired_width(44.0)
                            .hint_text("0"),
                    );
                    go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.checkbox(&mut st.relative, "Relative");
                    go |= ui.button("Add").clicked();
                    if finish_at == 0 {
                        finish |= ui.button("Finish").clicked();
                    }
                }
                Offer::Size => {
                    ui.label("Width");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.width)
                            .id(egui::Id::new("sketch-width"))
                            .desired_width(70.0),
                    );
                    go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.label("Height");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.height)
                            .id(egui::Id::new("sketch-height"))
                            .desired_width(70.0),
                    );
                    go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    go |= ui.button("Place").clicked();
                }
            });
        });
    app.edit.sketch = st;
    if go || finish {
        app.status = apply(app, finish);
    }
}

/// The Sketch to Scale bar's fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SketchState {
    pub length: String,
    pub angle: String,
    /// angles turn from the previous segment's direction
    pub relative: bool,
    pub width: String,
    pub height: String,
}

/// PDF points for a typed real length: `12'-6"`, `12.5` (the scale's first distance unit).
/// Without a scale the number is inches on the sheet.
pub fn to_points(text: &str, scale: Option<&Scale>) -> Option<f64> {
    let v = markupcraft_measure::parse_label(text.trim())?;
    if !v.is_finite() || v <= 0.0 {
        return None;
    }
    let pts = match scale.filter(|s| s.valid()) {
        Some(s) => {
            let per_point = s.length_of(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0)], false);
            if !(per_point.is_finite() && per_point > 0.0) {
                return None;
            }
            v / per_point
        }
        None => v * 72.0,
    };
    (pts.is_finite() && pts > 0.0 && pts < 1.0e7).then_some(pts)
}

/// Degrees from text (`45`, `-30`, `90°`); empty = 0.
pub fn parse_angle(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches('\u{b0}').trim();
    if t.is_empty() {
        return Some(0.0);
    }
    t.parse::<f64>().ok().filter(|a| a.is_finite())
}

/// The next vertex: `len` points from `last` at `angle` degrees (absolute, or turned from the
/// direction `prev -> last` when `relative` and there is a previous point).
pub fn next_point(prev: Option<Point>, last: Point, len: f64, angle: f64, relative: bool) -> Point {
    let base = match prev {
        Some(p) if relative && p.dist(last) > 1e-9 => (last.y - p.y).atan2(last.x - p.x),
        _ => 0.0,
    };
    let a = base + angle.to_radians();
    Point::new(last.x + len * a.cos(), last.y + len * a.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_lengths_follow_the_scale() {
        // 1/8" = 1': 9 points per foot.
        let s = Scale::architectural(0.125, 1.0);
        let p = to_points("10'", Some(&s)).unwrap();
        assert!((p - 90.0).abs() < 1e-6, "{p}");
        let p = to_points("12'-6\"", Some(&s)).unwrap();
        assert!((p - 112.5).abs() < 1e-6, "{p}");
        assert_eq!(to_points("2", None), Some(144.0));
        assert_eq!(to_points("abc", None), None);
        assert_eq!(to_points("-3", None), None);
    }

    #[test]
    fn angles_absolute_and_relative() {
        let o = Point::new(0.0, 0.0);
        let p = next_point(None, o, 10.0, 90.0, false);
        assert!(p.x.abs() < 1e-9 && (p.y - 10.0).abs() < 1e-9);
        let q = next_point(Some(o), p, 10.0, -90.0, true);
        assert!((q.x - 10.0).abs() < 1e-9 && (q.y - 10.0).abs() < 1e-9, "{q:?}");
        assert_eq!(parse_angle("45°"), Some(45.0));
        assert_eq!(parse_angle(""), Some(0.0));
        assert_eq!(parse_angle("x"), None);
    }
}
