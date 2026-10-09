//! The Flatten tab's further options: onto a named layer, recoverable (Unflatten restores),
//! overlay text (font, size, position), properties kept in a pop-up per flattened markup, and
//! a capture summary attached for flattened File Attachments.

use markupcraft_engine::finish::flatten_more::{FlattenExtras, OverlayPos};
use markupcraft_engine::flatten::FlattenOptions;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlattenMore {
    pub layer: String,
    pub recoverable: bool,
    pub extras: FlattenExtras,
}

const KEEP: [(&str, &str); 6] = [
    ("subject", "Subject"),
    ("author", "Author"),
    ("comments", "Comments"),
    ("label", "Label"),
    ("status", "Status"),
    ("date", "Date"),
];

const POS: [(OverlayPos, &str); 6] = [
    (OverlayPos::TopLeft, "Top left"),
    (OverlayPos::TopCenter, "Top centre"),
    (OverlayPos::TopRight, "Top right"),
    (OverlayPos::BottomLeft, "Bottom left"),
    (OverlayPos::BottomCenter, "Bottom centre"),
    (OverlayPos::BottomRight, "Bottom right"),
];

pub fn rows(ui: &mut egui::Ui, m: &mut FlattenMore) {
    ui.horizontal(|ui| {
        ui.label("Onto layer:");
        ui.add(egui::TextEdit::singleline(&mut m.layer).hint_text("none"));
        ui.checkbox(&mut m.recoverable, "Recoverable (Unflatten restores)");
    });
    let x = &mut m.extras;
    ui.horizontal(|ui| {
        ui.label("Overlay text:");
        ui.add(
            egui::TextEdit::singleline(&mut x.overlay)
                .hint_text("none")
                .desired_width(140.0),
        );
    });
    if !x.overlay.is_empty() {
        ui.horizontal(|ui| {
            if x.font.is_empty() {
                x.font = "Helvetica".into();
            }
            if x.size <= 0.0 {
                x.size = 10.0;
            }
            egui::ComboBox::from_id_salt("flatten-font")
                .selected_text(x.font.clone())
                .show_ui(ui, |ui| {
                    for f in ["Helvetica", "Times-Roman", "Courier"] {
                        ui.selectable_value(&mut x.font, f.to_string(), f);
                    }
                });
            ui.add(egui::DragValue::new(&mut x.size).range(4.0..=144.0).suffix(" pt"));
            egui::ComboBox::from_id_salt("flatten-pos")
                .selected_text(POS.iter().find(|p| p.0 == x.position).map_or("", |p| p.1))
                .show_ui(ui, |ui| {
                    for (p, l) in POS {
                        ui.selectable_value(&mut x.position, p, l);
                    }
                });
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Keep in a pop-up:");
        for (k, l) in KEEP {
            let mut on = x.keep.iter().any(|v| v == k);
            if ui.checkbox(&mut on, l).changed() {
                x.keep.retain(|v| v != k);
                if on {
                    x.keep.push(k.to_string());
                }
            }
        }
    });
    ui.checkbox(
        &mut x.capture_summary,
        "Attach a capture summary of flattened attachments",
    );
}

/// The engine's options.
pub fn options(m: &FlattenMore) -> FlattenOptions {
    FlattenOptions {
        recoverable: m.recoverable,
        layer: (!m.layer.trim().is_empty()).then(|| m.layer.trim().to_string()),
    }
}
