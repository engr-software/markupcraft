//! Apply Stamp: one stamp at the same spot on many pages (and, through batch tools, many
//! files): a page filter (a page list, odd / even, portrait / landscape), an anchor on a 3 x 3
//! grid of the page with an X / Y offset, a scale, a rotation, and the stamp settings' opacity,
//! blend mode and lock.

use std::collections::BTreeMap;

use markupcraft_geom::bbox;
use markupcraft_model::{Point, Rect};

use crate::stamps::{StampLibrary, StampPlace, StampSource};
use crate::{MarkupPatch, Result, Session, invalid};

/// Where on the page the stamp is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(
            match s.trim().to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
                "topleft" => Anchor::TopLeft,
                "top" | "topcenter" => Anchor::Top,
                "topright" => Anchor::TopRight,
                "left" | "middleleft" => Anchor::Left,
                "center" | "centre" | "middle" => Anchor::Center,
                "right" | "middleright" => Anchor::Right,
                "bottomleft" => Anchor::BottomLeft,
                "bottom" | "bottomcenter" => Anchor::Bottom,
                "bottomright" => Anchor::BottomRight,
                _ => return None,
            },
        )
    }

    /// (-1, 0 or 1 across, -1, 0 or 1 up).
    fn grid(self) -> (i8, i8) {
        match self {
            Anchor::TopLeft => (-1, 1),
            Anchor::Top => (0, 1),
            Anchor::TopRight => (1, 1),
            Anchor::Left => (-1, 0),
            Anchor::Center => (0, 0),
            Anchor::Right => (1, 0),
            Anchor::BottomLeft => (-1, -1),
            Anchor::Bottom => (0, -1),
            Anchor::BottomRight => (1, -1),
        }
    }
}

/// Which of the chosen pages get the stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageFilter {
    #[default]
    All,
    Odd,
    Even,
    Portrait,
    Landscape,
}

impl PageFilter {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "all" => PageFilter::All,
            "odd" => PageFilter::Odd,
            "even" => PageFilter::Even,
            "portrait" => PageFilter::Portrait,
            "landscape" => PageFilter::Landscape,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyStamp {
    pub source: StampSource,
    /// 0-based pages (empty = every page), then `filter`.
    pub pages: Vec<usize>,
    pub filter: PageFilter,
    pub anchor: Anchor,
    /// Points from the anchor: X towards the page's middle from a left / right anchor (right
    /// from a centred one), Y likewise from a top / bottom anchor (up from a middle one).
    pub offset: (f64, f64),
    /// 0.1 to 10 times the stamp's own size.
    pub scale: f64,
    /// Degrees counter-clockwise.
    pub rotation: f64,
    /// 0.05 to 1.
    pub opacity: f64,
    pub multiply: bool,
    pub lock: bool,
    /// Fill `{prompt:...}` fields.
    pub answers: BTreeMap<String, String>,
    /// Fixed time for the date fields.
    pub when: Option<i64>,
}

impl ApplyStamp {
    pub fn new(source: StampSource) -> Self {
        Self {
            source,
            pages: Vec::new(),
            filter: PageFilter::All,
            anchor: Anchor::Center,
            offset: (0.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            multiply: false,
            lock: false,
            answers: BTreeMap::new(),
            when: None,
        }
    }
}

impl Session {
    /// The pages a page list and filter pick (0-based).
    pub fn filter_pages(&self, pages: &[usize], filter: PageFilter) -> Result<Vec<usize>> {
        for p in pages {
            self.page(*p)?;
        }
        let list: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        Ok(list
            .into_iter()
            .filter(|p| {
                let Ok(info) = self.page(*p) else { return false };
                let r = info.crop.normalized();
                let (mut w, mut h) = (r.width(), r.height());
                if info.rotate.rem_euclid(180) == 90 {
                    std::mem::swap(&mut w, &mut h);
                }
                match filter {
                    PageFilter::All => true,
                    PageFilter::Odd => p % 2 == 0,
                    PageFilter::Even => p % 2 == 1,
                    PageFilter::Portrait => h >= w,
                    PageFilter::Landscape => w > h,
                }
            })
            .collect())
    }

    /// Place the stamp on every chosen page. Returns the new markups' ids. One undo step.
    pub fn apply_stamp(&mut self, a: &ApplyStamp, library: Option<&StampLibrary>) -> Result<Vec<String>> {
        if !(a.scale.is_finite() && (0.1..=10.0).contains(&a.scale)) {
            return Err(invalid("scale is 0.1 to 10"));
        }
        if !(a.rotation.is_finite() && a.offset.0.is_finite() && a.offset.1.is_finite()) {
            return Err(invalid("rotation and offsets are numbers"));
        }
        if a.offset.0.abs() > 14_400.0 || a.offset.1.abs() > 14_400.0 {
            return Err(invalid("offsets are at most 14400 points"));
        }
        if !(a.opacity.is_finite() && (0.05..=1.0).contains(&a.opacity)) {
            return Err(invalid("opacity is 0.05 to 1"));
        }
        let pages = self.filter_pages(&a.pages, a.filter)?;
        if pages.is_empty() {
            return Err(invalid("no page matches the page filter"));
        }
        self.set_merge_key(Some("apply-stamp"));
        let r = (|| -> Result<Vec<String>> {
            let mut ids = Vec::new();
            for p in pages {
                let crop = self.page(p)?.crop.normalized();
                let mid = Point::new((crop.x0 + crop.x1) / 2.0, (crop.y0 + crop.y1) / 2.0);
                let id = self.place_stamp(p, StampPlace::Center(mid), &a.source, library, &a.answers, a.when)?;
                let b = bbox(&self.markup(&id)?.pts).ok_or_else(|| invalid("the stamp has no size"))?;
                let (w, h) = (b.width() * a.scale, b.height() * a.scale);
                // Scaled and turned about the page middle first; the turned box is then
                // anchored.
                self.resize_markup(
                    &id,
                    Rect::new(mid.x - w / 2.0, mid.y - h / 2.0, mid.x + w / 2.0, mid.y + h / 2.0),
                )?;
                let turned = a.rotation.rem_euclid(360.0);
                if turned.abs() > 1e-9 && (360.0 - turned).abs() > 1e-9 {
                    self.rotate_markups(std::slice::from_ref(&id), a.rotation, None)?;
                }
                let (s, c) = a.rotation.to_radians().sin_cos();
                let (tw, th) = (w * c.abs() + h * s.abs(), w * s.abs() + h * c.abs());
                let (gx, gy) = a.anchor.grid();
                let cx = match gx {
                    -1 => crop.x0 + a.offset.0 + tw / 2.0,
                    1 => crop.x1 - a.offset.0 - tw / 2.0,
                    _ => mid.x + a.offset.0,
                };
                let cy = match gy {
                    -1 => crop.y0 + a.offset.1 + th / 2.0,
                    1 => crop.y1 - a.offset.1 - th / 2.0,
                    _ => mid.y + a.offset.1,
                };
                if (cx - mid.x).abs() > 1e-9 || (cy - mid.y).abs() > 1e-9 {
                    self.move_markups(std::slice::from_ref(&id), cx - mid.x, cy - mid.y)?;
                }
                if a.opacity < 1.0 || a.multiply || a.lock {
                    let patch = MarkupPatch {
                        opacity: Some(a.opacity),
                        multiply: Some(a.multiply),
                        locked: Some(a.lock),
                        ..Default::default()
                    };
                    self.set_properties(std::slice::from_ref(&id), &patch)?;
                }
                ids.push(id);
            }
            Ok(ids)
        })();
        self.set_merge_key(None);
        self.seal();
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse() {
        assert_eq!(Anchor::from_name("Top Right"), Some(Anchor::TopRight));
        assert_eq!(Anchor::from_name("bottom_left"), Some(Anchor::BottomLeft));
        assert_eq!(PageFilter::from_name("Landscape"), Some(PageFilter::Landscape));
        assert_eq!(Anchor::from_name("nowhere"), None);
    }
}
