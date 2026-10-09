//! `--root` confinement of tool paths. Adapted from PdfCraft's `pdfcraft-automation`
//! (MIT OR Apache-2.0; see THIRD_PARTY.md).
//!
//! With a root, every path must resolve inside it, symlinks and `..` included. Relative paths
//! resolve inside the root. Every path outside gets the same refusal whether or not it exists,
//! so a confined agent cannot probe the rest of the disk; on Windows another network share or
//! device path is refused without being contacted.

use std::path::{Component, Path, PathBuf};

use crate::{Result, failed};

pub fn resolve(root: Option<&Path>, path: &str, for_write: bool) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(crate::bad_args("the path is empty"));
    }
    let p = Path::new(path);
    let Some(root) = root else { return Ok(p.to_path_buf()) };
    let outside = || failed(format!("{path} is outside the allowed directory {}", root.display()));
    let joined = lexical(&root.join(p)).ok_or_else(outside)?;
    if foreign_share(&joined, root) {
        return Err(outside());
    }
    let mut existing = joined.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        if existing.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(outside()); // a broken link: going on would tell whether its target exists
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent;
            }
            _ => return Err(outside()),
        }
    }
    let mut real = existing.canonicalize().map_err(|_| outside())?;
    if !real.starts_with(root) {
        return Err(outside());
    }
    if !rest.is_empty() && !for_write {
        real = joined.canonicalize().map_err(|e| failed(format!("{path}: {e}")))?;
        if !real.starts_with(root) {
            return Err(outside());
        }
        return Ok(real);
    }
    real.extend(rest.iter().rev());
    Ok(real)
}

/// `path` with `.` and `..` resolved by name; `None` if a `..` climbs above its start.
fn lexical(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<Component> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                _ => return None,
            },
            other => parts.push(other),
        }
    }
    Some(parts.iter().collect())
}

/// Whether `path` names another network share or device namespace than `root`.
fn foreign_share(path: &Path, root: &Path) -> bool {
    use std::path::Prefix;
    fn share(p: &Path) -> Option<String> {
        let Some(Component::Prefix(prefix)) = p.components().next() else {
            return None;
        };
        let (kind, a, b) = match prefix.kind() {
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) => return None,
            Prefix::UNC(host, share) | Prefix::VerbatimUNC(host, share) => ("unc", host, share),
            Prefix::DeviceNS(name) => ("device", name, std::ffi::OsStr::new("")),
            Prefix::Verbatim(name) => ("verbatim", name, std::ffi::OsStr::new("")),
        };
        Some(format!("{kind}\\{}\\{}", a.to_string_lossy(), b.to_string_lossy()).to_lowercase())
    }
    share(path).is_some_and(|s| share(root).as_ref() != Some(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_refuses_climbing_out() {
        assert_eq!(lexical(Path::new("a/../b")), Some(PathBuf::from("b")));
        assert_eq!(lexical(Path::new("../b")), None);
    }
}
