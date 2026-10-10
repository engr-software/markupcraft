//! Saved Markup Summary column configurations: a named list of columns in order, and whether
//! columns with no value in any row are kept. They live in `summary_columns.json` in the config
//! folder, so every document can load them.

use std::path::{Path, PathBuf};

use markupcraft_model::Document;
use markupcraft_model::table::MarkupTable;
use serde::{Deserialize, Serialize};

use crate::summary::SummaryOptions;
use crate::{Result, invalid};

/// Most saved configurations.
pub const MAX_CONFIGS: usize = 500;
/// Most columns in one configuration.
pub const MAX_COLUMNS: usize = 500;
/// Largest configuration file read.
const MAX_FILE: u64 = 4 << 20;

fn yes() -> bool {
    true
}

/// A saved column configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnConfig {
    pub name: String,
    /// Markups List column ids, in report order.
    pub columns: Vec<String>,
    /// Keep columns that are empty in every row of the report.
    #[serde(default = "yes")]
    pub include_empty: bool,
}

/// The file the configurations are kept in.
pub fn configs_file(config_dir: &Path) -> PathBuf {
    config_dir.join("summary_columns.json")
}

/// Every saved configuration (none when the file does not exist yet).
pub fn load_configs(config_dir: &Path) -> Result<Vec<ColumnConfig>> {
    let f = configs_file(config_dir);
    let Ok(meta) = std::fs::metadata(&f) else {
        return Ok(Vec::new());
    };
    if meta.len() > MAX_FILE {
        return Err(invalid(format!("{} is too large", f.display())));
    }
    let text = markupcraft_revu::fsio::read_to_string(&f).map_err(|e| invalid(format!("{}: {e}", f.display())))?;
    let mut v: Vec<ColumnConfig> = serde_json::from_str(&text).map_err(|e| invalid(format!("{}: {e}", f.display())))?;
    v.truncate(MAX_CONFIGS);
    Ok(v)
}

fn store(config_dir: &Path, v: &[ColumnConfig]) -> Result<()> {
    std::fs::create_dir_all(config_dir).map_err(|e| invalid(format!("{}: {e}", config_dir.display())))?;
    let json = serde_json::to_string_pretty(v).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(&configs_file(config_dir), json.as_bytes())
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Save `c` (replacing a configuration of the same name).
pub fn save_config(config_dir: &Path, c: &ColumnConfig) -> Result<()> {
    let name = c.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(invalid("a configuration needs a name of 1 to 100 characters"));
    }
    if c.columns.is_empty() || c.columns.len() > MAX_COLUMNS {
        return Err(invalid(format!("a configuration holds 1 to {MAX_COLUMNS} columns")));
    }
    if c.columns.iter().any(|id| id.trim().is_empty() || id.len() > 200) {
        return Err(invalid("column ids must be 1 to 200 characters"));
    }
    let mut v = load_configs(config_dir)?;
    let c = ColumnConfig {
        name: name.to_string(),
        ..c.clone()
    };
    match v.iter_mut().find(|x| same(&x.name, name)) {
        Some(x) => *x = c,
        None => {
            if v.len() >= MAX_CONFIGS {
                return Err(invalid(format!("at most {MAX_CONFIGS} configurations")));
            }
            v.push(c);
        }
    }
    store(config_dir, &v)
}

/// The configuration called `name`.
pub fn find_config(config_dir: &Path, name: &str) -> Result<ColumnConfig> {
    load_configs(config_dir)?
        .into_iter()
        .find(|c| same(&c.name, name))
        .ok_or_else(|| invalid(format!("no saved column configuration {name:?}")))
}

/// Delete the configuration called `name`; returns whether there was one.
pub fn delete_config(config_dir: &Path, name: &str) -> Result<bool> {
    let mut v = load_configs(config_dir)?;
    let n = v.len();
    v.retain(|c| !same(&c.name, name));
    if v.len() == n {
        return Ok(false);
    }
    store(config_dir, &v)?;
    Ok(true)
}

/// The columns of `columns` that have a value in at least one markup of the report (`pages`
/// 0-based; empty = every page). Unknown column ids are dropped.
pub fn non_empty_columns(doc: &Document, columns: &[String], pages: &[usize]) -> Vec<String> {
    let table = MarkupTable::new(doc);
    let rows: Vec<usize> = doc
        .markups
        .iter()
        .enumerate()
        .filter(|(_, m)| pages.is_empty() || pages.contains(&m.page))
        .map(|(i, _)| i)
        .collect();
    columns
        .iter()
        .filter(|id| {
            table
                .column_index(id)
                .is_some_and(|c| rows.iter().any(|r| !table.cell(*r, c).text.trim().is_empty()))
        })
        .cloned()
        .collect()
}

impl SummaryOptions {
    /// Use a saved configuration's columns (in its order); empty columns are dropped unless
    /// the configuration keeps them.
    pub fn use_columns(&mut self, doc: &Document, c: &ColumnConfig) {
        self.columns = c.columns.clone();
        if !c.include_empty {
            self.drop_empty_columns(doc);
        }
    }

    /// Leave out the chosen columns that are empty in every row of the report (with no columns
    /// chosen, the Markups List's default ones are checked).
    pub fn drop_empty_columns(&mut self, doc: &Document) {
        if self.columns.is_empty() {
            let table = MarkupTable::new(doc);
            self.columns = table
                .columns()
                .iter()
                .filter(|c| c.visible_by_default)
                .map(|c| c.id.clone())
                .collect();
        }
        self.columns = non_empty_columns(doc, &self.columns, &self.pages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configs_save_load_replace_and_delete() {
        let dir = std::env::temp_dir().join(format!("markupcraft-sumcols-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_configs(&dir).unwrap().is_empty());
        let c = ColumnConfig {
            name: "Takeoff".into(),
            columns: vec!["subject".into(), "page".into()],
            include_empty: false,
        };
        save_config(&dir, &c).unwrap();
        save_config(
            &dir,
            &ColumnConfig {
                columns: vec!["page".into()],
                ..c.clone()
            },
        )
        .unwrap();
        let v = load_configs(&dir).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].columns, ["page"]);
        assert_eq!(find_config(&dir, "takeoff").unwrap().name, "Takeoff");
        assert!(save_config(&dir, &ColumnConfig { name: " ".into(), ..c }).is_err());
        assert!(delete_config(&dir, "TAKEOFF").unwrap());
        assert!(!delete_config(&dir, "TAKEOFF").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
