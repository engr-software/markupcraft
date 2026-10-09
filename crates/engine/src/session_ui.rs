//! What an interactive front end needs beyond the command set: one undo step for a whole
//! gesture (a slider drag, a typed word), Markups List cell edits and custom columns, and the
//! bytes of the current state for a page renderer.

use std::sync::Arc;

use markupcraft_model::{CustomColumn, MarkupTable, Point, measure_extras};
use markupcraft_revu::cos::{SaveOptions, write_full};

use crate::{EngineError, Result, Session, invalid};

/// Most custom columns a document may define.
pub const MAX_COLUMNS: usize = 256;

impl Session {
    /// Edits made while `key` is set join the previous undo step when that step was made under
    /// the same key (and nothing else happened since), until [`Session::seal`]. `None` makes
    /// every edit its own step again.
    pub fn set_merge_key(&mut self, key: Option<&str>) {
        self.merge = key.map(str::to_string);
    }

    /// End the current gesture: the next edit starts a new undo step.
    pub fn seal(&mut self) {
        self.last_merge = None;
    }

    /// Change one Markups List cell of markup `id` from text (custom columns, Subject, Label,
    /// Comments, Status, Checkmark, Lock). Returns false when the column is read-only or the
    /// text is not accepted for it.
    pub fn set_cell(&mut self, id: &str, column_id: &str, text: &str) -> Result<bool> {
        let i = self.index_of(id)?;
        if text.len() > crate::props::MAX_TEXT {
            return Err(invalid("the text is too long"));
        }
        if let Some(m) = self.doc.markups.get(i)
            && m.locked()
            && column_id != "lock"
        {
            return Err(EngineError::Locked(m.id.clone()));
        }
        self.edit("Edit Cell", |s| {
            let ok = MarkupTable::set_cell(&mut s.doc, i, column_id, text);
            Ok((ok, ok))
        })
    }

    /// Replace the document's custom column definitions (the Manage Columns dialog). Values
    /// stored for removed columns stay on the markups and come back with the column.
    pub fn set_custom_columns(&mut self, columns: Vec<CustomColumn>) -> Result<()> {
        if columns.len() > MAX_COLUMNS {
            return Err(invalid(format!("at most {MAX_COLUMNS} custom columns")));
        }
        let mut ids = std::collections::HashSet::new();
        for c in &columns {
            let id_ok =
                !c.id.is_empty() && c.id.len() <= 64 && c.id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            if !id_ok || !ids.insert(c.id.as_str()) {
                return Err(invalid(format!(
                    "column id {:?} must be unique letters, digits or _",
                    c.id
                )));
            }
            if c.name.trim().is_empty() || c.name.len() > 256 {
                return Err(invalid("every column needs a name (up to 256 bytes)"));
            }
            if c.formula.len() > 4096 || c.items.len() > 10_000 {
                return Err(invalid(format!("column {} is too large", c.name)));
            }
        }
        if columns == self.doc.columns {
            return Ok(());
        }
        self.edit("Columns", move |s| {
            s.doc.columns = columns;
            s.doc.columns_changed = true;
            Ok(((), true))
        })
    }

    /// Cut a hole (deduction) out of an Area or Volume measurement. The ring must lie inside
    /// its outline.
    pub fn add_cutout(&mut self, id: &str, ring: Vec<Point>) -> Result<()> {
        let i = self.index_of(id)?;
        crate::geometry::check_finite(&ring)?;
        if ring.len() > crate::geometry::MAX_POINTS {
            return Err(invalid("too many cutout points"));
        }
        if let Some(m) = self.doc.markups.get(i)
            && m.locked()
        {
            return Err(EngineError::Locked(m.id.clone()));
        }
        self.edit("Add Cutout", |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            if !measure_extras::add_cutout(m, ring) {
                return Err(invalid(
                    "a cutout needs an Area or Volume measurement and at least 3 points inside its outline",
                ));
            }
            Ok(((), true))
        })
    }

    /// Remove cutout `hole` of markup `id`.
    pub fn remove_cutout(&mut self, id: &str, hole: usize) -> Result<()> {
        let i = self.index_of(id)?;
        self.edit("Remove Cutout", |s| {
            let m = s
                .doc
                .markups
                .get_mut(i)
                .ok_or_else(|| invalid("markup index out of range"))?;
            if m.locked() {
                return Err(EngineError::Locked(m.id.clone()));
            }
            if !measure_extras::remove_cutout(m, hole) {
                return Err(invalid(format!("cutout {} does not exist", hole + 1)));
            }
            Ok(((), true))
        })
    }

    /// The document as it is now (page operations included) for a renderer: the file's bytes
    /// while the page structure is untouched, else a full rewrite of the object graph. Markups
    /// the model holds but has not saved are not in it.
    pub fn render_bytes(&self) -> Result<Arc<Vec<u8>>> {
        if self.file.cos.modified_objects().is_empty() {
            return Ok(self.file.bytes());
        }
        Ok(Arc::new(write_full(&self.file.cos, &SaveOptions::default())?))
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{ColumnType, Kind, Markup, Point};

    use crate::{MarkupPatch, Session};

    fn session() -> Session {
        let mut s = Session::new_blank("ui.pdf", &[(612.0, 792.0)]).unwrap();
        let pts = vec![Point::new(10.0, 10.0), Point::new(100.0, 10.0)];
        s.add_markup(Markup::new(Kind::Line, 0, pts)).unwrap();
        s
    }

    #[test]
    fn a_merged_gesture_is_one_undo_step() {
        let mut s = session();
        let id = s.doc().markups[0].id.clone();
        let depth = s.undo_depth();
        s.set_merge_key(Some("opacity"));
        for v in [0.9, 0.7, 0.5] {
            let p = MarkupPatch {
                opacity: Some(v),
                ..Default::default()
            };
            s.set_properties(std::slice::from_ref(&id), &p).unwrap();
        }
        s.set_merge_key(None);
        assert_eq!(s.undo_depth(), depth + 1);
        // After a seal the same key starts a new step.
        s.seal();
        s.set_merge_key(Some("opacity"));
        let p = MarkupPatch {
            opacity: Some(0.2),
            ..Default::default()
        };
        s.set_properties(std::slice::from_ref(&id), &p).unwrap();
        s.set_merge_key(None);
        assert_eq!(s.undo_depth(), depth + 2);
        s.undo().unwrap();
        assert_eq!(s.doc().markups[0].opacity, 0.5);
        s.undo().unwrap();
        assert_eq!(s.doc().markups[0].opacity, 1.0);
    }

    #[test]
    fn cutouts_are_one_step_each() {
        let mut s = session();
        let sq = |x0: f64, y0: f64, x1: f64, y1: f64| {
            vec![
                Point::new(x0, y0),
                Point::new(x1, y0),
                Point::new(x1, y1),
                Point::new(x0, y1),
            ]
        };
        let id = s
            .add_markup(Markup::new(Kind::Area, 0, sq(0.0, 0.0, 100.0, 100.0)))
            .unwrap();
        s.add_cutout(&id, sq(10.0, 10.0, 20.0, 20.0)).unwrap();
        assert_eq!(s.markup(&id).unwrap().holes.len(), 1);
        // Outside the outline: refused, nothing changes.
        assert!(s.add_cutout(&id, sq(200.0, 200.0, 210.0, 210.0)).is_err());
        s.remove_cutout(&id, 0).unwrap();
        assert!(s.markup(&id).unwrap().holes.is_empty());
        s.undo().unwrap();
        assert_eq!(s.markup(&id).unwrap().holes.len(), 1);
    }

    #[test]
    fn cells_and_custom_columns() {
        let mut s = session();
        let id = s.doc().markups[0].id.clone();
        let col = markupcraft_model::CustomColumn {
            id: "cost".into(),
            name: "Cost".into(),
            kind: ColumnType::Currency,
            ..Default::default()
        };
        s.set_custom_columns(vec![col.clone()]).unwrap();
        assert!(s.set_cell(&id, "c:cost", "12.5").unwrap());
        assert!(!s.set_cell(&id, "c:cost", "lots").unwrap());
        assert_eq!(
            s.doc().markups[0].column_data.get("cost").map(String::as_str),
            Some("12.5")
        );
        assert!(s.set_cell(&id, "status", "Accepted").unwrap());
        assert_eq!(s.doc().markups[0].status, "Accepted");
        let dup = vec![col.clone(), col];
        assert!(s.set_custom_columns(dup).is_err());
        assert!(s.render_bytes().unwrap().starts_with(b"%PDF"));
    }
}
