//! The Stamp library (built-in designs and the user's text and image stamps, with dynamic
//! fields such as date, user and page label), Markup > Image, and placing them on the page.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::RichText;
use markupcraft_engine::stamps::{StampEntry, StampLibrary, StampPlace, StampSource, stamp_prompts};
use markupcraft_geom::Point;
use markupcraft_model::Color;

use super::{Ask, Pick};
use crate::dialogs::Purpose;
use crate::{AppState, actions};

/// What the next click places.
#[derive(Debug, Clone, PartialEq)]
pub enum Placing {
    Library(String),
    Image(PathBuf),
}

pub struct StampsState {
    pub open: bool,
    /// The library folder (default `<config>/stamps`).
    pub dir: Option<PathBuf>,
    pub entries: Vec<StampEntry>,
    pub loaded: bool,
    pub chosen: Option<String>,
    /// Answers to the chosen stamp's `{prompt:...}` fields.
    pub answers: BTreeMap<String, String>,
    pub place: Option<Placing>,
    pub new_name: String,
    pub new_text: String,
    pub new_color: Color,
    pub message: String,
}

impl Default for StampsState {
    fn default() -> Self {
        Self {
            open: false,
            dir: None,
            entries: Vec::new(),
            loaded: false,
            chosen: None,
            answers: BTreeMap::new(),
            place: None,
            new_name: String::new(),
            new_text: "REVIEWED\nBy {user} on {date:MM/dd/yyyy}".into(),
            new_color: Color::rgb(0.0, 0.3, 0.8),
            message: String::new(),
        }
    }
}

impl StampsState {
    pub fn library(&self) -> Option<StampLibrary> {
        self.dir
            .clone()
            .or_else(|| crate::chest::config_dir().map(|d| d.join("stamps")))
            .map(StampLibrary::new)
    }

    pub fn reload(&mut self) {
        self.loaded = true;
        match self.library().map(|l| l.all()) {
            Some(Ok(v)) => self.entries = v,
            Some(Err(e)) => {
                self.entries = markupcraft_engine::stamps::builtin_stamps();
                self.message = e.to_string();
            }
            None => self.entries = markupcraft_engine::stamps::builtin_stamps(),
        }
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.stamps.open {
        return;
    }
    if !app.features.stamps.loaded {
        app.features.stamps.reload();
    }
    let (mut open, mut place, mut add_text, mut add_image, mut remove) = (true, false, false, false, None);
    super::window("Stamps")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            let s = &mut app.features.stamps;
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for e in &s.entries {
                    ui.horizontal(|ui| {
                        let color = markupcraft_engine::props::parse_color(&e.color).unwrap_or(Color::RED);
                        let tag = if e.image.is_some() {
                            "image"
                        } else if e.builtin {
                            "built-in"
                        } else {
                            "text"
                        };
                        if ui
                            .selectable_label(
                                s.chosen.as_deref() == Some(e.id.as_str()),
                                RichText::new(&e.name)
                                    .color(crate::theme::color32(&color, 1.0))
                                    .strong(),
                            )
                            .clicked()
                        {
                            s.chosen = Some(e.id.clone());
                            s.answers.clear();
                        }
                        ui.label(RichText::new(tag).small().weak());
                        if !e.builtin && ui.small_button("Remove").clicked() {
                            remove = Some(e.id.clone());
                        }
                    });
                }
            });
            if let Some(e) = s.chosen.as_ref().and_then(|id| s.entries.iter().find(|e| &e.id == id)) {
                let prompts = stamp_prompts(&e.text);
                if !prompts.is_empty() {
                    ui.label(RichText::new("Fields").strong());
                    for p in prompts {
                        ui.horizontal(|ui| {
                            ui.label(&p.label);
                            let v = s.answers.entry(p.label.clone()).or_insert(p.default.clone());
                            ui.text_edit_singleline(v);
                        });
                    }
                }
            }
            ui.add_enabled_ui(s.chosen.is_some(), |ui| {
                if ui.button("Place on Page").clicked() {
                    place = true;
                }
            });
            ui.separator();
            ui.label(RichText::new("New stamp").strong());
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut s.new_name);
                super::color_edit(ui, &mut s.new_color);
            });
            ui.add(egui::TextEdit::multiline(&mut s.new_text).desired_rows(2));
            ui.label(
                RichText::new("Fields: {user} {date:MM/dd/yyyy} {time} {page} {pages} {file} {prompt:Label}").small(),
            );
            ui.horizontal(|ui| {
                if ui.button("Add Text Stamp").clicked() {
                    add_text = true;
                }
                if ui.button("Add Image Stamp...").clicked() {
                    add_image = true;
                }
            });
            if !s.message.is_empty() {
                ui.label(RichText::new(&s.message).small());
            }
        });
    if !open {
        app.features.stamps.open = false;
    }
    if place && let Some(id) = app.features.stamps.chosen.clone() {
        app.features.stamps.place = Some(Placing::Library(id));
        // The page is the target: the library steps aside (it reopens from the menu).
        app.features.stamps.open = false;
        super::start_pick(app, Pick::Stamp, "Click to place the stamp, or drag its box");
    }
    if add_text {
        let s = &mut app.features.stamps;
        let name = if s.new_name.trim().is_empty() {
            "Custom".to_string()
        } else {
            s.new_name.trim().to_string()
        };
        let r = match s.library() {
            Some(l) => l.add_text(&name, &s.new_text, s.new_color),
            None => Err(markupcraft_engine::EngineError::Invalid(
                "no configuration folder".into(),
            )),
        };
        s.message = actions::report(r, |id| {
            s.chosen = Some(id);
            format!("Added {name}")
        });
        s.reload();
    }
    if add_image {
        app.dialogs
            .open(Purpose::Feature(Ask::StampImage), super::IMAGES, false);
    }
    if let Some(id) = remove {
        let s = &mut app.features.stamps;
        if let Some(l) = s.library() {
            s.message = actions::report(l.remove(&id), |_| "Removed".into());
        }
        s.reload();
    }
}

/// Add an image stamp to the library.
pub fn add_image(app: &mut AppState, path: &Path) {
    let s = &mut app.features.stamps;
    let name = if s.new_name.trim().is_empty() {
        path.file_stem()
            .map_or_else(|| "Image".to_string(), |n| n.to_string_lossy().into_owned())
    } else {
        s.new_name.trim().to_string()
    };
    match s.library() {
        Some(l) => {
            s.message = actions::report(l.add_image(&name, path), |id| {
                s.chosen = Some(id);
                format!("Added image stamp {name}")
            });
        }
        None => s.message = "No configuration folder for the stamp library".into(),
    }
    s.reload();
}

/// Where to place it was picked: a click (centre) or a box.
pub fn picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let at = match (super::rect_of(pts), pts.first()) {
        (Some(r), _) => StampPlace::Rect(r),
        (None, Some(p)) => StampPlace::Center(*p),
        _ => return,
    };
    let Some(placing) = app.features.stamps.place.take() else {
        return;
    };
    let source = match &placing {
        Placing::Library(id) => StampSource::Library(id.clone()),
        Placing::Image(p) => StampSource::File {
            path: p.clone(),
            page: 0,
        },
    };
    let lib = app.features.stamps.library();
    let answers = app.features.stamps.answers.clone();
    let Some(d) = app.doc_mut() else { return };
    let r = d.session.place_stamp(page, at, &source, lib.as_ref(), &answers, None);
    app.status = actions::report(r, |_| "Stamp placed".into());
}
