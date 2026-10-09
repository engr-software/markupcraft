//! Page scales, calibration, viewports, and measuring a set of points.
//!
//! A page's scales are its `/VP` viewports. A page scale is a viewport covering the whole page
//! (the shape Revu writes); other viewports cover part of it and win where they apply. The
//! session writes `/VP` itself for the pages it changed, so partial viewports survive a page
//! recalibration.

use std::collections::BTreeSet;

use markupcraft_measure::units::LengthUnit;
use markupcraft_measure::{Fmt, NumberFormat};
use markupcraft_model::{Document, Kind, Markup, Point, Rect, Scale, Viewport};
use markupcraft_revu::cos::{Dict, Document as CosDoc, Object};
use markupcraft_revu::pdf::{self, n, rect_arr, s};

use crate::{Result, Session, invalid};

fn full_page(vp: &Viewport, media: &Rect) -> bool {
    let (a, b) = (vp.bbox.normalized(), media.normalized());
    (a.x0 - b.x0).abs() < 0.5 && (a.y0 - b.y0).abs() < 0.5 && (a.x1 - b.x1).abs() < 0.5 && (a.y1 - b.y1).abs() < 0.5
}

/// Write `/VP` on every page in `changed` from the model's viewports.
pub(crate) fn write_viewports(cos: &mut CosDoc, doc: &mut Document, changed: &BTreeSet<usize>) {
    if changed.is_empty() {
        return;
    }
    let pages = pdf::pages(cos);
    for &i in changed {
        let (Some(pg), Some(info)) = (pages.get(i), doc.pages.get_mut(i)) else {
            continue;
        };
        let (ox, oy) = (info.media.x0, info.media.y0);
        let vps: Vec<Object> = info
            .viewports
            .iter()
            .map(|v| {
                let b = v.bbox.normalized();
                let mut d = Dict::new();
                pdf::set(&mut d, "Type", n("Viewport"));
                pdf::set(
                    &mut d,
                    "BBox",
                    rect_arr(&Rect::new(b.x0 - ox, b.y0 - oy, b.x1 - ox, b.y1 - oy)),
                );
                pdf::set(&mut d, "Measure", markupcraft_revu::scale::object(&v.scale));
                if !v.name.is_empty() {
                    pdf::set(&mut d, "Name", s(&v.name));
                }
                if !v.id.is_empty() {
                    pdf::set(&mut d, "NM", s(&v.id));
                }
                Object::Dict(d)
            })
            .collect();
        let _ = cos.update_dict(pg.id, |d| {
            if vps.is_empty() {
                d.remove(b"VP");
            } else {
                d.set(b"VP".to_vec(), Object::Array(vps));
            }
        });
        // Written here; the markup writer must not replace it with a single page viewport.
        info.scale_changed = false;
    }
}

/// A decimal scale in a metric (or any) unit: `pt_to_unit` units per PDF point.
pub fn decimal_scale(pt_to_unit: f64, unit: LengthUnit) -> Scale {
    let u = unit.label();
    Scale {
        ratio: format!("1 in = {} {u}", markupcraft_measure::fmt_g(pt_to_unit * 72.0, 6)),
        x: vec![NumberFormat::new(u, pt_to_unit, Fmt::Decimal, 100, " ", "")],
        dist: vec![NumberFormat::new(u, 1.0, Fmt::Decimal, 100, " ", "")],
        area: vec![NumberFormat::new(&format!("sq {u}"), 1.0, Fmt::Decimal, 100, " ", "")],
        volume: vec![NumberFormat::new(&format!("cu {u}"), 1.0, Fmt::Decimal, 100, " ", "")],
        ..Default::default()
    }
}

/// The scale that makes a line `points` long on the sheet measure `length` `unit`s. Feet and
/// inches give Revu's feet-inches layout; other units a decimal scale in that unit.
pub fn calibrated_scale(points: f64, length: f64, unit: LengthUnit) -> Result<Scale> {
    if !(points.is_finite() && points > 1e-6) {
        return Err(invalid("the two calibration points must be apart"));
    }
    if !(length.is_finite() && length > 0.0) {
        return Err(invalid("the real length must be a positive number"));
    }
    Ok(match unit {
        LengthUnit::Foot | LengthUnit::Inch => {
            let feet = length * unit.meters() / LengthUnit::Foot.meters();
            Scale::calibrated(points, feet)
        }
        u => decimal_scale(length / points, u),
    })
}

/// The result of measuring a set of points.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub kind: Kind,
    pub value: f64,
    /// formatted like Revu writes it
    pub text: String,
    pub unit: String,
    /// the scale used (`/R` text)
    pub scale: String,
}

impl Session {
    fn set_full_page_scale(&mut self, page: usize, sc: &Scale) {
        let id = self.new_id();
        let Some(info) = self.doc.pages.get_mut(page) else {
            return;
        };
        let media = info.media;
        info.viewports.retain(|v| !full_page(v, &media));
        info.viewports.push(Viewport {
            bbox: media,
            name: String::new(),
            id,
            scale: sc.clone(),
        });
        info.scale = Some(sc.clone());
        info.scale_changed = false;
        self.vp_changed.insert(page);
    }

    /// Set the page scale of `pages`. With `apply_to_markups`, measurements on those pages
    /// that are not inside a partial viewport take the new scale too.
    pub fn set_page_scale(&mut self, pages: &[usize], sc: &Scale, apply_to_markups: bool) -> Result<usize> {
        if !sc.valid() {
            return Err(invalid("the scale has no usable /X conversion"));
        }
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        for p in pages {
            self.page(*p)?;
        }
        self.edit("Set Scale", |s| {
            let mut updated = 0;
            for &p in pages {
                s.set_full_page_scale(p, sc);
                if !apply_to_markups {
                    continue;
                }
                let Some(info) = s.doc.pages.get(p).cloned() else {
                    continue;
                };
                for m in s.doc.markups.iter_mut().filter(|m| m.page == p) {
                    if !m.kind.is_measurement() || m.kind == Kind::Count || m.locked() {
                        continue;
                    }
                    let at = m.pts.first().copied().unwrap_or_default();
                    let partial = info
                        .viewports
                        .iter()
                        .any(|v| !full_page(v, &info.media) && v.bbox.contains(at));
                    if partial {
                        continue;
                    }
                    m.scale = Some(sc.clone());
                    m.dirty = true;
                    updated += 1;
                }
            }
            Ok((updated, true))
        })
    }

    /// Calibrate `pages` from two points on `page` that are `length` `unit`s apart.
    pub fn calibrate(
        &mut self,
        pages: &[usize],
        a: Point,
        b: Point,
        length: f64,
        unit: LengthUnit,
        apply_to_markups: bool,
    ) -> Result<Scale> {
        let sc = calibrated_scale(a.dist(b), length, unit)?;
        self.set_page_scale(pages, &sc, apply_to_markups)?;
        Ok(sc)
    }

    /// Add a viewport (a scale for part of a page).
    pub fn add_viewport(&mut self, page: usize, bbox: Rect, name: &str, sc: &Scale) -> Result<String> {
        self.page(page)?;
        if !sc.valid() {
            return Err(invalid("the scale has no usable /X conversion"));
        }
        let b = bbox.normalized();
        if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) || b.width() <= 0.0 || b.height() <= 0.0 {
            return Err(invalid("the viewport box must have a positive width and height"));
        }
        let id = self.new_id();
        self.edit("Add Viewport", |s| {
            let Some(info) = s.doc.pages.get_mut(page) else {
                return Err(invalid("page vanished"));
            };
            // Partial viewports go before the page scale, so they win where they apply.
            let at = info
                .viewports
                .iter()
                .position(|v| full_page(v, &info.media))
                .unwrap_or(info.viewports.len());
            info.viewports.insert(
                at,
                Viewport {
                    bbox: b,
                    name: name.to_string(),
                    id: id.clone(),
                    scale: sc.clone(),
                },
            );
            s.vp_changed.insert(page);
            Ok((id, true))
        })
    }

    /// Remove viewport `index` (0-based, in the page's order) from `page`.
    pub fn delete_viewport(&mut self, page: usize, index: usize) -> Result<()> {
        let count = self.page(page)?.viewports.len();
        if index >= count {
            return Err(invalid(format!(
                "page {} has {count} viewports; there is no viewport {}",
                page + 1,
                index + 1
            )));
        }
        self.edit("Delete Viewport", |s| {
            if let Some(info) = s.doc.pages.get_mut(page) {
                info.viewports.remove(index);
                if !info.viewports.iter().any(|v| full_page(v, &info.media)) {
                    info.scale = None;
                }
            }
            s.vp_changed.insert(page);
            Ok(((), true))
        })
    }

    /// Measure `pts` on `page` as a `kind` measurement, with `scale` or the page's scale
    /// where the first point is.
    pub fn measure(&self, page: usize, kind: Kind, pts: &[Point], scale: Option<&Scale>) -> Result<Measurement> {
        let info = self.page(page)?;
        if !kind.is_measurement() {
            return Err(invalid(format!("{} is not a measurement kind", kind.name())));
        }
        crate::geometry::check_points(kind, pts)?;
        let first = pts.first().copied().unwrap_or_default();
        let sc = match scale {
            Some(s) => Some(s.clone()),
            None => info.scale_at(first).cloned(),
        };
        if kind != Kind::Count && sc.as_ref().is_none_or(|s| !s.valid()) {
            return Err(invalid(format!(
                "page {} has no scale at ({}, {}); set one with scale_set or scale_calibrate, or pass a scale",
                page + 1,
                first.x,
                first.y
            )));
        }
        let mut m = Markup::new(kind, page, pts.to_vec());
        m.scale = sc;
        let value = m
            .quantity()
            .ok_or_else(|| invalid(format!("{} has no quantity MarkupCraft computes yet", kind.name())))?;
        Ok(Measurement {
            kind,
            value,
            text: m.quantity_text(),
            unit: m.unit(),
            scale: m.scale.as_ref().map(|s| s.ratio.clone()).unwrap_or_default(),
        })
    }
}
