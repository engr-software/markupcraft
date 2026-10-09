//! Compare results: every change Compare Documents found, to step through, accept (keep the
//! cloud) or reject (delete it).

use egui::RichText;

use super::{PanelDef, Slot};
use crate::AppState;
use crate::features::compare::{self, Review};

pub static PANEL: PanelDef = PanelDef {
    id: "compare",
    title: "Compare",
    icon: "columns-3",
    slot: Slot::Bottom,
    keys: None,
    ui,
};

fn ui(app: &mut AppState, ui: &mut egui::Ui) {
    let (mut goto, mut review, mut new, mut delete_all, mut split) = (None, None, false, false, false);
    {
        let c = &app.features.compare;
        ui.horizontal(|ui| {
            if ui.button("Compare Documents...").clicked() {
                new = true;
            }
            let open = c.regions.iter().filter(|(_, s)| *s == Review::Open).count();
            ui.label(format!("{} changes, {open} to review", c.regions.len()));
            ui.add_enabled_ui(!c.regions.is_empty(), |ui| {
                if ui.button("< Prev").clicked() {
                    goto = Some(c.current.map_or(0, |i| i.saturating_sub(1)));
                }
                if ui.button("Next >").clicked() {
                    goto = Some(c.current.map_or(0, |i| (i + 1).min(c.regions.len().saturating_sub(1))));
                }
                if let Some(i) = c.current {
                    if ui.button("Accept").on_hover_text("Keep the cloud").clicked() {
                        review = Some((i, true));
                    }
                    if ui.button("Reject").on_hover_text("Delete the cloud").clicked() {
                        review = Some((i, false));
                    }
                }
                if ui.button("Delete all clouds").clicked() {
                    delete_all = true;
                }
                if ui
                    .button("Split + Dimmer")
                    .on_hover_text("Review beside the older revision, the drawing dimmed")
                    .clicked()
                {
                    split = true;
                }
            });
        });
        if c.regions.is_empty() {
            super::empty(ui, "Run Tools > Compare Documents to list the changes.");
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("compare-list")
                .striped(true)
                .num_columns(5)
                .show(ui, |ui| {
                    for h in ["#", "Page", "Change", "Found in", "Text"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, (r, s)) in c.regions.iter().enumerate() {
                        let mark = match s {
                            Review::Open => "",
                            Review::Accepted => " (accepted)",
                            Review::Rejected => " (rejected)",
                        };
                        if ui
                            .selectable_label(c.current == Some(i), format!("{}{mark}", i + 1))
                            .clicked()
                        {
                            goto = Some(i);
                        }
                        ui.label(format!("{}", r.page + 1));
                        ui.label(&r.kind);
                        ui.label(&r.source);
                        ui.label(r.text.chars().take(80).collect::<String>());
                        ui.end_row();
                    }
                });
        });
    }
    if new {
        compare::open(app);
    }
    if let Some(i) = goto {
        compare::go_to(app, i);
    }
    if let Some((i, a)) = review {
        compare::review(app, i, a);
    }
    if delete_all {
        compare::delete_all(app);
    }
    if split {
        compare::review_split(app);
    }
}
