//! The Tool Chest model: tool sets of saved markup templates (My Tools plus the user's sets),
//! Recent Tools (this session only), and Set as Default templates per tool. Persisted as one
//! JSON file in the user's configuration folder:
//!
//! ```text
//! { "format": "markupcraft-toolchest", "version": 1,
//!   "sets": [ { "id": "mytools", "title": "My Tools", "collapsed": false,
//!               "items": [ { "id": "...", "name": "Duct Length", "tool": "polylength",
//!                            "mode": "Properties", "markup": { ...Markup... } } ] } ],
//!   "defaults": { "rectangle": { ...Markup... } } }
//! ```
//!
//! A tool item is a whole markup (look, subject, label, columns, geometry) plus the tool that
//! draws it. Properties mode: the tool draws new geometry with the item's look. Drawing mode: a
//! click places a copy of the item's markup. The file is untrusted input: size-capped, and
//! anything unreadable is reported and left alone (never overwritten until the user saves).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use markupcraft_geom::bbox;
use markupcraft_model::{Kind, Markup, Point};
use serde::{Deserialize, Serialize};

pub const MY_TOOLS: &str = "mytools";
/// Recent Tools keeps this many.
pub const RECENT_MAX: usize = 12;
/// Largest tool chest file read.
const MAX_FILE: u64 = 32 << 20;
/// Most points a saved template keeps.
const MAX_TEMPLATE_POINTS: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// the tool draws new geometry with the saved look
    #[default]
    Properties,
    /// a click places a copy of the saved markup
    Drawing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolItem {
    pub id: String,
    pub name: String,
    /// `tools::TOOLS` id
    pub tool: String,
    #[serde(default)]
    pub mode: Mode,
    pub markup: Markup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSet {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub items: Vec<ToolItem>,
    /// Tool set scale: the scale its Drawing-mode items were drawn at (they are resized to the
    /// page's scale when placed)
    #[serde(default)]
    pub scale: Option<markupcraft_model::Scale>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileFormat {
    format: String,
    version: u32,
    sets: Vec<ToolSet>,
    #[serde(default)]
    defaults: BTreeMap<String, Markup>,
    #[serde(default)]
    view: crate::chest_sets::ChestView,
    #[serde(default = "crate::chest_sets::default_icon_size")]
    icon_size: f32,
}

/// The user's Tool Chest.
#[derive(Debug, Clone)]
pub struct ToolChest {
    /// My Tools first, then the user's sets.
    pub sets: Vec<ToolSet>,
    /// Recent Tools, newest first (not saved).
    pub recent: Vec<ToolItem>,
    /// Set as Default templates, by tool id.
    pub defaults: BTreeMap<String, Markup>,
    /// Where it is saved; `None` = memory only.
    pub path: Option<PathBuf>,
    /// The last load or save problem.
    pub error: Option<String>,
    /// Detail (rows) or Symbol (tiles) view
    pub view: crate::chest_sets::ChestView,
    /// Symbol view tile size, points
    pub icon_size: f32,
    counter: u64,
}

impl Default for ToolChest {
    fn default() -> Self {
        Self {
            sets: vec![ToolSet {
                id: MY_TOOLS.into(),
                title: "My Tools".into(),
                collapsed: false,
                items: Vec::new(),
                scale: None,
            }],
            recent: Vec::new(),
            defaults: BTreeMap::new(),
            path: None,
            error: None,
            view: Default::default(),
            icon_size: crate::chest_sets::default_icon_size(),
            counter: 0,
        }
    }
}

/// The folder MarkupCraft keeps its settings in: `MARKUPCRAFT_CONFIG_DIR`, else the platform's
/// configuration folder.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("MARKUPCRAFT_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("MarkupCraft"))
}

/// A markup as a template: no file identity, review state, group or replies; geometry capped.
pub fn template_of(m: &Markup) -> Markup {
    let mut t = m.clone();
    t.id.clear();
    t.obj = (0, 0);
    t.annot_index = None;
    t.created.clear();
    t.modified.clear();
    t.subtype.clear();
    t.intent.clear();
    t.set_locked(false);
    t.status.clear();
    t.checked = false;
    t.replies.clear();
    t.state_replies.clear();
    t.irt = None;
    t.group.clear();
    t.stored_look = false;
    t.foreign_look = false;
    t.extra.clear();
    t.snapshot = None;
    t.pts.truncate(MAX_TEMPLATE_POINTS);
    t.dirty = true;
    t
}

impl ToolChest {
    /// Read `path` (a missing file is an empty chest that saves there).
    pub fn load(path: &Path) -> Self {
        let mut c = ToolChest {
            path: Some(path.to_path_buf()),
            ..Default::default()
        };
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return c,
        };
        if meta.len() > MAX_FILE {
            c.error = Some(format!("{} is too large to be a tool chest", path.display()));
            c.path = None;
            return c;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                c.error = Some(format!("{}: {e}", path.display()));
                c.path = None;
                return c;
            }
        };
        match serde_json::from_str::<FileFormat>(&text) {
            Ok(f) if f.format == "markupcraft-toolchest" => {
                c.sets = f.sets;
                c.defaults = f.defaults;
                c.view = f.view;
                c.icon_size = f.icon_size.clamp(16.0, 96.0);
                c.sanitize();
            }
            Ok(_) | Err(_) => {
                // Keep the user's file: work in memory until they change something.
                c.error = Some(format!(
                    "{} is not a MarkupCraft tool chest; it was left alone",
                    path.display()
                ));
                c.path = None;
            }
        }
        c
    }

    /// Keep what a hand-edited file may break within bounds: My Tools exists and comes first,
    /// ids are unique, templates point at tools that make their kind.
    pub(crate) fn sanitize(&mut self) {
        if !self.sets.iter().any(|s| s.id == MY_TOOLS) {
            self.sets.insert(0, ToolChest::default().sets.remove(0));
        }
        if let Some(i) = self.sets.iter().position(|s| s.id == MY_TOOLS) {
            let my = self.sets.remove(i);
            self.sets.insert(0, my);
        }
        let mut seen = std::collections::HashSet::new();
        self.sets.retain(|s| seen.insert(s.id.clone()));
        for s in &mut self.sets {
            s.items
                .retain(|it| crate::tools::find(&it.tool).is_some_and(|t| t.creates() == Some(it.markup.kind)));
            for it in &mut s.items {
                it.markup = template_of(&it.markup);
            }
        }
        self.defaults
            .retain(|tool, m| crate::tools::find(tool).is_some_and(|t| t.creates() == Some(m.kind)));
    }

    /// Write the chest (atomically) when it has a path.
    pub fn save(&mut self) {
        let Some(path) = self.path.clone() else { return };
        let f = FileFormat {
            format: "markupcraft-toolchest".into(),
            version: 1,
            sets: self.sets.clone(),
            defaults: self.defaults.clone(),
            view: self.view,
            icon_size: self.icon_size,
        };
        let r = serde_json::to_string_pretty(&f)
            .map_err(|e| e.to_string())
            .and_then(|text| write_atomic(&path, text.as_bytes()));
        self.error = r.err();
    }

    pub(crate) fn new_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        // The clock keeps ids unique across sessions (the browser build has no system clock).
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        #[cfg(target_arch = "wasm32")]
        let now = self.sets.iter().map(|s| s.items.len() as u128).sum::<u128>() + self.sets.len() as u128;
        format!("{prefix}{now:x}{:x}", self.counter)
    }

    pub fn find_set(&self, id: &str) -> Option<&ToolSet> {
        self.sets.iter().find(|s| s.id == id)
    }

    /// An item by set and item id ("recent" is Recent Tools).
    pub fn item(&self, set: &str, item: &str) -> Option<&ToolItem> {
        if set == "recent" {
            return self.recent.iter().find(|i| i.id == item);
        }
        self.find_set(set)?.items.iter().find(|i| i.id == item)
    }

    /// Save `m`'s look as a tool in set `set` (My Tools when unknown). Returns the item id.
    pub fn add_markup(&mut self, set: &str, m: &Markup) -> Option<String> {
        let tool = crate::tools::tool_for_kind(m.kind)?;
        let id = self.new_id("t");
        let name = if m.subject.is_empty() {
            tool.label.to_string()
        } else {
            m.subject.clone()
        };
        let item = ToolItem {
            id: id.clone(),
            name,
            tool: tool.id.into(),
            mode: Mode::Properties,
            markup: template_of(m),
        };
        let target = if self.sets.iter().any(|s| s.id == set) {
            set
        } else {
            MY_TOOLS
        };
        let s = self.sets.iter_mut().find(|s| s.id == target)?;
        s.items.push(item);
        self.save();
        Some(id)
    }

    /// Recent Tools: put a tool and look first (the same tool, subject and colour replace the
    /// older entry).
    pub fn add_recent(&mut self, tool: &str, m: &Markup) {
        let same = |i: &ToolItem| i.tool == tool && i.markup.subject == m.subject && i.markup.color == m.color;
        self.recent.retain(|i| !same(i));
        let name = crate::tools::find(tool).map_or(tool, |t| t.label).to_string();
        let id = self.new_id("r");
        self.recent.insert(
            0,
            ToolItem {
                id,
                name,
                tool: tool.into(),
                mode: Mode::Properties,
                markup: template_of(m),
            },
        );
        self.recent.truncate(RECENT_MAX);
    }

    /// A new, empty tool set; returns its id.
    pub fn add_set(&mut self, title: &str) -> String {
        let id = self.new_id("s");
        self.sets.push(ToolSet {
            id: id.clone(),
            title: title.into(),
            collapsed: false,
            items: Vec::new(),
            scale: None,
        });
        self.save();
        id
    }

    /// Delete a tool set (not My Tools).
    pub fn remove_set(&mut self, id: &str) {
        if id == MY_TOOLS {
            return;
        }
        self.sets.retain(|s| s.id != id);
        self.save();
    }

    pub fn rename_set(&mut self, id: &str, title: &str) {
        if let Some(s) = self.sets.iter_mut().find(|s| s.id == id) {
            s.title = title.into();
        }
        self.save();
    }

    pub fn remove_item(&mut self, set: &str, item: &str) {
        if set == "recent" {
            self.recent.retain(|i| i.id != item);
            return;
        }
        if let Some(s) = self.sets.iter_mut().find(|s| s.id == set) {
            s.items.retain(|i| i.id != item);
        }
        self.save();
    }

    /// Change an item (rename, mode).
    pub fn update_item(&mut self, set: &str, item: &str, f: impl FnOnce(&mut ToolItem)) {
        let slot = if set == "recent" {
            self.recent.iter_mut().find(|i| i.id == item)
        } else {
            self.sets
                .iter_mut()
                .find(|s| s.id == set)
                .and_then(|s| s.items.iter_mut().find(|i| i.id == item))
        };
        if let Some(i) = slot {
            f(i);
        }
        if set != "recent" {
            self.save();
        }
    }

    /// Copy an item (e.g. from Recent Tools) into a set.
    pub fn copy_item_to(&mut self, from_set: &str, item: &str, to_set: &str) {
        let Some(mut it) = self.item(from_set, item).cloned() else {
            return;
        };
        it.id = self.new_id("t");
        if let Some(s) = self.sets.iter_mut().find(|s| s.id == to_set) {
            s.items.push(it);
        }
        self.save();
    }

    /// Set as Default: new markups from `m`'s tool take its look.
    pub fn set_default(&mut self, m: &Markup) -> Option<&'static str> {
        let tool = crate::tools::tool_for_kind(m.kind)?;
        self.defaults.insert(tool.id.into(), template_of(m));
        self.save();
        Some(tool.label)
    }
}

/// A Drawing-mode copy of `template` centred on `at`.
pub fn place_copy(template: &Markup, page: usize, at: Point) -> Markup {
    let mut m = template_of(template);
    m.page = page;
    let c = if m.kind == Kind::Count || crate::actions::uses_rect(m.kind) {
        crate::actions::center_of(&m)
    } else {
        bbox(&m.pts).map_or(at, |b| Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0))
    };
    let (dx, dy) = (at.x - c.x, at.y - c.y);
    markupcraft_engine::geometry::translate(&mut m, dx, dy);
    m
}

/// Write `bytes` to `path` through a temporary file and a rename (creating the folder).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".markupcraft-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use markupcraft_model::Color;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("markupcraft-chest-{}-{name}", std::process::id()))
            .join("toolchest.json")
    }

    #[test]
    fn tool_sets_persist_and_reload() {
        let path = temp("persist");
        let mut c = ToolChest::load(&path);
        assert!(c.error.is_none());
        let mut m =
            crate::tools::new_markup(Kind::Polylength, 0, &[Point::new(0.0, 0.0), Point::new(50.0, 0.0)]).unwrap();
        m.subject = "Duct Length".into();
        m.color = Color::rgb(0.0, 0.0, 1.0);
        m.id = "XYZ".into();
        m.obj = (12, 0);
        let id = c.add_markup(MY_TOOLS, &m).unwrap();
        let set = c.add_set("HVAC");
        c.copy_item_to(MY_TOOLS, &id, &set);
        c.update_item(&set, &c.sets[1].items[0].id.clone(), |i| i.mode = Mode::Drawing);
        let mut r =
            crate::tools::new_markup(Kind::Rectangle, 0, &[Point::new(0.0, 0.0), Point::new(9.0, 9.0)]).unwrap();
        r.color = Color::rgb(0.0, 0.5, 0.0);
        assert_eq!(c.set_default(&r), Some("Rectangle"));
        assert!(c.error.is_none(), "{:?}", c.error);

        let back = ToolChest::load(&path);
        assert!(back.error.is_none(), "{:?}", back.error);
        assert_eq!(back.sets.len(), 2);
        let it = &back.sets[0].items[0];
        assert_eq!(it.name, "Duct Length");
        assert_eq!(it.tool, "polylength");
        assert!(it.markup.id.is_empty() && it.markup.obj == (0, 0));
        assert_eq!(back.sets[1].title, "HVAC");
        assert_eq!(back.sets[1].items[0].mode, Mode::Drawing);
        assert_eq!(
            back.defaults.get("rectangle").map(|m| m.color),
            Some(Color::rgb(0.0, 0.5, 0.0))
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_foreign_file_is_left_alone() {
        let path = temp("foreign");
        write_atomic(&path, b"{\"hello\": 1}").unwrap();
        let mut c = ToolChest::load(&path);
        assert!(c.error.is_some());
        assert!(c.path.is_none());
        c.add_set("x");
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"hello\": 1}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recent_tools_dedupe_and_cap() {
        let mut c = ToolChest::default();
        let m = crate::tools::new_markup(Kind::Rectangle, 0, &[Point::new(0.0, 0.0), Point::new(9.0, 9.0)]).unwrap();
        for _ in 0..3 {
            c.add_recent("rectangle", &m);
        }
        assert_eq!(c.recent.len(), 1);
        for i in 0..20 {
            let mut x = m.clone();
            x.subject = format!("r{i}");
            c.add_recent("rectangle", &x);
        }
        assert_eq!(c.recent.len(), RECENT_MAX);
        assert_eq!(c.recent[0].markup.subject, "r19");
    }

    #[test]
    fn drawing_mode_places_a_copy() {
        let m = crate::tools::new_markup(Kind::Rectangle, 0, &[Point::new(0.0, 0.0), Point::new(10.0, 20.0)]).unwrap();
        let c = place_copy(&m, 3, Point::new(100.0, 100.0));
        assert_eq!(c.page, 3);
        assert_eq!(crate::actions::center_of(&c), Point::new(100.0, 100.0));
    }
}
