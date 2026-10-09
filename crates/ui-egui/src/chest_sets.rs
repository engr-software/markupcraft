//! Tool Chest management beyond the basics in `chest.rs`: reordering sets and items, the
//! Symbol / Detail view and its icon size, the tool set scale (Drawing-mode items placed on a
//! page at another scale are resized so their real size stays the same), and import / export
//! of one tool set as a file:
//!
//! ```text
//! { "format": "markupcraft-toolset", "version": 1,
//!   "set": { "id": "...", "title": "HVAC", "items": [ ... ], "scale": { ... } } }
//! ```
//!
//! An imported file is untrusted: size-capped, and its items are sanitised like the chest's.

use markupcraft_geom::{Rect, bbox};
use markupcraft_model::{Markup, Scale};
use serde::{Deserialize, Serialize};

use crate::chest::{MY_TOOLS, ToolChest, ToolSet};

/// Largest tool set file read.
pub const MAX_SET_FILE: u64 = 16 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChestView {
    /// rows: icon, name, hint
    #[default]
    Detail,
    /// tiles: icons only, sized by the slider
    Symbol,
}

pub fn default_icon_size() -> f32 {
    32.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetFile {
    format: String,
    version: u32,
    set: ToolSet,
}

impl ToolChest {
    /// Move tool set `id` up (`-1`) or down (`+1`); My Tools stays first.
    pub fn move_set(&mut self, id: &str, delta: i32) {
        let Some(i) = self.sets.iter().position(|s| s.id == id) else {
            return;
        };
        let j = i as i64 + i64::from(delta);
        if id == MY_TOOLS || j < 1 || j >= self.sets.len() as i64 {
            return;
        }
        self.sets.swap(i, j as usize);
        self.save();
    }

    /// Move an item up or down within its set.
    pub fn move_item(&mut self, set: &str, item: &str, delta: i32) {
        let Some(s) = self.sets.iter_mut().find(|s| s.id == set) else {
            return;
        };
        let Some(i) = s.items.iter().position(|it| it.id == item) else {
            return;
        };
        let j = i as i64 + i64::from(delta);
        if j < 0 || j >= s.items.len() as i64 {
            return;
        }
        s.items.swap(i, j as usize);
        self.save();
    }

    /// Set (or clear) a tool set's scale.
    pub fn set_scale(&mut self, set: &str, scale: Option<Scale>) {
        if let Some(s) = self.sets.iter_mut().find(|s| s.id == set) {
            s.scale = scale.filter(Scale::valid);
        }
        self.save();
    }

    pub fn set_view(&mut self, view: ChestView, icon_size: f32) {
        self.view = view;
        self.icon_size = icon_size.clamp(16.0, 96.0);
        self.save();
    }

    /// The set `id` as a file's text.
    pub fn export_set(&self, id: &str) -> Result<String, String> {
        let set = self.find_set(id).ok_or("no such tool set")?.clone();
        serde_json::to_string_pretty(&SetFile {
            format: "markupcraft-toolset".into(),
            version: 1,
            set,
        })
        .map_err(|e| e.to_string())
    }

    /// Add the set in `text` (a new id; My Tools is imported as a copy). Returns its id.
    pub fn import_set(&mut self, text: &str) -> Result<String, String> {
        if text.len() as u64 > MAX_SET_FILE {
            return Err("the file is too large to be a tool set".into());
        }
        let f: SetFile = serde_json::from_str(text).map_err(|e| format!("not a MarkupCraft tool set: {e}"))?;
        if f.format != "markupcraft-toolset" {
            return Err("not a MarkupCraft tool set".into());
        }
        let mut set = f.set;
        set.id = self.new_id("s");
        if set.title.trim().is_empty() || set.title.chars().count() > 200 {
            set.title = "Imported Tools".into();
        }
        for it in &mut set.items {
            it.id = self.new_id("t");
        }
        set.scale = set.scale.filter(Scale::valid);
        let id = set.id.clone();
        self.sets.push(set);
        self.sanitize();
        self.save();
        Ok(id)
    }

    /// The scale of the set holding item (`set`, `item`), if any.
    pub fn item_set_scale(&self, set: &str) -> Option<&Scale> {
        self.find_set(set).and_then(|s| s.scale.as_ref())
    }
}

/// Resize a placed Drawing-mode copy drawn at `set_scale` for a page at `page_scale`, about its
/// centre, so it keeps its real size. Unchanged when either scale is unusable or their units
/// differ.
pub fn rescale_copy(m: &mut Markup, set_scale: &Scale, page_scale: &Scale) {
    // Metres per point of each scale (their /X units may be written differently).
    let metres = |s: &Scale| {
        let f = s.x.first()?;
        let u = markupcraft_measure::units::LengthUnit::from_label(&f.unit)?;
        Some(f.conv * u.meters())
    };
    let (Some(a), Some(b)) = (metres(set_scale), metres(page_scale)) else {
        return;
    };
    if !(set_scale.valid() && page_scale.valid()) {
        return;
    }
    let k = a / b;
    if !(k.is_finite() && k > 0.0) || (k - 1.0).abs() < 1e-9 {
        return;
    }
    let Some(b) = bbox(&m.pts) else { return };
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    let (hw, hh) = (b.width() * k / 2.0, b.height() * k / 2.0);
    let _ = markupcraft_engine::geometry::resize(m, Rect::new(cx - hw, cy - hh, cx + hw, cy + hh));
    if m.kind == markupcraft_model::Kind::Count || !m.symbol_paths.is_empty() {
        m.symbol_scale = (m.symbol_scale * k).clamp(0.05, 50.0);
    }
    if m.kind.is_text() || m.kind == markupcraft_model::Kind::Stamp {
        m.text.size = (m.text.size * k).clamp(1.0, 1000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::{Kind, Point};

    #[test]
    fn reorder_export_import_and_rescale() {
        let mut c = ToolChest::default();
        let a = c.add_set("A");
        let b = c.add_set("B");
        c.move_set(&b, -1);
        assert_eq!(c.sets[1].id, b);
        c.move_set(&b, -1);
        assert_eq!(c.sets[1].id, b, "My Tools stays first");
        let m = Markup::new(Kind::Rectangle, 0, Rect::new(0.0, 0.0, 10.0, 10.0).corners().to_vec());
        let i1 = c.add_markup(&a, &m).unwrap();
        let i2 = c.add_markup(&a, &m).unwrap();
        c.move_item(&a, &i2, -1);
        assert_eq!(c.find_set(&a).unwrap().items[0].id, i2);
        c.set_scale(&a, Some(Scale::architectural(0.25, 1.0)));
        let text = c.export_set(&a).unwrap();
        let n = c.sets.len();
        let id = c.import_set(&text).unwrap();
        assert_eq!(c.sets.len(), n + 1);
        let s = c.find_set(&id).unwrap();
        assert_eq!(s.items.len(), 2);
        assert!(s.items.iter().all(|i| i.id != i1 && i.id != i2), "fresh ids");
        assert!(s.scale.is_some());
        assert!(c.import_set("{}").is_err());
        // drawn at 1/4" = 1', placed on 1/8" = 1': half the paper size
        let mut r = Markup::new(Kind::Rectangle, 0, Rect::new(0.0, 0.0, 20.0, 10.0).corners().to_vec());
        rescale_copy(
            &mut r,
            &Scale::architectural(0.25, 1.0),
            &Scale::architectural(0.125, 1.0),
        );
        let b = bbox(&r.pts).unwrap();
        assert!(
            (b.width() - 10.0).abs() < 1e-9 && (b.height() - 5.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(
            Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0),
            Point::new(10.0, 5.0)
        );
    }
}
