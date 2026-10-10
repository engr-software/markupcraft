//! More markup and takeoff commands: the unified Measure tool (M), Recalculate, Import / Export
//! Markups (Ctrl+F3 / Ctrl+F2, XFDF or FDF), Review Text (Shift+Alt+R), Count series (Resume,
//! delete one item, split, merge) and the segment items of the canvas menu (Convert to Arc /
//! Line, a cutout as its own measurement).
//!
//! Hooks: [`COMMANDS`] joins the command table; `editing::handles` / `editing::run` route the
//! ids here; [`frame`] draws the Review Text window; [`context_items`] adds the canvas menu
//! rows; [`finish_resumed`] lets a resumed Count add to its series.

use egui::{Key, RichText};
use markupcraft_geom::Point;
use markupcraft_model::{Kind, Markup, measure_extras};

use crate::canvas::{CanvasCx, CanvasOut};
use crate::commands::{Command, ctrl, key, shift_alt};
use crate::dialogs::{Filter, Purpose};
use crate::interact::ContextTarget;
use crate::{AppState, DocTab, actions};

/// XFDF / FDF markup exchange files.
pub const XFDF: Filter = ("Markups (PDF, XFDF, FDF)", &["pdf", "xfdf", "fdf", "xml"]);

/// The measurement tools the Measure tool switches between (tool ids).
pub const MEASURE_MODES: &[&str] = &[
    "length",
    "polylength",
    "area",
    "perimeter",
    "count",
    "volume",
    "angle",
    "diameter",
    "radius",
];

/// State of these commands (kept in `EditState`).
#[derive(Debug, Clone)]
pub struct MoreState {
    /// Resume Count: new Count clicks add to this series.
    pub resume_count: Option<String>,
    /// The Review Text window is open; the row it is on.
    pub review_open: bool,
    pub review_at: usize,
    /// The mode the Measure tool starts in.
    pub measure_tool: &'static str,
    /// Measure > Make Annotations from Measurements (off: a measurement is only read out).
    pub make_annotations: bool,
    /// Measure > Keep Last Subject and Label: new measurements take the last one's.
    pub keep_subject: bool,
    /// The subject and label of the last measurement drawn, by kind.
    pub last_subject: std::collections::BTreeMap<Kind, (String, String)>,
    /// Markups the Markups List's filters leave out: drawn faded on the page.
    pub dimmed: std::collections::HashSet<String>,
    /// View > Line Weights: lines drawn at their weight (off: hairlines at every zoom).
    pub line_weights: bool,
    /// View > Rollover Comments: a markup's comments show when the pointer rests on it.
    pub rollover: bool,
    /// Page scales protected from change: (document, page)
    pub protected: std::collections::HashSet<(u64, usize)>,
    /// The Edit Action window: (markup id, web address, page number text)
    pub edit_action: Option<(String, String, String)>,
    /// View > Note Pop-ups: open note pop-ups are drawn on the page.
    pub popups: bool,
    /// Apply to Pages, Reply and the Markup Layer (`markups_more.rs`).
    pub g2: crate::markups_more::State,
}

impl Default for MoreState {
    fn default() -> Self {
        Self {
            resume_count: None,
            review_open: false,
            review_at: 0,
            measure_tool: "length",
            make_annotations: true,
            keep_subject: false,
            last_subject: std::collections::BTreeMap::new(),
            dimmed: std::collections::HashSet::new(),
            line_weights: true,
            rollover: true,
            protected: std::collections::HashSet::new(),
            edit_action: None,
            popups: true,
            g2: Default::default(),
        }
    }
}

const fn c(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    group: u8,
    keys: Option<crate::commands::Keys>,
    icon: &'static str,
) -> Command {
    Command {
        id,
        label,
        menu,
        group,
        keys,
        alias: None,
        icon,
        built: true,
    }
}

#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    c("measure.tool", "Measure", "Measure", 30, key(Key::M), "ruler"),
    c("measure.recalculate", "Recalculate Measurements", "Measure", 30, None, ""),
    c("measure.make_annotations", "Make Annotations from Measurements", "Measure", 31, None, ""),
    c("measure.keep_subject", "Keep Last Subject and Label", "Measure", 31, None, ""),
    c("view.rollover_comments", "Rollover Comments", "View", 30, None, ""),
    c("view.note_popups", "Note Pop-ups", "View", 30, None, ""),
    c("markup.import", "Import Markups...", "Markup", 30, ctrl(Key::F3), "upload"),
    c("markup.export", "Export Markups...", "Markup", 30, ctrl(Key::F2), "download"),
    c("markup.review_text", "Review Text", "Markup", 30, shift_alt(Key::R), "type"),
    c("markup.line_styles", "Line Styles...", "Markup", 30, None, ""),
    c("markup.save_attachment", "Save Attached File...", "", 0, None, ""),
    c("markup.summary_append", "Append Summary with Links", "Markup", 30, None, ""),
    c("window.takeoff_workspace", "Takeoff Workspace", "Window", 30, None, "ruler"),
    c("measure.resume_count", "Resume Count", "", 0, None, ""),
    c("measure.merge_counts", "Merge Counts", "", 0, None, ""),
];

pub fn handles(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id) || crate::markups_more::handles(id)
}

pub fn enabled(app: &AppState, id: &str) -> bool {
    if crate::markups_more::handles(id) {
        return crate::markups_more::enabled(app, id);
    }
    let sel = |kind: Kind, n: usize| {
        app.doc().is_some_and(|d| {
            let k: Vec<&Markup> = d
                .selection()
                .iter()
                .filter_map(|i| d.session.doc().find(i))
                .filter(|m| m.kind == kind)
                .collect();
            k.len() >= n
        })
    };
    match id {
        "measure.resume_count" => sel(Kind::Count, 1),
        "measure.merge_counts" => sel(Kind::Count, 2),
        "markup.save_attachment" => sel(Kind::Attachment, 1),
        "markup.edit_action" => app.doc().is_some_and(|d| d.selection().len() == 1),
        "markup.line_styles" => true,
        _ => app.has_doc(),
    }
}

pub fn run(app: &mut AppState, id: &str) {
    if crate::markups_more::handles(id) {
        crate::markups_more::run(app, id);
        return;
    }
    match id {
        "measure.tool" => {
            app.show_panel("measurements");
            let t = app.edit.more.measure_tool;
            app.set_tool(t);
            app.status = "Measure: pick the measurement in the Measurements panel".into();
        }
        "measure.recalculate" => {
            if let Some(d) = app.doc_mut() {
                let pages: Vec<usize> = (0..d.session.page_count()).collect();
                let r = d.session.recalculate_measurements(&pages);
                app.status = actions::report(r, |n| format!("Recalculated {}", actions::plural(n, "measurement")));
            }
        }
        "markup.import" => app
            .dialogs
            .open(Purpose::Edit("import_markups", String::new()), XFDF, false),
        "markup.export" => {
            let name = app
                .doc()
                .map(|d| format!("{}.xfdf", d.name.trim_end_matches(".pdf")))
                .unwrap_or_default();
            app.dialogs
                .save(Purpose::Edit("export_markups", String::new()), XFDF, &name);
        }
        "markup.review_text" => {
            app.edit.more.review_open = true;
            app.edit.more.review_at = 0;
        }
        "measure.make_annotations" => {
            let m = &mut app.edit.more;
            m.make_annotations = !m.make_annotations;
            app.status = if m.make_annotations {
                "Measurements are kept as markups".into()
            } else {
                "Temporary measurements: the value is read out, nothing is added".into()
            };
        }
        "measure.keep_subject" => app.edit.more.keep_subject = !app.edit.more.keep_subject,
        "view.rollover_comments" => app.edit.more.rollover = !app.edit.more.rollover,
        "view.note_popups" => app.edit.more.popups = !app.edit.more.popups,
        "markup.line_styles" => app.edit.chest_rt.styles_open = true,
        "markup.edit_action" => {
            if let Some(id) = app.doc().and_then(|d| d.selection().first().cloned()) {
                app.edit.more.edit_action = Some((id, String::new(), String::new()));
            }
        }
        "window.takeoff_workspace" => crate::features::partials_more::takeoff_profile(app),
        "markup.summary_append" => {
            let threads = app.threads;
            if let Some(d) = app.doc_mut() {
                let r = d.session.append_summary_with_links("", false);
                d.sync_pages(threads);
                app.status = actions::report(r, |n| {
                    format!(
                        "Appended a summary of {} with links to their pages",
                        actions::plural(n, "markup")
                    )
                });
            }
        }
        "markup.save_attachment" => {
            let found = app.doc().and_then(|d| {
                d.selection()
                    .iter()
                    .filter_map(|i| d.session.doc().find(i))
                    .find(|m| m.kind == Kind::Attachment)
                    .map(|m| (m.id.clone(), m.attachment_name.clone()))
            });
            if let Some((id, name)) = found {
                let name = if name.is_empty() {
                    "attachment".to_string()
                } else {
                    name
                };
                app.dialogs
                    .save(Purpose::Edit("save_attached", id), crate::features::ANY, &name);
            }
        }
        "document.unflatten" => {
            let threads = app.threads;
            if let Some(d) = app.doc_mut() {
                let r = d.session.unflatten_flattened(&[]);
                d.sync_pages(threads);
                app.status = actions::report(r, |n| format!("Unflattened {}", actions::plural(n, "markup")));
            }
        }
        "measure.resume_count" => {
            let id = app.doc().and_then(|d| {
                d.selection()
                    .iter()
                    .find(|i| d.session.doc().find(i).is_some_and(|m| m.kind == Kind::Count))
                    .cloned()
            });
            if let Some(id) = id {
                app.set_tool("count");
                app.edit.more.resume_count = Some(id);
                app.status = "Resume Count: each click adds to the series (Esc ends)".into();
            }
        }
        "measure.merge_counts" => {
            if let Some(d) = app.doc_mut() {
                let ids: Vec<String> = d
                    .selection()
                    .iter()
                    .filter(|i| d.session.doc().find(i).is_some_and(|m| m.kind == Kind::Count))
                    .cloned()
                    .collect();
                let r = d.session.merge_counts(&ids);
                app.status = actions::report(r, |n| format!("Merged into one count of {n}"));
            }
        }
        _ => {}
    }
}

/// The file dialog tags answered here.
pub fn handles_dialog(tag: &str) -> bool {
    matches!(
        tag,
        "import_markups" | "export_markups" | "attach_file" | "save_attached"
    )
}

/// File Attachment: the chosen file goes into a markup where the tool was clicked.
fn attach_answer(app: &mut AppState, arg: &str, path: &std::path::Path) -> String {
    let v: Vec<&str> = arg.split(' ').collect();
    let parse = || -> Option<(u64, usize, f64, f64)> {
        Some((
            v.first()?.parse().ok()?,
            v.get(1)?.parse().ok()?,
            v.get(2)?.parse().ok()?,
            v.get(3)?.parse().ok()?,
        ))
    };
    let Some((uid, page, x, y)) = parse() else {
        return "File Attachment: the click was lost".into();
    };
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return "File Attachment: the document was closed".into();
    };
    let r = d.session.attach_file_markup(page, Point::new(x, y), path, "PushPin");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let done = r.is_ok();
    let msg = actions::report(r, |_| format!("Attached {name}"));
    if done && !app.tool_locked {
        app.set_tool("select");
    }
    msg
}

pub fn dialog_answer(app: &mut AppState, tag: &str, arg: &str, path: &std::path::Path) -> String {
    if tag == "attach_file" {
        return attach_answer(app, arg, path);
    }
    let Some(d) = app.doc_mut() else {
        return "No document open".into();
    };
    match tag {
        "import_markups" if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) => {
            match d.session.import_markups_from_pdf(path) {
                Ok((n, skipped)) => format!(
                    "Imported {} from {}{}",
                    actions::plural(n, "markup"),
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    if skipped > 0 {
                        format!(" ({skipped} skipped)")
                    } else {
                        String::new()
                    }
                ),
                Err(e) => format!("Import failed: {e}"),
            }
        }
        "import_markups" => match d.session.import_xfdf_file(path) {
            Ok(r) => format!(
                "Imported {} from {}",
                actions::plural(r.imported, "markup"),
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            ),
            Err(e) => format!("Import failed: {e}"),
        },
        "save_attached" => match d
            .session
            .attachment_file(arg)
            .map_err(|e| e.to_string())
            .and_then(|(_, bytes)| crate::chest::write_atomic(path, &bytes))
        {
            Ok(()) => format!("Saved the attached file to {}", path.display()),
            Err(e) => format!("Save failed: {e}"),
        },
        "export_markups" => match d.session.export_xfdf_file(path) {
            Ok(_) => format!(
                "Exported {} to {}",
                actions::plural(d.session.doc().markups.len(), "markup"),
                path.display()
            ),
            Err(e) => format!("Export failed: {e}"),
        },
        _ => String::new(),
    }
}

/// Esc and tool changes end a resumed count.
pub fn escape(app: &mut AppState) {
    app.edit.more.resume_count = None;
}

/// Checkmarks of the toggles here.
pub fn checked(app: &AppState, id: &str) -> Option<bool> {
    match id {
        "measure.make_annotations" => Some(app.edit.more.make_annotations),
        "measure.keep_subject" => Some(app.edit.more.keep_subject),
        "view.rollover_comments" => Some(app.edit.more.rollover),
        "view.note_popups" => Some(app.edit.more.popups),
        _ => None,
    }
}

/// A finished points draft the commands here take over: a resumed Count adds its clicks to
/// that series; with Make Annotations off, a measurement is only read out. Returns whether
/// the draft was taken.
pub fn finish_resumed(
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    (kind, page): (Kind, usize),
    pts: &[Point],
    out: &mut CanvasOut,
) -> bool {
    if let Some(id) = cx.edit.more.resume_count.as_ref()
        && kind == Kind::Count
        && !pts.is_empty()
        && doc.session.doc().find(id).is_some()
    {
        let r = doc.session.add_count_items(id, pts);
        out.status = Some(actions::report(r, |n| format!("Count: {n} in the series")));
        return true;
    }
    if cx.edit.more.make_annotations || !kind.is_measurement() || pts.is_empty() {
        return false;
    }
    let tool_pts = crate::tools::role_points(
        match cx.tool.kind {
            crate::tools::ToolKind::Points { role, .. } => role,
            _ => crate::tools::Role::Markup,
        },
        pts,
    );
    let text = crate::tools::new_markup(kind, 0, &tool_pts)
        .map(|mut m| {
            m.page = page;
            m.scale = pts
                .first()
                .and_then(|p| doc.session.doc().pages.get(page)?.scale_at(*p).cloned());
            m.quantity_text()
        })
        .unwrap_or_default();
    out.status = Some(if text.is_empty() {
        format!("{} (temporary): no value", kind.name())
    } else {
        format!("{} (temporary): {text}", kind.name())
    });
    true
}

/// A markup was drawn: remember a measurement's subject and label (Keep Last Subject).
pub fn created(app: &mut AppState, m: &Markup) {
    crate::chest_more::created(app, m);
    if m.kind.is_measurement() {
        app.edit
            .more
            .last_subject
            .insert(m.kind, (m.subject.clone(), m.label.clone()));
    }
}

/// Keep Last Subject and Label: a new measurement takes the last one's.
pub fn keep_subject(m: &mut Markup, cx: &CanvasCx<'_>) {
    // Tool Chest > Options > Keep comments: a tool's saved comment on new markups.
    if let Some(t) = cx.template.filter(|t| t.kind == m.kind)
        && !m.kind.is_measurement()
        && m.kind != Kind::Stamp
    {
        match cx.edit.chest_comments {
            crate::chest_more::CommentMode::Never if !m.kind.is_text() => m.contents.clear(),
            crate::chest_more::CommentMode::Always if m.kind.is_text() && m.contents.is_empty() => {
                m.contents = t.contents.clone();
            }
            _ => {}
        }
    }
    if !cx.edit.more.keep_subject || !m.kind.is_measurement() {
        return;
    }
    if let Some((s, l)) = cx.edit.more.last_subject.get(&m.kind) {
        m.subject = s.clone();
        m.label = l.clone();
    }
}

/// The Cloud tool's rectangle mode: a press-drag-release (with no points placed yet) draws a
/// rectangular cloud. Returns whether the frame was taken.
pub fn cloud_drag(
    ix: &crate::interact::Input<'_>,
    doc: &mut DocTab,
    cx: &CanvasCx<'_>,
    out: &mut CanvasOut,
    (press, release, cur): (Option<egui::Pos2>, Option<egui::Pos2>, Option<egui::Pos2>),
) -> bool {
    use crate::interact::{Draft, Stage};
    if let Some(s) = press
        && doc.view.draft.is_none()
        && let Some((page, xf)) = ix.page_at(s)
    {
        doc.view.draft = Some(Draft {
            tool: cx.tool.id,
            page,
            pts: vec![xf.to_user(s)],
            stage: Stage::Dragging,
        });
    }
    let Some(d) = doc.view.draft.clone().filter(|d| d.stage == Stage::Dragging) else {
        return false;
    };
    let (Some(xf), Some(c)) = (ix.xf(d.page), release.or(cur)) else {
        return true;
    };
    let a = d.pts.first().copied().unwrap_or_default();
    let ring = crate::tools::drag_ring(crate::tools::DragShape::Rect, a, xf.to_user(c));
    let m = ring
        .and_then(|r| crate::tools::new_markup(Kind::Cloud, d.page, &r))
        .map(|m| crate::interact::styled(m, cx));
    if release.is_some() {
        doc.view.draft = None;
        if let Some(m) = m {
            crate::interact::add_markup(doc, cx.tool, m, None, out);
        }
    } else if let Some(m) = m {
        crate::interact::preview(ix, d.page, &m);
    }
    true
}

/// Manage Columns > a Choice column's Import Items: `item, subject, value` per line (the
/// subject and value optional; a header row is skipped). Fills the open editor's column.
pub fn choice_csv(app: &mut AppState, col_id: &str, path: &std::path::Path) -> String {
    let text = match std::fs::metadata(path) {
        Ok(m) if m.len() > 4 << 20 => return "The file is too large for a list of choices".into(),
        _ => std::fs::read_to_string(path),
    };
    let text = match text {
        Ok(t) => t,
        Err(e) => return format!("Import failed: {e}"),
    };
    let items = parse_choice_csv(&text);
    let Some(cols) = app.list.columns_editor.as_mut() else {
        return "Open Manage Columns first".into();
    };
    let Some(c) = cols.iter_mut().find(|c| c.id == col_id) else {
        return "The column is gone".into();
    };
    let n = items.len();
    c.items = items;
    format!("Imported {} into {}", actions::plural(n, "choice"), c.name)
}

/// `item, subject, value` lines (quoted fields allowed) as choice items; a first line that
/// reads like a header (`Item, Subject, Value`) is skipped. At most 10 000 items.
pub fn parse_choice_csv(text: &str) -> Vec<markupcraft_model::ChoiceItem> {
    let rows = csv_rows(text);
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate().take(10_001) {
        let field = |k: usize| r.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
        let (text, subject, value) = (field(0), field(1), field(2));
        if text.is_empty() || (i == 0 && text.eq_ignore_ascii_case("item")) {
            continue;
        }
        let value = value
            .trim_start_matches(['$', '\u{20ac}', '\u{a3}'])
            .replace(',', "")
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite());
        out.push(markupcraft_model::ChoiceItem { text, subject, value });
    }
    out.truncate(10_000);
    out
}

/// Rows of comma-separated fields; double quotes enclose a field (`""` is a quote).
pub fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in text.lines().take(100_000) {
        let mut fields = Vec::new();
        let mut cur = String::new();
        let mut quoted = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if quoted && chars.peek() == Some(&'"') => {
                    cur.push('"');
                    chars.next();
                }
                '"' => quoted = !quoted,
                ',' if !quoted => fields.push(std::mem::take(&mut cur)),
                c => cur.push(c),
            }
        }
        fields.push(cur);
        if fields.iter().any(|f| !f.trim().is_empty()) {
            rows.push(fields);
        }
    }
    rows
}

/// Markups List > Layer: the selected markups go on layer `name` ("" = none).
pub fn list_layer(doc: &mut DocTab, name: &str) -> String {
    let ids = doc.selection().to_vec();
    actions::report(doc.session.assign_layer(&ids, name), |n| {
        if name.is_empty() {
            format!("{} taken off their layer", actions::plural(n, "markup"))
        } else {
            format!("{} put on layer {name}", actions::plural(n, "markup"))
        }
    })
}

/// Markups List > Create Legend: a legend of the selected rows' subjects, at the top left of
/// the first one's page (the whole document when they span pages).
pub fn list_legend(doc: &mut DocTab) -> String {
    let sel: Vec<Markup> = doc
        .selection()
        .iter()
        .filter_map(|i| doc.session.doc().find(i).cloned())
        .collect();
    let Some(first) = sel.first() else {
        return "Select the rows the legend lists".into();
    };
    let mut subjects: Vec<String> = Vec::new();
    for m in &sel {
        if !m.subject.is_empty() && !subjects.contains(&m.subject) {
            subjects.push(m.subject.clone());
        }
    }
    let page = first.page;
    let Some(crop) = doc.session.doc().pages.get(page).map(|p| p.crop.normalized()) else {
        return "No such page".into();
    };
    let o = markupcraft_engine::legend::LegendOptions {
        subjects,
        document: sel.iter().any(|m| m.page != page),
        ..Default::default()
    };
    let at = Point::new(crop.x0 + 36.0, crop.y1 - 36.0);
    actions::report(doc.session.add_legend(page, at, &o), |_| {
        format!("Added a legend of {}", actions::plural(o.subjects.len(), "subject"))
    })
}

/// While counting: the running count in the lower-right corner of the view.
pub fn paint_count_readout(painter: &egui::Painter, view: egui::Rect, n: usize, resumed: bool) {
    let text = if resumed {
        format!("Count (resumed): +{n}")
    } else {
        format!("Count: {n}")
    };
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(14.0), egui::Color32::BLACK);
    let r = egui::Rect::from_min_size(
        view.right_bottom() - galley.size() - egui::vec2(20.0, 16.0),
        galley.size(),
    )
    .expand2(egui::vec2(8.0, 4.0));
    painter.rect_filled(r, 4.0, egui::Color32::from_rgb(255, 255, 220));
    painter.rect_stroke(
        r,
        4.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
        egui::StrokeKind::Inside,
    );
    painter.galley(r.min + egui::vec2(8.0, 4.0), galley, egui::Color32::BLACK);
}

/// Texts the Review Text window lists: text boxes, callouts, typewriter text, notes, carets
/// and any other markup with a comment, in page order.
pub fn review_rows(doc: &markupcraft_model::Document) -> Vec<String> {
    let mut rows: Vec<&Markup> = doc
        .markups
        .iter()
        .filter(|m| {
            !m.kind.is_measurement()
                && (m.kind.is_text() || matches!(m.kind, Kind::Note | Kind::Caret) || !m.contents.trim().is_empty())
        })
        .collect();
    rows.sort_by_key(|m| m.page);
    rows.into_iter().map(|m| m.id.clone()).collect()
}

/// The Review Text window and the end of a resumed count.
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    crate::chest_more::frame(app, ctx);
    // A File Attachment click: ask for the file.
    let id = egui::Id::new(crate::gestures::ATTACH_AT);
    if let Some((uid, page, x, y)) = ctx.data_mut(|d| d.remove_temp::<(u64, usize, f64, f64)>(id)) {
        app.dialogs.open(
            Purpose::Edit("attach_file", format!("{uid} {page} {x} {y}")),
            crate::features::ANY,
            false,
        );
    }
    app.edit.more.dimmed = filtered_out(app);
    if app.tool != "count" {
        app.edit.more.resume_count = None;
    }
    // Keep Last Subject and Label follows edits of the selected measurement.
    if app.edit.more.keep_subject
        && let Some(m) = app
            .doc()
            .and_then(|d| match d.selection() {
                [id] => d.session.doc().find(id),
                _ => None,
            })
            .filter(|m| m.kind.is_measurement())
            .cloned()
    {
        created(app, &m);
    }
    edit_action_window(app, ctx);
    crate::markups_more::frame(app, ctx);
    if !app.edit.more.review_open {
        return;
    }
    let mut open = true;
    let mut status = None;
    let at = app.edit.more.review_at;
    let mut next_at = at;
    if let Some(d) = app.doc_mut() {
        let rows = review_rows(d.session.doc());
        egui::Window::new("Review Text")
            .open(&mut open)
            .default_width(420.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} with text", actions::plural(rows.len(), "markup")));
                    if ui.add_enabled(at > 0, egui::Button::new("Previous")).clicked() {
                        next_at = at.saturating_sub(1);
                    }
                    if ui.add_enabled(at + 1 < rows.len(), egui::Button::new("Next")).clicked() {
                        next_at = at + 1;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (i, id) in rows.iter().enumerate().take(5000) {
                        let Some(m) = d.session.doc().find(id).cloned() else {
                            continue;
                        };
                        let current = i == at;
                        ui.horizontal(|ui| {
                            let head = format!("p{}  {}  {}", m.page + 1, m.kind.name(), m.author);
                            if ui.selectable_label(current, RichText::new(head).size(11.0)).clicked() {
                                next_at = i;
                            }
                        });
                        let mut text = m.contents.replace('\r', "\n");
                        let writable = actions::editable(&m) && !m.locked();
                        let r = ui.add_enabled(
                            writable,
                            egui::TextEdit::multiline(&mut text)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .id_salt(("review-text", id)),
                        );
                        if r.changed() {
                            d.session.set_merge_key(Some("review-text"));
                            let patch = markupcraft_engine::MarkupPatch {
                                contents: Some(text.replace('\n', "\r")),
                                ..Default::default()
                            };
                            let res = d.session.set_properties(std::slice::from_ref(id), &patch);
                            d.session.set_merge_key(None);
                            status = Some(actions::report(res, |_| "Text changed".to_string()));
                        }
                        if r.lost_focus() {
                            d.session.seal();
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        if next_at != at
            && let Some(id) = rows.get(next_at)
            && let Some(page) = d.session.doc().find(id).map(|m| m.page)
        {
            actions::select(&mut d.session, vec![id.clone()]);
            let count = d.session.page_count();
            d.view.go_to_page(page, count);
        }
    }
    app.edit.more.review_at = next_at;
    app.edit.more.review_open = open;
    if let Some(s) = status {
        app.status = s;
    }
}

/// Edit Action: the selected markup links to a page or a web address (a link annotation over
/// it, which other viewers follow too).
fn edit_action_window(app: &mut AppState, ctx: &egui::Context) {
    let Some((id, mut url, mut page)) = app.edit.more.edit_action.clone() else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    egui::Window::new("Edit Action")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("When the markup is clicked:");
            egui::Grid::new("edit-action").num_columns(2).show(ui, |ui| {
                ui.label("Go to page");
                ui.add(
                    egui::TextEdit::singleline(&mut page)
                        .hint_text("number")
                        .desired_width(60.0),
                );
                ui.end_row();
                ui.label("or open");
                ui.add(
                    egui::TextEdit::singleline(&mut url)
                        .hint_text("https://...")
                        .desired_width(220.0),
                );
                ui.end_row();
            });
            if ui.button("Apply").clicked() {
                apply = true;
            }
        });
    app.edit.more.edit_action = open.then(|| (id.clone(), url.clone(), page.clone()));
    if !apply {
        return;
    }
    let Some(d) = app.doc_mut() else { return };
    let Some(m) = d.session.doc().find(&id).cloned() else {
        app.edit.more.edit_action = None;
        return;
    };
    let target = match page.trim().parse::<usize>() {
        Ok(p) if p >= 1 => Some(markupcraft_engine::links::LinkTarget::Page(p - 1)),
        _ if !url.trim().is_empty() => Some(markupcraft_engine::links::LinkTarget::Url(url.trim().to_string())),
        _ => None,
    };
    app.status = match target {
        Some(t) => {
            let r = d
                .session
                .add_link(m.page, actions::markup_bbox(&m), &t, Default::default());
            actions::report(r, |_| "The markup now links there".into())
        }
        None => "Edit Action: type a page number or a web address".into(),
    };
    app.edit.more.edit_action = None;
}

/// The Measurements panel's mode row: the measurement tools the Measure tool switches between.
pub fn measure_modes(ui: &mut egui::Ui, current: &str) -> Option<&'static str> {
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        for id in MEASURE_MODES {
            let Some(t) = crate::tools::find(id) else { continue };
            if crate::icons::button(ui, t.icon, 22.0, current == t.id, t.label).clicked() {
                pick = Some(t.id);
            }
        }
    });
    pick
}

/// The markups the Markups List's filters and search leave out (empty without a filter).
pub fn filtered_out(app: &AppState) -> std::collections::HashSet<String> {
    let v = &app.list.view;
    let Some(d) = app.doc() else { return Default::default() };
    if v.filters.is_empty() && v.search.trim().is_empty() {
        return Default::default();
    }
    let doc = d.session.doc();
    let table = markupcraft_model::MarkupTable::new(doc);
    let view = markupcraft_model::View {
        scope: markupcraft_model::Scope::AllPages,
        ..v.clone()
    };
    let root = table.build(&view);
    let mut kept = std::collections::HashSet::new();
    let mut stack = vec![&root];
    while let Some(g) = stack.pop() {
        kept.extend(g.rows.iter().copied());
        stack.extend(g.children.iter());
    }
    doc.markups
        .iter()
        .enumerate()
        .filter(|(i, _)| !kept.contains(i))
        .map(|(_, m)| m.id.clone())
        .collect()
}

/// Draw a markup with the display options: faded when the list's filters leave it out,
/// hairlines with Line Weights off.
pub fn paint(p: &egui::Painter, xf: &crate::painter::Xf, m: &Markup, cx: &CanvasCx<'_>) {
    let o = &cx.edit.more;
    if m.kind == Kind::Note && m.popup_open && o.popups {
        paint_popup(p, xf, m);
    }
    if !o.dimmed.contains(&m.id) && o.line_weights {
        crate::painter::paint_markup(p, xf, m);
        return;
    }
    let mut c = m.clone();
    if o.dimmed.contains(&m.id) {
        c.opacity *= 0.2;
        c.fill_opacity *= 0.5;
    }
    if !o.line_weights && c.line_width > 0.0 {
        // one screen pixel at any zoom
        c.line_width = (1.0 / f64::from(xf.k.max(1e-3))).min(c.line_width);
    }
    crate::painter::paint_markup(p, xf, &c);
}

/// An open note's pop-up: its comment in a pale box beside the icon.
pub fn paint_popup(p: &egui::Painter, xf: &crate::painter::Xf, m: &Markup) {
    let b = actions::box_of(m);
    let r = m.popup.unwrap_or(markupcraft_geom::Rect::new(
        b.x1 + 6.0,
        b.y1 - 120.0,
        b.x1 + 226.0,
        b.y1,
    ));
    let sr = xf.rect_of(r);
    if sr.width() < 8.0 || sr.height() < 8.0 {
        return;
    }
    p.rect_filled(sr, 2.0, egui::Color32::from_rgb(255, 252, 205));
    p.rect_stroke(
        sr,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
        egui::StrokeKind::Inside,
    );
    let head = if m.author.is_empty() {
        m.subject.clone()
    } else {
        m.author.clone()
    };
    let body: String = m.contents.replace('\r', "\n").chars().take(2000).collect();
    let galley = p.layout(
        format!("{head}\n{body}"),
        egui::FontId::proportional(xf.len(9.0).clamp(7.0, 18.0)),
        egui::Color32::BLACK,
        (sr.width() - 8.0).max(10.0),
    );
    p.with_clip_rect(sr)
        .galley(sr.min + egui::vec2(4.0, 3.0), galley, egui::Color32::BLACK);
}

/// Rollover Comments: the subject and comments of the markup under the pointer.
pub fn rollover(p: &egui::Painter, at: egui::Pos2, m: &Markup, cx: &CanvasCx<'_>) {
    if !cx.edit.more.rollover || m.kind.is_measurement() {
        return;
    }
    let text = m.contents.replace('\r', "\n");
    if text.trim().is_empty() {
        return;
    }
    let shown: String = text.chars().take(400).collect();
    let body = if m.subject.is_empty() {
        shown
    } else {
        format!("{}\n{shown}", m.subject)
    };
    let galley = p.layout(body, egui::FontId::proportional(12.0), egui::Color32::BLACK, 320.0);
    let r = egui::Rect::from_min_size(at + egui::vec2(16.0, 18.0), galley.size()).expand2(egui::vec2(6.0, 4.0));
    p.rect_filled(r, 3.0, egui::Color32::from_rgb(255, 255, 225));
    p.rect_stroke(
        r,
        3.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
        egui::StrokeKind::Inside,
    );
    p.galley(r.min + egui::vec2(6.0, 4.0), galley, egui::Color32::BLACK);
}

/// Ctrl+drag on a straight segment of the (single) selected markup that can take arcs: the
/// segment becomes an arc and the drag bends it. Returns (markup, the arc handle's index).
pub fn ctrl_curve(doc: &mut DocTab, page: usize, at: Point, tol: f64) -> Option<(String, usize)> {
    let [id] = doc.selection() else { return None };
    let m = doc.session.doc().find(id)?.clone();
    if m.page != page || m.locked() || !measure_extras::can_have_arcs(m.kind) {
        return None;
    }
    let closed = measure_extras::closed_shape(m.kind) || m.kind == Kind::Polygon;
    let (start, _) = measure_extras::nearest_segment(&m.pts, closed, at, tol * 2.0)?;
    if measure_extras::arc_containing(&m, start).is_some() || m.arcs.iter().any(|a| a.0 == start) {
        return None;
    }
    let arc = doc.session.convert_segment_to_arc(&m.id, start).ok()?;
    let m = doc.session.doc().find(&m.id)?;
    Some((m.id.clone(), measure_extras::arc_handle(m, arc)?))
}

/// Canvas menu rows for Counts, arcs and cutouts.
pub fn context_items(ui: &mut egui::Ui, doc: &mut DocTab, out: &mut CanvasOut, target: &ContextTarget, m: &Markup) {
    let mut any = false;
    if m.kind == Kind::Attachment {
        if ui.button("Save Attached File...").clicked() {
            out.commands.push("markup.save_attachment".into());
            ui.close();
        }
        any = true;
    }
    if m.kind == Kind::Count {
        if ui.button("Resume Count").clicked() {
            out.commands.push("measure.resume_count".into());
            ui.close();
        }
        if let Some(v) = target.vertex {
            if ui.button("Delete Count Item").clicked() {
                let r = doc.session.delete_count_item(&m.id, v);
                out.status = Some(actions::report(r, |n| format!("Count item deleted ({n} left)")));
                ui.close();
            }
            if m.pts.len() > 1 && ui.button("Split Off This Item").clicked() {
                let r = doc.session.split_count(&m.id, &[v]);
                out.status = Some(actions::report(r, |_| "Split into a new count".into()));
                ui.close();
            }
            if v > 0 && v + 1 < m.pts.len() && ui.button("Split Count Here").clicked() {
                let items: Vec<usize> = (v..m.pts.len()).collect();
                let r = doc.session.split_count(&m.id, &items);
                out.status = Some(actions::report(r, |_| "Split into two counts".into()));
                ui.close();
            }
        }
        let counts = doc
            .selection()
            .iter()
            .filter(|i| doc.session.doc().find(i).is_some_and(|x| x.kind == Kind::Count))
            .count();
        if counts > 1 && ui.button("Merge Counts").clicked() {
            out.commands.push("measure.merge_counts".into());
            ui.close();
        }
        any = true;
    }
    if measure_extras::can_have_arcs(m.kind) {
        let arc = target
            .vertex
            .or(target.segment.map(|(i, _)| i))
            .and_then(|i| measure_extras::arc_containing(m, i));
        if let Some(a) = arc {
            if ui.button("Convert to Line").clicked() {
                let r = doc.session.straighten_arc(&m.id, a);
                out.status = Some(actions::report(r, |_| "Arc straightened".into()));
                ui.close();
            }
            any = true;
        } else if let Some((after, _)) = target.segment {
            if ui.button("Convert to Arc").clicked() {
                let r = doc.session.convert_segment_to_arc(&m.id, after);
                out.status = Some(actions::report(r, |_| {
                    "Segment curved: drag its middle handle to bend it".into()
                }));
                ui.close();
            }
            any = true;
        }
    }
    if let Some(h) = target.hole
        && measure_extras::can_have_cutouts(m.kind)
    {
        if let Some(ring) = m.holes.get(h) {
            // the cutout vertex nearest the pointer, and the cutout edge under it
            let near = ring
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.dist(target.at).total_cmp(&b.1.dist(target.at)))
                .map(|(i, _)| i);
            if let Some((after, foot)) = measure_extras::nearest_segment(ring, true, target.at, f64::MAX)
                && ui.button("Add Cutout Vertex").clicked()
            {
                let r = doc.session.insert_hole_vertex(&m.id, h, after, foot);
                out.status = Some(actions::report(r, |_| "Cutout vertex added".into()));
                ui.close();
            }
            if let Some(v) = near
                && ring.len() > 3
                && ui.button("Delete Cutout Vertex").clicked()
            {
                let r = doc.session.delete_hole_vertex(&m.id, h, v);
                out.status = Some(actions::report(r, |_| "Cutout vertex deleted".into()));
                ui.close();
            }
        }
        if ui.button("Cutout to Measurement").clicked() {
            let r = doc.session.cutout_to_measurement(&m.id, h);
            out.status = Some(actions::report(r, |_| {
                format!("The cutout is now its own {}", m.kind.name())
            }));
            ui.close();
        }
        any = true;
    }
    if any {
        ui.separator();
    }
}
