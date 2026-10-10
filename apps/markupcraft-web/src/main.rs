//! MarkupCraft in the browser: the desktop app's interface (`markupcraft-ui-egui`) on a canvas.
//!
//! Build with `cargo xtask web` (or `trunk build --release` in this folder); the site lands in
//! `dist-web/`. Files come from the browser's file picker, drag and drop, or a `?file=<url>`
//! link (same origin or CORS-enabled); Save and every export are downloads; settings live in the
//! browser's storage (`markupcraft_ui_egui::browser`). Pages render on the interface thread
//! (the browser build has no worker threads).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;
    use markupcraft_ui_egui::{MarkupCraftApp, browser};

    let _ = eframe::WebLogger::init(log::LevelFilter::Warn);
    // Before anything reads settings: files and settings go through the browser.
    browser::install();
    let options = eframe::WebOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    wasm_bindgen_futures::spawn_local(async move {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(canvas) = document
            .get_element_by_id("markupcraft")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    browser::set_context(&cc.egui_ctx);
                    let app = MarkupCraftApp::with_user_settings();
                    if let Some(url) = query_param("file") {
                        wasm_bindgen_futures::spawn_local(async move {
                            let name = file_name_of(&url);
                            let r = fetch_bytes(&url).await;
                            if let Err(e) = &r {
                                log::warn!("could not fetch {url}: {e}");
                            }
                            browser::arrive(&name, r);
                        });
                    }
                    Ok(Box::new(app))
                }),
            )
            .await;
        match started {
            // The page shows a loading note until the app is up.
            Ok(()) => {
                if let Some(e) = document.get_element_by_id("loading") {
                    e.remove();
                }
            }
            Err(e) => {
                log::error!("MarkupCraft could not start: {e:?}");
                if let Some(e) = document.get_element_by_id("loading") {
                    e.set_text_content(Some("MarkupCraft could not start in this browser (it needs WebGL 2)."));
                }
            }
        }
    });
}

/// The last path component of a URL, without its query: the name the document gets.
#[cfg(target_arch = "wasm32")]
fn file_name_of(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .filter(|n| !n.is_empty())
        .unwrap_or("document.pdf")
        .to_string()
}

#[cfg(target_arch = "wasm32")]
fn query_param(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()?.get(key)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use eframe::wasm_bindgen::JsCast;
    let window = web_sys::window().ok_or("no window")?;
    let resp = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "not a response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let buf = resp.array_buffer().map_err(|e| format!("{e:?}"))?;
    let buf = wasm_bindgen_futures::JsFuture::from(buf)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "markupcraft-web is the browser build: run `cargo xtask web` (or `trunk build --release` \
         in apps/markupcraft-web) and serve dist-web/"
    );
}
