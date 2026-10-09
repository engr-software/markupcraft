//! Preferences and profiles (Revu's Preferences and Profiles): user settings kept as JSON in
//! the user's config folder, one file per profile, with the active profile's name beside them.
//!
//! ```text
//! <config>/profiles/<name>.json     a profile: the preferences below
//! <config>/active-profile           the active profile's name ("Default" when missing)
//! ```
//!
//! The config folder is `MARKUPCRAFT_CONFIG_DIR` when set, else `%APPDATA%\MarkupCraft` on
//! Windows, `~/Library/Application Support/MarkupCraft` on macOS and
//! `$XDG_CONFIG_HOME/markupcraft` (or `~/.config/markupcraft`) elsewhere. Every file is read as
//! untrusted input: unknown keys are ignored, missing keys take their defaults, bad values are
//! reported.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{EngineError, Result, invalid};

pub const DEFAULT_PROFILE: &str = "Default";
/// Largest preferences file read.
const MAX_FILE: u64 = 1 << 20;
/// Most profiles listed.
const MAX_PROFILES: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapping {
    pub content: bool,
    pub markup: bool,
    pub grid: bool,
    /// grid spacing in points
    pub grid_spacing: f64,
    /// snap distance in screen pixels
    pub sensitivity_px: f64,
}

impl Default for Snapping {
    fn default() -> Self {
        Self {
            content: true,
            markup: true,
            grid: false,
            grid_spacing: 18.0,
            sensitivity_px: 8.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    /// new markups' line colour, `#RRGGBB`
    pub markup: String,
    pub measurement: String,
    pub highlight: String,
    pub text: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            markup: "#FF0000".into(),
            measurement: "#0050C8".into(),
            highlight: "#FFFF00".into(),
            text: "#FF0000".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// author written on new markups
    pub author: String,
    /// default measurement units: `ft-in`, `ft`, `in`, `m`, `cm`, `mm`
    pub units: String,
    /// default decimal places / fraction precision shown
    pub precision: u32,
    pub colors: Colors,
    pub snapping: Snapping,
    /// minutes between automatic recovery saves; 0 = off
    pub autosave_minutes: u32,
    /// `incremental` or `full`
    pub save_mode: String,
    pub recent_files: u32,
    /// UI theme: `light`, `dark` or `system`
    pub theme: String,
    /// More pages: Markups List, Layers, Measure, Forms, Signature, Tablet, WebTab, Sets,
    /// Import/Export, Integrations (`prefs_pages.rs`).
    pub more: crate::prefs_pages::MorePrefs,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            author: String::new(),
            units: "ft-in".into(),
            precision: 2,
            colors: Colors::default(),
            snapping: Snapping::default(),
            autosave_minutes: 10,
            save_mode: "incremental".into(),
            recent_files: 20,
            theme: "system".into(),
            more: Default::default(),
        }
    }
}

const UNITS: &[&str] = &["ft-in", "ft", "in", "m", "cm", "mm", "yd", "pt"];

fn hex_ok(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s.chars().skip(1).all(|c| c.is_ascii_hexdigit())
}

impl Preferences {
    /// Check every value (after a merge or an import).
    pub fn validate(&self) -> Result<()> {
        if self.author.chars().count() > 256 || self.author.chars().any(char::is_control) {
            return Err(invalid("author: at most 256 characters, no control characters"));
        }
        if !UNITS.contains(&self.units.as_str()) {
            return Err(invalid(format!("units: one of {}", UNITS.join(", "))));
        }
        if self.precision > 8 {
            return Err(invalid("precision: 0 to 8"));
        }
        for (k, c) in [
            ("markup", &self.colors.markup),
            ("measurement", &self.colors.measurement),
            ("highlight", &self.colors.highlight),
            ("text", &self.colors.text),
        ] {
            if !hex_ok(c) {
                return Err(invalid(format!("colors.{k}: #RRGGBB")));
            }
        }
        let s = &self.snapping;
        if !(s.grid_spacing.is_finite() && (1.0..=1000.0).contains(&s.grid_spacing)) {
            return Err(invalid("snapping.grid_spacing: 1 to 1000 points"));
        }
        if !(s.sensitivity_px.is_finite() && (1.0..=100.0).contains(&s.sensitivity_px)) {
            return Err(invalid("snapping.sensitivity_px: 1 to 100"));
        }
        if self.autosave_minutes > 24 * 60 {
            return Err(invalid("autosave_minutes: 0 to 1440"));
        }
        if !matches!(self.save_mode.as_str(), "incremental" | "full" | "compressed") {
            return Err(invalid("save_mode: incremental, full or compressed"));
        }
        if self.recent_files > 200 {
            return Err(invalid("recent_files: 0 to 200"));
        }
        if !matches!(self.theme.as_str(), "light" | "dark" | "system") {
            return Err(invalid("theme: light, dark or system"));
        }
        self.more.validate()?;
        Ok(())
    }

    /// Apply a partial JSON object (`{"snapping": {"grid": true}}`) on top of these
    /// preferences; the result is validated.
    pub fn merged(&self, patch: &serde_json::Value) -> Result<Preferences> {
        let mut base = serde_json::to_value(self).map_err(|e| invalid(e.to_string()))?;
        merge(&mut base, patch, 0)?;
        let p: Preferences = serde_json::from_value(base).map_err(|e| invalid(format!("preferences: {e}")))?;
        p.validate()?;
        Ok(p)
    }
}

fn merge(base: &mut serde_json::Value, patch: &serde_json::Value, depth: usize) -> Result<()> {
    if depth > 8 {
        return Err(invalid("preferences nest too deeply"));
    }
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v, depth + 1)?,
                    Some(slot) => *slot = v.clone(),
                    None => return Err(invalid(format!("unknown preference {k:?}"))),
                }
            }
            Ok(())
        }
        _ => Err(invalid("preferences are a JSON object")),
    }
}

/// The user's config folder (see the module docs).
pub fn default_config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = env("MARKUPCRAFT_CONFIG_DIR") {
        return Some(d);
    }
    if cfg!(windows) {
        return env("APPDATA").map(|d| d.join("MarkupCraft"));
    }
    if cfg!(target_os = "macos") {
        return env("HOME").map(|h| h.join("Library/Application Support/MarkupCraft"));
    }
    env("XDG_CONFIG_HOME")
        .map(|d| d.join("markupcraft"))
        .or_else(|| env("HOME").map(|h| h.join(".config/markupcraft")))
}

fn check_profile_name(name: &str) -> Result<String> {
    let n = name.trim();
    let ok = !n.is_empty()
        && n.chars().count() <= 64
        && n.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        && !n.starts_with('.');
    if ok {
        Ok(n.to_string())
    } else {
        Err(invalid(
            "a profile name has 1 to 64 letters, digits, spaces, '-', '_' or '.' (not first)",
        ))
    }
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> EngineError + '_ {
    move |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    }
}

/// Read a preferences file (untrusted).
pub fn read_file(path: &Path) -> Result<Preferences> {
    let len = std::fs::metadata(path).map_err(io_err(path))?.len();
    if len > MAX_FILE {
        return Err(invalid(format!(
            "{} is too large for a preferences file",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(path).map_err(io_err(path))?;
    let p: Preferences = serde_json::from_str(&text)
        .map_err(|e| invalid(format!("{} is not a preferences file: {e}", path.display())))?;
    p.validate()?;
    Ok(p)
}

/// Write a preferences file (atomic).
pub fn write_file(path: &Path, p: &Preferences) -> Result<()> {
    p.validate()?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let text = serde_json::to_string_pretty(p).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, text.as_bytes())
}

/// Profiles and preferences in one config folder.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefStore {
    pub dir: PathBuf,
}

impl PrefStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The store in the user's config folder.
    pub fn user() -> Result<Self> {
        default_config_dir()
            .map(Self::new)
            .ok_or_else(|| invalid("no config folder (set MARKUPCRAFT_CONFIG_DIR)"))
    }

    fn profile_path(&self, name: &str) -> Result<PathBuf> {
        Ok(self
            .dir
            .join("profiles")
            .join(format!("{}.json", check_profile_name(name)?)))
    }

    /// The active profile's name.
    pub fn active(&self) -> String {
        let p = self.dir.join("active-profile");
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| check_profile_name(s.lines().next().unwrap_or_default()).ok())
            .unwrap_or_else(|| DEFAULT_PROFILE.to_string())
    }

    /// Profile names, sorted (the active one is always listed).
    pub fn profiles(&self) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(self.dir.join("profiles"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let p = e.path();
                        (p.extension().is_some_and(|x| x == "json"))
                            .then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                            .flatten()
                    })
                    .filter(|n| check_profile_name(n).is_ok())
                    .take(MAX_PROFILES)
                    .collect()
            })
            .unwrap_or_default();
        let active = self.active();
        if !out.contains(&active) {
            out.push(active);
        }
        out.sort_by_key(|n| n.to_lowercase());
        out
    }

    /// A profile's preferences (defaults when it was never saved).
    pub fn load_profile(&self, name: &str) -> Result<Preferences> {
        let p = self.profile_path(name)?;
        if !p.exists() {
            return Ok(Preferences::default());
        }
        read_file(&p)
    }

    /// The active profile's preferences.
    pub fn load(&self) -> Result<Preferences> {
        self.load_profile(&self.active())
    }

    pub fn save_profile(&self, name: &str, p: &Preferences) -> Result<()> {
        write_file(&self.profile_path(name)?, p)
    }

    /// Save into the active profile.
    pub fn save(&self, p: &Preferences) -> Result<()> {
        self.save_profile(&self.active(), p)
    }

    /// Make `name` the active profile; `copy_from_current` first saves the current preferences
    /// under it when it does not exist yet. Returns its preferences.
    pub fn switch(&self, name: &str, copy_from_current: bool) -> Result<Preferences> {
        let name = check_profile_name(name)?;
        let path = self.profile_path(&name)?;
        if !path.exists() {
            let p = if copy_from_current {
                self.load()?
            } else {
                Preferences::default()
            };
            write_file(&path, &p)?;
        }
        let p = read_file(&path)?;
        std::fs::create_dir_all(&self.dir).map_err(io_err(&self.dir))?;
        crate::write_atomic(&self.dir.join("active-profile"), name.as_bytes())?;
        Ok(p)
    }

    /// Delete a profile (not the active one).
    pub fn delete_profile(&self, name: &str) -> Result<()> {
        let name = check_profile_name(name)?;
        if name == self.active() {
            return Err(invalid("switch to another profile before deleting this one"));
        }
        let p = self.profile_path(&name)?;
        if !p.exists() {
            return Err(invalid(format!("no profile named {name:?}")));
        }
        std::fs::remove_file(&p).map_err(io_err(&p))
    }

    /// Copy a profile to a file.
    pub fn export(&self, name: &str, to: &Path) -> Result<()> {
        write_file(to, &self.load_profile(name)?)
    }

    /// Read a preferences file into profile `name` (created or replaced).
    pub fn import(&self, from: &Path, name: &str) -> Result<Preferences> {
        let p = read_file(from)?;
        self.save_profile(name, &p)?;
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_save_switch_export_import() {
        let dir = std::env::temp_dir().join(format!("mc-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = PrefStore::new(&dir);
        assert_eq!(st.active(), DEFAULT_PROFILE);
        assert_eq!(st.load().unwrap(), Preferences::default());
        let p = st
            .load()
            .unwrap()
            .merged(&serde_json::json!({ "author": "Estimator", "snapping": { "grid": true } }))
            .unwrap();
        assert!(p.snapping.grid && p.snapping.content);
        st.save(&p).unwrap();
        assert!(
            st.load()
                .unwrap()
                .merged(&serde_json::json!({ "units": "furlong" }))
                .is_err()
        );
        assert!(st.load().unwrap().merged(&serde_json::json!({ "nope": 1 })).is_err());
        let t = st.switch("Takeoff", true).unwrap();
        assert_eq!(t.author, "Estimator");
        assert_eq!(st.active(), "Takeoff");
        assert_eq!(st.profiles(), vec!["Default", "Takeoff"]);
        assert!(st.delete_profile("Takeoff").is_err());
        let out = dir.join("export.json");
        st.export("Takeoff", &out).unwrap();
        st.import(&out, "Copy").unwrap();
        assert_eq!(st.load_profile("Copy").unwrap().author, "Estimator");
        st.switch("Default", false).unwrap();
        st.delete_profile("Takeoff").unwrap();
        assert!(st.switch("../evil", false).is_err());
        std::fs::write(&out, "{ \"theme\": 5 }").unwrap();
        assert!(read_file(&out).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
