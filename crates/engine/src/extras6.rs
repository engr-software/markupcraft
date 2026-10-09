//! Smaller document features: Mark Text for Redaction (the words under a dragged box), stamp
//! settings (Tools > Stamp as a document feature: the stamp folder, the default stamp, and the
//! opacity, blend mode and lock every placed stamp gets), and three-point arcs drawn into a
//! polyline or polygon (Alt+click while drawing).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use markupcraft_model::{Markup, Point, Rect, measure_extras};
use serde::{Deserialize, Serialize};

use crate::redact::MarkStyle;
use crate::stamps::{StampLibrary, StampPlace, StampSource};
use crate::{EngineError, MarkupPatch, Result, Session, invalid};

impl Session {
    /// The word boxes of `page` that `rect` touches (Mark Text for Redaction).
    pub fn text_boxes_in(&self, page: usize, rect: Rect) -> Result<Vec<Rect>> {
        let r = rect.normalized();
        Ok(self
            .page_words(page)?
            .into_iter()
            .map(|w| w.rect.normalized())
            .filter(|w| w.x0 < r.x1 && w.x1 > r.x0 && w.y0 < r.y1 && w.y1 > r.y0)
            .collect())
    }

    /// Mark Text for Redaction: every word `rect` touches on `page` becomes part of one mark.
    /// Returns the number of words marked. Undoable.
    pub fn redact_mark_text(&mut self, page: usize, rect: Rect, style: &MarkStyle) -> Result<usize> {
        let boxes = self.text_boxes_in(page, rect)?;
        if boxes.is_empty() {
            return Err(invalid("no text under the box to mark"));
        }
        let n = boxes.len();
        self.redact_mark_many(&[(page, boxes)], style)?;
        Ok(n)
    }
}

// ---- stamp settings --------------------------------------------------------------------------

/// How placed stamps look and where the library lives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampSettings {
    /// The stamp folder ("" = `<config>/stamps`); a shared folder lets a team use one library.
    pub folder: String,
    /// The stamp the Stamp tool places when none is chosen (a library id).
    pub default_stamp: String,
    /// Opacity of placed stamps, 0 to 1.
    pub opacity: f64,
    /// Blend mode: `normal` or `multiply` (the stamp darkens what is under it).
    pub blend: String,
    /// Placed stamps are locked (cannot be moved or edited until unlocked).
    pub lock: bool,
}

impl Default for StampSettings {
    fn default() -> Self {
        Self {
            folder: String::new(),
            default_stamp: "Approved".into(),
            opacity: 1.0,
            blend: "normal".into(),
            lock: false,
        }
    }
}

const SETTINGS_FILE: &str = "settings.json";

impl StampSettings {
    pub fn validate(&self) -> Result<()> {
        if !(self.opacity.is_finite() && (0.05..=1.0).contains(&self.opacity)) {
            return Err(invalid("stamp opacity is 0.05 to 1"));
        }
        if !matches!(self.blend.as_str(), "normal" | "multiply") {
            return Err(invalid("stamp blend mode is normal or multiply"));
        }
        if self.folder.chars().count() > 1024 || self.folder.chars().any(char::is_control) {
            return Err(invalid("the stamp folder path is not valid"));
        }
        if self.default_stamp.chars().count() > 200 {
            return Err(invalid("the default stamp id is too long"));
        }
        Ok(())
    }

    /// The library these settings name (`config` holds the default folder).
    pub fn library(&self, config: &Path) -> StampLibrary {
        if self.folder.trim().is_empty() {
            StampLibrary::in_config(config)
        } else {
            StampLibrary::new(PathBuf::from(self.folder.trim()))
        }
    }

    /// Settings kept in `<config>/stamps/settings.json` (defaults when there are none).
    pub fn load(config: &Path) -> Result<Self> {
        let f = config.join("stamps").join(SETTINGS_FILE);
        if !f.exists() {
            return Ok(Self::default());
        }
        let io = |e: std::io::Error| EngineError::Io {
            path: f.display().to_string(),
            source: e,
        };
        if std::fs::metadata(&f).map_err(io)?.len() > 1 << 20 {
            return Err(invalid("the stamp settings file is too large"));
        }
        let text = std::fs::read_to_string(&f).map_err(io)?;
        let s: Self =
            serde_json::from_str(&text).map_err(|e| invalid(format!("the stamp settings are damaged: {e}")))?;
        s.validate()?;
        Ok(s)
    }

    pub fn save(&self, config: &Path) -> Result<()> {
        self.validate()?;
        let dir = config.join("stamps");
        std::fs::create_dir_all(&dir).map_err(|e| EngineError::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        let text = serde_json::to_string_pretty(self).map_err(|e| invalid(e.to_string()))?;
        crate::write_atomic(&dir.join(SETTINGS_FILE), text.as_bytes())
    }
}

impl Session {
    /// Place a stamp with the stamp settings' opacity, blend mode and lock (the default stamp
    /// when `source` is `None`). One undo step. Returns the new markup's id.
    pub fn place_stamp_with_settings(
        &mut self,
        page: usize,
        at: StampPlace,
        source: Option<&StampSource>,
        library: Option<&StampLibrary>,
        settings: &StampSettings,
    ) -> Result<String> {
        settings.validate()?;
        let default = StampSource::Library(settings.default_stamp.clone());
        let source = source.unwrap_or(&default);
        self.set_merge_key(Some("place-stamp-with-settings"));
        let r = (|| -> Result<String> {
            let id = self.place_stamp(page, at, source, library, &BTreeMap::new(), None)?;
            let patch = MarkupPatch {
                opacity: Some(settings.opacity),
                multiply: Some(settings.blend == "multiply"),
                locked: Some(settings.lock),
                ..Default::default()
            };
            self.set_properties(std::slice::from_ref(&id), &patch)?;
            Ok(id)
        })();
        self.set_merge_key(None);
        self.seal();
        r
    }
}

// ---- three-point arcs while drawing ------------------------------------------------------------

/// Turn the points at `through` (indices into `m.pts`, as clicked) into arcs: the point at `t`
/// is where the arc from point `t - 1` to point `t + 1` passes. Points that cannot carry an arc
/// (the first or last of an open shape, or a markup kind without arcs) are left as vertices.
/// Returns how many arcs were made.
pub fn arcs_through(m: &mut Markup, through: &[usize]) -> usize {
    if !measure_extras::can_have_arcs(m.kind) {
        return 0;
    }
    let mut list: Vec<usize> = through.to_vec();
    list.sort_unstable();
    list.dedup();
    let mut shift: isize = 0;
    let mut made = 0;
    for t in list {
        let Some(at) = t.checked_add_signed(shift) else {
            continue;
        };
        if at == 0 || at + 1 >= m.pts.len() {
            continue;
        }
        let Some(p) = m.pts.get(at).copied() else { continue };
        let saved = (m.pts.clone(), m.arcs.clone());
        m.pts.remove(at);
        match measure_extras::convert_to_arc(m, at - 1, 0.0) {
            Some(arc) => {
                measure_extras::bend_arc(m, arc, p);
                let n = m.arcs.get(arc).map_or(1, |a| a.1);
                shift += n as isize - 1;
                made += 1;
            }
            None => {
                (m.pts, m.arcs) = saved;
            }
        }
    }
    if made > 0 {
        m.dirty = true;
    }
    made
}

/// The point on the arc through `a`, `t`, `b` nearest `t` (for tests and previews): `t` itself
/// when the three are not collinear.
pub fn arc_passes(a: Point, t: Point, b: Point) -> bool {
    measure_extras::circle_through(a, t, b).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};
    use markupcraft_model::Kind;

    #[test]
    fn mark_text_for_redaction_marks_the_words_under_a_box() {
        let p = std::env::temp_dir().join("extras6-redact.pdf");
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(
                612.0,
                792.0,
                text(72.0, 700.0, 14.0, "SECRET plan here"),
            )]),
            &p,
        )
        .unwrap();
        let n = s
            .redact_mark_text(0, Rect::new(60.0, 690.0, 140.0, 720.0), &MarkStyle::default())
            .unwrap();
        assert!(n >= 1, "{n}");
        assert_eq!(s.redact_marks().len(), 1);
        assert!(
            s.redact_mark_text(0, Rect::new(300.0, 100.0, 320.0, 120.0), &MarkStyle::default())
                .is_err()
        );
    }

    #[test]
    fn stamp_settings_round_trip_and_style_placed_stamps() {
        let d = std::env::temp_dir().join(format!("extras6-stamps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(StampSettings::load(&d).unwrap(), StampSettings::default());
        // Sixteen or more built-in designs of our own.
        assert!(crate::stamps::builtin_stamps().len() >= 16);
        let st = StampSettings {
            default_stamp: "Reviewed".into(),
            opacity: 0.5,
            blend: "multiply".into(),
            lock: true,
            ..Default::default()
        };
        st.save(&d).unwrap();
        assert_eq!(StampSettings::load(&d).unwrap(), st);
        assert!(
            StampSettings {
                opacity: 2.0,
                ..Default::default()
            }
            .save(&d)
            .is_err()
        );
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), d.join("a.pdf")).unwrap();
        let lib = st.library(&d);
        let id = s
            .place_stamp_with_settings(0, StampPlace::Center(Point::new(300.0, 400.0)), None, Some(&lib), &st)
            .unwrap();
        let m = s.markup(&id).unwrap();
        assert!((m.opacity - 0.5).abs() < 1e-9);
        assert!(m.locked());
        assert!(m.multiply);
        // One undo removes it.
        s.undo().unwrap();
        assert!(s.markup(&id).is_err());
    }

    #[test]
    fn alt_clicked_points_become_three_point_arcs() {
        let mut m = Markup::new(
            Kind::Polyline,
            0,
            vec![
                Point::new(0.0, 0.0),
                Point::new(50.0, 50.0),
                Point::new(100.0, 0.0),
                Point::new(150.0, 0.0),
            ],
        );
        assert_eq!(arcs_through(&mut m, &[1]), 1);
        assert_eq!(m.arcs.len(), 1);
        // The arc passes near the clicked point.
        assert!(
            m.pts.iter().any(|p| p.dist(Point::new(50.0, 50.0)) < 3.0),
            "{:?}",
            m.pts
        );
        assert_eq!(m.pts.first(), Some(&Point::new(0.0, 0.0)));
        assert_eq!(m.pts.last(), Some(&Point::new(150.0, 0.0)));
        // End points cannot be arc through-points.
        let mut n = Markup::new(Kind::Polyline, 0, vec![Point::new(0.0, 0.0), Point::new(5.0, 5.0)]);
        assert_eq!(arcs_through(&mut n, &[0, 1]), 0);
        assert!(arc_passes(
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 0.0)
        ));
    }
}
