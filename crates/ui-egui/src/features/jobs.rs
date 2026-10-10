//! Window > Jobs: the background job queue (`markupcraft_engine::jobs`). Long batch work (the
//! Stapler's Create PDF from Files, Batch Flatten and Unflatten) is queued here and runs on a
//! worker thread, one job after another, while the interface stays live. The dialog lists each
//! job with its progress and a Cancel button; the status bar shows how many are running; a
//! finished job reports in the status bar (and its combined PDF opens).
//!
//! Without worker threads (`AppState::threads == 0`: headless tests) jobs run at once, unless
//! [`JobsState::background`] says otherwise.

use egui::RichText;
use markupcraft_engine::jobs::{JobQueue, JobState, Work};

use crate::AppState;

#[derive(Default)]
pub struct JobsState {
    pub queue: JobQueue,
    pub open: bool,
    /// Run jobs on the worker (`Some(true)`), on the interface thread (`Some(false)`), or by
    /// `AppState::threads` (`None`: a worker unless threads is 0).
    pub background: Option<bool>,
}

/// Whether a new job runs at once on this thread.
pub fn inline(app: &AppState) -> bool {
    app.features.jobs.background.map_or(app.threads == 0, |b| !b)
}

/// Queue a job built by one of `markupcraft_engine::jobs`' builders. Returns its id.
pub fn submit(app: &mut AppState, name: &str, built: markupcraft_engine::Result<(usize, Work)>) -> Option<u64> {
    match built {
        Ok((total, work)) => {
            let inline = inline(app);
            let id = app.features.jobs.queue.submit(name, total, inline, work);
            if !inline {
                app.status = format!("{name}: queued (Window > Jobs shows its progress)");
            }
            poll(app);
            Some(id)
        }
        Err(e) => {
            app.status = e.to_string();
            app.features.batch.message = app.status.clone();
            None
        }
    }
}

/// Report jobs that finished (each frame): the status bar, the batch dialog's message, and a
/// combined PDF opens.
pub fn poll(app: &mut AppState) {
    for j in app.features.jobs.queue.take_finished() {
        let msg = match j.state {
            JobState::Done => j.message.clone(),
            JobState::Cancelled => format!("{}: cancelled. {}", j.name, j.message),
            _ => format!("{} failed: {}", j.name, j.message),
        };
        app.features.batch.message = msg.clone();
        app.status = msg;
        if j.state == JobState::Done
            && let Some(p) = &j.open
        {
            app.features.batch.open = false;
            app.open_path(p);
        }
    }
}

/// Each frame: report finished jobs, keep repainting while any runs, draw the dialog.
pub fn frame(app: &mut AppState, ctx: &egui::Context) {
    poll(app);
    if app.features.jobs.queue.active() > 0 {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    window(app, ctx);
}

/// The status bar's indicator: "N jobs" while any is queued or running (click: the dialog).
pub fn indicator(app: &mut AppState, ui: &mut egui::Ui) {
    let n = app.features.jobs.queue.active();
    if n == 0 {
        return;
    }
    let text = crate::actions::plural(n, "job");
    if ui
        .add(egui::Button::new(RichText::new(text).size(11.0)).min_size(egui::vec2(0.0, 20.0)))
        .on_hover_text("Background jobs: click to see their progress")
        .clicked()
    {
        app.features.jobs.open = true;
    }
}

fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.jobs.open {
        return;
    }
    let mut open = true;
    let list = app.features.jobs.queue.list();
    let mut cancel = None;
    let mut clear = false;
    super::window("Jobs")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            if list.is_empty() {
                ui.label(
                    RichText::new("No jobs. Long batch work (Create PDF from Files, Batch Flatten) runs here.").weak(),
                );
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for j in list.iter().rev() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&j.name).strong());
                        ui.label(RichText::new(j.state.label()).weak());
                        if !j.state.finished() && ui.small_button("Cancel").clicked() {
                            cancel = Some(j.id);
                        }
                    });
                    ui.add(
                        egui::ProgressBar::new(j.fraction())
                            .desired_width(420.0)
                            .text(format!("{} of {}", j.done, j.total)),
                    );
                    if !j.message.is_empty() {
                        ui.label(RichText::new(&j.message).small());
                    }
                    ui.separator();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Clear Finished").clicked() {
                    clear = true;
                }
            });
        });
    if let Some(id) = cancel {
        app.features.jobs.queue.cancel(id);
    }
    if clear {
        app.features.jobs.queue.clear_finished();
    }
    if !open {
        app.features.jobs.open = false;
    }
}
