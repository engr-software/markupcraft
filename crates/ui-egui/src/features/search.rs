//! Search (Ctrl+F) and Visual Search: the state behind the Search panel, its actions on the
//! results (highlight, count, redact), and the hits drawn on the pages.

use markupcraft_engine::search::SearchOptions;
use markupcraft_engine::visual::{VisualAction, VisualSearchOptions};
use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Kind, Markup};

use super::{Mark, Pick, canvas};
use crate::{AppState, DocTab, actions};

/// Which pages a search covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    AllPages,
    CurrentPage,
    Pages,
}

/// One result.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub rects: Vec<Rect>,
    pub text: String,
    pub context: String,
    /// A markup whose text matched (markup search).
    pub markup: Option<String>,
    pub checked: bool,
    /// Visual Search similarity, 0..1.
    pub score: Option<f64>,
}

pub struct SearchState {
    /// The Visual Search tab is showing.
    pub visual: bool,
    /// Put the cursor in the search box next frame.
    pub focus: bool,
    pub query: String,
    pub case_sensitive: bool,
    pub whole_words: bool,
    pub page_text: bool,
    pub markups: bool,
    pub scope: Scope,
    pub range: String,
    /// The document the results belong to.
    pub doc: u64,
    pub hits: Vec<Hit>,
    pub current: Option<usize>,
    pub message: String,
    // Visual Search
    pub region: Option<(usize, Rect)>,
    pub sensitivity: f64,
    pub rotations: bool,
    pub visual_scope: Scope,
    pub visual_hits: Vec<Hit>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            visual: false,
            focus: false,
            query: String::new(),
            case_sensitive: false,
            whole_words: false,
            page_text: true,
            markups: false,
            scope: Scope::AllPages,
            range: String::new(),
            doc: 0,
            hits: Vec::new(),
            current: None,
            message: String::new(),
            region: None,
            sensitivity: 0.5,
            rotations: true,
            visual_scope: Scope::AllPages,
            visual_hits: Vec::new(),
        }
    }
}

impl SearchState {
    /// The results showing (text or visual).
    pub fn results(&self) -> &[Hit] {
        if self.visual { &self.visual_hits } else { &self.hits }
    }

    pub fn results_mut(&mut self) -> &mut Vec<Hit> {
        if self.visual {
            &mut self.visual_hits
        } else {
            &mut self.hits
        }
    }

    pub fn marks(&self, d: &DocTab, out: &mut Vec<Mark>) {
        if d.uid != self.doc {
            return;
        }
        for (i, h) in self.results().iter().enumerate() {
            let cur = self.current == Some(i);
            let (fill, stroke) = if cur {
                (canvas::CURRENT_FILL, canvas::CURRENT_STROKE)
            } else {
                (canvas::HIT_FILL, canvas::HIT_STROKE)
            };
            for r in &h.rects {
                out.push(Mark::rect(h.page, r.padded(1.0), fill, stroke));
            }
        }
        if self.visual
            && let Some((page, r)) = self.region
        {
            out.push(Mark::rect(
                page,
                r,
                egui::Color32::from_rgba_premultiplied(0, 40, 0, 40),
                egui::Color32::from_rgb(0, 160, 60),
            ));
        }
    }
}

fn scope_pages(scope: Scope, range: &str, current: usize, count: usize) -> Option<Vec<usize>> {
    match scope {
        Scope::AllPages => Some((0..count).collect()),
        Scope::CurrentPage => Some(vec![current.min(count.saturating_sub(1))]),
        Scope::Pages => super::parse_pages(range, count),
    }
}

/// Run the text search on the active document.
pub fn run_text(app: &mut AppState) {
    let Some(d) = app.docs.get(app.active) else { return };
    let s = &mut app.features.search;
    s.doc = d.uid;
    s.hits.clear();
    s.current = None;
    let needle = s.query.trim().to_string();
    if needle.is_empty() {
        s.message = "Type what to find".into();
        return;
    }
    let count = d.session.page_count();
    let Some(pages) = scope_pages(s.scope, &s.range, d.view.current, count) else {
        s.message = "Pages: a range like 1-3, 5".into();
        return;
    };
    let mut notes = Vec::new();
    if s.page_text {
        let opts = SearchOptions {
            case_sensitive: s.case_sensitive,
            whole_words: s.whole_words,
            pages: Some(pages.clone()),
            max_hits: 5_000,
        };
        match d.session.search_text(&needle, &opts) {
            Ok(rep) => {
                if !rep.unreadable.is_empty() {
                    notes.push(format!(
                        "{} without text (OCR them first)",
                        actions::plural(rep.unreadable.len(), "page")
                    ));
                }
                if rep.truncated {
                    notes.push("only the first 5000 shown".into());
                }
                s.hits.extend(rep.hits.into_iter().map(|h| Hit {
                    page: h.page,
                    rects: h.rects,
                    text: h.text,
                    context: h.context,
                    markup: None,
                    checked: true,
                    score: None,
                }));
            }
            Err(e) => notes.push(e.to_string()),
        }
    }
    if s.markups {
        for m in d.session.doc().markups.iter().filter(|m| pages.contains(&m.page)) {
            if let Some(text) = markup_match(m, &needle, s.case_sensitive, s.whole_words) {
                s.hits.push(Hit {
                    page: m.page,
                    rects: vec![actions::markup_bbox(m)],
                    text: m.subject.clone(),
                    context: text,
                    markup: Some(m.id.clone()),
                    checked: true,
                    score: None,
                });
            }
        }
    }
    s.hits.sort_by_key(|h| h.page);
    s.message = format!(
        "{} for \"{needle}\"{}",
        actions::plural(s.hits.len(), "result"),
        if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join("; "))
        }
    );
    if !s.hits.is_empty() {
        go_to(app, 0);
    }
}

/// The markup text that matches `needle` (subject, label, comment, author), if any.
pub fn markup_match(m: &Markup, needle: &str, case: bool, whole: bool) -> Option<String> {
    let fold = |s: &str| if case { s.to_string() } else { s.to_lowercase() };
    let n = fold(needle);
    for t in [&m.contents, &m.subject, &m.label, &m.author] {
        let h = fold(t);
        let mut from = 0;
        while let Some(i) = h.get(from..).and_then(|rest| rest.find(&n)).map(|i| i + from) {
            let before = h.get(..i).and_then(|s| s.chars().next_back());
            let after = h.get(i + n.len()..).and_then(|s| s.chars().next());
            let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
            if !whole || (!word(before) && !word(after)) {
                return Some(t.chars().take(120).collect());
            }
            from = i + n.len().max(1);
        }
    }
    None
}

/// Show result `i`: its page, centred, and its markup selected.
pub fn go_to(app: &mut AppState, i: usize) {
    let Some(h) = app.features.search.results().get(i).cloned() else {
        return;
    };
    app.features.search.current = Some(i);
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == app.features.search.doc) else {
        return;
    };
    if let Some(r) = h.rects.first()
        && let Some(render) = d.render.as_ref()
    {
        let c = Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
        d.view.center_on(h.page, c, render.pages());
    } else {
        let n = d.session.page_count();
        d.view.go_to_page(h.page, n);
    }
    if let Some(id) = &h.markup {
        actions::select(&mut d.session, vec![id.clone()]);
    }
}

/// Next (`step` 1) or previous (-1) result.
pub fn step(app: &mut AppState, step: isize) {
    let n = app.features.search.results().len();
    if n == 0 {
        return;
    }
    let cur = app.features.search.current.map_or(-1, |c| c as isize);
    let next = (cur + step).rem_euclid(n as isize) as usize;
    go_to(app, next);
}

/// Text quads of a hit rectangle (upper-left, upper-right, lower-left, lower-right).
fn quad(r: &Rect) -> [Point; 4] {
    [
        Point::new(r.x0, r.y1),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
    ]
}

/// What to do with the checked results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bulk {
    Highlight,
    Count,
    Redact,
}

/// Apply a markup to the checked results (one undo step).
pub fn apply(app: &mut AppState, what: Bulk) {
    let hits: Vec<Hit> = app
        .features
        .search
        .results()
        .iter()
        .filter(|h| h.checked && h.markup.is_none())
        .cloned()
        .collect();
    let uid = app.features.search.doc;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    if hits.is_empty() {
        app.status = "Check the results to act on".into();
        return;
    }
    let r = match what {
        Bulk::Highlight => {
            let list: Vec<Markup> = hits
                .iter()
                .filter_map(|h| {
                    let q: Vec<Point> = h.rects.iter().flat_map(quad).collect();
                    let mut m = crate::tools::new_markup(Kind::TextHighlight, h.page, &q)?;
                    m.contents = h.text.clone();
                    Some(m)
                })
                .collect();
            d.session
                .add_new_markups("Highlight Search Results", list)
                .map(|v| format!("Highlighted {}", actions::plural(v.len(), "result")))
        }
        Bulk::Count => {
            let mut by_page: std::collections::BTreeMap<usize, Vec<Point>> = Default::default();
            for h in &hits {
                if let Some(r) = h.rects.first() {
                    by_page
                        .entry(h.page)
                        .or_default()
                        .push(Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0));
                }
            }
            let label = app.features.search.query.trim().to_string();
            let list: Vec<Markup> = by_page
                .into_iter()
                .filter_map(|(page, pts)| {
                    let mut m = crate::tools::new_markup(Kind::Count, page, &pts)?;
                    m.pts = pts;
                    if !label.is_empty() {
                        m.subject = label.clone();
                    }
                    Some(m)
                })
                .collect();
            d.session
                .add_new_markups("Count Search Results", list)
                .map(|_| format!("Counted {}", actions::plural(hits.len(), "result")))
        }
        Bulk::Redact => {
            let mut by_page: std::collections::BTreeMap<usize, Vec<Rect>> = Default::default();
            for h in &hits {
                by_page.entry(h.page).or_default().extend(h.rects.iter().copied());
            }
            let marks: Vec<(usize, Vec<Rect>)> = by_page.into_iter().collect();
            d.session
                .redact_mark_many(&marks, &Default::default())
                .map(|n| format!("Marked {} for redaction", actions::plural(n, "area")))
        }
    };
    app.status = actions::report(r, |s| s);
}

/// The Visual Search region was picked.
pub fn region_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    match super::rect_of(pts) {
        Some(r) => {
            app.features.search.region = Some((page, r));
            app.features.search.visual = true;
            app.status = "Region set; press Search".into();
            app.show_panel("search");
        }
        None => app.status = "Drag a box around one symbol".into(),
    }
}

/// Start picking the Visual Search region.
pub fn pick_region(app: &mut AppState) {
    super::start_pick(app, Pick::VisualRegion, "Drag a box around the symbol to find");
}

/// Run Visual Search with the picked region.
pub fn run_visual(app: &mut AppState) {
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let s = &mut app.features.search;
    let Some((page, region)) = s.region else {
        s.message = "Pick the symbol's region first".into();
        return;
    };
    let count = d.session.page_count();
    let Some(pages) = scope_pages(s.visual_scope, &s.range, d.view.current, count) else {
        s.message = "Pages: a range like 1-3, 5".into();
        return;
    };
    let opts = VisualSearchOptions {
        page,
        region,
        pages,
        sensitivity: s.sensitivity.clamp(0.0, 1.0),
        rotations: s.rotations,
        action: VisualAction::None,
        ..Default::default()
    };
    s.doc = d.uid;
    s.current = None;
    match d.session.visual_search(&opts) {
        Ok(rep) => {
            s.visual_hits = rep
                .hits
                .into_iter()
                .map(|h| Hit {
                    page: h.page,
                    rects: vec![h.rect],
                    text: format!("{:.0}% match", h.score * 100.0),
                    context: if h.rotation == 0 {
                        String::new()
                    } else {
                        format!("rotated {} degrees", h.rotation)
                    },
                    markup: None,
                    checked: true,
                    score: Some(h.score),
                })
                .collect();
            s.message = actions::plural(s.visual_hits.len(), "match");
        }
        Err(e) => {
            s.visual_hits.clear();
            s.message = e.to_string();
        }
    }
}

/// Clear the results showing.
pub fn clear(app: &mut AppState) {
    let s = &mut app.features.search;
    s.results_mut().clear();
    s.current = None;
    s.message.clear();
    if s.visual {
        s.region = None;
    }
}

/// Nothing floats for search (the panel holds it); kept for symmetry with the other features.
pub fn window(_app: &mut AppState, _ctx: &egui::Context) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_text_matches() {
        let mut m = Markup::new(Kind::Rectangle, 0, Vec::new());
        m.contents = "Check the door swing".into();
        assert!(markup_match(&m, "door", false, true).is_some());
        assert!(markup_match(&m, "DOOR", false, false).is_some());
        assert!(markup_match(&m, "DOOR", true, false).is_none());
        assert!(markup_match(&m, "doo", false, true).is_none());
        assert_eq!(quad(&Rect::new(0.0, 0.0, 2.0, 1.0))[0], Point::new(0.0, 1.0));
    }
}
