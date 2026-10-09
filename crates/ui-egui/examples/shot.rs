//! Headless screenshot of the real MarkupCraft shell (egui_kittest + wgpu, no window).
//!
//! ```text
//! cargo run -p markupcraft-ui-egui --example shot -- out.png [file.pdf] [--size 1600x1000] [--scale 1] [--page 2 --zoom 150 --select 0 ...]
//! ```
//! Without a file it opens the built-in synthetic drawing set. `--width N` downscales the
//! image. Any other `--key value` goes to `MarkupCraftApp::set_option` (page, zoom, mode,
//! tool, select, panel, hide-markups, wheel, snap, group-by, columns).
//!
//! Input, in order, after the first render (points are PDF user space on the current page):
//! `--click X,Y`, `--rclick X,Y`, `--dblclick X,Y`, `--hover X,Y`, `--drag X,Y,X2,Y2`,
//! `--key Enter|Escape|Backspace`, `--type TEXT`, `--then-tool ID` (switch tools between
//! actions), `--run COMMAND` (a command id such as `markup.add_to_toolchest`).

use std::time::{Duration, Instant};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use markupcraft_ui_egui::MarkupCraftApp;

/// The screen position of a user-space point on the current page.
fn at(h: &Harness<'_, MarkupCraftApp>, v: &str) -> Option<Pos2> {
    let n: Vec<f64> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    let d = h.state().state.doc()?;
    let page = d.view.current;
    let pages = d.render.as_ref()?.pages();
    d.view
        .user_to_screen(page, markupcraft_geom::Point::new(*n.first()?, *n.get(1)?), pages)
}

fn press(h: &mut Harness<'_, MarkupCraftApp>, p: Pos2, b: PointerButton, down: bool) {
    h.event(Event::PointerButton {
        pos: p,
        button: b,
        pressed: down,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

/// Run one input action.
fn act(h: &mut Harness<'_, MarkupCraftApp>, k: &str, v: &str) -> Result<(), String> {
    let bad = || format!("--{k} {v}: expected X,Y in page points");
    match k {
        "click" | "rclick" | "dblclick" | "hover" => {
            let p = at(h, v).ok_or_else(bad)?;
            h.hover_at(p);
            h.step();
            let b = if k == "rclick" {
                PointerButton::Secondary
            } else {
                PointerButton::Primary
            };
            let n = match k {
                "hover" => 0,
                "dblclick" => 2,
                _ => 1,
            };
            for _ in 0..n {
                press(h, p, b, true);
                press(h, p, b, false);
            }
            h.run_steps(if n == 1 { 20 } else { 3 });
        }
        "drag" => {
            let n: Vec<&str> = v.split(',').collect();
            let (a, b) = match n.as_slice() {
                [x, y, x2, y2] => (at(h, &format!("{x},{y}")), at(h, &format!("{x2},{y2}"))),
                _ => (None, None),
            };
            let (a, b) = (a.ok_or_else(bad)?, b.ok_or_else(bad)?);
            h.hover_at(a);
            h.step();
            press(h, a, PointerButton::Primary, true);
            for i in 1..=10 {
                h.hover_at(a + (b - a) * (i as f32 / 10.0));
                h.step();
            }
            press(h, b, PointerButton::Primary, false);
            h.run_steps(3);
        }
        "key" => {
            // `Enter`, `ctrl+V`, `shift+alt+A`
            let mut m = Modifiers::NONE;
            let mut name = v;
            while let Some((pre, rest)) = name.split_once('+') {
                match pre.to_ascii_lowercase().as_str() {
                    "ctrl" | "cmd" => m.command = true,
                    "shift" => m.shift = true,
                    "alt" => m.alt = true,
                    _ => return Err(format!("--key {v}")),
                }
                name = rest;
            }
            let key = egui::Key::from_name(name).ok_or(format!("--key {v}"))?;
            h.key_press_modifiers(m, key);
            h.run_steps(3);
        }
        "type" => {
            h.event(Event::Text(v.to_string()));
            h.run_steps(3);
        }
        "then-tool" => {
            h.state_mut().set_option("tool", v);
            h.run_steps(3);
        }
        "run" => {
            h.state_mut().state.queue(v);
            h.run_steps(3);
        }
        _ => return Err(format!("unknown action --{k}")),
    }
    Ok(())
}

/// kittest's default prefers a CPU adapter; on some Windows machines that is WARP on D3D12,
/// which crashes in this headless mode. Prefer a hardware Vulkan, then D3D12/Metal, then GL,
/// then whatever exists (`WGPU_BACKEND` still narrows the list).
fn gpu_setup() -> eframe::egui_wgpu::WgpuSetup {
    use eframe::wgpu;
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup
        .instance_descriptor
        .backends
        .remove(wgpu::Backends::BROWSER_WEBGPU);
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _surface| {
        let rank = |a: &wgpu::Adapter| {
            let info = a.get_info();
            let backend = match info.backend {
                wgpu::Backend::Vulkan => 0,
                wgpu::Backend::Metal | wgpu::Backend::Dx12 => 1,
                wgpu::Backend::Gl => 2,
                _ => 3,
            };
            (info.device_type == wgpu::DeviceType::Cpu, backend)
        };
        adapters
            .iter()
            .min_by_key(|a| rank(a))
            .cloned()
            .ok_or_else(|| "no GPU adapter".to_string())
    }));
    eframe::egui_wgpu::WgpuSetup::CreateNew(setup)
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .ok_or("usage: shot <out.png> [file.pdf] [--option value ...]")?;
    let (mut file, mut opts) = (None, Vec::new());
    let mut actions: Vec<(String, String)> = Vec::new();
    let (mut size, mut scale) = (egui::vec2(1600.0, 1000.0), 1.0f32);
    let mut width: Option<u32> = None;
    while let Some(a) = args.next() {
        match a.strip_prefix("--") {
            Some("size") => {
                let v = args.next().unwrap_or_default();
                let (w, h) = v.split_once('x').ok_or("--size WxH")?;
                size = egui::vec2(
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            Some("scale") => scale = args.next().and_then(|v| v.parse().ok()).ok_or("bad --scale")?,
            Some("width") => width = Some(args.next().and_then(|v| v.parse().ok()).ok_or("bad --width")?),
            Some(k @ ("click" | "rclick" | "dblclick" | "hover" | "drag" | "key" | "type" | "then-tool" | "run")) => {
                actions.push((k.to_string(), args.next().unwrap_or_default()));
            }
            Some(k) => opts.push((k.to_string(), args.next().unwrap_or_default())),
            None => file = Some(a),
        }
    }
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(scale)
        .with_step_dt(1.0 / 60.0)
        .wgpu_setup(gpu_setup())
        .build_eframe(move |_cc| {
            let mut app = MarkupCraftApp::new();
            let r = match &file {
                Some(f) => {
                    let bytes = std::fs::read(f).map_err(|e| e.to_string());
                    let name = std::path::Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    bytes.and_then(|b| app.open_bytes(&name, Some(f.into()), b))
                }
                None => app.open_bytes("Sample Plan.pdf", None, markupcraft_render::synthetic::sample_pdf()),
            };
            if let Err(e) = r {
                eprintln!("shot: {e}");
            }
            for (k, v) in &opts {
                app.set_option(k, v);
            }
            app
        });
    // Let layout settle and background renders arrive.
    let start = Instant::now();
    harness.run_steps(4);
    while start.elapsed() < Duration::from_secs(60) {
        harness.run_steps(2);
        if !harness.state().render_pending() {
            harness.run_steps(3);
            if !harness.state().render_pending() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    if !actions.is_empty() {
        for (k, v) in &actions {
            act(&mut harness, k, v)?;
        }
        harness.run_steps(4);
    }
    if harness.state().render_pending() {
        eprintln!("shot: some pages were still rendering");
    }
    let mut image = harness.render()?;
    if let Some(w) = width.filter(|w| *w < image.width()) {
        let h = (image.height() as f64 * w as f64 / image.width() as f64).round() as u32;
        image = image::imageops::resize(&image, w, h, image::imageops::FilterType::Lanczos3);
    }
    image.save(&out).map_err(|e| e.to_string())?;
    eprintln!("shot: wrote {out} ({}x{})", image.width(), image.height());
    Ok(())
}
