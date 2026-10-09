//! Page boxes and page size: read the media, crop, bleed, trim and art boxes; crop pages (set
//! the crop box by rectangle or margins, or remove it); and resize pages (Page Setup) by
//! changing the page canvas around the drawing.
//!
//! Resizing keeps every drawing, markup and scaled viewport where it is in user space: the
//! media box grows or shrinks around it from an anchor (centre, a corner or an edge). Viewports
//! are stored relative to the media box corner, so they are moved to stay put; a page-wide
//! viewport (the page scale) is widened to the new page.

use markupcraft_model::Rect;
use markupcraft_revu::cos::{Document as CosDoc, ObjRef, Object};
use pdfcraft_organize::BoxSpec;
pub use pdfcraft_organize::PageBox;

use crate::docutil::{err, page_objs};
use crate::{Result, Session, invalid};

/// Largest page side (PDF's own limit, 200 inches).
const MAX_SIDE: f64 = 14_400.0;

/// The boxes of one page (effective values, user space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBoxes {
    pub media: Rect,
    pub crop: Rect,
    pub bleed: Rect,
    pub trim: Rect,
    pub art: Rect,
}

/// How to set a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxChange {
    /// Back to its default (the parent box). Not for the media box.
    Remove,
    Rect(Rect),
    /// Inward from the media box: left, bottom, right, top (points).
    Margins([f64; 4]),
}

/// Where a resized page keeps its drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub fn from_name(s: &str) -> Option<Anchor> {
        Some(match s.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
            "center" | "centre" | "middle" => Anchor::Center,
            "topleft" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "topright" => Anchor::TopRight,
            "left" => Anchor::Left,
            "right" => Anchor::Right,
            "bottomleft" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottomright" => Anchor::BottomRight,
            _ => return None,
        })
    }

    /// Fractions (0 left/bottom, 0.5 centre, 1 right/top) of the old box that stay in place.
    fn weights(self) -> (f64, f64) {
        match self {
            Anchor::Center => (0.5, 0.5),
            Anchor::TopLeft => (0.0, 1.0),
            Anchor::Top => (0.5, 1.0),
            Anchor::TopRight => (1.0, 1.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 0.0),
            Anchor::Bottom => (0.5, 0.0),
            Anchor::BottomRight => (1.0, 0.0),
        }
    }
}

fn rect(a: [f64; 4]) -> Rect {
    Rect::new(a[0], a[1], a[2], a[3])
}

fn arr(r: Rect) -> Object {
    Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect())
}

/// Shift the page's `/VP` boxes (stored relative to the media box corner) so they keep their
/// place when the media box moves from `old` to `new`; a page-wide one becomes the new page.
fn keep_viewports(cos: &mut CosDoc, page: ObjRef, old: Rect, new: Rect) -> Result<()> {
    let p = cos.get(page);
    let Some(vp) = p.as_dict().and_then(|d| d.get(b"VP")).cloned() else {
        return Ok(());
    };
    let Some(list) = cos.resolve(&vp).as_array().cloned() else {
        return Ok(());
    };
    let (dx, dy) = (old.x0 - new.x0, old.y0 - new.y0);
    let width_old = old.width();
    let height_old = old.height();
    let mut out = Vec::with_capacity(list.len());
    for v in list {
        let Some(mut d) = cos.dict(&v) else {
            out.push(v);
            continue;
        };
        let b: Option<Vec<f64>> = d.get(b"BBox").map(|b| cos.resolve(b)).and_then(|b| {
            b.as_array()
                .map(|a| a.iter().filter_map(|x| cos.resolve(x).as_f64()).collect())
        });
        if let Some(b) = b.filter(|b| b.len() == 4) {
            let r = Rect::new(b[0], b[1], b[2], b[3]).normalized();
            let whole = r.x0.abs() < 0.5
                && r.y0.abs() < 0.5
                && (r.width() - width_old).abs() < 0.5
                && (r.height() - height_old).abs() < 0.5;
            let moved = if whole {
                Rect::new(0.0, 0.0, new.width(), new.height())
            } else {
                Rect::new(r.x0 + dx, r.y0 + dy, r.x1 + dx, r.y1 + dy)
            };
            d.set(b"BBox".to_vec(), arr(moved));
        }
        match v.as_ref() {
            Some(r) => {
                cos.set(r, Object::Dict(d));
                out.push(v);
            }
            None => out.push(Object::Dict(d)),
        }
    }
    match vp.as_ref().filter(|r| cos.get(*r).as_array().is_some()) {
        Some(r) => cos.set(r, Object::Array(out)),
        None => cos.update_dict(page, |d| d.set(b"VP".to_vec(), Object::Array(out)))?,
    }
    Ok(())
}

impl Session {
    /// The boxes of every page.
    pub fn page_boxes(&self) -> Result<Vec<PageBoxes>> {
        Ok(pdfcraft_organize::page_boxes(&self.file.cos)
            .map_err(err)?
            .into_iter()
            .map(|b| PageBoxes {
                media: rect(b[0]),
                crop: rect(b[1]),
                bleed: rect(b[2]),
                trim: rect(b[3]),
                art: rect(b[4]),
            })
            .collect())
    }

    /// Set box `which` on `pages`. Undoable.
    pub fn set_page_box(&mut self, pages: &[usize], which: PageBox, change: BoxChange) -> Result<()> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let spec = match change {
            BoxChange::Remove => BoxSpec::Remove,
            BoxChange::Rect(r) => BoxSpec::Rect(r.normalized().as_array()),
            BoxChange::Margins(m) => BoxSpec::Margins(m),
        };
        if which == PageBox::Media {
            return Err(invalid("change the page size with page_resize"));
        }
        let label = if which == PageBox::Crop {
            "Crop Pages"
        } else {
            "Page Boxes"
        };
        self.graph_edit(label, |cos, _| {
            pdfcraft_organize::set_page_box(cos, pages, which, spec).map_err(err)?;
            Ok(())
        })
    }

    /// Resize `pages` to `width` x `height` points, keeping the drawing in place at `anchor`.
    /// The crop and other boxes are reset to the new page. Undoable.
    pub fn resize_pages(&mut self, pages: &[usize], width: f64, height: f64, anchor: Anchor) -> Result<()> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        if !(width.is_finite()
            && height.is_finite()
            && (3.0..=MAX_SIDE).contains(&width)
            && (3.0..=MAX_SIDE).contains(&height))
        {
            return Err(invalid(format!("page sides must be 3 to {MAX_SIDE} points")));
        }
        for p in pages {
            self.page(*p)?;
        }
        let boxes = self.page_boxes()?;
        let (wx, wy) = anchor.weights();
        self.graph_edit("Resize Pages", |cos, _| {
            let objs = page_objs(cos)?;
            for p in pages {
                let (Some(page), Some(b)) = (objs.get(*p).copied(), boxes.get(*p)) else {
                    continue;
                };
                // Size is as displayed: a page turned a quarter swaps its sides.
                let rot = cos
                    .get(page)
                    .as_dict()
                    .and_then(|d| d.get(b"Rotate").and_then(|r| cos.resolve(r).as_int()))
                    .unwrap_or(0)
                    .rem_euclid(360);
                let (w, h) = if rot % 180 == 90 {
                    (height, width)
                } else {
                    (width, height)
                };
                let old = b.crop.normalized();
                let x0 = old.x0 + wx * (old.width() - w);
                let y0 = old.y0 + wy * (old.height() - h);
                let new = Rect::new(x0, y0, x0 + w, y0 + h);
                keep_viewports(cos, page, b.media.normalized(), new)?;
                cos.update_dict(page, |d| {
                    d.set(b"MediaBox".to_vec(), arr(new));
                    for k in [&b"CropBox"[..], b"BleedBox", b"TrimBox", b"ArtBox"] {
                        d.remove(k);
                    }
                })?;
            }
            Ok(())
        })
    }
}
