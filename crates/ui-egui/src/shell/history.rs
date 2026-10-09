//! Previous / Next View (Alt+Left / Alt+Right): browser-style history of each document's
//! views (page, zoom, layout, position). A view is recorded once it has stayed the same for a
//! frame, so a wheel zoom or a drag is one step, not hundreds.

use egui::Vec2;

use crate::AppState;
use crate::canvas::{DocView, Fit, PageMode};

/// Most views kept per document.
const MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewSnap {
    pub page: usize,
    pub zoom: f32,
    pub fit: Fit,
    pub mode: PageMode,
    pub rotation: u16,
    pub offset: Vec2,
}

impl ViewSnap {
    pub fn of(v: &DocView) -> Self {
        Self {
            page: v.current,
            zoom: v.zoom,
            fit: v.fit,
            mode: v.mode,
            rotation: v.rotation,
            offset: v.offset,
        }
    }

    /// A different place to go back to (not a small scroll).
    fn differs(&self, o: &ViewSnap) -> bool {
        self.page != o.page
            || self.mode != o.mode
            || self.rotation != o.rotation
            || (self.zoom / o.zoom.max(1e-6) - 1.0).abs() > 0.01
    }

    fn apply(&self, v: &mut DocView) {
        v.mode = self.mode;
        v.rotation = self.rotation;
        v.place(self.page, self.zoom, self.fit, self.offset);
    }
}

#[derive(Debug, Clone, Default)]
pub struct History {
    pub back: Vec<ViewSnap>,
    pub forward: Vec<ViewSnap>,
    /// The view last recorded.
    committed: Option<ViewSnap>,
    /// The view last frame.
    last_frame: Option<ViewSnap>,
}

/// Record the active document's view when it settles somewhere new.
pub fn record(app: &mut AppState) {
    let busy = app.doc().is_some_and(|d| d.view.viewport().width() <= 0.0);
    let Some((uid, cur)) = app.doc().map(|d| (d.uid, ViewSnap::of(&d.view))) else {
        return;
    };
    if busy {
        return;
    }
    let h = app.shell.history.entry(uid).or_default();
    let stable = h.last_frame.is_some_and(|l| !l.differs(&cur));
    h.last_frame = Some(cur);
    match h.committed {
        None => h.committed = Some(cur),
        Some(c) if stable && c.differs(&cur) => {
            h.back.push(c);
            if h.back.len() > MAX {
                h.back.remove(0);
            }
            h.forward.clear();
            h.committed = Some(cur);
        }
        Some(c) if !c.differs(&cur) => h.committed = Some(cur),
        _ => {}
    }
}

/// Go back (or forward) one view.
pub fn step(app: &mut AppState, back: bool) {
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let Some(h) = app.shell.history.get_mut(&d.uid) else {
        return;
    };
    let cur = ViewSnap::of(&d.view);
    let target = if back { h.back.pop() } else { h.forward.pop() };
    let Some(t) = target else { return };
    if back {
        h.forward.push(cur);
    } else {
        h.back.push(cur);
    }
    let n = d.session.page_count();
    let mut t = t;
    t.page = t.page.min(n.saturating_sub(1));
    t.apply(&mut d.view);
    h.committed = Some(t);
    h.last_frame = Some(t);
}
