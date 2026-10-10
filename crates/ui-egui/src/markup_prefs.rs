//! Preferences > Tools > Markup (in `UiPrefs::markup`), beyond reusing tools and the colours
//! of new markups: autosize text boxes, remember each tool's last properties, scale line
//! widths and text with a shape resized from a corner, the author and date in pop-ups, print
//! open pop-ups, copy the text under a text markup into its comment, how pictures are stored
//! (JPEG or lossless), whether shapes are drawn from a corner or the centre, and whether
//! snapshots take the markups inside them along.
//!
//! The canvas cannot reach the app's state, so [`frame`] passes the options it needs to this
//! thread each frame ([`opts`]) together with the looks remembered per markup kind.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use egui::RichText;
use markupcraft_engine::Session;
use markupcraft_geom::Rect;
use markupcraft_model::{Kind, Markup};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkupPrefs {
    /// A text box grows or shrinks to fit its text (else it keeps the box it was drawn with).
    pub autosize_text: bool,
    /// A tool draws with the properties its last markup was given.
    pub remember_last: bool,
    /// Resizing a shape from a corner scales its line width and text size too.
    pub scale_appearance: bool,
    /// Pop-ups show the author and the date over the comment (else the subject).
    pub popup_author_date: bool,
    /// Open pop-ups print (as boxes with their author, date and comment).
    pub print_popups: bool,
    /// A highlight, underline, strikethrough or squiggly over page text takes that text as its
    /// comment.
    pub copy_text_to_comment: bool,
    /// Pictures placed as markups are stored as JPEG (smaller, lossy) at
    /// [`Self::jpeg_quality`], else losslessly.
    pub jpeg_images: bool,
    pub jpeg_quality: u8,
    /// Shapes are drawn from their centre (Alt then draws from a corner).
    pub draw_from_center: bool,
    /// A snapshot takes the markups inside its box along (pasted with it).
    pub snapshot_markups: bool,
}

impl Default for MarkupPrefs {
    fn default() -> Self {
        Self {
            autosize_text: true,
            remember_last: false,
            scale_appearance: false,
            popup_author_date: true,
            print_popups: false,
            copy_text_to_comment: false,
            jpeg_images: false,
            jpeg_quality: 85,
            draw_from_center: false,
            snapshot_markups: false,
        }
    }
}

impl MarkupPrefs {
    pub fn sanitize(&mut self) {
        self.jpeg_quality = self.jpeg_quality.clamp(10, 100);
    }
}

/// What the canvas needs, copied each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opts {
    pub autosize_text: bool,
    pub remember_last: bool,
    pub scale_appearance: bool,
    pub popup_author_date: bool,
    pub copy_text_to_comment: bool,
    pub draw_from_center: bool,
    pub snapshot_markups: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self::from(&MarkupPrefs::default())
    }
}

impl From<&MarkupPrefs> for Opts {
    fn from(p: &MarkupPrefs) -> Self {
        Self {
            autosize_text: p.autosize_text,
            remember_last: p.remember_last,
            scale_appearance: p.scale_appearance,
            popup_author_date: p.popup_author_date,
            copy_text_to_comment: p.copy_text_to_comment,
            draw_from_center: p.draw_from_center,
            snapshot_markups: p.snapshot_markups,
        }
    }
}

thread_local! {
    static OPTS: Cell<Opts> = Cell::new(Opts::default());
    static REMEMBERED: RefCell<HashMap<Kind, Markup>> = RefCell::new(HashMap::new());
}

/// This thread's options (the interface's, set each frame).
pub fn opts() -> Opts {
    OPTS.with(Cell::get)
}

/// Set this thread's options.
pub fn set(p: &MarkupPrefs) {
    OPTS.with(|o| o.set(Opts::from(p)));
}

/// Remember `m`'s look for its kind (when Remember Last Properties is on).
pub fn remember(m: &Markup) {
    if !opts().remember_last {
        return;
    }
    REMEMBERED.with(|r| {
        let mut r = r.borrow_mut();
        let same = r.get(&m.kind).is_some_and(|old| old == m);
        if !same {
            let mut look = m.clone();
            // The look only: no geometry, text or replies to carry.
            look.pts.clear();
            look.contents.clear();
            look.replies.clear();
            look.rich = Default::default();
            r.insert(m.kind, look);
        }
    });
}

/// The look remembered for markups of `kind`, if any.
pub fn remembered(kind: Kind) -> Option<Markup> {
    if !opts().remember_last {
        return None;
    }
    REMEMBERED.with(|r| r.borrow().get(&kind).cloned())
}

/// Each frame: pass the options on (to the canvas, the print dialog and the documents) and
/// remember the look of the one markup selected.
pub fn frame(app: &mut AppState) {
    let p = app.shell.ui.markup.clone();
    set(&p);
    app.features.print.popups = p.print_popups;
    let jpeg = p.jpeg_images.then_some(p.jpeg_quality);
    for d in &mut app.docs {
        if d.session.image_jpeg_quality() != jpeg {
            d.session.set_image_encoding(jpeg);
        }
    }
    if p.remember_last
        && let Some(d) = app.doc()
        && let [id] = d.session.selection()
        && let Some(m) = d.session.doc().find(id)
    {
        remember(m);
    }
}

/// The page text under a text markup's quads (four points each), one quad after another.
pub fn text_under(session: &Session, m: &Markup) -> String {
    let mut parts: Vec<String> = Vec::new();
    for q in m.pts.chunks(4).take(500) {
        let xs = q.iter().map(|p| p.x);
        let ys = q.iter().map(|p| p.y);
        let r = Rect::new(
            xs.clone().fold(f64::INFINITY, f64::min),
            ys.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.fold(f64::NEG_INFINITY, f64::max),
        );
        if let Ok(t) = session.text_in_rect(m.page, r) {
            let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if !t.is_empty() {
                parts.push(t);
            }
        }
    }
    parts.join(" ")
}

/// Whether a kind marks page text (highlight, underline, strikethrough, squiggly).
pub fn marks_text(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::TextHighlight | Kind::Underline | Kind::Strikeout | Kind::Squiggly
    )
}

/// How much a box grew: the square root of the area ratio (1 for a degenerate box).
pub fn scale_factor(old: Rect, new: Rect) -> f64 {
    let (a, b) = (old.normalized(), new.normalized());
    let (oa, na) = (a.width() * a.height(), b.width() * b.height());
    if oa > 1e-9 && na.is_finite() && na > 1e-9 {
        (na / oa).sqrt().clamp(0.01, 100.0)
    } else {
        1.0
    }
}

/// The page's options (drawn under Tools > Markup).
pub fn section(ui: &mut egui::Ui, p: &mut MarkupPrefs) {
    ui.add_space(6.0);
    ui.label(RichText::new("Markup behaviour").strong());
    ui.checkbox(&mut p.autosize_text, "Autosize text boxes to their text");
    ui.checkbox(
        &mut p.remember_last,
        "Remember last properties (each tool draws like its last markup)",
    );
    ui.checkbox(
        &mut p.scale_appearance,
        "Scale line width and text size when a shape is resized from a corner",
    );
    ui.checkbox(&mut p.popup_author_date, "Show the author and date in pop-ups");
    ui.checkbox(&mut p.print_popups, "Print open pop-ups");
    ui.checkbox(
        &mut p.copy_text_to_comment,
        "Copy the highlighted text into the comment",
    );
    ui.horizontal(|ui| {
        ui.label("Pictures are stored");
        ui.radio_value(&mut p.jpeg_images, false, "losslessly");
        ui.radio_value(&mut p.jpeg_images, true, "as JPEG, quality");
        ui.add_enabled(p.jpeg_images, egui::DragValue::new(&mut p.jpeg_quality).range(10..=100));
    });
    ui.horizontal(|ui| {
        ui.label("Shapes are drawn from");
        ui.radio_value(&mut p.draw_from_center, false, "a corner");
        ui.radio_value(&mut p.draw_from_center, true, "the centre");
        ui.label(RichText::new("(Alt for the other)").weak());
    });
    ui.checkbox(&mut p.snapshot_markups, "Snapshots include the markups inside them");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_looks_follow_the_option() {
        set(&MarkupPrefs::default());
        let mut m = Markup::new(Kind::Rectangle, 0, Vec::new());
        m.line_width = 7.0;
        remember(&m);
        assert!(remembered(Kind::Rectangle).is_none(), "off by default");
        set(&MarkupPrefs {
            remember_last: true,
            ..Default::default()
        });
        remember(&m);
        assert_eq!(remembered(Kind::Rectangle).map(|r| r.line_width), Some(7.0));
        assert!(remembered(Kind::Ellipse).is_none());
        assert!((scale_factor(Rect::new(0.0, 0.0, 10.0, 10.0), Rect::new(0.0, 0.0, 20.0, 20.0)) - 2.0).abs() < 1e-9);
        assert_eq!(
            scale_factor(Rect::new(0.0, 0.0, 0.0, 10.0), Rect::new(0.0, 0.0, 20.0, 20.0)),
            1.0
        );
    }
}
