//! Spaces (Revu's Spaces panel): named polygon regions per page. Markups inside a space show
//! its name in the Markups List Space column (`markupcraft_model::spaces`); counts can be split
//! by space; spaces can be exported to a JSON file and imported into another revision.
//!
//! Spaces are stored on their page (`/PCSpaces`, see `markupcraft_revu::spaces`), so they move
//! with their page through page operations.

use std::collections::BTreeMap;
use std::path::Path;

use markupcraft_geom::{Point, polygon_area};
pub use markupcraft_model::spaces::Space;
use markupcraft_model::spaces::{space_of, space_path, spaces_at};
use markupcraft_model::{Color, Kind};
use serde::{Deserialize, Serialize};

use crate::{EngineError, Result, Session, invalid};

/// Longest space name.
pub const MAX_NAME: usize = 256;
/// Most spaces on one page.
pub const MAX_PER_PAGE: usize = 10_000;
/// Largest spaces file read.
const MAX_FILE: u64 = 64 << 20;

/// How many markups (and counted items) of each subject lie in a space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpaceTally {
    pub page: usize,
    /// the space's path ("" = outside every space)
    pub space: String,
    pub markups: usize,
    /// subject -> counted items (each point of a Count measurement counts once, in its own space)
    pub counts: BTreeMap<String, usize>,
}

/// Spaces of one page in a spaces file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacesPage {
    /// 1-based
    pub page: usize,
    #[serde(default)]
    pub label: String,
    pub spaces: Vec<SpaceRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceRecord {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacesFile {
    pub format: String,
    pub version: u32,
    pub pages: Vec<SpacesPage>,
}

pub const FORMAT: &str = "markupcraft-spaces";

/// How imported spaces find their page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceMatch {
    /// the page with the same label (else the same number)
    Label,
    /// the page with the same number
    Number,
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) {
        return Err(invalid(format!(
            "a space name has 1 to {MAX_NAME} characters and no control characters"
        )));
    }
    Ok(n.to_string())
}

fn check_outline(pts: &[Point]) -> Result<()> {
    if pts.len() < 3 || pts.len() > markupcraft_revu::spaces::MAX_VERTICES {
        return Err(invalid("a space needs 3 or more points"));
    }
    if pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(invalid("space points must be finite numbers"));
    }
    if polygon_area(pts).abs() < 1e-6 {
        return Err(invalid("the space outline encloses no area"));
    }
    Ok(())
}

/// Changes to a space; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpacePatch {
    pub name: Option<String>,
    pub pts: Option<Vec<Point>>,
    pub color: Option<Color>,
    pub opacity: Option<f64>,
}

impl Session {
    /// Spaces on `page` (0-based), or on every page: (page, space).
    pub fn spaces(&self, page: Option<usize>) -> Vec<(usize, Space)> {
        self.doc
            .pages
            .iter()
            .enumerate()
            .filter(|(i, _)| page.is_none_or(|p| p == *i))
            .flat_map(|(i, p)| p.spaces.iter().map(move |s| (i, s.clone())))
            .collect()
    }

    fn find_space(&self, id: &str) -> Result<(usize, usize)> {
        for (pi, p) in self.doc.pages.iter().enumerate() {
            if let Some(k) = p.spaces.iter().position(|s| s.id == id) {
                return Ok((pi, k));
            }
        }
        Err(invalid(format!("no space with id {id:?} (space_list shows them)")))
    }

    /// Replace page `page`'s spaces in the model and the file (inside an edit).
    fn store_spaces(&mut self, page: usize, spaces: Vec<Space>) -> Result<()> {
        let pages = markupcraft_revu::pdf::pages(&self.file.cos);
        let pref = pages.get(page).map(|p| p.id).ok_or(EngineError::NoPage {
            page: page + 1,
            count: pages.len(),
        })?;
        markupcraft_revu::spaces::write_page(&mut self.file.cos, pref, &spaces);
        if let Some(info) = self.doc.pages.get_mut(page) {
            info.spaces = spaces;
        }
        Ok(())
    }

    /// Add a space; returns its id. Undoable.
    pub fn add_space(
        &mut self,
        page: usize,
        name: &str,
        pts: Vec<Point>,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<String> {
        let info = self.page(page)?;
        if info.spaces.len() >= MAX_PER_PAGE {
            return Err(invalid(format!("a page holds at most {MAX_PER_PAGE} spaces")));
        }
        let name = check_name(name)?;
        check_outline(&pts)?;
        let mut sp = Space {
            id: markupcraft_revu::new_markup_id(),
            name,
            pts,
            ..Default::default()
        };
        if let Some(c) = color {
            sp.color = c;
        }
        if let Some(o) = opacity {
            sp.opacity = o.clamp(0.0, 1.0);
        }
        let id = sp.id.clone();
        let mut list = info.spaces.clone();
        list.push(sp);
        self.edit("Add Space", |s| {
            s.store_spaces(page, list)?;
            Ok(((), true))
        })?;
        Ok(id)
    }

    /// Change a space. Undoable.
    pub fn edit_space(&mut self, id: &str, patch: &SpacePatch) -> Result<()> {
        let (page, k) = self.find_space(id)?;
        let mut list = self.page(page)?.spaces.clone();
        let sp = list.get_mut(k).ok_or_else(|| invalid("space vanished"))?;
        if let Some(n) = &patch.name {
            sp.name = check_name(n)?;
        }
        if let Some(p) = &patch.pts {
            check_outline(p)?;
            sp.pts = p.clone();
        }
        if let Some(c) = patch.color {
            sp.color = c;
        }
        if let Some(o) = patch.opacity {
            if !o.is_finite() {
                return Err(invalid("opacity must be a number from 0 to 1"));
            }
            sp.opacity = o.clamp(0.0, 1.0);
        }
        self.edit("Edit Space", |s| {
            s.store_spaces(page, list)?;
            Ok(((), true))
        })
    }

    /// Delete spaces by id; returns how many went. Undoable.
    pub fn delete_spaces(&mut self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Err(invalid("no space ids given (space_list shows them)"));
        }
        for id in ids {
            self.find_space(id)?;
        }
        let ids = ids.to_vec();
        self.edit("Delete Spaces", |s| {
            let mut n = 0;
            for page in 0..s.doc.pages.len() {
                let before = s.doc.pages.get(page).map(|p| p.spaces.clone()).unwrap_or_default();
                let after: Vec<Space> = before.iter().filter(|sp| !ids.contains(&sp.id)).cloned().collect();
                if after.len() != before.len() {
                    n += before.len() - after.len();
                    s.store_spaces(page, after)?;
                }
            }
            Ok((n, true))
        })
    }

    /// The Space column of a markup ("" = in no space).
    pub fn markup_space(&self, id: &str) -> Result<String> {
        Ok(space_path(&self.doc, self.markup(id)?))
    }

    /// Markups whose innermost space is `id`.
    pub fn markups_in_space(&self, id: &str) -> Result<Vec<String>> {
        self.find_space(id)?;
        Ok(self
            .doc
            .markups
            .iter()
            .filter(|m| space_of(&self.doc, m).is_some_and(|s| s.id == id))
            .map(|m| m.id.clone())
            .collect())
    }

    /// Markups and counted items per space (Split counts by space), on one page or all.
    pub fn space_tallies(&self, page: Option<usize>) -> Vec<SpaceTally> {
        let mut map: BTreeMap<(usize, String), SpaceTally> = BTreeMap::new();
        for m in self.doc.markups.iter().filter(|m| page.is_none_or(|p| p == m.page)) {
            let path = space_path(&self.doc, m);
            let t = map.entry((m.page, path.clone())).or_insert_with(|| SpaceTally {
                page: m.page,
                space: path,
                ..Default::default()
            });
            t.markups += 1;
            if m.kind == Kind::Count {
                let subject = if m.subject.is_empty() {
                    "Count".to_string()
                } else {
                    m.subject.clone()
                };
                if m.pts.is_empty() {
                    *t.counts.entry(subject).or_default() += 1;
                    continue;
                }
                for p in &m.pts {
                    let sp = spaces_at(&self.doc, m.page, *p)
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" > ");
                    let tt = map.entry((m.page, sp.clone())).or_insert_with(|| SpaceTally {
                        page: m.page,
                        space: sp,
                        ..Default::default()
                    });
                    *tt.counts.entry(subject.clone()).or_default() += 1;
                }
            }
        }
        map.into_values().collect()
    }

    /// Write every space to a JSON spaces file (atomic). Returns how many.
    pub fn export_spaces(&self, path: &Path) -> Result<usize> {
        let mut n = 0;
        let pages: Vec<SpacesPage> = self
            .doc
            .pages
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.spaces.is_empty())
            .map(|(i, p)| {
                n += p.spaces.len();
                SpacesPage {
                    page: i + 1,
                    label: p.label.clone(),
                    spaces: p
                        .spaces
                        .iter()
                        .map(|s| SpaceRecord {
                            name: s.name.clone(),
                            points: s.pts.iter().map(|p| [p.x, p.y]).collect(),
                            color: Some(s.color.hex()),
                            opacity: Some(s.opacity),
                        })
                        .collect(),
                }
            })
            .collect();
        let file = SpacesFile {
            format: FORMAT.into(),
            version: 1,
            pages,
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| invalid(e.to_string()))?;
        crate::write_atomic(path, text.as_bytes())?;
        Ok(n)
    }

    /// Add the spaces of a spaces file (pages matched by label or number; spaces whose name a
    /// page already has are replaced). Returns how many were imported. Undoable.
    pub fn import_spaces(&mut self, path: &Path, by: SpaceMatch) -> Result<usize> {
        let io = |e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        };
        let len = std::fs::metadata(path).map_err(io)?.len();
        if len > MAX_FILE {
            return Err(invalid("the spaces file is too large"));
        }
        let text = markupcraft_revu::fsio::read_to_string(path).map_err(io)?;
        let file: SpacesFile = serde_json::from_str(&text).map_err(|e| invalid(format!("not a spaces file: {e}")))?;
        if file.format != FORMAT {
            return Err(invalid(format!("not a spaces file (format {:?})", file.format)));
        }
        let mut plan: BTreeMap<usize, Vec<Space>> = BTreeMap::new();
        let mut n = 0;
        for fp in &file.pages {
            let target = match by {
                SpaceMatch::Label if !fp.label.is_empty() => self.doc.pages.iter().position(|p| p.label == fp.label),
                _ => fp.page.checked_sub(1).filter(|p| *p < self.doc.pages.len()),
            };
            let Some(target) = target else { continue };
            let list = plan
                .entry(target)
                .or_insert_with(|| self.doc.pages.get(target).map(|p| p.spaces.clone()).unwrap_or_default());
            for r in &fp.spaces {
                let name = check_name(&r.name)?;
                let pts: Vec<Point> = r.points.iter().map(|p| Point::new(p[0], p[1])).collect();
                check_outline(&pts)?;
                let mut sp = Space {
                    id: markupcraft_revu::new_markup_id(),
                    name: name.clone(),
                    pts,
                    ..Default::default()
                };
                if let Some(c) = r.color.as_deref().and_then(crate::props::parse_color) {
                    sp.color = c;
                }
                if let Some(o) = r.opacity.filter(|o| o.is_finite()) {
                    sp.opacity = o.clamp(0.0, 1.0);
                }
                list.retain(|s| s.name != name);
                if list.len() >= MAX_PER_PAGE {
                    return Err(invalid(format!("a page holds at most {MAX_PER_PAGE} spaces")));
                }
                list.push(sp);
                n += 1;
            }
        }
        if n == 0 {
            return Ok(0);
        }
        self.edit("Import Spaces", |s| {
            for (page, list) in plan {
                s.store_spaces(page, list)?;
            }
            Ok(((), true))
        })?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{Markup, Rect};

    use super::*;

    #[test]
    fn spaces_name_markups_split_counts_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("mc-spaces-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::new_blank("spaces.pdf", &[(612.0, 792.0), (612.0, 792.0)]).unwrap();
        let office = s
            .add_space(
                0,
                "Office",
                Rect::new(0.0, 0.0, 200.0, 200.0).corners().to_vec(),
                None,
                None,
            )
            .unwrap();
        s.add_space(
            0,
            "Lobby",
            Rect::new(200.0, 0.0, 400.0, 200.0).corners().to_vec(),
            None,
            None,
        )
        .unwrap();
        assert!(
            s.add_space(0, "", Rect::new(0.0, 0.0, 1.0, 1.0).corners().to_vec(), None, None)
                .is_err()
        );
        let r = s
            .add_markup(Markup::new(
                Kind::Rectangle,
                0,
                Rect::new(10.0, 10.0, 50.0, 50.0).corners().to_vec(),
            ))
            .unwrap();
        let mut c = Markup::new(
            Kind::Count,
            0,
            vec![
                Point::new(150.0, 20.0),
                Point::new(250.0, 20.0),
                Point::new(260.0, 30.0),
            ],
        );
        c.subject = "Outlet".into();
        s.add_markup(c).unwrap();
        assert_eq!(s.markup_space(&r).unwrap(), "Office");
        assert_eq!(s.markups_in_space(&office).unwrap(), vec![r.clone()]);
        let t = s.space_tallies(Some(0));
        let lobby = t.iter().find(|t| t.space == "Lobby").unwrap();
        assert_eq!(lobby.counts.get("Outlet"), Some(&2));
        let office_t = t.iter().find(|t| t.space == "Office").unwrap();
        assert_eq!(office_t.counts.get("Outlet"), Some(&1));
        // the table's Space column
        let table = markupcraft_model::MarkupTable::new(s.doc());
        let col = table.column_index("space").unwrap();
        let row = s.doc().markups.iter().position(|m| m.id == r).unwrap();
        assert_eq!(table.cell(row, col).text, "Office");

        let path = dir.join("a.pdf");
        s.save_as(&path, true).unwrap();
        let mut s2 = Session::open(&path).unwrap();
        assert_eq!(s2.spaces(Some(0)).len(), 2);
        let json = dir.join("spaces.json");
        assert_eq!(s2.export_spaces(&json).unwrap(), 2);
        s2.edit_space(
            &office,
            &SpacePatch {
                name: Some("Office 101".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s2.markup_space(&r).unwrap(), "Office 101");
        assert_eq!(s2.delete_spaces(&[office]).unwrap(), 1);
        assert_eq!(s2.markup_space(&r).unwrap(), "");
        s2.undo().unwrap();
        assert_eq!(s2.markup_space(&r).unwrap(), "Office 101");

        let mut s3 = Session::new_blank("other.pdf", &[(612.0, 792.0)]).unwrap();
        assert_eq!(s3.import_spaces(&json, SpaceMatch::Number).unwrap(), 2);
        assert_eq!(s3.spaces(None).len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
