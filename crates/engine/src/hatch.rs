//! Hatch patterns on closed markups (Properties > Hatch). Stored as `/PCHatch` and drawn into
//! the appearance on save (`markupcraft_revu::hatch`).

pub use markupcraft_model::hatch::{Hatch, HatchStyle};
use markupcraft_revu::hatch::hatchable;

use crate::{EngineError, Result, Session, invalid};

impl Session {
    /// Set (or with `None` remove) the hatch of markups. Returns how many changed. Undoable.
    pub fn set_hatch(&mut self, ids: &[String], hatch: Option<Hatch>) -> Result<usize> {
        if ids.is_empty() {
            return Err(invalid("no markups given"));
        }
        if let Some(h) = &hatch
            && (!h.valid() || h.spacing > 1_000.0 || h.width > 100.0)
        {
            return Err(invalid(
                "a hatch needs a spacing of 0.5 to 1000 points and a line width of 0 to 100 points",
            ));
        }
        for id in ids {
            let m = self.markup(id)?;
            if m.locked() {
                return Err(EngineError::Locked(id.clone()));
            }
            if hatch.is_some() && !hatchable(m.kind) {
                return Err(invalid(format!(
                    "{} markups cannot be hatched (Area, Polygon, Rectangle, Ellipse, Cloud and Volume can)",
                    m.kind.name()
                )));
            }
        }
        let ids = ids.to_vec();
        self.edit("Hatch", |s| {
            let mut n = 0;
            for id in &ids {
                let m = s.doc.find_mut(id).ok_or_else(|| EngineError::NoMarkup(id.clone()))?;
                if m.hatch != hatch {
                    m.hatch = hatch;
                    m.dirty = true;
                    n += 1;
                }
            }
            Ok((n, n > 0))
        })
    }
}

#[cfg(test)]
mod tests {
    use markupcraft_model::{Kind, Markup, Rect};

    use super::*;

    #[test]
    fn hatch_saves_into_the_appearance_and_reads_back() {
        let dir = std::env::temp_dir().join(format!("mc-hatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hatch.pdf");
        let mut s = Session::new_blank(&path, &[(612.0, 792.0)]).unwrap();
        let id = s
            .add_markup(Markup::new(
                Kind::Polygon,
                0,
                Rect::new(100.0, 100.0, 200.0, 200.0).corners().to_vec(),
            ))
            .unwrap();
        let line = s
            .add_markup(Markup::new(
                Kind::Line,
                0,
                vec![
                    markupcraft_model::Point::new(0.0, 0.0),
                    markupcraft_model::Point::new(9.0, 9.0),
                ],
            ))
            .unwrap();
        let h = Hatch {
            style: HatchStyle::Cross,
            spacing: 8.0,
            ..Default::default()
        };
        assert!(s.set_hatch(std::slice::from_ref(&line), Some(h)).is_err());
        assert_eq!(s.set_hatch(std::slice::from_ref(&id), Some(h)).unwrap(), 1);
        s.save(true).unwrap();
        let s2 = Session::open(&path).unwrap();
        let m = s2.markup(&id).unwrap();
        assert_eq!(m.hatch, Some(h));
        let ap = s2.pdf().cos.get(markupcraft_revu::cos::ObjRef::new(m.obj.0, m.obj.1));
        let ap = ap
            .as_dict()
            .unwrap()
            .get(b"AP")
            .and_then(|o| s2.pdf().cos.dict(o))
            .unwrap();
        let n = s2.pdf().cos.get(ap.reference(b"N").unwrap());
        let markupcraft_revu::cos::Object::Stream(st) = n.as_ref() else {
            panic!()
        };
        let text = String::from_utf8_lossy(&st.decoded().unwrap()).into_owned();
        assert!(text.contains("W* n"), "{text}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
