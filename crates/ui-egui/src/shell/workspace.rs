//! The look of the pages in the workspace: Dark Mode (pages drawn light on dark, the rest of
//! the interface keeps its theme), Disable Line Weights (the renderer reads a copy whose
//! linework is all one device pixel wide), and reply indicators on markups that have replies
//! (hover one to read them).

use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Rect, Stroke, vec2};
use markupcraft_render::{RenderDoc, RenderOptions};

use crate::AppState;
use crate::theme::Tokens;

#[derive(Default)]
pub struct WorkspaceState {
    /// Per document (uid): the bytes its renderer was made from (as a pointer) and whether that
    /// renderer reads the thin-line copy.
    pub thinned: HashMap<u64, (usize, bool)>,
    /// The page whose text Select All Text copied last.
    pub last_text_copy: Option<usize>,
}

/// Darken a premultiplied RGBA page raster for Dark Mode: white paper becomes dark grey, black
/// ink light grey, colours keep their hue reversed in lightness.
pub fn darken(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        if px[3] != 255 {
            continue;
        }
        for c in &mut px[..3] {
            let v = 255 - u16::from(*c);
            *c = (34 + v * 196 / 255) as u8;
        }
    }
    out
}

/// The paper colour of a page in Dark Mode.
pub const DARK_PAPER: Color32 = Color32::from_rgb(34, 34, 36);

fn ptr(b: &std::sync::Arc<Vec<u8>>) -> usize {
    std::sync::Arc::as_ptr(b) as usize
}

/// Keep every document's renderer on the thin-line copy (or off it) as the preference says.
pub fn begin_frame(app: &mut AppState, _ctx: &egui::Context) {
    let want = app.shell.ui.extra.thin_lines;
    let threads = app.threads;
    let state = &mut app.shell.extra.workspace.thinned;
    state.retain(|uid, _| app.docs.iter().any(|d| d.uid == *uid));
    for d in &mut app.docs {
        let key = ptr(&d.bytes);
        // A rerender (new bytes) always comes back with a normal renderer.
        let thin_now = state.get(&d.uid).is_some_and(|(k, thin)| *k == key && *thin);
        if want == thin_now {
            continue;
        }
        let bytes = if want {
            match markupcraft_render::thin::thin_lines(&d.bytes) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("line weights {}: {e}", d.name);
                    state.insert(d.uid, (key, true));
                    continue;
                }
            }
        } else {
            d.bytes.clone()
        };
        let opts = RenderOptions {
            hide: crate::actions::drawn_objects(d.session.doc()),
            threads,
            hide_all_markups: false,
        };
        if let Ok(r) = RenderDoc::open(bytes, &opts) {
            d.render = Some(r);
            d.view.invalidate();
        }
        state.insert(d.uid, (key, want));
    }
}

/// Whether document `uid` is drawn from the thin-line copy now.
pub fn is_thin(app: &AppState, uid: u64) -> bool {
    app.shell.extra.workspace.thinned.get(&uid).is_some_and(|(_, t)| *t)
}

/// The reply indicators to show in `canvas`: (markup id, the bubble's screen rect, how many
/// replies), for markups on screen that have replies (when the preference is on).
pub fn reply_badges(app: &AppState, canvas: Rect) -> Vec<(String, Rect, usize)> {
    let mut out = Vec::new();
    if !app.shell.ui.extra.reply_indicators {
        return out;
    }
    let Some(d) = app.doc() else { return out };
    let Some(r) = d.render.as_ref() else { return out };
    for m in d.session.doc().markups.iter().filter(|m| !m.replies.is_empty()) {
        if out.len() >= 2000 {
            break;
        }
        let b = crate::actions::markup_bbox(m);
        let Some(at) = d
            .view
            .user_to_screen(m.page, markupcraft_geom::Point::new(b.x1, b.y1), r.pages())
        else {
            continue;
        };
        if canvas.contains(at) {
            let bubble = Rect::from_center_size(at + vec2(10.0, -10.0), vec2(20.0, 15.0));
            out.push((m.id.clone(), bubble, m.replies.len()));
        }
    }
    out
}

/// Reply indicators: a small speech bubble with the count at the top right of each markup that
/// has replies; hovering it lists them.
pub fn paint_replies(app: &AppState, ui: &mut egui::Ui, canvas: Rect) {
    let badges = reply_badges(app, canvas);
    if badges.is_empty() {
        return;
    }
    let Some(d) = app.doc() else { return };
    let t = Tokens::get(ui.ctx());
    let painter = ui.painter_at(canvas);
    for (id, bubble, n) in badges {
        painter.rect_filled(bubble, 4.0, t.accent);
        painter.rect_stroke(bubble, 4.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
        painter.text(
            bubble.center(),
            Align2::CENTER_CENTER,
            n.to_string(),
            FontId::proportional(10.0),
            Color32::WHITE,
        );
        let Some(m) = d.session.doc().find(&id) else { continue };
        let resp = ui.interact(bubble, egui::Id::new(("reply-indicator", &id)), egui::Sense::hover());
        resp.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            for rep in m.replies.iter().take(20) {
                ui.label(egui::RichText::new(format!("{} {}", rep.author, rep.date)).strong());
                ui.label(&rep.text);
            }
        });
    }
}

/// Width of a scrollbar, screen points.
const BAR: f32 = 10.0;

/// Where a scrollbar's thumb sits on its track: (start, length) along the track.
pub fn thumb(track: f32, view: f32, content: f32, offset: f32) -> (f32, f32) {
    if content <= view || track <= 0.0 {
        return (0.0, track);
    }
    let len = (track * view / content).clamp(20.0_f32.min(track), track);
    let pos = (track - len) * (offset / (content - view)).clamp(0.0, 1.0);
    (pos, len)
}

/// Scrollbars on the document (General > Navigation): drag a thumb to scroll, click the track
/// to move a screen at a time.
pub fn scrollbars(app: &mut AppState, ui: &mut egui::Ui) {
    if !app.shell.ui.extra.scrollbars {
        return;
    }
    let left = app.shell.ui.extra.scrollbars_left;
    let Some(d) = app.doc_mut() else { return };
    let Some(r) = d.render.as_ref() else { return };
    let content = d.view.content_size(r.pages());
    let vp = d.view.viewport();
    let t = Tokens::get(ui.ctx());
    let painter = ui.painter().clone();
    for vertical in [true, false] {
        let (c, v, off) = if vertical {
            (content.y, vp.height(), d.view.offset.y)
        } else {
            (content.x, vp.width(), d.view.offset.x)
        };
        if c <= v + 1.0 {
            continue;
        }
        let track = if vertical {
            let x = if left { vp.left() } else { vp.right() - BAR };
            Rect::from_min_max(egui::pos2(x, vp.top()), egui::pos2(x + BAR, vp.bottom() - BAR))
        } else {
            Rect::from_min_max(
                egui::pos2(vp.left(), vp.bottom() - BAR),
                egui::pos2(vp.right() - BAR, vp.bottom()),
            )
        };
        let len_track = if vertical { track.height() } else { track.width() };
        let (pos, len) = thumb(len_track, v, c, off);
        let th = if vertical {
            Rect::from_min_size(egui::pos2(track.left(), track.top() + pos), vec2(BAR, len))
        } else {
            Rect::from_min_size(egui::pos2(track.left() + pos, track.top()), vec2(len, BAR))
        };
        painter.rect_filled(track, 0.0, t.panel.gamma_multiply(0.8));
        let id = egui::Id::new(("doc-scrollbar", vertical));
        let resp = ui.interact(track, id, egui::Sense::click_and_drag());
        painter.rect_filled(
            th.shrink(1.5),
            3.0,
            if resp.dragged() || resp.hovered() {
                t.accent
            } else {
                t.border
            },
        );
        let k = (c - v) / (len_track - len).max(1.0);
        if resp.dragged() {
            let delta = resp.drag_delta();
            let dv = if vertical { delta.y } else { delta.x } * k;
            if vertical {
                d.view.offset.y += dv;
            } else {
                d.view.offset.x += dv;
            }
        } else if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let before = if vertical { p.y < th.top() } else { p.x < th.left() };
            let step = if before { -v * 0.9 } else { v * 0.9 };
            if vertical {
                d.view.offset.y += step;
            } else {
                d.view.offset.x += step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_thumb_follows_the_offset() {
        assert_eq!(thumb(100.0, 50.0, 50.0, 0.0), (0.0, 100.0));
        assert_eq!(thumb(100.0, 50.0, 200.0, 0.0), (0.0, 25.0));
        assert_eq!(thumb(100.0, 50.0, 200.0, 150.0), (75.0, 25.0));
    }

    #[test]
    fn dark_mode_turns_paper_dark_and_ink_light() {
        let out = darken(&[255, 255, 255, 255, 0, 0, 0, 255, 10, 10, 10, 128]);
        assert_eq!(&out[..4], &[34, 34, 34, 255]);
        assert_eq!(&out[4..8], &[230, 230, 230, 255]);
        // Transparent pixels are left alone.
        assert_eq!(&out[8..], &[10, 10, 10, 128]);
    }
}
