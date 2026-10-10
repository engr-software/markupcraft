//! Further completions of partial rows (their windows and commands).

use crate::AppState;

use egui::RichText;
use markupcraft_engine::finish::rasterfill::RasterFill;
use markupcraft_engine::finish::status::{StatusRow, status_color, status_csv, status_report};

use crate::actions;
use crate::dialogs::Purpose;

#[derive(Default)]
pub struct MoreState2 {
    pub status_open: bool,
    /// Status report: Counts only (else every markup with a status).
    pub status_all: bool,
    pub status_message: String,
}

/// Run one of these commands; false when `id` is not one of them.
pub fn run(app: &mut AppState, id: &str) -> bool {
    match id {
        "measure.status_report" => app.features.partials.more2.status_open = true,
        "edit.snapshot_cut" => super::start_pick(
            app,
            super::Pick::SnapshotCut,
            "Drag over the region to cut; Ctrl+V pastes it",
        ),
        _ => return super::partials_more3::run(app, id),
    }
    true
}

#[derive(Clone, Copy, Default)]
struct CtrlClick(u64, usize, markupcraft_geom::Point);

/// A Ctrl+click on the page (the canvas layer records it).
pub fn push_ctrl_click(ctx: &egui::Context, doc: u64, page: usize, at: markupcraft_geom::Point) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("ctrl-click"), CtrlClick(doc, page, at)));
}

/// Pages moved: with "Reorder bookmarks when pages move", bookmarks follow their pages.
pub fn after_page_move(app: &mut AppState) {
    if !app.shell.ui.extra.reorder_bookmarks {
        return;
    }
    if let Some(d) = app.doc_mut() {
        let _ = d.session.sort_bookmarks_by_page();
    }
}

/// Ctrl+click on a link: a linked file opens behind the current tab (Revu's background
/// open); a page or web link is followed as usual.
fn ctrl_click(app: &mut AppState, ctx: &egui::Context) {
    let Some(CtrlClick(uid, page, at)) = ctx.data_mut(|d| d.remove_temp::<CtrlClick>(egui::Id::new("ctrl-click")))
    else {
        return;
    };
    let Some(d) = app.docs.iter().find(|d| d.uid == uid) else {
        return;
    };
    let Some(link) = d
        .session
        .links()
        .into_iter()
        .find(|l| l.page == page && l.rect.normalized().contains(at))
    else {
        // a web address in the page text (General > Document: detect web addresses)
        if app.shell.ui.extra.detect_urls
            && let Some((w, _)) = crate::context_text::word_at(d, page, at)
        {
            let w = w.trim_matches(|c: char| ",.;:()<>\"'".contains(c));
            let lower = w.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.") {
                let url = if lower.starts_with("www.") {
                    format!("https://{w}")
                } else {
                    w.to_string()
                };
                ctx.open_url(egui::OpenUrl::new_tab(&url));
                app.status = format!("Opened {url}");
            }
        }
        return;
    };
    use markupcraft_engine::links::LinkTarget;
    match &link.target {
        LinkTarget::File { path, .. } | LinkTarget::FileView { path, .. } | LinkTarget::FilePlace { path, .. } => {
            let base = d
                .path
                .as_ref()
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
            let p = std::path::PathBuf::from(path);
            let p = if p.is_relative() {
                base.map_or(p.clone(), |b| b.join(&p))
            } else {
                p
            };
            let keep = app.active;
            let had = app.docs.len();
            app.open_path(&p);
            if app.docs.len() > had {
                app.active = keep;
                app.status = format!("Opened {} in the background", p.display());
            }
        }
        _ => super::links::follow(app, &link, ctx),
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    ctrl_click(app, ctx);
    if let Some(q) = crate::context_text::take_search(ctx) {
        app.features.search.query = q;
    }
    crate::viewports_more::frame(app);
    status_window(app, ctx);
    super::partials_more3::window(app, ctx);
}

/// Dynamic Fill's detection and cursor settings.
#[derive(Debug, Clone, PartialEq)]
pub struct FillMore {
    /// Detect on the rendered page image (scans) instead of the vector linework.
    pub raster: bool,
    pub dpi: f64,
    pub sensitivity: u8,
    /// Markups are left out of the image (they are not walls).
    pub hide_markups: bool,
    /// The fill cursor's ring: diameter (points on screen) and colour.
    pub cursor_size: f32,
    pub cursor_color: egui::Color32,
}

impl Default for FillMore {
    fn default() -> Self {
        let r = RasterFill::default();
        Self {
            raster: false,
            dpi: r.dpi,
            sensitivity: r.sensitivity,
            hide_markups: r.hide_markups,
            cursor_size: 18.0,
            cursor_color: egui::Color32::from_rgb(0, 150, 220),
        }
    }
}

impl FillMore {
    /// The engine's raster settings when raster detection is on.
    pub fn raster(&self, gap: f64, cutouts: bool) -> Option<RasterFill> {
        self.raster.then_some(RasterFill {
            dpi: self.dpi.clamp(36.0, 300.0),
            sensitivity: self.sensitivity.clamp(1, 254),
            gap,
            hide_markups: self.hide_markups,
            cutouts,
        })
    }
}

/// The detection and cursor rows of the Dynamic Fill bar.
pub fn fill_settings(ui: &mut egui::Ui, m: &mut FillMore) {
    ui.collapsing("Detection and cursor", |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut m.raster, false, "Vector linework");
            ui.radio_value(&mut m.raster, true, "Page image (scans)");
        });
        ui.add_enabled_ui(m.raster, |ui| {
            ui.horizontal(|ui| {
                ui.label("Detection DPI");
                ui.add(egui::DragValue::new(&mut m.dpi).range(36.0..=300.0));
                ui.label("Edge sensitivity");
                ui.add(egui::DragValue::new(&mut m.sensitivity).range(1..=254));
            });
            ui.checkbox(&mut m.hide_markups, "Hide markups while filling");
        });
        ui.horizontal(|ui| {
            ui.label("Cursor size");
            ui.add(egui::DragValue::new(&mut m.cursor_size).range(6.0..=96.0));
            ui.color_edit_button_srgba(&mut m.cursor_color);
        });
    });
}

/// The fill cursor: a ring of the chosen size and colour at the pointer.
pub fn fill_cursor(m: &FillMore, ctx: &egui::Context) {
    let Some(p) = ctx.pointer_hover_pos() else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("fill-cursor"),
    ));
    painter.circle_stroke(p, m.cursor_size / 2.0, egui::Stroke::new(2.0, m.cursor_color));
    painter.line_segment(
        [p - egui::vec2(4.0, 0.0), p + egui::vec2(4.0, 0.0)],
        egui::Stroke::new(1.0, m.cursor_color),
    );
    painter.line_segment(
        [p - egui::vec2(0.0, 4.0), p + egui::vec2(0.0, 4.0)],
        egui::Stroke::new(1.0, m.cursor_color),
    );
}

fn rows(app: &AppState) -> Vec<StatusRow> {
    let all = app.features.partials.more2.status_all;
    app.doc()
        .map(|d| status_report(&d.session.doc().markups, !all))
        .unwrap_or_default()
}

/// Measure > Count Status Report: items by subject and status, each status in its colour,
/// exported as a table or as the visual report (the drawings coloured by status).
fn status_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.partials.more2.status_open {
        return;
    }
    let list = rows(app);
    let (mut open, mut pdf, mut csv) = (true, false, false);
    super::window("Count Status Report").open(&mut open).show(ctx, |ui| {
        let st = &mut app.features.partials.more2;
        ui.checkbox(&mut st.status_all, "Every markup with a status (not only counts)");
        if list.is_empty() {
            ui.label(RichText::new("No counts on this document.").weak());
        }
        egui::Grid::new("status-report")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                for h in ["Subject", "Status", "Items", "Markups"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for r in &list {
                    ui.label(&r.subject);
                    ui.horizontal(|ui| {
                        let c = status_color(&r.status);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            egui::Color32::from_rgb((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8),
                        );
                        ui.label(&r.status);
                    });
                    ui.label(r.items.to_string());
                    ui.label(r.markups.to_string());
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            if ui.button("Export Visual Report...").clicked() {
                pdf = true;
            }
            if ui.button("Export Table...").clicked() {
                csv = true;
            }
        });
        if !st.status_message.is_empty() {
            ui.label(RichText::new(&st.status_message).small());
        }
    });
    app.features.partials.more2.status_open = open;
    if pdf {
        app.dialogs.save(
            Purpose::Feature(super::Ask::StatusReportPdf),
            crate::dialogs::PDF,
            "Status Report.pdf",
        );
    }
    if csv {
        app.dialogs.save(
            Purpose::Feature(super::Ask::StatusReportCsv),
            crate::dialogs::CSV,
            "Status Report.csv",
        );
    }
}

/// Cut Snapshot: the region was dragged.
pub fn cut_picked(app: &mut AppState, page: usize, pts: &[markupcraft_geom::Point]) {
    let Some(r) = super::rect_of(pts) else { return };
    let threads = app.threads;
    let Some(d) = app.doc_mut() else { return };
    let res = d.session.snapshot_cut(page, r);
    d.rerender(threads);
    app.status = actions::report(res, |_| "Cut to a snapshot: Ctrl+V pastes it".into());
}

/// A status report file was chosen.
pub fn status_file(app: &mut AppState, pdf: bool, path: &std::path::Path) {
    let all = app.features.partials.more2.status_all;
    let r = if pdf {
        app.doc()
            .ok_or_else(|| markupcraft_engine::EngineError::Invalid("No document open".into()))
            .and_then(|d| d.session.visual_status_report(path, !all))
            .map(|rows| rows.len())
    } else {
        let list = rows(app);
        markupcraft_engine::finish::write_file(path, status_csv(&list).as_bytes()).map(|_| list.len())
    };
    app.features.partials.more2.status_message = actions::report(r, |n| {
        format!("Wrote {} to {}", actions::plural(n, "row"), path.display())
    });
}

/// While counting: the running count in the lower-right corner of the view. Its close button
/// hides it; a small Count button there brings it back.
pub fn count_readout(ctx: &egui::Context, view: egui::Rect, n: usize, resumed: bool) {
    let id = egui::Id::new("count-readout-hidden");
    let hidden = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let text = if resumed {
        format!("Count (resumed): +{n}")
    } else {
        format!("Count: {n}")
    };
    let at = view.right_bottom() - egui::vec2(16.0, 12.0);
    egui::Area::new(egui::Id::new("count-readout"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(at)
        .show(ctx, |ui| {
            if hidden {
                if ui
                    .small_button("Show Count")
                    .on_hover_text("Show the running count")
                    .clicked()
                {
                    ctx.data_mut(|d| d.insert_temp(id, false));
                }
                return;
            }
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(255, 255, 220))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(120)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text).color(egui::Color32::BLACK).size(14.0));
                        if ui.small_button("Hide").on_hover_text("Hide the count").clicked() {
                            ctx.data_mut(|d| d.insert_temp(id, true));
                        }
                    });
                });
        });
}
