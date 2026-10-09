//! Overlay Pages: pick the layers (open documents or files), a colour, opacity and page for
//! each, how to align them, and write the overlay to a new PDF that opens in a new tab.

use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::overlay::{
    LayerAdjust, OverlayAlign, OverlayBlend, OverlayLayer, default_color, overlay_pages,
};
use markupcraft_geom::{Point, Rect};
use markupcraft_model::Color;

use super::Ask;
use crate::dialogs::{PDF, Purpose};
use crate::{AppState, actions};

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Doc(u64),
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Row {
    pub source: Source,
    pub name: String,
    pub on: bool,
    /// 1-based page of this layer.
    pub page: usize,
    pub color: Color,
    pub opacity: f64,
    /// Points on this layer that land on `to` on the first layer (two or three points:
    /// x0 y0 x1 y1 x2 y2).
    pub from: [f64; 6],
    pub to: [f64; 6],
    /// The layer's whitespace colour (Edit Layer > Background).
    pub background: Option<Color>,
    /// Edit Layer > Select Region: x0 y0 x1 y1 on this layer's page.
    pub region: Option<[f64; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Page,
    Bounds,
    Points,
    Three,
    Auto,
}

#[derive(Default)]
pub struct OverlayState {
    pub open: bool,
    pub rows: Vec<Row>,
    pub align: Align,
    pub message: String,
    /// Edit Defaults: blend and position for every layer after the first.
    pub blend: OverlayBlend,
    pub adjust: LayerAdjust,
    pub include_flattened: bool,
}

fn row(source: Source, name: String, i: usize) -> Row {
    Row {
        source,
        name,
        on: true,
        page: 1,
        color: default_color(i),
        opacity: 1.0,
        from: [0.0, 0.0, 100.0, 0.0, 0.0, 100.0],
        to: [0.0, 0.0, 100.0, 0.0, 0.0, 100.0],
        background: None,
        region: None,
    }
}

/// Fill the list with the open documents not listed yet.
fn sync_docs(app: &mut AppState) {
    let docs: Vec<(u64, String)> = app.docs.iter().map(|d| (d.uid, d.name.clone())).collect();
    let o = &mut app.features.overlay;
    o.rows.retain(|r| match r.source {
        Source::Doc(u) => docs.iter().any(|(d, _)| *d == u),
        Source::File(_) => true,
    });
    for (u, n) in docs {
        if !o.rows.iter().any(|r| r.source == Source::Doc(u)) {
            let i = o.rows.len();
            o.rows.push(row(Source::Doc(u), n, i));
        }
    }
}

pub fn add_files(app: &mut AppState, paths: &[PathBuf]) {
    let o = &mut app.features.overlay;
    for p in paths {
        let i = o.rows.len();
        let name = p
            .file_name()
            .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
        o.rows.push(row(Source::File(p.clone()), name, i));
    }
    o.open = true;
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.overlay.open {
        return;
    }
    sync_docs(app);
    let (mut open, mut add, mut ok) = (true, false, false);
    let mut remove = None;
    super::window("Overlay Pages").open(&mut open).default_width(560.0).show(ctx, |ui| {
        let o = &mut app.features.overlay;
        ui.label("The first checked layer is the base page; the others are drawn over it.");
        egui::Grid::new("overlay-rows").striped(true).num_columns(8).show(ui, |ui| {
            for h in ["", "Layer", "Page", "Color", "Opacity", "Background", "Region", ""] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for (i, r) in o.rows.iter_mut().enumerate() {
                ui.checkbox(&mut r.on, "");
                ui.label(&r.name);
                ui.add(egui::DragValue::new(&mut r.page).range(1..=100_000));
                super::color_edit(ui, &mut r.color);
                ui.add(egui::Slider::new(&mut r.opacity, 0.0..=1.0).fixed_decimals(2));
                ui.horizontal(|ui| {
                    let mut on = r.background.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        r.background = on.then_some(Color::rgb(1.0, 1.0, 0.85));
                    }
                    if let Some(c) = r.background.as_mut() {
                        super::color_edit(ui, c);
                    }
                });
                ui.horizontal(|ui| {
                    let mut on = r.region.is_some();
                    if ui.checkbox(&mut on, "").on_hover_text("Overlay only a region of this page").changed() {
                        r.region = on.then_some([0.0, 0.0, 612.0, 792.0]);
                    }
                    if let Some(g) = r.region.as_mut() {
                        for v in g.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(1.0));
                        }
                    }
                });
                if matches!(r.source, Source::File(_)) && ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if ui.button("Add Files...").clicked() {
            add = true;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Align:");
            ui.selectable_value(&mut o.align, Align::Page, "Page");
            ui.selectable_value(&mut o.align, Align::Bounds, "Fit to page");
            ui.selectable_value(&mut o.align, Align::Points, "Two points");
            ui.selectable_value(&mut o.align, Align::Three, "Manual (3 points)");
            ui.selectable_value(&mut o.align, Align::Auto, "Auto Align");
        });
        if matches!(o.align, Align::Points | Align::Three) {
            let n = if o.align == Align::Three { 6 } else { 4 };
            ui.label(RichText::new("For each layer after the first: the matching points on it (from) and where they land on the base page (to), in PDF points, in the same order.").small());
            for r in o.rows.iter_mut().filter(|r| r.on).skip(1) {
                ui.horizontal(|ui| {
                    ui.label(&r.name);
                    for v in r.from.iter_mut().take(n) {
                        ui.add(egui::DragValue::new(v).speed(1.0));
                    }
                    ui.label("to");
                    for v in r.to.iter_mut().take(n) {
                        ui.add(egui::DragValue::new(v).speed(1.0));
                    }
                });
            }
        }
        ui.collapsing("Edit Defaults", |ui| {
            ui.horizontal(|ui| {
                ui.label("Blend:");
                for (b, label) in [
                    (OverlayBlend::Multiply, "Multiply"),
                    (OverlayBlend::Darken, "Darken"),
                    (OverlayBlend::Normal, "Normal"),
                    (OverlayBlend::Screen, "Screen"),
                    (OverlayBlend::Difference, "Difference"),
                ] {
                    ui.selectable_value(&mut o.blend, b, label);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Rotation");
                ui.add(egui::DragValue::new(&mut o.adjust.rotation).range(-360.0..=360.0).suffix(" deg"));
                ui.label("Scale");
                ui.add(egui::DragValue::new(&mut o.adjust.scale).range(0.01..=100.0).speed(0.01));
                ui.label("X");
                ui.add(egui::DragValue::new(&mut o.adjust.dx));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut o.adjust.dy));
            });
            ui.checkbox(&mut o.include_flattened, "Include flattened markups");
        });
        if !o.message.is_empty() {
            ui.label(RichText::new(&o.message).small());
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                o.open = false;
            }
        });
    });
    if let Some(i) = remove {
        app.features.overlay.rows.remove(i);
    }
    if !open {
        app.features.overlay.open = false;
    }
    if add {
        app.dialogs.open(Purpose::Feature(Ask::OverlayAdd), PDF, true);
    }
    if ok {
        if app.features.overlay.rows.iter().filter(|r| r.on).count() < 2 {
            app.features.overlay.message = "Check at least two layers".into();
        } else {
            app.dialogs.save(Purpose::Feature(Ask::OverlayOut), PDF, "Overlay.pdf");
        }
    }
}

/// Build the layers from the dialog.
pub fn layers(app: &AppState) -> Result<Vec<OverlayLayer>, String> {
    let o = &app.features.overlay;
    let mut out = Vec::new();
    for (k, r) in o.rows.iter().filter(|r| r.on).enumerate() {
        let bytes = match &r.source {
            Source::Doc(u) => app
                .docs
                .iter()
                .find(|d| d.uid == *u)
                .ok_or_else(|| format!("{} is closed", r.name))?
                .session
                .render_bytes()
                .map_err(|e| e.to_string())?,
            Source::File(p) => markupcraft_engine::raster::read_pdf(p).map_err(|e| e.to_string())?,
        };
        let bytes = if o.include_flattened {
            bytes
        } else {
            markupcraft_engine::flatten::without_flattened(bytes)
        };
        let pt = |v: &[f64; 6], i: usize| Point::new(v[2 * i], v[2 * i + 1]);
        let align = match (o.align, k) {
            (_, 0) | (Align::Page, _) => OverlayAlign::Page,
            (Align::Bounds, _) => OverlayAlign::Bounds,
            (Align::Points, _) => OverlayAlign::Points {
                from: [pt(&r.from, 0), pt(&r.from, 1)],
                to: [pt(&r.to, 0), pt(&r.to, 1)],
            },
            (Align::Three, _) => OverlayAlign::Three {
                from: [pt(&r.from, 0), pt(&r.from, 1), pt(&r.from, 2)],
                to: [pt(&r.to, 0), pt(&r.to, 1), pt(&r.to, 2)],
            },
            (Align::Auto, _) => OverlayAlign::Auto,
        };
        out.push(OverlayLayer {
            pages: vec![r.page.saturating_sub(1)],
            color: r.color,
            opacity: r.opacity.clamp(0.0, 1.0),
            name: r.name.clone(),
            align,
            background: r.background,
            blend: if k == 0 { OverlayBlend::Multiply } else { o.blend },
            region: r.region.map(|g| Rect::new(g[0], g[1], g[2], g[3])),
            adjust: if k == 0 { LayerAdjust::default() } else { o.adjust },
            ..OverlayLayer::new(bytes)
        });
    }
    Ok(out)
}

/// Write the overlay to `out` and open it.
pub fn write(app: &mut AppState, out: &Path) {
    let r = layers(app).and_then(|l| overlay_pages(&l, out).map_err(|e| e.to_string()));
    match r {
        Ok(rep) => {
            app.features.overlay.open = false;
            app.features.overlay.message.clear();
            app.open_path(out);
            app.status = format!(
                "Overlay of {} written to {}",
                actions::plural(rep.layers.len(), "layer"),
                out.display()
            );
        }
        Err(e) => app.features.overlay.message = e,
    }
}
