//! File reads and writes that a host can redirect.
//!
//! The desktop app reads and writes the file system directly. The browser build has no file
//! system: it installs a [`Host`] once at startup that serves files the user picked from memory,
//! keeps settings in the browser's storage and turns every other write into a download. With no
//! host installed (desktop, CLI, tests) these are plain `std::fs` calls.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where files go when there is no file system.
#[derive(Clone, Copy)]
pub struct Host {
    /// Read a file: `None` falls through to the file system.
    pub read: fn(&Path) -> Option<io::Result<Vec<u8>>>,
    /// Write a whole file.
    pub write: fn(&Path, &[u8]) -> io::Result<()>,
    /// Delete a file.
    pub remove: fn(&Path) -> io::Result<()>,
}

/// Where the browser build keeps its settings (a key prefix in the browser's storage, not a
/// folder on disk).
pub const BROWSER_CONFIG_DIR: &str = "/markupcraft-settings";

/// The browser build's temporary folder (kept in memory, never downloaded).
pub const BROWSER_TEMP_DIR: &str = "/markupcraft-temp";

static HOST: OnceLock<Host> = OnceLock::new();

/// Redirect reads and writes for the rest of the process. The first host wins; false when one
/// was already installed.
pub fn install(host: Host) -> bool {
    HOST.set(host).is_ok()
}

/// A host is installed (the browser build).
pub fn hosted() -> bool {
    HOST.get().is_some()
}

/// Read a whole file.
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    if let Some(r) = HOST.get().and_then(|h| (h.read)(path)) {
        return r;
    }
    std::fs::read(path)
}

/// A file's size in bytes.
pub fn len(path: impl AsRef<Path>) -> io::Result<u64> {
    let path = path.as_ref();
    if hosted() {
        return read(path).map(|b| b.len() as u64);
    }
    std::fs::metadata(path).map(|m| m.len())
}

/// The file exists.
pub fn exists(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    if hosted() {
        return read(path).is_ok();
    }
    path.exists()
}

/// Create a folder and its parents (nothing to do under a host: it has no folders).
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    if hosted() {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// Delete a file.
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(h) = HOST.get() {
        return (h.remove)(path);
    }
    std::fs::remove_file(path)
}

/// The folder for temporary files. The browser build has none: a made-up path whose files the
/// host keeps in memory (`std::env::temp_dir` would stop the program there).
pub fn temp_dir() -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        return PathBuf::from(BROWSER_TEMP_DIR);
    }
    std::env::temp_dir()
}

/// This process's id (0 in the browser, which has no processes), for unique temporary names.
pub fn process_id() -> u32 {
    if cfg!(target_arch = "wasm32") {
        return 0;
    }
    std::process::id()
}

/// Wait a little. The browser cannot block its only thread: there this returns at once, so
/// callers that poll must have a deadline (they all do).
pub fn sleep(d: std::time::Duration) {
    if !cfg!(target_arch = "wasm32") {
        std::thread::sleep(d);
    }
}

/// Read a whole file as UTF-8 text.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let bytes = read(path)?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write `bytes` to `path` through a temporary file and a rename (creating the folder), so a
/// failed write never leaves half a file where the old one was.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(h) = HOST.get() {
        return (h.write)(path, bytes);
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".markupcraft-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back_without_a_host() {
        let dir = std::env::temp_dir().join(format!("markupcraft-fsio-{}", std::process::id()));
        let p = dir.join("sub").join("a.txt");
        write_atomic(&p, b"hello").unwrap();
        assert_eq!(read_to_string(&p).unwrap(), "hello");
        assert!(!hosted());
        assert_eq!(len(&p).unwrap(), 5);
        assert!(exists(&p));
        remove_file(&p).unwrap();
        assert!(!exists(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
