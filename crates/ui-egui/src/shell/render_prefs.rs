//! Preferences > Advanced > 2D Rendering (in `UiPrefs::render`): Enhance Thin Lines (linework
//! thinner than a minimum drawn at that minimum, from a line-weight copy the renderer reads,
//! `markupcraft_render::thin`), progressive display (tiles appear as they are rendered, or the
//! page waits until every tile on screen is ready), blend modes (or everything composited as
//! Normal, from the same copy), the low-resolution preview under tiled
//! pages, the screen resolution pages are rendered at, and the highest resolution of Print as
//! Image. The view reads them through `ShellState::view_opts`; the printer through
//! [`print_dpi`].

use egui::RichText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderPrefs {
    /// Linework thinner than [`Self::min_line_pt`] on the page draws that wide (hairlines too).
    pub enhance_thin_lines: bool,
    /// The thinnest line drawn with Enhance Thin Lines, points (0.1 to 4).
    pub min_line_pt: f32,
    /// Tiles of a large page show as soon as each is rendered (else the page waits until all
    /// the tiles on screen are ready, showing its preview meanwhile).
    pub progressive: bool,
    /// A low-resolution image of a large page shows under its tiles while they render.
    pub low_res_preview: bool,
    /// Pages render at this share of the screen's resolution, percent (25 to 200).
    pub resolution_pct: f32,
    /// Print as Image never rasterizes above this many dots per inch (72 to 2400).
    pub max_print_dpi: f32,
    /// Blend modes (Multiply, Screen...) are drawn (else everything composites as Normal).
    pub blend_modes: bool,
}

impl Default for RenderPrefs {
    fn default() -> Self {
        Self {
            enhance_thin_lines: false,
            min_line_pt: 0.5,
            progressive: true,
            low_res_preview: true,
            resolution_pct: 100.0,
            max_print_dpi: 600.0,
            blend_modes: true,
        }
    }
}

impl RenderPrefs {
    pub fn sanitize(&mut self) {
        let fin = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let d = Self::default();
        self.min_line_pt = fin(self.min_line_pt, 0.1, 4.0, d.min_line_pt);
        self.resolution_pct = fin(self.resolution_pct, 25.0, 200.0, d.resolution_pct);
        self.max_print_dpi = fin(self.max_print_dpi, 72.0, 2400.0, d.max_print_dpi);
    }

    /// The minimum line width the renderer's copy enforces, if any (hundredths of a point,
    /// so equal settings compare equal).
    pub fn min_line_key(&self) -> Option<u32> {
        self.enhance_thin_lines
            .then(|| (self.min_line_pt.clamp(0.1, 4.0) * 100.0).round() as u32)
    }
}

/// The resolution Print as Image uses: the dialog's, capped by the preference.
pub fn print_dpi(asked: f64, prefs: &RenderPrefs) -> f64 {
    asked.min(f64::from(prefs.max_print_dpi.clamp(72.0, 2400.0)))
}

/// The page's options (drawn under Advanced).
pub fn section(ui: &mut egui::Ui, r: &mut RenderPrefs) {
    ui.add_space(6.0);
    ui.label(RichText::new("Rendering").strong());
    ui.horizontal(|ui| {
        ui.checkbox(&mut r.enhance_thin_lines, "Enhance thin lines: draw linework at least");
        ui.add_enabled(
            r.enhance_thin_lines,
            egui::DragValue::new(&mut r.min_line_pt)
                .range(0.1..=4.0)
                .speed(0.05)
                .suffix(" pt"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Display");
        ui.radio_value(&mut r.progressive, true, "Progressive (tiles as they render)");
        ui.radio_value(&mut r.progressive, false, "Wait for the whole view");
    });
    ui.checkbox(
        &mut r.blend_modes,
        "Draw blend modes (else everything composites as Normal)",
    );
    ui.checkbox(
        &mut r.low_res_preview,
        "Low-resolution preview under large pages while they render",
    );
    ui.horizontal(|ui| {
        ui.label("Screen resolution");
        ui.add(egui::Slider::new(&mut r.resolution_pct, 25.0..=200.0).suffix(" %"));
    });
    ui.horizontal(|ui| {
        ui.label("Highest print resolution (Print as Image)");
        ui.add(
            egui::DragValue::new(&mut r.max_print_dpi)
                .range(72.0..=2400.0)
                .suffix(" dpi"),
        );
    });
    ui.label(
        RichText::new(
            "One rendering engine draws everything. CMYK colours are converted to screen RGB without colour calibration; fills are always anti-aliased.",
        )
        .weak()
        .size(11.0),
    );
}
