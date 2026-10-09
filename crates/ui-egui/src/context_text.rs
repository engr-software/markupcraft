//! The right-click menu on page text: the word under the pointer can be copied, highlighted,
//! struck through or underlined, or searched for.

use markupcraft_geom::{Point, Rect};
use markupcraft_model::Kind;

use crate::DocTab;
use crate::actions;
use crate::canvas::CanvasOut;

/// The word of the page text under `at`, if any.
pub fn word_at(doc: &DocTab, page: usize, at: Point) -> Option<(String, Rect)> {
    let words = doc.session.page_words(page).ok()?;
    words
        .into_iter()
        .find(|w| {
            let r = w.rect.normalized();
            Rect::new(r.x0 - 1.0, r.y0 - 1.0, r.x1 + 1.0, r.y1 + 1.0).contains(at)
        })
        .map(|w| (w.text, w.rect.normalized()))
}

fn quad(r: &Rect) -> Vec<Point> {
    vec![
        Point::new(r.x0, r.y1),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
    ]
}

/// The text rows of the page menu (nothing when the pointer is not on text).
pub fn rows(ui: &mut egui::Ui, doc: &mut DocTab, out: &mut CanvasOut, page: usize, at: Point) {
    // looked up once per menu (the page's text layer is not free to read)
    let key = egui::Id::new(("context-word", doc.uid, page, at.x.to_bits(), at.y.to_bits()));
    let found = match ui.ctx().data(|d| d.get_temp::<Option<(String, Rect)>>(key)) {
        Some(f) => f,
        None => {
            let f = word_at(doc, page, at);
            ui.ctx().data_mut(|d| d.insert_temp(key, f.clone()));
            f
        }
    };
    let Some((text, rect)) = found else { return };
    ui.label(egui::RichText::new(format!("Text: {text}")).weak());
    if ui.button("Copy Text").clicked() {
        ui.ctx().copy_text(text.clone());
        out.status = Some(format!("Copied {text:?}"));
        ui.close();
    }
    for (label, kind) in [
        ("Highlight Text", Kind::TextHighlight),
        ("Underline Text", Kind::Underline),
        ("Strikethrough Text", Kind::Strikeout),
    ] {
        if ui.button(label).clicked() {
            let r = crate::tools::new_markup(kind, page, &quad(&rect))
                .ok_or_else(|| markupcraft_engine::EngineError::Invalid("cannot mark this text".into()))
                .and_then(|mut m| {
                    m.contents = text.clone();
                    doc.session.add_markup(m)
                });
            out.status = Some(actions::report(r, |_| format!("{label}: {text}")));
            ui.close();
        }
    }
    if ui.button("Search for This Text").clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("context-search-text"), text.clone()));
        out.commands.push("tools.search".into());
        ui.close();
    }
    ui.separator();
}

/// A search asked for from the text menu: the query to put in the Search panel.
pub fn take_search(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("context-search-text")))
}
