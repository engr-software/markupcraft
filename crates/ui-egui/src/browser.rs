//! The browser build's files (wasm32 only).
//!
//! A browser has no file system, so [`install`] redirects `markupcraft_revu::fsio` (every read
//! and write the app and the engine make through it):
//!
//! - files the user picked or dropped are kept in memory under a made-up path
//!   (`/picked/<n>/<name>`), so everything that opens a path works on them;
//! - settings (everything under `fsio::BROWSER_CONFIG_DIR`) live in the browser's
//!   `localStorage`;
//! - temporary files (`fsio::BROWSER_TEMP_DIR`) are kept in memory;
//! - every other write (Save, Save As, exports) becomes a download of that file name, and is
//!   kept in memory so it can be read back in this session.
//!
//! Picked and dropped files arrive asynchronously; [`take_arrived`] hands them to the frame loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use eframe::wasm_bindgen::JsCast;
use markupcraft_revu::fsio;

thread_local! {
    /// Files picked, dropped or written in this session, by path.
    static FILES: RefCell<HashMap<PathBuf, Vec<u8>>> = RefCell::new(HashMap::new());
    /// Files that arrived from a drop: (name, contents or the browser's error).
    static ARRIVED: RefCell<Vec<(String, Result<PathBuf, String>)>> = const { RefCell::new(Vec::new()) };
    /// The interface, to wake when a file arrives.
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    static COUNTER: RefCell<u64> = const { RefCell::new(0) };
}

/// The largest file kept from a pick or drop (1 GiB), so one bad drop cannot exhaust the
/// browser's memory.
const MAX_FILE: usize = 1 << 30;

/// Install the browser host. Call once, before the app reads its settings.
pub fn install() {
    fsio::install(fsio::Host { read, write, remove });
}

/// Remember the interface's context so async arrivals can wake it.
pub fn set_context(ctx: &egui::Context) {
    CTX.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(ctx.clone());
        }
    });
}

fn wake() {
    CTX.with(|c| {
        if let Some(ctx) = c.borrow().as_ref() {
            ctx.request_repaint();
        }
    });
}

/// Keep `bytes` as a file named `name`; returns the path that opens it.
pub fn remember(name: &str, bytes: Vec<u8>) -> PathBuf {
    let n = COUNTER.with(|c| {
        let mut c = c.borrow_mut();
        *c += 1;
        *c
    });
    // Only the last component of what the browser calls the file: never a path of its own.
    let name = Path::new(name)
        .file_name()
        .map_or_else(|| "document.pdf".to_string(), |n| n.to_string_lossy().into_owned());
    let path = PathBuf::from(format!("/picked/{n}")).join(name);
    FILES.with(|f| f.borrow_mut().insert(path.clone(), bytes));
    path
}

/// Files dropped on the window since the last call: (name, path to open or the error).
pub fn take_arrived() -> Vec<(String, Result<PathBuf, String>)> {
    ARRIVED.with(|a| std::mem::take(&mut *a.borrow_mut()))
}

/// A file that arrived some other way (the web app's `?file=` link): opened on the next frame
/// like a drop.
pub fn arrive(name: &str, r: Result<Vec<u8>, String>) {
    let r = match r {
        Ok(b) if b.len() > MAX_FILE => Err("the file is too large to open in the browser".to_string()),
        Ok(b) => Ok(remember(name, b)),
        Err(e) => Err(e),
    };
    ARRIVED.with(|a| a.borrow_mut().push((name.to_string(), r)));
    wake();
}

/// Read dropped files (the browser reads them asynchronously); they arrive in [`take_arrived`].
pub fn read_dropped(files: Vec<egui::DroppedFileHandle>) {
    for f in files {
        wasm_bindgen_futures::spawn_local(async move {
            let name = f
                .path()
                .file_name()
                .map_or_else(|| "dropped.pdf".to_string(), |n| n.to_string_lossy().into_owned());
            arrive(&name, f.bytes_async().await);
        });
    }
}

/// Ask the browser for files (its file picker); answers go to `tx` as paths [`remember`]ed.
pub fn pick(filter: crate::dialogs::Filter, many: bool, tx: std::sync::mpsc::Sender<Vec<PathBuf>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let d = rfd::AsyncFileDialog::new().add_filter(filter.0, filter.1);
        let handles = if many {
            d.pick_files().await.unwrap_or_default()
        } else {
            d.pick_file().await.into_iter().collect()
        };
        let mut out = Vec::new();
        for h in handles {
            let bytes = h.read().await;
            if bytes.len() <= MAX_FILE {
                out.push(remember(&h.file_name(), bytes));
            }
        }
        let _ = tx.send(out);
        wake();
    });
}

/// The browser tab's title.
pub fn set_title(title: &str) {
    if let Some(d) = web_sys::window().and_then(|w| w.document()) {
        d.set_title(title);
    }
}

/// Where a document saved in the browser goes: a download named `name`.
pub fn download_path(name: &str) -> PathBuf {
    let name = Path::new(name)
        .file_name()
        .map_or_else(|| "document.pdf".to_string(), |n| n.to_string_lossy().into_owned());
    PathBuf::from("/downloads").join(name)
}

fn is_setting(path: &Path) -> bool {
    path.starts_with(fsio::BROWSER_CONFIG_DIR)
}

fn storage() -> io::Result<web_sys::Storage> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "the browser's storage is not available"))
}

fn key(path: &Path) -> String {
    format!("markupcraft:{}", path.to_string_lossy().replace('\\', "/"))
}

/// Settings are stored as text when they are UTF-8 (`t` + text), else as hex (`x` + hex).
fn encode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(t) => format!("t{t}"),
        Err(_) => {
            let mut s = String::with_capacity(1 + bytes.len() * 2);
            s.push('x');
            for b in bytes {
                s.push_str(&format!("{b:02x}"));
            }
            s
        }
    }
}

fn decode(s: &str) -> Option<Vec<u8>> {
    if let Some(t) = s.strip_prefix('t') {
        return Some(t.as_bytes().to_vec());
    }
    let hex = s.strip_prefix('x')?;
    let digits = hex.as_bytes();
    if digits.len() % 2 != 0 {
        return None;
    }
    digits
        .chunks(2)
        .map(|p| std::str::from_utf8(p).ok().and_then(|p| u8::from_str_radix(p, 16).ok()))
        .collect()
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} is not in the browser", path.display()),
    )
}

fn read(path: &Path) -> Option<io::Result<Vec<u8>>> {
    if is_setting(path) {
        let r = storage().and_then(|s| {
            s.get_item(&key(path))
                .ok()
                .flatten()
                .and_then(|v| decode(&v))
                .ok_or_else(|| not_found(path))
        });
        return Some(r);
    }
    if let Some(b) = FILES.with(|f| f.borrow().get(path).cloned()) {
        return Some(Ok(b));
    }
    Some(Err(not_found(path)))
}

fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if is_setting(path) {
        return storage()?
            .set_item(&key(path), &encode(bytes))
            .map_err(|_| io::Error::other("the browser's storage is full"));
    }
    // Temporary files stay in memory; everything else is something the user saves.
    if !path.starts_with(fsio::BROWSER_TEMP_DIR) {
        let name = path
            .file_name()
            .map_or_else(|| "download".to_string(), |n| n.to_string_lossy().into_owned());
        download(&name, bytes)?;
    }
    FILES.with(|f| f.borrow_mut().insert(path.to_path_buf(), bytes.to_vec()));
    Ok(())
}

fn remove(path: &Path) -> io::Result<()> {
    if is_setting(path) {
        return storage()?
            .remove_item(&key(path))
            .map_err(|_| io::Error::other("the browser's storage refused"));
    }
    FILES.with(|f| f.borrow_mut().remove(path));
    Ok(())
}

/// Hand `bytes` to the browser as a download named `name`.
fn download(name: &str, bytes: &[u8]) -> io::Result<()> {
    let fail = |what: &str| io::Error::other(format!("could not download {name}: {what}"));
    let window = web_sys::window().ok_or_else(|| fail("no window"))?;
    let document = window.document().ok_or_else(|| fail("no document"))?;
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array.buffer());
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime(name));
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &opts)
        .map_err(|_| fail("the browser refused the data"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| fail("no object URL"))?;
    let anchor = document
        .create_element("a")
        .ok()
        .and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok())
        .ok_or_else(|| fail("no link element"))?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

fn mime(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "xml" | "xfdf" => "application/xml",
        "json" | "mctools" => "application/json",
        "png" => "image/png",
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}
