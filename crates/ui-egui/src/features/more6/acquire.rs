//! Scanners and cameras: Markup > Image From Scanner (Shift+I) and Camera (Ctrl+Alt+I),
//! Document > Insert from Scanner or Camera, File > Create from Scanner or Camera. Pages are
//! acquired one after another (Scan / Scan Next, Take Picture), then Finish puts them where the
//! command said: an image markup, pages after the current one, or a new PDF.
//!
//! Scanners are network scanners reached over eSCL (AirScan), on a worker thread. A camera is a
//! `markupcraft_engine::devices::Camera`: the system camera when the app is built with the
//! `camera` feature, else none (tests and headless runs supply a test camera).

use std::path::Path;
use std::sync::mpsc::{Receiver, channel};

use markupcraft_engine::devices::{Camera, EsclScanner, ScanSettings, ScannerCaps, pdf_from_images};
use markupcraft_engine::stamps::StampPlace;
use markupcraft_geom::Point;

use super::{Ask6, Pick6, doc_step};
use crate::AppState;
use crate::dialogs::Purpose;
use crate::features::{Ask, start_pick};

/// Where acquired pages go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    ImageMarkup,
    InsertPages,
    NewPdf,
}

type ScanJob = Receiver<Result<(Option<ScannerCaps>, Vec<Vec<u8>>), String>>;

#[derive(Default)]
pub struct AcquireState {
    pub open: bool,
    pub target: Target,
    /// The Camera tab is showing (else the Scanner tab).
    pub camera_tab: bool,
    pub scanner_url: String,
    pub caps: Option<ScannerCaps>,
    pub source: String,
    pub resolution: u32,
    pub color_mode: String,
    pub job: Option<ScanJob>,
    /// Pages acquired so far (image or PDF bytes).
    pub acquired: Vec<Vec<u8>>,
    /// The camera (see the module notes).
    pub camera: Option<Box<dyn Camera>>,
    pub camera_index: usize,
    /// Show OCR after creating a PDF.
    pub ocr_after: bool,
    pub message: String,
}

impl AcquireState {
    pub fn open_for(&mut self, target: Target) {
        self.open = true;
        self.target = target;
        self.camera_tab = false;
        self.acquired.clear();
        self.message.clear();
        if self.camera.is_none() {
            self.camera = system_camera();
        }
    }
}

/// The system camera, when this build has one.
#[cfg(all(feature = "camera", not(target_arch = "wasm32")))]
fn system_camera() -> Option<Box<dyn Camera>> {
    crate::camera::SystemCamera::new().map(|c| Box::new(c) as Box<dyn Camera>)
}

#[cfg(not(all(feature = "camera", not(target_arch = "wasm32"))))]
fn system_camera() -> Option<Box<dyn Camera>> {
    None
}

/// Shrink an acquired image whose longest side passes `max` (Preferences > Import/Export).
fn limit_size(bytes: Vec<u8>, max: u32) -> Vec<u8> {
    markupcraft_engine::devices::limit_image_side(&bytes, max).unwrap_or(bytes)
}

/// Start a scan on a worker thread.
fn start_scan(app: &mut AppState, caps_only: bool) {
    let st = &mut app.features.more6.acquire;
    let url = st.scanner_url.trim().to_string();
    let settings = ScanSettings {
        source: if st.source.is_empty() {
            "Platen".into()
        } else {
            st.source.clone()
        },
        resolution: st.resolution.max(50),
        color_mode: if st.color_mode.is_empty() {
            "RGB24".into()
        } else {
            st.color_mode.clone()
        },
        format: "image/jpeg".into(),
    };
    let scanner = match EsclScanner::new(&url) {
        Ok(s) => s,
        Err(e) => {
            st.message = e.to_string();
            return;
        }
    };
    // Remember the scanner for next time.
    if app.shell.prefs.more.import_export.scanner_url != url {
        app.shell.prefs.more.import_export.scanner_url = url;
        app.shell.save_prefs();
    }
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new().name("scan".into()).spawn(move || {
        let r = if caps_only {
            scanner.capabilities().map(|c| (Some(c), Vec::new()))
        } else {
            scanner.scan(&settings).map(|p| (None, p))
        };
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    let st = &mut app.features.more6.acquire;
    match spawned {
        Ok(_) => {
            st.job = Some(rx);
            st.message = if caps_only {
                "Asking the scanner..."
            } else {
                "Scanning..."
            }
            .into();
        }
        Err(e) => st.message = e.to_string(),
    }
}

/// Take a picture with the camera.
fn take_picture(app: &mut AppState) {
    let max = app.shell.prefs.more.import_export.capture_max_side;
    let st = &mut app.features.more6.acquire;
    let Some(cam) = st.camera.as_mut() else {
        st.message = "No camera is available in this build (build MarkupCraft with its camera feature).".into();
        return;
    };
    match cam.capture(st.camera_index).and_then(|f| f.png()) {
        Ok(png) => {
            st.acquired.push(limit_size(png, max));
            st.message = format!("{} taken", crate::actions::plural(st.acquired.len(), "picture"));
        }
        Err(e) => st.message = e.to_string(),
    }
}

/// Finish: put the acquired pages where the command said.
fn finish(app: &mut AppState) {
    let st = &mut app.features.more6.acquire;
    if st.acquired.is_empty() {
        st.message = "Scan or take a picture first".into();
        return;
    }
    match st.target {
        Target::ImageMarkup => {
            st.open = false;
            start_pick(
                app,
                Pick6::AcquiredImage.into(),
                "Click where the image goes, or drag its box",
            );
        }
        Target::InsertPages => {
            let images = std::mem::take(&mut st.acquired);
            st.open = false;
            let at = app.doc().map_or(0, |d| d.view.current + 1);
            doc_step(app, |s| {
                s.insert_acquired_pages(at, &images)
                    .map(|n| format!("Inserted {}", crate::actions::plural(n, "page")))
            });
        }
        Target::NewPdf => app.dialogs.save(
            Purpose::Feature(Ask::More6(Ask6::AcquireOut)),
            crate::dialogs::PDF,
            "Scanned.pdf",
        ),
    }
}

/// File > Create from Scanner or Camera: the new PDF's file was chosen.
pub fn write_pdf(app: &mut AppState, out: &Path) {
    let images = std::mem::take(&mut app.features.more6.acquire.acquired);
    match pdf_from_images(&images, out) {
        Ok(n) => {
            app.features.more6.acquire.open = false;
            super::open_written(app, out);
            app.status = format!("Created {} with {}", out.display(), crate::actions::plural(n, "page"));
            if app.features.more6.acquire.ocr_after {
                app.features.ocr.open = true;
            }
        }
        Err(e) => {
            app.features.more6.acquire.acquired = images;
            app.features.more6.acquire.message = e.to_string();
        }
    }
}

/// Image From Scanner / Camera: the image's place was picked.
pub fn image_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(first) = app.features.more6.acquire.acquired.first().cloned() else {
        return;
    };
    let place = match crate::features::rect_of(pts) {
        Some(r) => StampPlace::Rect(r),
        None => match pts.first() {
            Some(p) => StampPlace::Center(*p),
            None => return,
        },
    };
    app.features.more6.acquire.acquired.clear();
    doc_step(app, |s| {
        s.add_acquired_image(page, place, &first)
            .map(|_| "Image placed".to_string())
    });
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    // A finished scan.
    let done = app
        .features
        .more6
        .acquire
        .job
        .as_ref()
        .and_then(|rx| rx.try_recv().ok());
    if let Some(r) = done {
        let max = app.shell.prefs.more.import_export.capture_max_side;
        let st = &mut app.features.more6.acquire;
        st.job = None;
        match r {
            Ok((Some(caps), _)) => {
                st.message = format!(
                    "{} ready",
                    if caps.make_and_model.is_empty() {
                        "The scanner"
                    } else {
                        &caps.make_and_model
                    }
                );
                if let Some(s) = caps.sources.first() {
                    st.source = s.clone();
                }
                st.caps = Some(caps);
            }
            Ok((None, pages)) => {
                let n = pages.len();
                st.acquired.extend(pages.into_iter().map(|p| limit_size(p, max)));
                st.message = format!(
                    "Scanned {} ({} in all)",
                    crate::actions::plural(n, "page"),
                    st.acquired.len()
                );
            }
            Err(e) => st.message = e,
        }
    }
    if app.features.more6.acquire.job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    if !app.features.more6.acquire.open {
        return;
    }
    // Preferences fill in the scanner and resolution the first time.
    let ie = app.shell.prefs.more.import_export.clone();
    let st = &mut app.features.more6.acquire;
    if st.scanner_url.is_empty() {
        st.scanner_url = ie.scanner_url.clone();
    }
    if st.resolution == 0 {
        st.resolution = ie.scan_dpi;
    }
    let title = match st.target {
        Target::ImageMarkup => "Image From Scanner or Camera",
        Target::InsertPages => "Insert from Scanner or Camera",
        Target::NewPdf => "Create from Scanner or Camera",
    };
    let mut open = true;
    let (mut caps, mut scan, mut picture, mut done, mut clear) = (false, false, false, false, false);
    egui::Window::new(title)
        .id(egui::Id::new("acquire"))
        .open(&mut open)
        .default_width(460.0)
        .default_pos(egui::pos2(620.0, 260.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut st.camera_tab, false, "Scanner");
                ui.selectable_value(&mut st.camera_tab, true, "Camera");
            });
            ui.separator();
            if st.camera_tab {
                match st.camera.as_mut() {
                    Some(cam) => {
                        let devices = cam.devices();
                        egui::ComboBox::from_label("Camera")
                            .selected_text(devices.get(st.camera_index).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, d) in devices.iter().enumerate() {
                                    ui.selectable_value(&mut st.camera_index, i, d);
                                }
                            });
                        picture = ui.button("Take Picture").clicked();
                    }
                    None => {
                        ui.label("No camera is available in this build. Build MarkupCraft with its camera feature to use the system camera, or scan instead.");
                    }
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("Scanner (eSCL)");
                    ui.add(
                        egui::TextEdit::singleline(&mut st.scanner_url)
                            .id(egui::Id::new("scanner-url"))
                            .desired_width(240.0)
                            .hint_text("http://192.168.1.20/eSCL"),
                    );
                    caps = ui.button("Connect").clicked();
                });
                if let Some(c) = &st.caps {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_label("Source")
                            .selected_text(st.source.clone())
                            .show_ui(ui, |ui| {
                                for s in &c.sources {
                                    ui.selectable_value(&mut st.source, s.clone(), s);
                                }
                            });
                        egui::ComboBox::from_label("Color")
                            .selected_text(st.color_mode.clone())
                            .show_ui(ui, |ui| {
                                for m in &c.color_modes {
                                    ui.selectable_value(&mut st.color_mode, m.clone(), m);
                                }
                            });
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Resolution");
                    ui.add(egui::DragValue::new(&mut st.resolution).range(50..=1200).suffix(" dpi"));
                });
                ui.horizontal(|ui| {
                    let label = if st.acquired.is_empty() { "Scan" } else { "Scan Next" };
                    scan = ui.add_enabled(st.job.is_none(), egui::Button::new(label)).clicked();
                    if st.job.is_some() {
                        ui.spinner();
                    }
                });
                ui.weak("Network scanners that speak eSCL (AirScan / Mopria) over http.");
            }
            ui.separator();
            ui.label(format!("Acquired: {}", crate::actions::plural(st.acquired.len(), "page")));
            if st.target == Target::NewPdf {
                ui.checkbox(&mut st.ocr_after, "Run OCR on the new PDF");
            }
            ui.horizontal(|ui| {
                done = ui
                    .add_enabled(!st.acquired.is_empty(), egui::Button::new("Finish"))
                    .clicked();
                clear = ui.button("Start Over").clicked();
            });
            if !st.message.is_empty() {
                ui.label(&st.message);
            }
        });
    app.features.more6.acquire.open = open;
    if clear {
        app.features.more6.acquire.acquired.clear();
    }
    if caps {
        start_scan(app, true);
    }
    if scan {
        start_scan(app, false);
    }
    if picture {
        take_picture(app);
    }
    if done {
        finish(app);
    }
}
