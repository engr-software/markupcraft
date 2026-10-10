//! File Access Explorer path favourites: folders the user goes back to often, kept in
//! `file_access_favorites.json` in the config folder (in the order they were added).

use std::path::{Path, PathBuf};

use crate::{Result, invalid};

/// Most favourites kept.
pub const MAX_FAVORITES: usize = 200;
/// Largest favourites file read.
const MAX_FILE: u64 = 1 << 20;

/// The file the favourites are kept in.
pub fn favorites_file(config_dir: &Path) -> PathBuf {
    config_dir.join("file_access_favorites.json")
}

/// The favourite folders (none when the file does not exist yet).
pub fn load_favorites(config_dir: &Path) -> Result<Vec<PathBuf>> {
    let f = favorites_file(config_dir);
    let Ok(meta) = std::fs::metadata(&f) else {
        return Ok(Vec::new());
    };
    if meta.len() > MAX_FILE {
        return Err(invalid(format!("{} is too large", f.display())));
    }
    let text = markupcraft_revu::fsio::read_to_string(&f).map_err(|e| invalid(format!("{}: {e}", f.display())))?;
    let mut v: Vec<PathBuf> = serde_json::from_str(&text).map_err(|e| invalid(format!("{}: {e}", f.display())))?;
    v.truncate(MAX_FAVORITES);
    Ok(v)
}

fn store(config_dir: &Path, v: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(config_dir).map_err(|e| invalid(format!("{}: {e}", config_dir.display())))?;
    let json = serde_json::to_string_pretty(v).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(&favorites_file(config_dir), json.as_bytes())
}

/// Add `folder` (it must be a folder); returns the list. Adding one already there changes
/// nothing.
pub fn add_favorite(config_dir: &Path, folder: &Path) -> Result<Vec<PathBuf>> {
    if !folder.is_dir() {
        return Err(invalid(format!("{} is not a folder", folder.display())));
    }
    let mut v = load_favorites(config_dir)?;
    if !v.iter().any(|f| f == folder) {
        if v.len() >= MAX_FAVORITES {
            return Err(invalid(format!("at most {MAX_FAVORITES} favourites")));
        }
        v.push(folder.to_path_buf());
        store(config_dir, &v)?;
    }
    Ok(v)
}

/// Remove `folder`; returns the list.
pub fn remove_favorite(config_dir: &Path, folder: &Path) -> Result<Vec<PathBuf>> {
    let mut v = load_favorites(config_dir)?;
    let n = v.len();
    v.retain(|f| f != folder);
    if v.len() != n {
        store(config_dir, &v)?;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_add_once_remove_and_persist() {
        let dir = std::env::temp_dir().join(format!("markupcraft-favs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = dir.join("cfg");
        let a = dir.join("a");
        std::fs::create_dir_all(&a).unwrap();
        assert!(load_favorites(&cfg).unwrap().is_empty());
        add_favorite(&cfg, &a).unwrap();
        assert_eq!(add_favorite(&cfg, &a).unwrap(), vec![a.clone()]);
        assert!(add_favorite(&cfg, &dir.join("missing")).is_err());
        assert_eq!(load_favorites(&cfg).unwrap(), vec![a.clone()]);
        assert!(remove_favorite(&cfg, &a).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
