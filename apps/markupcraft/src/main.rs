//! MarkupCraft desktop app: an eframe window (wgpu) around `markupcraft-ui-egui`.
//!
//! `markupcraft [file.pdf ...] [--page N] [--zoom 150|fit-page|fit-width] [--mode single] ...`
//! opens the files given; `--key value` pairs are the same options the headless screenshot
//! example takes.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
// No console window behind the app on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use markupcraft_ui_egui::MarkupCraftApp;

/// The window and taskbar icon (contributor-original, `assets/logo-512.png`).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/logo-512.png");

/// Files to open and `--key value` options from the command line.
fn parse_args(args: impl Iterator<Item = String>) -> (Vec<PathBuf>, Vec<(String, String)>) {
    let (mut files, mut opts) = (Vec::new(), Vec::new());
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        match a.strip_prefix("--") {
            Some(k) => {
                let v = args.next_if(|n| !n.starts_with("--")).unwrap_or_default();
                opts.push((k.to_string(), v));
            }
            None => files.push(PathBuf::from(a)),
        }
    }
    (files, opts)
}

fn main() -> eframe::Result {
    let (files, opts) = parse_args(std::env::args().skip(1));
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("MarkupCraft")
        .with_app_id("markupcraft")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([720.0, 480.0]);
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    let mut native = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // D3D12 is the best-supported backend on Windows; some machines carry broken Vulkan layers.
    if cfg!(target_os = "windows")
        && std::env::var_os("WGPU_BACKEND").is_none()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native.wgpu_options.wgpu_setup
    {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    eframe::run_native(
        "MarkupCraft",
        native,
        Box::new(move |_cc| {
            let mut app = MarkupCraftApp::with_user_settings();
            for f in &files {
                app.open_path(f);
            }
            for (k, v) in &opts {
                app.set_option(k, v);
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split_into_files_and_options() {
        let args = ["a.pdf", "--page", "3", "--hide-markups", "--zoom", "fit-width", "b.pdf"].map(String::from);
        let (files, opts) = parse_args(args.into_iter());
        assert_eq!(files, vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]);
        assert_eq!(
            opts,
            vec![
                ("page".to_string(), "3".to_string()),
                ("hide-markups".to_string(), String::new()),
                ("zoom".to_string(), "fit-width".to_string())
            ]
        );
    }

    #[test]
    fn the_icon_decodes() {
        assert!(eframe::icon_data::from_png_bytes(APP_ICON_PNG).is_ok());
    }
}
