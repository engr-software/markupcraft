//! More of the application shell: Help (F1) with MarkupCraft's own help topics, the
//! Administrator and Manage Profiles items of the MarkupCraft menu, the interface of the
//! profiles MarkupCraft ships, and the interface preferences of [`Prefs2`] (kept in
//! `UiPrefs::extra2`).
//!
//! The rest of the shell reaches this module through one-line hooks, like `extra`:
//! [`COMMANDS`] joins the command table; [`enabled`], [`checked`] and [`run`] are the first
//! arms of the shell's; [`windows`] and [`begin_frame`] run from the shell's own.

use egui::{Key, RichText};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::commands::{Command, Keys};

const fn c(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    group: u8,
    keys: Option<Keys>,
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

/// Commands of this module.
#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    c("help.contents", "Help", "Help", 0, crate::commands::key(Key::F1), "circle-help"),
    c("app.administrator", "Administrator...", "", 0, None, ""),
    c("app.manage_profiles", "Manage Profiles...", "", 0, None, ""),
];

fn ours(id: &str) -> bool {
    COMMANDS.iter().any(|c| c.id == id)
}

/// Interface preferences of this module (in `UiPrefs::extra2`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs2 {
    /// General > Options: open a Web Tab of the favourites when MarkupCraft starts.
    pub home_web_tab: bool,
    /// Messages the user asked not to see again ([`SIGN_WARNING`]...).
    pub hidden_messages: Vec<String>,
    /// General > Document: links to a slip-sheeted page follow it to the new sheet (else they
    /// are removed with the superseded sheet).
    pub redirect_slip_links: bool,
    /// General > Navigation: a tilt wheel pans sideways.
    pub tilt_pans: bool,
    /// Window > Presentation: `none`, `fade` or `wipe` between pages.
    pub transition: String,
    /// Window > Presentation: the colour around the page.
    pub presentation_background: [u8; 3],
    /// Window > Presentation: hide the mouse pointer.
    pub presentation_hide_cursor: bool,
}

impl Default for Prefs2 {
    fn default() -> Self {
        Self {
            home_web_tab: false,
            hidden_messages: Vec::new(),
            redirect_slip_links: true,
            tilt_pans: true,
            transition: "none".into(),
            presentation_background: [0, 0, 0],
            presentation_hide_cursor: true,
        }
    }
}

impl Prefs2 {
    pub fn sanitize(&mut self) {
        if !["none", "fade", "wipe"].contains(&self.transition.as_str()) {
            self.transition = "none".into();
        }
        self.hidden_messages.truncate(100);
        self.hidden_messages.retain(|m| m.len() <= 64);
    }
}

/// The "page edits invalidate the signatures" question.
pub const SIGN_WARNING: &str = "sign-warning";

/// Whether the user asked not to see message `id` again.
pub fn hidden(app: &AppState, id: &str) -> bool {
    app.shell.ui.extra2.hidden_messages.iter().any(|m| m == id)
}

/// Don't show message `id` again (until General > Reset Hidden Messages).
pub fn hide(app: &mut AppState, id: &str) {
    if !hidden(app, id) {
        app.shell.ui.extra2.hidden_messages.push(id.to_string());
        app.shell.save_ui();
    }
}

/// The options of Preferences page `page` this module adds.
pub fn section(ui: &mut egui::Ui, page: &str, u: &mut super::UiPrefs) {
    let p = &mut u.extra2;
    match page {
        "General" => {
            ui.add_space(6.0);
            ui.checkbox(&mut p.home_web_tab, "Open a Web Tab of the favourites on start");
            ui.horizontal(|ui| {
                let n = p.hidden_messages.len();
                if ui
                    .add_enabled(n > 0, egui::Button::new("Reset Hidden Messages"))
                    .on_hover_text("Show the messages you asked not to see again")
                    .clicked()
                {
                    p.hidden_messages.clear();
                }
                ui.label(RichText::new(format!("{n} hidden")).weak());
            });
        }
        "Document" => {
            ui.checkbox(
                &mut p.redirect_slip_links,
                "Links to a slip-sheeted page go to the new sheet (else they are removed)",
            );
        }
        "Navigation" => {
            ui.checkbox(&mut p.tilt_pans, "Tilt wheel pans sideways");
        }
        "Window" => {
            ui.add_space(6.0);
            ui.label(RichText::new("Presentation look").strong());
            ui.horizontal(|ui| {
                ui.label("Transition");
                for (v, l) in [("none", "None"), ("fade", "Fade"), ("wipe", "Wipe")] {
                    ui.radio_value(&mut p.transition, v.to_string(), l);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Background");
                ui.color_edit_button_srgb(&mut p.presentation_background);
            });
            ui.checkbox(&mut p.presentation_hide_cursor, "Hide the mouse pointer");
        }
        _ => {}
    }
}

/// The links that go to a page Slip Sheet replaced, removed (General > Document without
/// redirection). Returns how many went.
pub fn drop_slip_links(
    s: &mut markupcraft_engine::Session,
    rep: &markupcraft_engine::batch::SlipSheetReport,
) -> markupcraft_engine::Result<usize> {
    use markupcraft_engine::links::LinkTarget;
    let pages: Vec<usize> = rep.matched.iter().map(|m| m.old_page).collect();
    let ids: Vec<String> = s
        .links()
        .into_iter()
        .filter(|l| match &l.target {
            LinkTarget::Page(p) | LinkTarget::Zoomed { page: p, .. } | LinkTarget::View { page: p, .. } => {
                pages.contains(p)
            }
            _ => false,
        })
        .map(|l| l.id)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    s.delete_links(&ids)
}

/// Startup preferences of this module.
pub fn startup(app: &mut AppState) {
    if app.shell.ui.extra2.home_web_tab {
        app.queue("view.web_tab");
    }
}

/// Presentation: the transition over a page that just came up, and the hidden pointer.
pub fn presentation_paint(app: &mut AppState, ui: &mut egui::Ui, rect: egui::Rect) {
    if app.shell.screen != super::Screen::Presentation {
        app.shell.extra2.shown_page = None;
        return;
    }
    let now = ui.input(|i| i.time);
    let Some((uid, page)) = app.doc().map(|d| (d.uid, d.view.current)) else {
        return;
    };
    let at = match app.shell.extra2.shown_page {
        Some((u, p, t)) if u == uid && p == page => t,
        Some(_) => {
            app.shell.extra2.shown_page = Some((uid, page, now));
            now
        }
        None => {
            // The first page shows at once.
            app.shell.extra2.shown_page = Some((uid, page, now - 10.0));
            now - 10.0
        }
    };
    let p = &app.shell.ui.extra2;
    let [r, g, b] = p.presentation_background;
    let k = ((now - at) / TRANSITION_SECS).clamp(0.0, 1.0) as f32;
    if k < 1.0 {
        let painter = ui.painter_at(rect);
        match p.transition.as_str() {
            "fade" => {
                let a = ((1.0 - k) * 255.0) as u8;
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
            }
            "wipe" => {
                let x = rect.left() + rect.width() * k;
                let cover = egui::Rect::from_min_max(egui::pos2(x, rect.top()), rect.max);
                painter.rect_filled(cover, 0.0, egui::Color32::from_rgb(r, g, b));
            }
            _ => {}
        }
        ui.ctx().request_repaint();
    }
    if p.presentation_hide_cursor && ui.rect_contains_pointer(rect) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }
}

/// How long a presentation transition takes.
pub const TRANSITION_SECS: f64 = 0.4;

/// Filtered-out markups to dim: (doc uid, version, the view's filters and search, boxes).
type DimCache = Option<(u64, u64, String, Vec<(usize, markupcraft_geom::Rect)>)>;

thread_local! {
    static DIM: std::cell::RefCell<DimCache> = const { std::cell::RefCell::new(None) };
}

/// Preferences > Markups List: the markups the list's filters hide are dimmed on the page.
pub fn dim_marks(app: &AppState, d: &crate::DocTab, marks: &mut Vec<crate::features::Mark>) {
    let pct = app.shell.prefs.more.markups_list.dim_filtered_pct.min(95);
    if pct == 0 {
        return;
    }
    let view = &app.list.view;
    if view.filters.is_empty() && view.search.trim().is_empty() {
        return;
    }
    let key = format!("{:?}|{}", view.filters, view.search);
    let version = d.session.state_version();
    let boxes = DIM.with(|c| {
        let mut c = c.borrow_mut();
        match &*c {
            Some((u, v, k, b)) if *u == d.uid && *v == version && *k == key => b.clone(),
            _ => {
                let b = crate::panels::markups_list::filtered_out(d, view);
                *c = Some((d.uid, version, key, b.clone()));
                b
            }
        }
    });
    let a = (f32::from(pct) / 100.0 * 255.0) as u8;
    let fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, a);
    for (page, r) in boxes {
        marks.push(crate::features::Mark::rect(
            page,
            r.padded(1.0),
            fill,
            egui::Color32::TRANSPARENT,
        ));
    }
}

/// Preferences > Signature: forget a digital ID's password once its minutes are up.
fn forget_password(app: &mut AppState, now: f64) {
    let minutes = app.shell.prefs.more.signature.password_minutes;
    if app.features.signatures.password.is_empty() || app.features.signatures.sign_open {
        app.shell.extra2.password_since = None;
        return;
    }
    let since = *app.shell.extra2.password_since.get_or_insert(now);
    if now - since >= f64::from(minutes) * 60.0 {
        app.features.signatures.password.clear();
        app.shell.extra2.password_since = None;
    }
}

/// The Help window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HelpState {
    pub open: bool,
    pub topic: usize,
    pub filter: String,
}

/// State of this module (in `Shell::extra2`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State2 {
    pub help: HelpState,
    /// Admin > Profiles: bundle the shared settings files with an exported profile.
    pub export_dependencies: bool,
    /// Admin > Profiles: the new name typed for Rename.
    pub rename_to: String,
    /// Split counts by space: the document version last looked at (uid, version).
    pub counts_checked: Option<(u64, u64)>,
    /// Presentation: the page on screen and when it came up (uid, page, time).
    pub shown_page: Option<(u64, usize, f64)>,
    /// When the digital ID password was last kept after signing.
    pub password_since: Option<f64>,
}

pub fn enabled(_app: &AppState, id: &str) -> Option<bool> {
    ours(id).then_some(true)
}

pub fn checked(app: &AppState, id: &str) -> Option<bool> {
    match id {
        "help.contents" => Some(app.shell.extra2.help.open),
        _ => None,
    }
}

/// Run a command of this module; `false` when `id` is not one.
pub fn run(app: &mut AppState, id: &str, _ctx: &egui::Context) -> bool {
    if !ours(id) {
        return false;
    }
    match id {
        "help.contents" => {
            let h = &mut app.shell.extra2.help;
            h.open = !h.open;
        }
        "app.administrator" | "app.manage_profiles" => {
            app.shell.show_prefs = true;
            app.shell.prefs_page = "Admin";
        }
        _ => {}
    }
    true
}

/// The interface of a profile MarkupCraft ships, used until the profile is changed and saved.
pub fn shipped_ui(name: &str) -> Option<super::UiPrefs> {
    use super::toolbars_more::Dock;
    let mut u = super::UiPrefs::default();
    let tb = &mut u.toolbars;
    match name {
        // Measuring all day: the measure tools down the left, rulers on.
        "Takeoff" => {
            tb.show_measure = true;
            tb.more.docks.insert("Measure".into(), Dock::Left);
            u.rulers = true;
        }
        // Everything at hand: markup and measure tools in a second row.
        "Construction" => {
            tb.show_markup = true;
            tb.show_measure = true;
            tb.more.docks.insert("Markup".into(), Dock::Row2);
            tb.more.docks.insert("Measure".into(), Dock::Row2);
        }
        // Commenting: markup tools, no measuring.
        "Design Review" => {
            tb.show_measure = false;
        }
        // Reading: no tool strips, no rulers.
        "Simple" => {
            tb.show_markup = false;
            tb.show_measure = false;
        }
        _ => return None,
    }
    Some(u)
}

/// Start of a frame: preferences that act as the document changes.
pub fn begin_frame(app: &mut AppState, ctx: &egui::Context) {
    split_counts(app);
    forget_password(app, ctx.input(|i| i.time));
}

/// Preferences > Measure > Split counts by space: a Count that spans spaces is split as soon
/// as it is placed or edited (not while a gesture is under way).
fn split_counts(app: &mut AppState) {
    if !app.shell.prefs.more.measure.split_counts_by_space {
        return;
    }
    let checked = app.shell.extra2.counts_checked;
    let Some(d) = app.doc_mut() else { return };
    if d.view.gesture.is_some() || d.view.draft.is_some() {
        return;
    }
    let now = (d.uid, d.session.state_version());
    if checked == Some(now) {
        return;
    }
    let made = d.session.split_counts_by_space(None);
    let after = (d.uid, d.session.state_version());
    app.shell.extra2.counts_checked = Some(after);
    if let Ok(v) = made
        && !v.is_empty()
    {
        app.status = format!("Count split by space ({} more)", v.len());
    }
}

/// The windows of this module.
pub fn windows(app: &mut AppState, ctx: &egui::Context) {
    help_window(app, ctx);
}

/// MarkupCraft's own help: (title, paragraphs).
pub const TOPICS: &[(&str, &[&str])] = &[
    (
        "Getting started",
        &[
            "Open a PDF with File > Open (Ctrl+O) or by dropping it on the window. Every open file has a tab; Ctrl+Tab moves between them.",
            "The wheel zooms (hold Ctrl to scroll instead); drag with the middle button, or hold Space, to pan. Ctrl+9 fits the page, Ctrl+0 fits the width.",
            "Markups are saved into the PDF itself as standard annotations, so other PDF readers show them too.",
        ],
    ),
    (
        "Drawing markups",
        &[
            "Pick a tool from the Markup toolbar, the Tool Chest or its key, then click or drag on the page. Hold Shift for straight lines, squares and circles.",
            "With the Select tool (V), click a markup to select it, drag it to move it, and drag its handles to reshape it. Corner handles of images, stamps, polylines and polygons keep the shape's proportions; hold Shift to stretch freely.",
            "The Properties panel (Alt+P) changes colour, line width, fill, opacity and text. Set as Default in the Tool Chest makes a look the tool's default.",
        ],
    ),
    (
        "Measuring and takeoff",
        &[
            "Calibrate the page (or pick a scale) first, then use Length, Polylength, Area, Perimeter, Count and Volume. Values update as you edit the shapes.",
            "The Measurements panel lists every measurement with its value; the Markups List can total them by subject, layer or space and export the table.",
            "Spaces name regions of a sheet. Markups inside a space show its name, and counts can be split per space (Preferences > Tools > Measure).",
        ],
    ),
    (
        "Search and compare",
        &[
            "Search (Ctrl+F) finds words in the page text, markup comments and other files. Check results, then highlight, underline, strike, link, bookmark, count or redact them all at once.",
            "Visual Search finds every copy of a boxed symbol and can count them; Limit to Selection ignores lines that run out of the box.",
            "Compare Documents clouds every difference between two revisions, on the open document or in a separate _Diff file.",
        ],
    ),
    (
        "Pages and documents",
        &[
            "The Document menu inserts, deletes, extracts, rotates and crops pages. Thumbnails can be dragged to reorder pages.",
            "Split the workspace with Ctrl+2 (side by side) or Ctrl+H (stacked); split again for more panes, up to sixteen. Synchronize keeps panes on the same page.",
        ],
    ),
    (
        "Profiles and preferences",
        &[
            "Preferences (Ctrl+K) are kept per profile with the toolbars, panels and keyboard shortcuts. MarkupCraft ships Takeoff, Construction, Design Review and Simple profiles.",
            "Manage profiles in Preferences > Admin: create, rename, delete, and export a profile (optionally with your Tool Chest and other shared settings) to give to someone else.",
        ],
    ),
    (
        "Automation",
        &[
            "Everything MarkupCraft does is also a tool that scripts and AI assistants can call: `markupcraft-cli run` runs a script of tool calls, and the opt-in MCP server (Preferences > Admin) lets an assistant drive the app on this computer only.",
        ],
    ),
    (
        "Keyboard",
        &[
            "Help > Keyboard Shortcuts lists every command with its keys; Tools > Customize Keyboard changes them. F1 opens this help.",
        ],
    ),
];

fn help_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.shell.extra2.help.open {
        return;
    }
    let mut open = true;
    let mut shortcuts = false;
    let h = &mut app.shell.extra2.help;
    egui::Window::new("MarkupCraft Help")
        .open(&mut open)
        .default_size([620.0, 420.0])
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut h.filter)
                        .hint_text("Search help")
                        .desired_width(200.0),
                );
                if ui.button("Keyboard Shortcuts").clicked() {
                    shortcuts = true;
                }
            });
            ui.separator();
            let needle = h.filter.trim().to_lowercase();
            let matches = |i: usize| {
                TOPICS.get(i).is_some_and(|(t, ps)| {
                    needle.is_empty()
                        || t.to_lowercase().contains(&needle)
                        || ps.iter().any(|p| p.to_lowercase().contains(&needle))
                })
            };
            if !matches(h.topic)
                && let Some(i) = (0..TOPICS.len()).find(|i| matches(*i))
            {
                h.topic = i;
            }
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(160.0);
                    for (i, (t, _)) in TOPICS.iter().enumerate() {
                        if matches(i) && ui.selectable_label(h.topic == i, *t).clicked() {
                            h.topic = i;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    if let Some((t, ps)) = TOPICS.get(h.topic).filter(|_| matches(h.topic)) {
                        ui.heading(*t);
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for p in *ps {
                                ui.label(*p);
                                ui.add_space(6.0);
                            }
                        });
                    } else {
                        ui.label(RichText::new("No help topic matches.").weak());
                    }
                });
            });
        });
    app.shell.extra2.help.open = open;
    if shortcuts {
        app.queue("help.shortcuts");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_topics_are_ours_and_short() {
        for (t, ps) in super::TOPICS {
            assert!(!t.is_empty() && !ps.is_empty());
            assert!(ps.iter().all(|p| p.len() < 600), "{t}");
        }
        assert!(super::shipped_ui("Simple").is_some_and(|u| !u.toolbars.show_markup));
    }
}
