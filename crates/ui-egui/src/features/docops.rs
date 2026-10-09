//! Document operations in one tabbed dialog: Flatten, Reduce File Size, Security, Header &
//! Footer, Watermark, Bates Numbering and Attachments. Each applies to the active document
//! through the engine (undoable where the engine makes it so) and re-renders the pages.

use std::path::Path;

use egui::RichText;
use markupcraft_engine::docs_more::{
    HfTemplate, SecurityPreset, load_hf_templates, load_security_presets, save_hf_templates, save_security_presets,
};
use markupcraft_engine::flatten::FlattenFilter;
use markupcraft_engine::marks::{Bates, HeaderFooter, MarkKind, Slot, Watermark};
use markupcraft_engine::reduce::ReduceSettings;
use markupcraft_engine::security::{Encryption, Permissions, SecuritySettings};

use super::Ask;
use crate::dialogs::Purpose;
use crate::{AppState, actions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Flatten,
    Reduce,
    Security,
    HeaderFooter,
    Watermark,
    Bates,
    Attachments,
}

/// What a dialog button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Flatten,
    Reduce,
    Security,
    RemoveSecurity,
    HeaderFooter,
    RemoveHeaderFooter,
    SavePreset,
    ApplyPreset,
    SaveTemplate,
    ApplyTemplate,
    EditKept,
    UpdateKept,
    Watermark,
    RemoveWatermark,
    Bates,
}

const TABS: &[(Tab, &str)] = &[
    (Tab::Flatten, "Flatten"),
    (Tab::Reduce, "Reduce Size"),
    (Tab::Security, "Security"),
    (Tab::HeaderFooter, "Header & Footer"),
    (Tab::Watermark, "Watermark"),
    (Tab::Bates, "Bates"),
    (Tab::Attachments, "Attachments"),
];

pub struct DocOpsState {
    pub open: bool,
    pub tab: Tab,
    pub pages: String,
    pub message: String,
    // Flatten
    pub flatten_selected: bool,
    pub flatten_kinds: String,
    pub flatten_layers: String,
    // Reduce (runs on a worker thread)
    pub reduce: ReduceSettings,
    pub reducing: Option<(u64, std::sync::mpsc::Receiver<ReduceOutcome>)>,
    // Security
    pub open_password: String,
    pub permissions_password: String,
    pub permissions: Permissions,
    pub encryption: Encryption,
    // Marks
    pub hf: HeaderFooter,
    pub wm: Watermark,
    pub bates: Bates,
    pub bates_slot: usize,
    pub replace: bool,
    /// Where security presets and header/footer templates are kept (default: the config folder).
    pub presets_dir: Option<std::path::PathBuf>,
    pub preset_name: String,
    pub template_name: String,
    /// Shrink the page content so the header and footer do not overlap it.
    pub fit_content: bool,
}

impl Default for DocOpsState {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Flatten,
            pages: String::new(),
            message: String::new(),
            flatten_selected: false,
            flatten_kinds: String::new(),
            flatten_layers: String::new(),
            reduce: ReduceSettings::default(),
            reducing: None,
            open_password: String::new(),
            permissions_password: String::new(),
            permissions: Permissions::ALL,
            encryption: Encryption::default(),
            hf: HeaderFooter::default(),
            wm: Watermark::default(),
            bates: Bates::default(),
            bates_slot: 5,
            replace: true,
            presets_dir: None,
            preset_name: String::new(),
            template_name: String::new(),
            fit_content: false,
        }
    }
}

impl DocOpsState {
    pub fn open(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
        self.message.clear();
    }
}

const SLOTS: [(Slot, &str); 6] = [
    (Slot::HeaderLeft, "Header left"),
    (Slot::HeaderCenter, "Header centre"),
    (Slot::HeaderRight, "Header right"),
    (Slot::FooterLeft, "Footer left"),
    (Slot::FooterCenter, "Footer centre"),
    (Slot::FooterRight, "Footer right"),
];

fn presets_file(s: &DocOpsState, name: &str) -> Option<std::path::PathBuf> {
    s.presets_dir
        .clone()
        .or_else(crate::chest::config_dir)
        .map(|d| d.join(name))
}

fn security_presets(s: &DocOpsState) -> Vec<SecurityPreset> {
    presets_file(s, "security_presets.json")
        .filter(|p| p.exists())
        .and_then(|p| load_security_presets(&p).ok())
        .unwrap_or_default()
}

fn hf_templates(s: &DocOpsState) -> Vec<HfTemplate> {
    presets_file(s, "header_footer_templates.json")
        .filter(|p| p.exists())
        .and_then(|p| load_hf_templates(&p).ok())
        .unwrap_or_default()
}

fn list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// The reduced graph and its report, from the worker.
pub type ReduceOutcome = (
    markupcraft_revu::cos::Document,
    markupcraft_engine::Result<markupcraft_engine::reduce::ReduceReport>,
);

/// Take a finished Reduce File Size into its document.
fn poll_reduce(app: &mut AppState, ctx: &egui::Context) {
    let Some((uid, rx)) = &app.features.docops.reducing else {
        return;
    };
    let uid = *uid;
    let (cos, report) = match rx.try_recv() {
        Ok(o) => o,
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.features.docops.reducing = None;
            app.features.docops.message = "Reduce File Size stopped".into();
            return;
        }
    };
    app.features.docops.reducing = None;
    let threads = app.threads;
    let Some(d) = app.docs.iter_mut().find(|d| d.uid == uid) else {
        return;
    };
    let r = report.and_then(|r| d.session.apply_graph("Reduce File Size", cos).map(|_| r));
    d.rerender(threads);
    let msg = actions::report(r, |r| {
        format!(
            "Reduced from {} KB to {} KB ({} of {} images resampled)",
            r.bytes_before / 1024,
            r.bytes_after / 1024,
            r.images_resampled,
            r.images
        )
    });
    app.status = msg.clone();
    app.features.docops.message = msg;
}

/// Start Reduce File Size on the active document (on a worker thread).
fn start_reduce(app: &mut AppState) {
    if app.features.docops.reducing.is_some() {
        return;
    }
    let Some(d) = app.docs.get(app.active) else { return };
    let mut cos = d.session.graph_copy();
    let settings = app.features.docops.reduce;
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new()
        .name("markupcraft-reduce".into())
        .spawn(move || {
            let r = markupcraft_engine::reduce::reduce_graph(&mut cos, &settings);
            let _ = tx.send((cos, r));
        });
    match started {
        Ok(_) => {
            app.features.docops.reducing = Some((d.uid, rx));
            app.features.docops.message = "Reducing...".into();
        }
        Err(e) => app.features.docops.message = format!("could not start: {e}"),
    }
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    poll_reduce(app, ctx);
    if !app.features.docops.open {
        return;
    }
    let Some(d) = app.docs.get(app.active) else {
        app.features.docops.open = false;
        return;
    };
    let selected = d.selection().len();
    let attachments = d.session.attachments();
    let security = d.session.security();
    let status = d.session.security_status();
    let kept = d.session.kept_header_footer().is_some();
    let presets: Vec<String> = match app.features.docops.tab {
        Tab::Security => security_presets(&app.features.docops)
            .into_iter()
            .map(|p| p.name)
            .collect(),
        Tab::HeaderFooter => hf_templates(&app.features.docops).into_iter().map(|p| p.name).collect(),
        _ => Vec::new(),
    };
    let mut open = true;
    let mut apply = None;
    let mut att_action: Option<(String, bool)> = None;
    let mut add_attachment = false;
    super::window("Document")
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
            let s = &mut app.features.docops;
            ui.horizontal_wrapped(|ui| {
                for (t, label) in TABS {
                    ui.selectable_value(&mut s.tab, *t, *label);
                }
            });
            ui.separator();
            match s.tab {
                Tab::Flatten => {
                    ui.label("Flatten draws markups into the page content; they can no longer be edited.");
                    ui.checkbox(&mut s.flatten_selected, format!("Selected markups only ({selected})"));
                    ui.horizontal(|ui| {
                        ui.label("Types:");
                        ui.add(egui::TextEdit::singleline(&mut s.flatten_kinds).hint_text("all (or e.g. Cloud, Area)"));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Layers:");
                        ui.add(egui::TextEdit::singleline(&mut s.flatten_layers).hint_text("all"));
                    });
                    super::pages_field(ui, &mut s.pages);
                    if ui.button("Flatten").clicked() {
                        apply = Some(Action::Flatten);
                    }
                }
                Tab::Reduce => {
                    ui.checkbox(&mut s.reduce.downsample, "Downsample images");
                    ui.horizontal(|ui| {
                        ui.label("to");
                        ui.add(
                            egui::DragValue::new(&mut s.reduce.target_ppi)
                                .range(36.0..=1200.0)
                                .suffix(" ppi"),
                        );
                        ui.label("when above");
                        ui.add(
                            egui::DragValue::new(&mut s.reduce.above_ppi)
                                .range(36.0..=2400.0)
                                .suffix(" ppi"),
                        );
                    });
                    let mut jpeg = s.reduce.jpeg_quality.is_some();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut jpeg, "JPEG quality");
                        let mut q = s.reduce.jpeg_quality.unwrap_or(60);
                        ui.add_enabled(jpeg, egui::Slider::new(&mut q, 10..=100));
                        s.reduce.jpeg_quality = jpeg.then_some(q);
                    });
                    let mut gray = s.reduce.gray.is_some();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut gray, "Gray images:");
                        let mut g =
                            s.reduce
                                .gray
                                .unwrap_or((s.reduce.target_ppi, s.reduce.above_ppi, s.reduce.jpeg_quality));
                        ui.add_enabled(gray, egui::DragValue::new(&mut g.0).range(36.0..=1200.0).suffix(" ppi"));
                        ui.label("above");
                        ui.add_enabled(gray, egui::DragValue::new(&mut g.1).range(36.0..=2400.0).suffix(" ppi"));
                        s.reduce.gray = gray.then_some(g);
                    });
                    ui.checkbox(&mut s.reduce.discard_thumbnails, "Discard page thumbnails");
                    ui.checkbox(&mut s.reduce.discard_alternate_images, "Discard alternate images");
                    ui.checkbox(&mut s.reduce.discard_metadata, "Discard metadata");
                    ui.checkbox(&mut s.reduce.discard_private, "Discard private application data");
                    ui.checkbox(&mut s.reduce.discard_tags, "Discard structure tags");
                    ui.checkbox(&mut s.reduce.remove_invalid_links, "Remove links that go nowhere");
                    ui.checkbox(&mut s.reduce.compress_streams, "Compress uncompressed streams");
                    ui.checkbox(&mut s.reduce.crop_to_crop_box, "Crop pages to their crop box");
                    if s.reducing.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reducing...");
                        });
                    }
                    ui.add_enabled_ui(s.reducing.is_none(), |ui| {
                        if ui.button("Reduce").clicked() {
                            apply = Some(Action::Reduce);
                        }
                    });
                }
                Tab::Security => {
                    ui.label(if security.encrypted {
                        format!("Encrypted ({})", security.method.clone().unwrap_or_default())
                    } else {
                        "Not encrypted".to_string()
                    });
                    egui::Grid::new("security").num_columns(2).show(ui, |ui| {
                        ui.label("Open password");
                        ui.add(egui::TextEdit::singleline(&mut s.open_password).password(true));
                        ui.end_row();
                        ui.label("Permissions password");
                        ui.add(egui::TextEdit::singleline(&mut s.permissions_password).password(true));
                        ui.end_row();
                        ui.label("Encryption");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut s.encryption, Encryption::Aes256, "AES 256");
                            ui.selectable_value(&mut s.encryption, Encryption::Aes128, "AES 128");
                            ui.selectable_value(&mut s.encryption, Encryption::Rc4_128, "RC4 128");
                        });
                        ui.end_row();
                    });
                    let p = &mut s.permissions;
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut p.print, "Print");
                        ui.checkbox(&mut p.print_high_quality, "High-quality print");
                        ui.checkbox(&mut p.modify, "Change");
                        ui.checkbox(&mut p.copy, "Copy");
                        ui.checkbox(&mut p.annotate, "Markups");
                        ui.checkbox(&mut p.fill_forms, "Forms");
                        ui.checkbox(&mut p.accessibility, "Accessibility");
                        ui.checkbox(&mut p.assemble, "Assemble");
                    });
                    ui.label(RichText::new("Security applies when the document is saved.").small());
                    ui.label(format!("Status: {}", status.label()));
                    ui.horizontal(|ui| {
                        ui.label("Preset");
                        ui.add(
                            egui::TextEdit::singleline(&mut s.preset_name)
                                .desired_width(140.0)
                                .hint_text("name"),
                        );
                        if ui.button("Save Preset").clicked() {
                            apply = Some(Action::SavePreset);
                        }
                    });
                    if !presets.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Apply:");
                            for p in &presets {
                                if ui.small_button(p).clicked() {
                                    s.preset_name = p.clone();
                                    apply = Some(Action::ApplyPreset);
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Apply Security").clicked() {
                            apply = Some(Action::Security);
                        }
                        if security.encrypted && ui.button("Remove Security").clicked() {
                            s.open_password.clear();
                            s.permissions_password.clear();
                            apply = Some(Action::RemoveSecurity);
                        }
                    });
                }
                Tab::HeaderFooter => {
                    ui.label(RichText::new("Tokens: <<1>> page, <<n>> count, <<Page 1 of n>>, <<m/d/yyyy>>").small());
                    egui::Grid::new("hf").num_columns(2).show(ui, |ui| {
                        for (i, (_, label)) in SLOTS.iter().enumerate() {
                            ui.label(*label);
                            if let Some(t) = s.hf.text.get_mut(i) {
                                ui.text_edit_singleline(t);
                            }
                            ui.end_row();
                        }
                        ui.label("Font size");
                        ui.add(egui::DragValue::new(&mut s.hf.font_size).range(4.0..=72.0));
                        ui.end_row();
                        ui.label("Colour");
                        super::color_edit(ui, &mut s.hf.color);
                        ui.end_row();
                    });
                    egui::Grid::new("hf-more").num_columns(2).show(ui, |ui| {
                        ui.label("Margins (T B L R)");
                        ui.horizontal(|ui| {
                            for m in &mut s.hf.margins {
                                ui.add(egui::DragValue::new(m).range(0.0..=360.0));
                            }
                        });
                        ui.end_row();
                        ui.label("Start number");
                        ui.add(egui::DragValue::new(&mut s.hf.start_number).range(1..=999_999));
                        ui.end_row();
                    });
                    ui.checkbox(&mut s.hf.underline, "Underline");
                    ui.checkbox(&mut s.fit_content, "Shrink page content to fit inside the margins");
                    super::pages_field(ui, &mut s.pages);
                    ui.checkbox(
                        &mut s.replace,
                        "Replace existing (and keep these settings with the document)",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply = Some(Action::HeaderFooter);
                        }
                        if ui.button("Remove").clicked() {
                            apply = Some(Action::RemoveHeaderFooter);
                        }
                        ui.add_enabled_ui(kept, |ui| {
                            if ui
                                .button("Edit")
                                .on_hover_text("Load the settings kept with the document")
                                .clicked()
                            {
                                apply = Some(Action::EditKept);
                            }
                            if ui
                                .button("Update")
                                .on_hover_text("Re-apply the kept settings to the pages as they are now")
                                .clicked()
                            {
                                apply = Some(Action::UpdateKept);
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Template");
                        ui.add(
                            egui::TextEdit::singleline(&mut s.template_name)
                                .desired_width(140.0)
                                .hint_text("name"),
                        );
                        if ui.button("Save Template").clicked() {
                            apply = Some(Action::SaveTemplate);
                        }
                    });
                    if !presets.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Apply:");
                            for p in &presets {
                                if ui.small_button(p).clicked() {
                                    s.template_name = p.clone();
                                    apply = Some(Action::ApplyTemplate);
                                }
                            }
                        });
                    }
                }
                Tab::Watermark => {
                    egui::Grid::new("wm").num_columns(2).show(ui, |ui| {
                        ui.label("Text");
                        ui.text_edit_singleline(&mut s.wm.text);
                        ui.end_row();
                        ui.label("Font size");
                        ui.add(egui::DragValue::new(&mut s.wm.font_size).range(4.0..=400.0));
                        ui.end_row();
                        ui.label("Colour");
                        super::color_edit(ui, &mut s.wm.color);
                        ui.end_row();
                        ui.label("Opacity");
                        ui.add(egui::Slider::new(&mut s.wm.opacity, 0.0..=1.0));
                        ui.end_row();
                        ui.label("Rotation");
                        ui.add(
                            egui::DragValue::new(&mut s.wm.rotation)
                                .range(-360.0..=360.0)
                                .suffix(" deg"),
                        );
                        ui.end_row();
                        ui.label("Position");
                        ui.checkbox(&mut s.wm.behind, "Behind page content");
                        ui.end_row();
                    });
                    super::pages_field(ui, &mut s.pages);
                    ui.checkbox(&mut s.replace, "Replace existing");
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply = Some(Action::Watermark);
                        }
                        if ui.button("Remove").clicked() {
                            apply = Some(Action::RemoveWatermark);
                        }
                    });
                }
                Tab::Bates => {
                    egui::Grid::new("bates").num_columns(2).show(ui, |ui| {
                        ui.label("Prefix");
                        ui.text_edit_singleline(&mut s.bates.prefix);
                        ui.end_row();
                        ui.label("Suffix");
                        ui.text_edit_singleline(&mut s.bates.suffix);
                        ui.end_row();
                        ui.label("Digits");
                        ui.add(egui::DragValue::new(&mut s.bates.digits).range(1..=12));
                        ui.end_row();
                        ui.label("Start at");
                        ui.add(egui::DragValue::new(&mut s.bates.start).range(0..=999_999_999));
                        ui.end_row();
                        ui.label("Position");
                        egui::ComboBox::from_id_salt("bates-slot")
                            .selected_text(SLOTS.get(s.bates_slot).map_or("", |x| x.1))
                            .show_ui(ui, |ui| {
                                for (i, (_, l)) in SLOTS.iter().enumerate() {
                                    ui.selectable_value(&mut s.bates_slot, i, *l);
                                }
                            });
                        ui.end_row();
                    });
                    super::pages_field(ui, &mut s.pages);
                    if ui.button("Apply").clicked() {
                        apply = Some(Action::Bates);
                    }
                }
                Tab::Attachments => {
                    if attachments.is_empty() {
                        ui.label(RichText::new("This document has no attachments.").weak());
                    }
                    egui::Grid::new("att").striped(true).num_columns(4).show(ui, |ui| {
                        for a in &attachments {
                            ui.label(&a.name);
                            ui.label(a.size.map_or_else(String::new, |n| format!("{n} bytes")));
                            if ui.small_button("Extract...").clicked() {
                                att_action = Some((a.name.clone(), false));
                            }
                            if ui.small_button("Delete").clicked() {
                                att_action = Some((a.name.clone(), true));
                            }
                            ui.end_row();
                        }
                    });
                    if ui.button("Add File...").clicked() {
                        add_attachment = true;
                    }
                }
            }
            if !s.message.is_empty() {
                ui.separator();
                ui.label(RichText::new(&s.message).small());
            }
        });
    if !open {
        app.features.docops.open = false;
    }
    if add_attachment {
        app.dialogs.open(Purpose::Feature(Ask::Attachment), super::ANY, false);
    }
    if let Some((name, delete)) = att_action {
        if delete {
            if let Some(d) = app.doc_mut() {
                let r = d.session.delete_attachment(&name);
                app.features.docops.message = actions::report(r, |_| format!("Deleted {name}"));
            }
        } else {
            app.dialogs.save(
                Purpose::Feature(Ask::AttachmentExtract(name.clone())),
                super::ANY,
                &name,
            );
        }
    }
    if let Some(t) = apply {
        run(app, t);
    }
}

/// Apply the dialog's tab to the active document.
pub fn run(app: &mut AppState, action: Action) {
    if matches!(action, Action::Reduce) {
        start_reduce(app);
        return;
    }
    if matches!(action, Action::SavePreset | Action::SaveTemplate) {
        let s = &app.features.docops;
        let r = if action == Action::SavePreset {
            let set = SecuritySettings {
                open_password: s.open_password.clone(),
                permissions_password: s.permissions_password.clone(),
                permissions: s.permissions,
                encryption: s.encryption,
            };
            let mut list = security_presets(s);
            let p = SecurityPreset::from_settings(&s.preset_name, &set);
            list.retain(|x| x.name != p.name);
            let name = p.name.clone();
            list.push(p);
            match presets_file(s, "security_presets.json") {
                Some(f) => save_security_presets(&f, &list).map(|_| format!("Preset {name} saved")),
                None => Ok("No settings folder to keep presets in".into()),
            }
        } else {
            let mut list = hf_templates(s);
            let mut t = HfTemplate::from_settings(&s.template_name, &s.hf);
            t.fit_content = s.fit_content;
            list.retain(|x| x.name != t.name);
            let name = t.name.clone();
            list.push(t);
            match presets_file(s, "header_footer_templates.json") {
                Some(f) => save_hf_templates(&f, &list).map(|_| format!("Template {name} saved")),
                None => Ok("No settings folder to keep templates in".into()),
            }
        };
        app.features.docops.message = actions::report(r, |m| m);
        return;
    }
    if action == Action::EditKept {
        let kept = app.doc().and_then(|d| d.session.kept_header_footer());
        if let Some(t) = kept {
            let s = &mut app.features.docops;
            s.hf = t.settings();
            s.fit_content = t.fit_content;
            s.pages = t
                .pages
                .iter()
                .map(|p| (p + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            s.message = "Loaded the header and footer kept with the document".into();
        }
        return;
    }
    let threads = app.threads;
    let Some(d) = app.docs.get_mut(app.active) else { return };
    let s = &app.features.docops;
    let count = d.session.page_count();
    let pages = match super::parse_pages(&s.pages, count) {
        Some(p) => p,
        None => {
            app.features.docops.message = "Pages: leave empty for all, or a range like 1-3, 5".into();
            return;
        }
    };
    let msg = match action {
        Action::RemoveHeaderFooter => actions::report(d.session.remove_marks(&pages, MarkKind::HeaderFooter), |n| {
            format!("Removed headers and footers from {}", actions::plural(n, "page"))
        }),
        Action::RemoveWatermark => actions::report(d.session.remove_marks(&pages, MarkKind::Watermark), |n| {
            format!("Removed watermarks from {}", actions::plural(n, "page"))
        }),
        Action::Flatten => {
            let filter = FlattenFilter {
                ids: if s.flatten_selected {
                    d.selection().to_vec()
                } else {
                    Vec::new()
                },
                pages: if s.pages.trim().is_empty() { Vec::new() } else { pages },
                kinds: list(&s.flatten_kinds),
                layers: list(&s.flatten_layers),
                authors: Vec::new(),
            };
            actions::report(d.session.flatten_markups(&filter), |n| {
                format!("Flattened {}", actions::plural(n, "markup"))
            })
        }
        Action::Security => {
            let set = SecuritySettings {
                open_password: s.open_password.clone(),
                permissions_password: s.permissions_password.clone(),
                permissions: s.permissions,
                encryption: s.encryption,
            };
            actions::report(d.session.set_security(&set), |_| {
                "Security set; save to apply it".into()
            })
        }
        Action::RemoveSecurity => actions::report(d.session.remove_security(), |_| {
            "Security removed; save to apply".into()
        }),
        Action::HeaderFooter if s.replace => {
            let mut t = HfTemplate::from_settings("kept", &s.hf);
            t.fit_content = s.fit_content;
            let which = if s.pages.trim().is_empty() {
                Vec::new()
            } else {
                pages.clone()
            };
            actions::report(d.session.apply_header_footer_kept(&which, &t), |_| {
                format!("Header and footer on {}", actions::plural(pages.len(), "page"))
            })
        }
        Action::HeaderFooter => actions::report(d.session.add_header_footer(&pages, &s.hf, s.replace), |_| {
            format!("Header and footer on {}", actions::plural(pages.len(), "page"))
        }),
        Action::UpdateKept => actions::report(d.session.update_header_footer(), |_| "Header and footer updated".into()),
        Action::ApplyPreset => {
            let found = security_presets(s).into_iter().find(|p| p.name == s.preset_name);
            match found {
                Some(p) => actions::report(d.session.set_security(&p.settings()), |_| {
                    format!("Preset {} set; save to apply it", p.name)
                }),
                None => "No such preset".into(),
            }
        }
        Action::ApplyTemplate => {
            let found = hf_templates(s).into_iter().find(|p| p.name == s.template_name);
            let which = if s.pages.trim().is_empty() {
                Vec::new()
            } else {
                pages.clone()
            };
            match found {
                Some(t) => actions::report(d.session.apply_header_footer_kept(&which, &t), |_| {
                    format!("Template {} applied", t.name)
                }),
                None => "No such template".into(),
            }
        }
        Action::SavePreset | Action::SaveTemplate | Action::EditKept | Action::Reduce => String::new(),
        Action::Watermark => actions::report(d.session.add_watermark(&pages, &s.wm, s.replace), |_| {
            format!("Watermark on {}", actions::plural(pages.len(), "page"))
        }),
        Action::Bates => {
            let mut b = s.bates.clone();
            if let Some((slot, _)) = SLOTS.get(s.bates_slot) {
                b.slot = *slot;
            }
            actions::report(d.session.add_bates(&pages, &b, true), |(a, z)| {
                format!("Bates {a} to {z}")
            })
        }
    };
    d.rerender(threads);
    app.status = msg.clone();
    app.features.docops.message = msg;
}

/// Attachment file dialogs answered.
pub fn attachment_file(app: &mut AppState, ask: &Ask, path: &Path) {
    let Some(d) = app.doc_mut() else { return };
    let msg = match ask {
        Ask::Attachment => actions::report(d.session.add_attachment(path, None, ""), |n| format!("Attached {n}")),
        Ask::AttachmentExtract(name) => actions::report(d.session.extract_attachment(name, path), |n| {
            format!("Extracted {name} ({n} bytes)")
        }),
        _ => return,
    };
    app.status = msg.clone();
    app.features.docops.message = msg;
}
