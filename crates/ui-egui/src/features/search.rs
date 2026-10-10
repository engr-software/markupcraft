//! Search (Ctrl+F) and Visual Search: the state behind the Search panel, its actions on the
//! results (highlight, underline, squiggly, strikethrough, hyperlink, bookmark, count,
//! redact), and the hits drawn on the pages.

use std::path::PathBuf;

use markupcraft_engine::search::SearchOptions;
use markupcraft_engine::search_more::{FoundHit, HitSource, SearchTargets, folder_pdfs, search_files};
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
    /// Every open document.
    AllOpen,
    /// Every file of the Set open in the Sets panel.
    CurrentSet,
    /// Every PDF in a folder (and its subfolders when asked).
    Folder,
    /// The recent files.
    Recent,
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
    /// Another open document the hit is in (its uid), or a file that is not open.
    pub doc: Option<u64>,
    pub file: Option<PathBuf>,
    /// Where it was found (page text, a property, a form field ...).
    pub source: String,
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
    pub file_names: bool,
    pub properties: bool,
    pub form_fields: bool,
    pub folder: String,
    pub subfolders: bool,
    /// Replace: the new text, and whether a line whose font cannot show it uses Helvetica.
    pub replace: String,
    pub fallback: bool,
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
    pub fine_rotations: bool,
    pub color_filter: bool,
    pub limit_to_selection: bool,
    pub visual_scope: Scope,
    pub visual_hits: Vec<Hit>,
    /// Visual results thumbnails, by result index.
    pub thumbs: std::collections::HashMap<usize, egui::TextureHandle>,
    /// Hyperlink on checked results: where the links go (a web address or a page number).
    pub link_to: String,
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
            file_names: false,
            properties: false,
            form_fields: false,
            folder: String::new(),
            subfolders: true,
            replace: String::new(),
            fallback: true,
            scope: Scope::AllPages,
            range: String::new(),
            doc: 0,
            hits: Vec::new(),
            current: None,
            message: String::new(),
            region: None,
            sensitivity: 0.5,
            rotations: true,
            fine_rotations: false,
            color_filter: false,
            limit_to_selection: false,
            visual_scope: Scope::AllPages,
            visual_hits: Vec::new(),
            thumbs: Default::default(),
            link_to: "https://".into(),
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
        for (i, h) in self.results().iter().enumerate() {
            let mine = match (&h.doc, &h.file) {
                (Some(u), _) => *u == d.uid,
                (None, Some(f)) => d.path.as_ref() == Some(f),
                (None, None) => d.uid == self.doc,
            };
            if !mine {
                continue;
            }
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
        Scope::AllPages | Scope::AllOpen | Scope::CurrentSet | Scope::Folder | Scope::Recent => {
            Some((0..count).collect())
        }
        Scope::CurrentPage => Some(vec![current.min(count.saturating_sub(1))]),
        Scope::Pages => super::parse_pages(range, count),
    }
}

fn found_to_hit(h: FoundHit, doc: Option<u64>, file: Option<PathBuf>) -> Hit {
    let markup = match &h.source {
        HitSource::Markup(id) => Some(id.clone()),
        _ => None,
    };
    Hit {
        page: h.page.unwrap_or(0),
        rects: h.rects,
        text: h.text,
        context: h.context,
        markup,
        checked: true,
        score: None,
        doc,
        file,
        source: h.source.label(),
    }
}

fn targets(s: &SearchState) -> SearchTargets {
    SearchTargets {
        page_text: s.page_text,
        markups: s.markups,
        file_name: s.file_names,
        properties: s.properties,
        form_fields: s.form_fields,
    }
}

/// Search other documents: every open one, the Set's files, or a folder.
fn run_many(app: &mut AppState, needle: &str) {
    let s = &app.features.search;
    let opts = SearchOptions {
        case_sensitive: s.case_sensitive,
        whole_words: s.whole_words,
        pages: None,
        max_hits: 5_000,
    };
    let t = targets(s);
    let mut hits = Vec::new();
    let mut notes = Vec::new();
    match s.scope {
        Scope::AllOpen => {
            for d in &app.docs {
                match d.session.search_all(needle, &opts, &t) {
                    Ok(v) => hits.extend(v.into_iter().map(|h| found_to_hit(h, Some(d.uid), None))),
                    Err(e) => notes.push(format!("{}: {e}", d.name)),
                }
            }
        }
        Scope::CurrentSet | Scope::Folder | Scope::Recent => {
            let files: Result<Vec<PathBuf>, String> = if s.scope == Scope::Recent {
                Ok(app
                    .shell
                    .recent
                    .files
                    .iter()
                    .map(|f| f.path.clone())
                    .filter(|p| p.is_file())
                    .collect())
            } else if s.scope == Scope::CurrentSet {
                let f = app.features.sets.files();
                if f.is_empty() {
                    Err("Open a Set in the Sets panel first".into())
                } else {
                    Ok(f)
                }
            } else if s.folder.trim().is_empty() {
                Err("Type the folder to search".into())
            } else {
                folder_pdfs(std::path::Path::new(s.folder.trim()), s.subfolders).map_err(|e| e.to_string())
            };
            match files {
                Ok(files) => {
                    // Files already open are searched as they are now.
                    let (open, closed): (Vec<PathBuf>, Vec<PathBuf>) = files
                        .into_iter()
                        .partition(|f| app.docs.iter().any(|d| d.path.as_ref() == Some(f)));
                    for f in open {
                        if let Some(d) = app.docs.iter().find(|d| d.path.as_ref() == Some(&f))
                            && let Ok(v) = d.session.search_all(needle, &opts, &t)
                        {
                            hits.extend(v.into_iter().map(|h| found_to_hit(h, Some(d.uid), None)));
                        }
                    }
                    for fh in search_files(&closed, needle, &opts, &t) {
                        if let Some(e) = fh.error {
                            notes.push(format!("{}: {e}", fh.path.display()));
                        }
                        let p = fh.path.clone();
                        hits.extend(fh.hits.into_iter().map(|h| found_to_hit(h, None, Some(p.clone()))));
                    }
                }
                Err(e) => notes.push(e),
            }
        }
        _ => {}
    }
    let s = &mut app.features.search;
    s.hits = hits;
    s.message = format!(
        "{} for \"{needle}\"{}",
        actions::plural(s.hits.len(), "result"),
        if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join("; "))
        }
    );
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
    if matches!(
        s.scope,
        Scope::AllOpen | Scope::CurrentSet | Scope::Folder | Scope::Recent
    ) {
        run_many(app, &needle);
        if !app.features.search.hits.is_empty() {
            go_to(app, 0);
        }
        return;
    }
    let count = d.session.page_count();
    let Some(pages) = scope_pages(s.scope, &s.range, d.view.current, count) else {
        s.message = "Pages: a range like 1-3, 5".into();
        return;
    };
    let mut notes = Vec::new();
    if s.file_names || s.properties || s.form_fields {
        let t = SearchTargets {
            page_text: false,
            markups: false,
            ..targets(s)
        };
        let o = SearchOptions {
            case_sensitive: s.case_sensitive,
            whole_words: s.whole_words,
            pages: Some(pages.clone()),
            max_hits: 5_000,
        };
        match d.session.search_all(&needle, &o, &t) {
            Ok(v) => s.hits.extend(v.into_iter().map(|h| found_to_hit(h, None, None))),
            Err(e) => notes.push(e.to_string()),
        }
    }
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
                    doc: None,
                    file: None,
                    source: "text".into(),
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
                    doc: None,
                    file: None,
                    source: "markup".into(),
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
    // A hit in another document: bring it forward (opening the file when needed).
    if let Some(f) = &h.file
        && !app.docs.iter().any(|d| d.path.as_ref() == Some(f))
    {
        app.open_path(f);
    }
    let target = match (&h.doc, &h.file) {
        (Some(u), _) => app.docs.iter().position(|d| d.uid == *u),
        (None, Some(f)) => app.docs.iter().position(|d| d.path.as_ref() == Some(f)),
        (None, None) => app.docs.iter().position(|d| d.uid == app.features.search.doc),
    };
    let Some(pos) = target else { return };
    app.active = pos;
    let Some(d) = app.docs.get_mut(pos) else {
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
    Underline,
    Squiggly,
    Strikeout,
    /// A link over each result, to [`SearchState::link_to`].
    Hyperlink,
    /// A bookmark to each result's page, titled with the result.
    Bookmark,
    Count,
    Redact,
}

impl Bulk {
    /// The text markups that only make sense on page text.
    pub fn text_only(self) -> bool {
        matches!(self, Bulk::Underline | Bulk::Squiggly | Bulk::Strikeout)
    }
}

/// Where Hyperlink sends the links: a page number of this document or a web address.
pub fn link_target(text: &str, pages: usize) -> Result<markupcraft_engine::links::LinkTarget, String> {
    use markupcraft_engine::links::LinkTarget;
    let t = text.trim();
    if let Ok(n) = t.trim_start_matches(['p', 'P', '.', ' ']).parse::<usize>() {
        return match n.checked_sub(1).filter(|p| *p < pages) {
            Some(p) => Ok(LinkTarget::Page(p)),
            None => Err(format!("Link to: a page from 1 to {pages}")),
        };
    }
    if markupcraft_engine::webtab::check_url(t).is_ok() {
        return Ok(LinkTarget::Url(t.to_string()));
    }
    Err("Link to: a web address (https://...) or a page number".into())
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
    let visual = app.features.search.visual;
    let link_to = app.features.search.link_to.clone();
    let threads = app.threads;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    if hits.is_empty() {
        app.status = "Check the results to act on".into();
        return;
    }
    let r = match what {
        Bulk::Highlight if visual => {
            let list: Vec<Markup> = hits
                .iter()
                .filter_map(|h| {
                    let r = h.rects.first()?.padded(1.0);
                    let mut m = crate::tools::new_markup(
                        Kind::Rectangle,
                        h.page,
                        &[Point::new(r.x0, r.y0), Point::new(r.x1, r.y1)],
                    )?;
                    m.fill = Some(m.color);
                    m.fill_opacity = 0.3;
                    m.opacity = 0.6;
                    m.line_width = 1.0;
                    Some(m)
                })
                .collect();
            d.session
                .add_new_markups("Highlight Search Results", list)
                .map(|v| format!("Highlighted {}", actions::plural(v.len(), "result")))
        }
        Bulk::Underline | Bulk::Squiggly | Bulk::Strikeout => {
            let kind = match what {
                Bulk::Underline => Kind::Underline,
                Bulk::Squiggly => Kind::Squiggly,
                _ => Kind::Strikeout,
            };
            let list: Vec<Markup> = hits
                .iter()
                .filter(|h| h.source == "text" || h.source.is_empty())
                .filter_map(|h| {
                    let q: Vec<Point> = h.rects.iter().flat_map(quad).collect();
                    let mut m = crate::tools::new_markup(kind, h.page, &q)?;
                    m.contents = h.text.clone();
                    Some(m)
                })
                .collect();
            let n = list.len();
            if n == 0 {
                Ok("Only page-text results take text markups".to_string())
            } else {
                d.session
                    .add_new_markups(&format!("{} Search Results", kind.name()), list)
                    .map(|v| format!("Marked {} ({})", actions::plural(v.len(), "result"), kind.name()))
            }
        }
        Bulk::Hyperlink => match link_target(&link_to, d.session.page_count()) {
            Ok(target) => {
                let items: Vec<(usize, Rect)> = hits
                    .iter()
                    .flat_map(|h| h.rects.iter().map(move |r| (h.page, *r)))
                    .collect();
                let r = d
                    .session
                    .add_links(&items, &target, Default::default())
                    .map(|v| format!("Linked {}", actions::plural(v.len(), "result")));
                d.rerender(threads);
                r
            }
            Err(e) => Ok(e),
        },
        Bulk::Bookmark => {
            let items: Vec<(String, usize)> = hits
                .iter()
                .map(|h| {
                    let t = if h.text.trim().is_empty() {
                        format!("Page {}", h.page + 1)
                    } else {
                        h.text.clone()
                    };
                    (format!("{} (p. {})", t.trim(), h.page + 1), h.page)
                })
                .collect();
            d.session
                .add_bookmarks(&items)
                .map(|n| format!("Added {}", actions::plural(n, "bookmark")))
        }
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
    s.thumbs.clear();
    let opts = VisualSearchOptions {
        page,
        region,
        pages,
        sensitivity: s.sensitivity.clamp(0.0, 1.0),
        rotations: s.rotations,
        fine_rotations: s.fine_rotations,
        color_filter: s.color_filter,
        limit_to_selection: s.limit_to_selection,
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
                    doc: None,
                    file: None,
                    source: "visual".into(),
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

/// Replace Checked: the checked text results' text replaced in the page content.
pub fn replace_checked(app: &mut AppState) {
    let threads = app.threads;
    let s = &app.features.search;
    let needle = s.query.trim().to_string();
    let with = s.replace.clone();
    let only: Vec<(usize, Rect)> = s
        .hits
        .iter()
        .filter(|h| h.checked && h.markup.is_none() && h.doc.is_none() && h.file.is_none() && h.source == "text")
        .flat_map(|h| h.rects.iter().map(move |r| (h.page, *r)))
        .collect();
    if only.is_empty() {
        app.status = "Check the page-text results to replace".into();
        return;
    }
    let opts = SearchOptions {
        case_sensitive: s.case_sensitive,
        whole_words: s.whole_words,
        pages: None,
        max_hits: 0,
    };
    let fallback = s.fallback;
    let uid = s.doc;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    let r = d.session.replace_text(&needle, &with, &opts, Some(&only), fallback);
    d.rerender(threads);
    let msg = actions::report(r, |r| {
        format!(
            "Replaced {} in {} ({} in Helvetica, {} skipped)",
            actions::plural(r.replaced, "occurrence"),
            actions::plural(r.lines, "line"),
            r.substituted,
            r.skipped
        )
    });
    app.status = msg.clone();
    app.features.search.message = msg;
    app.features.search.hits.clear();
    app.features.search.current = None;
}

/// Search Selected Text: the box picked on the page becomes the search.
pub fn selection_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    let Some(d) = app.doc() else { return };
    match d.session.text_in_rect(page, r) {
        Ok(t) if !t.trim().is_empty() => {
            let s = &mut app.features.search;
            s.query = t;
            s.visual = false;
            app.show_panel("search");
            run_text(app);
        }
        Ok(_) => app.status = "No text in the box".into(),
        Err(e) => app.status = e.to_string(),
    }
}

/// A thumbnail of visual result `i` (made on first use).
pub fn thumb(app: &mut AppState, ctx: &egui::Context, i: usize) -> Option<egui::TextureHandle> {
    if let Some(t) = app.features.search.thumbs.get(&i) {
        return Some(t.clone());
    }
    let h = app.features.search.visual_hits.get(i)?.clone();
    let r = h.rects.first()?.padded(2.0);
    let uid = app.features.search.doc;
    let d = app.docs.iter().find(|d| d.uid == uid)?;
    let (w, hh, rgba) = d.session.region_rgba(h.page, r, 48).ok()?;
    let img = egui::ColorImage::from_rgba_unmultiplied([w, hh], &rgba);
    let tex = ctx.load_texture(format!("visual-hit-{i}"), img, egui::TextureOptions::LINEAR);
    app.features.search.thumbs.insert(i, tex.clone());
    Some(tex)
}

/// Clear the results showing.
pub fn clear(app: &mut AppState) {
    let s = &mut app.features.search;
    s.thumbs.clear();
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
