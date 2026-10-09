//! A viewport's scale by calibration (drag between the ends of a known length inside the
//! viewport) and with a separate Y scale, in the Measurements panel's rescale box.

use markupcraft_geom::Point;
use markupcraft_measure::units::{LengthUnit, real_units};

use crate::viewports::NewViewport;
use crate::{AppState, actions};

/// The rescale box's further options.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleMore {
    /// The known length and its unit for Calibrate.
    pub length: String,
    pub unit: LengthUnit,
    /// A separate Y scale (Y measured differently from X).
    pub separate_y: bool,
    pub y: Option<NewViewport>,
    /// Calibrate was asked for: (page, viewport index, length, unit); the frame starts the pick.
    pub want_pick: Option<(usize, usize, f64, LengthUnit)>,
    /// The calibration waiting for its two points.
    pub pending: Option<(usize, usize, f64, LengthUnit)>,
}

impl Default for ScaleMore {
    fn default() -> Self {
        Self {
            length: "10".into(),
            unit: LengthUnit::Foot,
            separate_y: false,
            y: None,
            want_pick: None,
            pending: None,
        }
    }
}

/// The calibrate and separate-Y rows inside the rescale box of viewport `i` on `page`.
pub fn rescale_rows(ui: &mut egui::Ui, more: &mut ScaleMore, page: usize, i: usize, nv: &NewViewport) {
    ui.horizontal(|ui| {
        ui.label("Calibrate: length");
        ui.add(egui::TextEdit::singleline(&mut more.length).desired_width(44.0));
        egui::ComboBox::from_id_salt("viewport-calibrate-unit")
            .selected_text(more.unit.label())
            .width(48.0)
            .show_ui(ui, |ui| {
                for u in real_units() {
                    ui.selectable_value(&mut more.unit, u, u.name());
                }
            });
    });
    ui.horizontal(|ui| {
        if ui
            .button("Pick Two Points")
            .on_hover_text("Drag from one end of the known length to the other")
            .clicked()
        {
            match more.length.trim().parse::<f64>() {
                Ok(l) if l > 0.0 && l.is_finite() => more.want_pick = Some((page, i, l, more.unit)),
                _ => {}
            }
        }
    });
    ui.checkbox(&mut more.separate_y, "Separate Y scale");
    if more.separate_y {
        let y = more.y.get_or_insert_with(|| nv.clone());
        ui.horizontal(|ui| {
            ui.label("Y");
            crate::viewports::scale_picker_pub(ui, "viewport-y-scale", y);
        });
    }
}

/// Apply the separate Y scale (when on) to `sc`.
pub fn with_y(more: &ScaleMore, mut sc: markupcraft_model::Scale) -> markupcraft_model::Scale {
    if more.separate_y
        && let Some(ys) = more.y.as_ref().and_then(NewViewport::scale)
    {
        sc.y = ys.x.clone();
    }
    sc
}

/// Once a frame: start the pick Calibrate asked for.
pub fn frame(app: &mut AppState) {
    if let Some(p) = app.edit.viewports_panel.more.want_pick.take() {
        app.edit.viewports_panel.more.pending = Some(p);
        crate::features::start_pick(
            app,
            crate::features::Pick::ViewportCalibrate,
            "Drag from one end of the known length to the other",
        );
    }
}

/// The two points were picked: calibrate the viewport.
pub fn picked(app: &mut AppState, pts: &[Point]) {
    let Some((page, i, length, unit)) = app.edit.viewports_panel.more.pending.take() else {
        return;
    };
    let (Some(a), Some(b)) = (pts.first(), pts.get(1)) else {
        return;
    };
    let r = markupcraft_engine::calibrated_scale(a.dist(*b), length, unit);
    let Some(d) = app.doc_mut() else { return };
    let res = r.and_then(|sc| d.session.set_viewport_scale(page, i, &sc, true).map(|n| (sc, n)));
    app.status = actions::report(res, |(sc, n)| {
        format!(
            "Viewport calibrated: {} ({} updated)",
            sc.ratio,
            actions::plural(n, "measurement")
        )
    });
    app.edit.viewports_panel.rescale = None;
}
