//! Profiles beyond the basics of [`crate::prefs::PrefStore`]: the profiles MarkupCraft ships
//! (Takeoff, Construction, Design Review, Simple), renaming a profile, and profile bundles
//! that share a profile as one file.
//!
//! A bundle (`.mcprofile`, JSON) holds a profile's preferences, its interface settings
//! (`<config>/ui/<name>.json`) and keyboard shortcuts (`<config>/keys/<name>.json`) and, when
//! asked, its dependencies: the shared settings files of the config folder (the Tool Chest
//! with its tool sets and line styles, compare presets, bookmark structures, templates...).
//! Importing a bundle is untrusted input: sizes are capped, only plain `*.json` file names
//! are written, and the preferences are validated.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::prefs::{PrefStore, Preferences, check_profile_name, io_err};
use crate::{Result, invalid};

/// The profiles MarkupCraft ships, by name.
pub const SHIPPED: &[&str] = &["Construction", "Design Review", "Simple", "Takeoff"];

/// The bundle format name.
pub const BUNDLE_FORMAT: &str = "markupcraft-profile";
/// Largest bundle read.
const MAX_BUNDLE: u64 = 64 << 20;
/// Largest single settings file carried in a bundle.
const MAX_PART: u64 = 8 << 20;
/// Most dependency files in a bundle.
const MAX_DEPS: usize = 64;
/// Settings files that belong to the user, not to a profile: never bundled or overwritten.
const PRIVATE: &[&str] = &["recent.json"];

/// The preferences of a shipped profile.
pub fn shipped_preferences(name: &str) -> Option<Preferences> {
    let mut p = Preferences::default();
    match name {
        // Quantities first: snapping to content and markups, feet and inches to the inch.
        "Takeoff" => {
            p.units = "ft-in".into();
            p.precision = 0;
            p.snapping.content = true;
            p.snapping.markup = true;
            p.more.measure.fill_cutouts = true;
            p.colors.measurement = "#0050C8".into();
        }
        // Field and coordination work: red markups, grid snapping for sketches.
        "Construction" => {
            p.colors.markup = "#E00000".into();
            p.snapping.grid = true;
            p.autosave_minutes = 5;
        }
        // Review: comments and clouds in a review colour, no snapping to page content.
        "Design Review" => {
            p.colors.markup = "#C800C8".into();
            p.colors.text = "#C800C8".into();
            p.snapping.content = false;
        }
        // The fewest moving parts: no snapping at all.
        "Simple" => {
            p.snapping.content = false;
            p.snapping.markup = false;
            p.snapping.grid = false;
        }
        _ => return None,
    }
    Some(p)
}

/// A profile shared as one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub preferences: Preferences,
    /// Interface settings (`ui/<name>.json`), as stored.
    #[serde(default)]
    pub interface: Option<Value>,
    /// Keyboard shortcuts (`keys/<name>.json`), as stored.
    #[serde(default)]
    pub keys: Option<Value>,
    /// Shared settings files of the config folder, by file name.
    #[serde(default)]
    pub dependencies: std::collections::BTreeMap<String, Value>,
}

/// What an imported bundle brought.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Imported {
    pub name: String,
    pub dependencies: Vec<String>,
}

fn read_json(path: &Path, max: u64) -> Option<Value> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > max {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// A plain settings file name (`toolchest.json`): no folders, no hidden files.
fn dependency_name_ok(n: &str) -> bool {
    n.len() <= 100
        && n.ends_with(".json")
        && !n.starts_with('.')
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !PRIVATE.contains(&n)
}

impl PrefStore {
    fn profile_file(&self, sub: &str, name: &str) -> Result<PathBuf> {
        Ok(self.dir.join(sub).join(format!("{}.json", check_profile_name(name)?)))
    }

    /// Whether `name` is a profile MarkupCraft ships.
    pub fn is_shipped(name: &str) -> bool {
        SHIPPED.contains(&name)
    }

    /// Rename a profile (its preferences, interface settings and shortcuts). A shipped profile
    /// that was never changed is saved under the new name. The active profile stays active.
    pub fn rename_profile(&self, from: &str, to: &str) -> Result<()> {
        let from = check_profile_name(from)?;
        let to = check_profile_name(to)?;
        if from == to {
            return Ok(());
        }
        if !self.profiles().contains(&from) {
            return Err(invalid(format!("no profile named {from:?}")));
        }
        if self.profiles().iter().any(|p| p.eq_ignore_ascii_case(&to)) && !from.eq_ignore_ascii_case(&to) {
            return Err(invalid(format!("a profile named {to:?} exists already")));
        }
        let src = self.profile_file("profiles", &from)?;
        if !src.exists() {
            crate::prefs::write_file(&src, &self.load_profile(&from)?)?;
        }
        for sub in ["profiles", "ui", "keys"] {
            let a = self.profile_file(sub, &from)?;
            if a.exists() {
                let b = self.profile_file(sub, &to)?;
                std::fs::rename(&a, &b).map_err(io_err(&a))?;
            }
        }
        if self.active() == from {
            std::fs::create_dir_all(&self.dir).map_err(io_err(&self.dir))?;
            crate::write_atomic(&self.dir.join("active-profile"), to.as_bytes())?;
        }
        Ok(())
    }

    /// The bundle of profile `name`; `dependencies` adds the shared settings files.
    pub fn bundle(&self, name: &str, dependencies: bool) -> Result<Bundle> {
        let name = check_profile_name(name)?;
        let mut deps = std::collections::BTreeMap::new();
        if dependencies && let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
            files.sort();
            for f in files {
                let Some(n) = f.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
                    continue;
                };
                if deps.len() >= MAX_DEPS || !dependency_name_ok(&n) {
                    continue;
                }
                if let Some(v) = read_json(&f, MAX_PART) {
                    deps.insert(n, v);
                }
            }
        }
        Ok(Bundle {
            format: BUNDLE_FORMAT.into(),
            version: 1,
            preferences: self.load_profile(&name)?,
            interface: read_json(&self.profile_file("ui", &name)?, MAX_PART),
            keys: read_json(&self.profile_file("keys", &name)?, MAX_PART),
            dependencies: deps,
            name,
        })
    }

    /// Write profile `name` as a bundle file (atomic).
    pub fn export_bundle(&self, name: &str, to: &Path, dependencies: bool) -> Result<usize> {
        let b = self.bundle(name, dependencies)?;
        let text = serde_json::to_vec_pretty(&b).map_err(|e| invalid(e.to_string()))?;
        if let Some(dir) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        crate::write_atomic(to, &text)?;
        Ok(b.dependencies.len())
    }

    /// Read a bundle file into a profile (named `as_name`, else the bundle's own name),
    /// created or replaced, with its interface settings, shortcuts and dependencies.
    pub fn import_bundle(&self, from: &Path, as_name: Option<&str>) -> Result<Imported> {
        let len = std::fs::metadata(from).map_err(io_err(from))?.len();
        if len > MAX_BUNDLE {
            return Err(invalid(format!("{} is too large for a profile file", from.display())));
        }
        let bytes = std::fs::read(from).map_err(io_err(from))?;
        let b: Bundle = serde_json::from_slice(&bytes)
            .map_err(|e| invalid(format!("{} is not a profile file: {e}", from.display())))?;
        if b.format != BUNDLE_FORMAT || b.version == 0 || b.version > 1 {
            return Err(invalid(format!(
                "{} is not a MarkupCraft profile (version 1)",
                from.display()
            )));
        }
        b.preferences.validate()?;
        let name = check_profile_name(as_name.unwrap_or(&b.name))?;
        if b.dependencies.len() > MAX_DEPS || b.dependencies.keys().any(|k| !dependency_name_ok(k)) {
            return Err(invalid("the profile file names settings files it may not write"));
        }
        self.save_profile(&name, &b.preferences)?;
        for (sub, v) in [("ui", &b.interface), ("keys", &b.keys)] {
            if let Some(v) = v.as_ref().filter(|v| v.is_object()) {
                let p = self.profile_file(sub, &name)?;
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
                }
                let text = serde_json::to_vec_pretty(v).map_err(|e| invalid(e.to_string()))?;
                crate::write_atomic(&p, &text)?;
            }
        }
        let mut deps = Vec::new();
        for (n, v) in &b.dependencies {
            let text = serde_json::to_vec_pretty(v).map_err(|e| invalid(e.to_string()))?;
            std::fs::create_dir_all(&self.dir).map_err(io_err(&self.dir))?;
            crate::write_atomic(&self.dir.join(n), &text)?;
            deps.push(n.clone());
        }
        Ok(Imported {
            name,
            dependencies: deps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_rename_and_bundles() {
        let dir = std::env::temp_dir().join(format!("mc-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = PrefStore::new(&dir);
        for s in SHIPPED {
            assert!(st.profiles().contains(&s.to_string()), "{s} ships");
        }
        assert!(!st.load_profile("Simple").unwrap().snapping.content);
        let t = st.switch("Takeoff", true).unwrap();
        assert_eq!(t.precision, 0, "a shipped profile starts from its own settings");
        // Rename the active profile: it stays active under the new name.
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join("ui").join("Takeoff.json"), "{\"rulers\": true}").unwrap();
        st.rename_profile("Takeoff", "Estimating").unwrap();
        assert_eq!(st.active(), "Estimating");
        assert!(dir.join("ui").join("Estimating.json").exists());
        assert!(st.rename_profile("Estimating", "Simple").is_err(), "taken");
        // A bundle with dependencies.
        std::fs::write(dir.join("toolchest.json"), "{\"sets\": []}").unwrap();
        std::fs::write(dir.join("recent.json"), "{}").unwrap();
        let out = dir.join("share.mcprofile");
        assert_eq!(st.export_bundle("Estimating", &out, true).unwrap(), 1);
        let other = PrefStore::new(dir.join("other"));
        let got = other.import_bundle(&out, None).unwrap();
        assert_eq!(got.name, "Estimating");
        assert_eq!(got.dependencies, vec!["toolchest.json".to_string()]);
        assert_eq!(other.load_profile("Estimating").unwrap().precision, 0);
        assert!(dir.join("other").join("ui").join("Estimating.json").exists());
        assert!(!dir.join("other").join("recent.json").exists());
        // A hostile bundle is refused.
        let bad = dir.join("bad.mcprofile");
        let mut b = st.bundle("Estimating", false).unwrap();
        b.dependencies.insert("../evil.json".into(), Value::Null);
        std::fs::write(&bad, serde_json::to_vec(&b).unwrap()).unwrap();
        assert!(other.import_bundle(&bad, None).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
