//! Preferences > Window > Tablet (stored in `prefs.more.tablet`), beside the eraser: pinch
//! zoom, the pen cursor, highlighting text with the Highlight pen, the pen commit delay
//! (strokes in quick succession join one markup), copying pen strokes as a picture, the
//! right-button lasso, pen pressure and the touch input mode (larger handles and pick areas).
//!
//! The canvas reads them through [`opts`] (set each frame by [`frame`], on the interface's
//! thread), and keeps the pen's last stroke and pressure readings here.

use std::cell::{Cell, RefCell};

use egui::{Color32, Pos2, Stroke};
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup};

use crate::AppState;

/// What the canvas needs, copied each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opts {
    pub pen_dot: bool,
    pub pen_text_highlight: bool,
    pub pen_commit_ms: u32,
    pub ink_copy_picture: bool,
    pub right_click_lasso: bool,
    pub pressure: bool,
    pub touch_mode: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self::from(&markupcraft_engine::prefs_pages::TabletPrefs::default())
    }
}

impl From<&markupcraft_engine::prefs_pages::TabletPrefs> for Opts {
    fn from(t: &markupcraft_engine::prefs_pages::TabletPrefs) -> Self {
        Self {
            pen_dot: t.pen_cursor == "dot",
            pen_text_highlight: t.pen_text_highlight,
            pen_commit_ms: t.pen_commit_ms.min(10_000),
            ink_copy_picture: t.ink_copy_picture,
            right_click_lasso: t.right_click_lasso,
            pressure: t.pressure,
            touch_mode: t.touch_mode,
        }
    }
}

/// The last pen stroke: (document uid, markup id, page, when, in seconds of input time).
type LastStroke = (u64, String, usize, f64);

thread_local! {
    static OPTS: Cell<Opts> = Cell::new(Opts::default());
    static LAST: RefCell<Option<LastStroke>> = const { RefCell::new(None) };
    static FORCES: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static LASSO: RefCell<Option<(usize, Vec<Point>)>> = const { RefCell::new(None) };
}

/// This thread's options.
pub fn opts() -> Opts {
    OPTS.with(Cell::get)
}

/// Each frame: the options from the preferences.
pub fn frame(app: &AppState) {
    OPTS.with(|o| o.set(Opts::from(&app.shell.prefs.more.tablet)));
}

/// How much larger handles and pick areas are (2 in touch input mode).
pub fn touch_factor() -> f32 {
    if opts().touch_mode { 2.0 } else { 1.0 }
}

/// Pen pressure: forget the readings (a stroke starts).
pub fn start_stroke() {
    FORCES.with(|f| f.borrow_mut().clear());
}

/// Pen pressure: the touch forces this frame brought.
pub fn read_forces(ui: &egui::Ui) {
    if !opts().pressure {
        return;
    }
    let got: Vec<f32> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Touch { force: Some(f), .. } if f.is_finite() => Some(f.clamp(0.0, 1.0)),
                _ => None,
            })
            .collect()
    });
    FORCES.with(|f| {
        let mut f = f.borrow_mut();
        if f.len() + got.len() <= 100_000 {
            f.extend(got);
        }
    });
}

/// A finished stroke's width from the pen's pressure: the tool's width at half pressure,
/// thinner when lighter, thicker when harder (unchanged without readings).
pub fn pressed_width(width: f64) -> f64 {
    if !opts().pressure {
        return width;
    }
    let avg = FORCES.with(|f| {
        let f = f.borrow();
        (!f.is_empty()).then(|| f.iter().map(|v| f64::from(*v)).sum::<f64>() / f.len() as f64)
    });
    match avg {
        Some(a) => (width * (a * 2.0).clamp(0.25, 4.0)).clamp(0.1, 72.0),
        None => width,
    }
}

/// The pen commit delay: the markup a stroke made now on `page` joins, if any.
pub fn joins(uid: u64, page: usize, now: f64) -> Option<String> {
    let ms = opts().pen_commit_ms;
    if ms == 0 {
        return None;
    }
    LAST.with(|l| {
        l.borrow()
            .as_ref()
            .filter(|(u, _, p, t)| *u == uid && *p == page && now - *t <= f64::from(ms) / 1000.0 && now >= *t)
            .map(|(_, id, _, _)| id.clone())
    })
}

/// A pen stroke was made (as markup `id`, or joined to it).
pub fn stroke_made(uid: u64, id: &str, page: usize, now: f64) {
    LAST.with(|l| *l.borrow_mut() = Some((uid, id.to_string(), page, now)));
}

/// The pen cursor: a dot the size of the pen's line instead of the crosshair.
pub fn paint_pen_dot(p: &egui::Painter, at: Pos2, radius: f32, color: Color32) {
    p.circle_filled(at, radius.clamp(1.5, 40.0), color);
    p.circle_stroke(at, radius.clamp(1.5, 40.0) + 1.0, Stroke::new(1.0, Color32::WHITE));
}

/// The right-button lasso: a drag with the secondary button. Returns the ring and its page
/// when the drag ended this frame; draws it while it is drawn.
pub fn right_lasso(
    resp: &egui::Response,
    ui: &egui::Ui,
    page_at: impl Fn(Pos2) -> Option<(usize, Point)>,
) -> Option<(usize, Vec<Point>)> {
    if !opts().right_click_lasso {
        return None;
    }
    let pos = ui.input(|i| i.pointer.latest_pos());
    if resp.drag_started_by(egui::PointerButton::Secondary)
        && let Some(start) = ui.input(|i| i.pointer.press_origin()).or(pos)
        && let Some((page, p)) = page_at(start)
    {
        LASSO.with(|l| *l.borrow_mut() = Some((page, vec![p])));
    }
    if resp.dragged_by(egui::PointerButton::Secondary)
        && let Some(c) = pos
        && let Some((page, p)) = page_at(c)
    {
        LASSO.with(|l| {
            if let Some((pg, ring)) = l.borrow_mut().as_mut()
                && *pg == page
                && ring.len() < 100_000
            {
                ring.push(p);
            }
        });
    }
    if resp.drag_stopped_by(egui::PointerButton::Secondary) {
        return LASSO.with(|l| l.borrow_mut().take()).filter(|(_, r)| r.len() >= 3);
    }
    None
}

/// The ring being drawn with the right button, if any.
pub fn lasso_ring() -> Option<(usize, Vec<Point>)> {
    LASSO.with(|l| l.borrow().clone())
}

/// Pen and Highlight strokes as a picture (white background, each stroke in its colour), at
/// most `max_side` pixels on the longer side. `None` without strokes.
pub fn ink_picture(markups: &[Markup], max_side: usize) -> Option<egui::ColorImage> {
    let ink: Vec<&Markup> = markups
        .iter()
        .filter(|m| matches!(m.kind, Kind::Ink | Kind::Highlight) && !m.pts.is_empty())
        .collect();
    let pts = ink.iter().flat_map(|m| m.pts.iter());
    let b = markupcraft_geom::bbox(&pts.copied().collect::<Vec<_>>())?;
    let pad = ink.iter().map(|m| m.line_width).fold(1.0, f64::max);
    let (x0, y1) = (b.x0 - pad, b.y1 + pad);
    let (w, h) = (b.width() + 2.0 * pad, b.height() + 2.0 * pad);
    let k = (max_side.clamp(16, 4096) as f64 / w.max(h).max(1.0)).min(4.0);
    let (pw, ph) = (((w * k).ceil() as usize).max(1), ((h * k).ceil() as usize).max(1));
    let mut img = egui::ColorImage::new([pw, ph], vec![Color32::WHITE; pw * ph]);
    for m in ink {
        let c = m.color;
        let col = Color32::from_rgb(
            (c.r.clamp(0.0, 1.0) * 255.0) as u8,
            (c.g.clamp(0.0, 1.0) * 255.0) as u8,
            (c.b.clamp(0.0, 1.0) * 255.0) as u8,
        );
        let r = (m.line_width * k / 2.0).max(0.6);
        let to_px = |p: &Point| ((p.x - x0) * k, (y1 - p.y) * k);
        let mut starts: Vec<usize> = m.strokes.clone();
        starts.push(m.pts.len());
        let mut from = 0;
        for end in starts {
            let s = m.pts.get(from..end.min(m.pts.len())).unwrap_or(&[]);
            for seg in s.windows(2) {
                let (a, b) = (to_px(&seg[0]), to_px(&seg[1]));
                let n = (((b.0 - a.0).hypot(b.1 - a.1) / (r.max(0.5))).ceil() as usize).clamp(1, 10_000);
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    dot(&mut img, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, r, col);
                }
            }
            if let [one] = s {
                let (x, y) = to_px(one);
                dot(&mut img, x, y, r, col);
            }
            from = end;
        }
    }
    Some(img)
}

fn dot(img: &mut egui::ColorImage, cx: f64, cy: f64, r: f64, c: Color32) {
    let [w, h] = img.size;
    let (x0, x1) = (
        (cx - r).floor().max(0.0) as usize,
        ((cx + r).ceil().max(0.0) as usize).min(w),
    );
    let (y0, y1) = (
        (cy - r).floor().max(0.0) as usize,
        ((cy + r).ceil().max(0.0) as usize).min(h),
    );
    for y in y0..y1 {
        for x in x0..x1 {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r
                && let Some(p) = img.pixels.get_mut(y * w + x)
            {
                *p = c;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_pictures_draw_the_strokes() {
        let mut m = Markup::new(Kind::Ink, 0, vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]);
        m.line_width = 4.0;
        m.color = markupcraft_model::Color::rgb(1.0, 0.0, 0.0);
        let img = ink_picture(&[m], 200).unwrap();
        assert!(img.size[0] > img.size[1]);
        assert!(img.pixels.iter().any(|p| *p == Color32::from_rgb(255, 0, 0)));
        assert!(ink_picture(&[], 200).is_none());
    }
}
