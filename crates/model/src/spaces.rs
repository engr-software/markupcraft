//! Spaces (Revu's Spaces): named regions of a page (rooms, zones). A markup inside a space
//! shows the space's name in the Markups List Space column, so takeoff can be grouped by room.
//!
//! A markup belongs to the smallest space that contains its anchor (the centre of its points'
//! box); nested spaces read "Outer > Inner" in the column.

use markupcraft_geom::{Point, bbox, point_in_polygon, polygon_area};
use serde::{Deserialize, Serialize};

use crate::{Color, Document, Markup};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    /// stable id (`/NM`)
    pub id: String,
    pub name: String,
    /// outline, PDF user space
    pub pts: Vec<Point>,
    pub color: Color,
    /// fill opacity the space is highlighted with
    pub opacity: f64,
}

impl Default for Space {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            pts: Vec::new(),
            color: Color::rgb(0.2, 0.5, 0.9),
            opacity: 0.25,
        }
    }
}

impl Space {
    pub fn area(&self) -> f64 {
        polygon_area(&self.pts).abs()
    }

    pub fn contains(&self, p: Point) -> bool {
        point_in_polygon(p, &self.pts)
    }
}

/// The point that decides which space a markup is in.
pub fn anchor(m: &Markup) -> Option<Point> {
    let b = bbox(&m.pts).or_else(|| (m.rect.width() > 0.0).then_some(m.rect))?;
    Some(Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0))
}

/// The spaces on `page` that contain `p`, largest first.
pub fn spaces_at(doc: &Document, page: usize, p: Point) -> Vec<&Space> {
    let Some(info) = doc.pages.get(page) else {
        return Vec::new();
    };
    let mut hits: Vec<&Space> = info.spaces.iter().filter(|s| s.contains(p)).collect();
    hits.sort_by(|a, b| b.area().total_cmp(&a.area()));
    hits
}

/// The Space column text of a markup: "" outside every space, "Outer > Inner" when nested.
pub fn space_path(doc: &Document, m: &Markup) -> String {
    let Some(p) = anchor(m) else { return String::new() };
    spaces_at(doc, m.page, p)
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(" > ")
}

/// The innermost space a markup is in.
pub fn space_of<'a>(doc: &'a Document, m: &Markup) -> Option<&'a Space> {
    let p = anchor(m)?;
    spaces_at(doc, m.page, p).last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, PageInfo, Rect};

    #[test]
    fn markups_take_the_innermost_space() {
        let mut doc = Document {
            pages: vec![PageInfo::default()],
            ..Default::default()
        };
        let room = |name: &str, r: Rect| Space {
            name: name.into(),
            pts: r.corners().to_vec(),
            ..Default::default()
        };
        if let Some(p) = doc.pages.get_mut(0) {
            p.spaces.push(room("Suite", Rect::new(0.0, 0.0, 300.0, 300.0)));
            p.spaces.push(room("Office", Rect::new(0.0, 0.0, 100.0, 100.0)));
        }
        let m = Markup::new(Kind::Rectangle, 0, Rect::new(10.0, 10.0, 20.0, 20.0).corners().to_vec());
        assert_eq!(space_path(&doc, &m), "Suite > Office");
        assert_eq!(space_of(&doc, &m).map(|s| s.name.as_str()), Some("Office"));
        let out = Markup::new(
            Kind::Rectangle,
            0,
            Rect::new(400.0, 400.0, 420.0, 420.0).corners().to_vec(),
        );
        assert_eq!(space_path(&doc, &out), "");
    }
}
