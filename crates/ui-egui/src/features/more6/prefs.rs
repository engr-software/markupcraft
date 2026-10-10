//! The Preferences pages for `markupcraft_engine::prefs_pages` (Interface > Markups List and
//! Layers, Tools > Measure, Forms and Signature, Window > Tablet and WebTab, Sets,
//! Import/Export, Integrations), and where the interface reads them: [`apply`] runs when the
//! preferences change; the Markups List, Layers panel, form highlights, single-key form
//! shortcuts, the eraser, the Web Tab and the scanner dialog read them directly.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Color32;
use markupcraft_engine::prefs::Preferences;
use markupcraft_engine::prefs_pages::Integration;

use crate::AppState;
use crate::commands::Keys;
use crate::features::Mark;

/// The pages, each after its parent page in the list.
pub const PAGES: &[(&str, &str)] = &[
    ("Markups List", "Interface"),
    ("Layers", "Interface"),
    ("Measure", "Tools"),
    ("Forms", "Tools"),
    ("Signature", "Tools"),
    ("Tablet", "Window"),
    ("WebTab", "Window"),
    ("Sets", ""),
    ("Import/Export", ""),
    ("Integrations", ""),
];

static ZOOM_TO_SELECTED: AtomicBool = AtomicBool::new(true);

/// Markups List: a selected row scrolls the page to its markup.
pub fn zoom_to_selected() -> bool {
    ZOOM_TO_SELECTED.load(Ordering::Relaxed)
}

/// Files in `dir` with one of `exts` (case-insensitive), sorted, at most `max`.
fn files_in(dir: &str, exts: &[&str], max: usize) -> Vec<std::path::PathBuf> {
    let d = dir.trim();
    if d.is_empty() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(Path::new(d)) else {
        return Vec::new();
    };
    let mut v: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
        })
        .take(10_000)
        .collect();
    v.sort();
    v.truncate(max);
    v
}

/// The preferences changed (or were loaded): pass them to the features that keep a copy.
pub fn apply(app: &mut AppState) {
    let m = app.shell.prefs.more.clone();
    ZOOM_TO_SELECTED.store(m.markups_list.zoom_to_selected, Ordering::Relaxed);
    app.features.fill.gap = m.measure.fill_gap_pt.clamp(0.0, 72.0);
    app.features.fill.cutouts = m.measure.fill_cutouts;
    let fm = &mut app.features.fill.more;
    fm.raster = m.measure.fill_raster;
    // The fill speed trades the page image's resolution for time.
    let base = m.measure.fill_dpi.clamp(36.0, 300.0);
    fm.dpi = match m.measure.fill_speed.as_str() {
        "fast" => (base / 2.0).max(36.0),
        "accurate" => (base * 2.0).min(300.0),
        _ => base,
    };
    app.edit.more.keep_subject = m.measure.keep_subject_label;
    fm.sensitivity = m.measure.fill_sensitivity.clamp(1, 254);
    fm.hide_markups = m.measure.fill_hide_markups;
    fm.cursor_size = m.measure.fill_cursor_px.clamp(8.0, 64.0);
    if let Some([r, g, b]) = hex(&m.measure.fill_cursor_color) {
        fm.cursor_color = Color32::from_rgb(r, g, b);
    }
    crate::gestures::set_eraser(m.tablet.eraser_scales_with_zoom, m.tablet.eraser_px);
    app.features.export.dpi = f64::from(m.import_export.image_dpi);
    app.features.sets.latest_only = m.sets.latest_only;
    // Sets: the panel's defaults (sort, categories, earlier revisions, revision filter).
    {
        use markupcraft_engine::batch::SetSort;
        use markupcraft_engine::sets_more::CategoryMode;
        let s = &mut app.features.sets;
        let sort = match m.sets.sort.as_str() {
            "sheet" => SetSort::Label,
            "file_sheet" => SetSort::FileThenLabel,
            _ => SetSort::FileOrder,
        };
        if s.sort != sort {
            s.sort = sort;
            s.stale = true;
        }
        s.categories = match m.sets.categories.as_str() {
            "file_name" => CategoryMode::FileName,
            "sheet_number" => CategoryMode::SheetNumber,
            _ => CategoryMode::Off,
        };
        s.previous = m.sets.earlier_revisions.min(3);
        s.revision_filter = m.sets.revision_filter.clone();
    }
    // Signature: the first digital ID in the folder is offered; the folder's certificates are
    // trusted when validating.
    let sig = &mut app.features.signatures;
    if sig.id_path.is_none() {
        sig.id_path = files_in(&m.signature.digital_id_folder, &["p12", "pfx"], 1)
            .into_iter()
            .next();
    }
    if app.features.more6.batch.p12.is_none() {
        app.features.more6.batch.p12 = app.features.signatures.id_path.clone();
    }
    for f in files_in(&m.signature.trusted_folder, &["pem", "cer", "crt", "der"], 200) {
        let Ok(meta) = std::fs::metadata(&f) else { continue };
        if meta.len() > 1 << 20 {
            continue;
        }
        if let Ok(b) = std::fs::read(&f)
            && markupcraft_engine::signatures::load_certificates(&b).is_ok()
            && !app.features.signatures.trust.contains(&b)
        {
            app.features.signatures.trust.push(b);
        }
    }
}

/// Preferences > Layers: the child layers (at any depth) that hiding or showing `name` in the
/// Layers panel also hides or shows; none unless the preference is on.
pub fn layer_children(app: &AppState, name: &str) -> Vec<String> {
    let Some(d) = app.doc() else { return Vec::new() };
    if !app.shell.prefs.more.layers.hide_children_with_parent {
        return Vec::new();
    }
    let tree = d.session.layer_tree();
    let mut kids: Vec<String> = Vec::new();
    let mut parents = vec![name.to_string()];
    while let Some(p) = parents.pop() {
        for c in tree.iter().filter(|t| t.parent.as_deref() == Some(p.as_str())) {
            if !kids.contains(&c.name) && kids.len() < 10_000 {
                kids.push(c.name.clone());
                parents.push(c.name.clone());
            }
        }
    }
    kids
}

/// Whether a key binding may run: single-key form tool shortcuts are off when Preferences >
/// Forms says so.
pub fn key_allowed(app: &AppState, k: &Keys, id: &str) -> bool {
    let single = !k.ctrl && !k.alt && !k.shift;
    !(single && id.starts_with("forms.") && !app.shell.prefs.more.forms.single_key_shortcuts)
}

fn hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Form field highlights (Preferences > Forms) for the canvas layer.
pub fn form_marks(app: &AppState, d: &crate::DocTab, marks: &mut Vec<Mark>) {
    let f = &app.shell.prefs.more.forms;
    if !f.highlight_fields {
        return;
    }
    let [r, g, b] = hex(&f.highlight_color).unwrap_or([200, 220, 255]);
    let a = (f32::from(f.highlight_opacity_pct.min(100)) / 100.0 * 255.0) as u8;
    let fill = Color32::from_rgba_unmultiplied(r, g, b, a);
    let stroke = Color32::from_rgba_unmultiplied(r / 2, g / 2, b, a.max(60));
    for field in d.session.form_fields().iter().take(5_000) {
        if let (Some(page), Some(rect)) = (field.page, field.rect) {
            marks.push(Mark::rect(page, rect, fill, stroke));
        }
    }
}

fn color_hex(ui: &mut egui::Ui, s: &mut String) {
    let mut rgb = hex(s).unwrap_or([200, 220, 255]);
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        *s = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    }
}

fn folder(ui: &mut egui::Ui, label: &str, s: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(s).desired_width(260.0).hint_text("(none)"));
    });
}

/// Advanced: document JavaScript and trusted locations; the PDF/A conversion steps.
fn advanced_section(ui: &mut egui::Ui, a: &mut markupcraft_engine::prefs_pages::AdvancedPrefs) {
    ui.checkbox(
        &mut a.js_enabled,
        "Run a document's JavaScript when it opens (sandboxed)",
    );
    ui.add_enabled(
        a.js_enabled,
        egui::Checkbox::new(&mut a.js_trusted_only, "Only documents in a trusted location"),
    );
    ui.label("Trusted locations (one folder per line)");
    let id = egui::Id::new("trusted-locations");
    let mut text: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| a.trusted_locations.join("\n"));
    if ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(2)
                .desired_width(300.0),
        )
        .changed()
    {
        a.trusted_locations = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(100)
            .map(str::to_string)
            .collect();
    }
    ui.data_mut(|d| d.insert_temp(id, text));
    ui.add_space(4.0);
    ui.label("Archive as PDF/A first:");
    ui.checkbox(&mut a.pdfa_flatten_markups, "Flatten the markups into the pages");
    ui.checkbox(&mut a.pdfa_remove_attachments, "Remove embedded files");
    ui.checkbox(&mut a.pdfa_opaque_markups, "Make transparent markups opaque");
}

/// Sets: paths, display, categories and their templates, sort, revisions, auto-tagging.
fn sets_section(ui: &mut egui::Ui, st: &mut markupcraft_engine::prefs_pages::SetsPrefs) {
    ui.checkbox(
        &mut st.relative_paths,
        "Store the files relative to the set file's folder",
    );
    ui.horizontal(|ui| {
        ui.label("Earlier revisions are");
        for (v, l) in [(0, "shown"), (1, "hidden"), (2, "greyed"), (3, "crossed out")] {
            ui.radio_value(&mut st.earlier_revisions, v, l);
        }
    });
    ui.checkbox(&mut st.stack_revisions, "Stack earlier revisions under the latest");
    ui.horizontal(|ui| {
        ui.label("Categories");
        for (v, l) in [
            ("off", "off"),
            ("file_name", "by file name"),
            ("sheet_number", "by sheet number"),
        ] {
            ui.radio_value(&mut st.categories, v.to_string(), l);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Sort");
        for (v, l) in [
            ("file", "file order"),
            ("sheet", "sheet number"),
            ("file_sheet", "file, then sheet"),
        ] {
            ui.radio_value(&mut st.sort, v.to_string(), l);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Revision filter");
        ui.add(
            egui::TextEdit::singleline(&mut st.revision_filter)
                .hint_text("e.g. @?#")
                .desired_width(80.0),
        );
    });
    ui.label("Category templates (prefix = category, one per line; empty = the NCS disciplines)");
    let id = egui::Id::new("sets-category-rules");
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| {
        st.category_rules
            .iter()
            .map(|r| format!("{} = {}", r.prefix, r.name))
            .collect::<Vec<_>>()
            .join("\n")
    });
    if ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(3)
                .desired_width(300.0),
        )
        .changed()
    {
        st.category_rules = parse_rules(&text);
    }
    ui.data_mut(|d| d.insert_temp(id, text));
    ui.checkbox(
        &mut st.copy_markups_to_revision,
        "Copy markups to a new revision added to a Set",
    );
    ui.checkbox(&mut st.stamp_superseded, "Stamp the previous revision SUPERSEDED");
    ui.checkbox(&mut st.auto_tags, "Tag discipline and sheet type from the sheet number");
}

/// `prefix = category` lines (blank and malformed lines are skipped).
pub fn parse_rules(text: &str) -> Vec<markupcraft_engine::sets_more::CategoryRule> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(p, n)| (p.trim(), n.trim()))
        .filter(|(p, n)| !p.is_empty() && !n.is_empty() && p.chars().count() <= 16 && n.chars().count() <= 64)
        .take(200)
        .map(|(p, n)| markupcraft_engine::sets_more::CategoryRule {
            prefix: p.to_string(),
            name: n.to_string(),
        })
        .collect()
}

/// The page `page`'s options. Returns a web address to open (Integrations > Sign In).
pub fn section(ui: &mut egui::Ui, page: &str, p: &mut Preferences) -> Option<String> {
    let m = &mut p.more;
    let mut open = None;
    match page {
        "Markups List" => {
            let l = &mut m.markups_list;
            ui.checkbox(
                &mut l.zoom_to_selected,
                "Selecting a markup in the list goes to it on the page",
            );
            ui.checkbox(
                &mut l.dominant_only,
                "Show a group's measurement on its dominant (first) markup only",
            );
            ui.checkbox(&mut l.rich_comments, "Show comments as rich text");
            ui.checkbox(&mut l.wrap_comments, "Wrap long comments");
            ui.checkbox(
                &mut l.exclude_filtered_from_export,
                "Leave filtered-out markups out of exports",
            );
            ui.horizontal(|ui| {
                ui.label("Dim filtered-out markups on the page by");
                ui.add(egui::Slider::new(&mut l.dim_filtered_pct, 0..=95).suffix("%"));
            });
        }
        "Layers" => {
            ui.checkbox(
                &mut m.layers.hide_children_with_parent,
                "Hiding or showing a layer does the same to its child layers",
            );
            ui.checkbox(
                &mut m.layers.current_page_only,
                "List only the layers used on the current page",
            );
        }
        "Measure" => {
            ui.label("Dynamic Fill");
            ui.horizontal(|ui| {
                ui.label("Close gaps up to");
                ui.add(
                    egui::DragValue::new(&mut m.measure.fill_gap_pt)
                        .range(0.0..=72.0)
                        .suffix(" pt"),
                );
            });
            ui.checkbox(&mut m.measure.fill_cutouts, "Islands inside a region become cutouts");
            ui.checkbox(&mut m.measure.fill_raster, "Detect on the page image (scans)");
            ui.horizontal(|ui| {
                ui.label("Page image resolution");
                ui.add(
                    egui::DragValue::new(&mut m.measure.fill_dpi)
                        .range(36.0..=300.0)
                        .suffix(" dpi"),
                );
                ui.label("Edge sensitivity");
                ui.add(egui::DragValue::new(&mut m.measure.fill_sensitivity).range(1..=254));
            });
            ui.checkbox(&mut m.measure.fill_hide_markups, "Hide markups while filling");
            ui.horizontal(|ui| {
                ui.label("Fill speed");
                for (v, l) in [("fast", "Fast"), ("balanced", "Balanced"), ("accurate", "Accurate")] {
                    ui.radio_value(&mut m.measure.fill_speed, v.to_string(), l);
                }
            });
            ui.checkbox(
                &mut m.measure.keep_subject_label,
                "New measurements keep the last subject and label",
            );
            ui.horizontal(|ui| {
                ui.label("Fill cursor");
                ui.add(
                    egui::DragValue::new(&mut m.measure.fill_cursor_px)
                        .range(8.0..=64.0)
                        .suffix(" px"),
                );
                color_hex(ui, &mut m.measure.fill_cursor_color);
            });
            ui.checkbox(
                &mut m.measure.split_counts_by_space,
                "Split counts by space (a Count across several spaces becomes one per space)",
            );
        }
        "Forms" => {
            ui.checkbox(&mut m.forms.highlight_fields, "Highlight form fields");
            ui.horizontal(|ui| {
                ui.label("Highlight colour");
                color_hex(ui, &mut m.forms.highlight_color);
                ui.label("Opacity");
                ui.add(egui::Slider::new(&mut m.forms.highlight_opacity_pct, 0..=100).suffix("%"));
            });
            ui.checkbox(
                &mut m.forms.single_key_shortcuts,
                "Single-key shortcuts for the form tools",
            );
        }
        "Signature" => {
            folder(ui, "Digital ID folder", &mut m.signature.digital_id_folder);
            folder(ui, "Trusted certificates folder", &mut m.signature.trusted_folder);
            ui.horizontal(|ui| {
                ui.label("Remember the digital ID password for");
                ui.add(
                    egui::DragValue::new(&mut m.signature.password_minutes)
                        .range(0..=1440)
                        .suffix(" min"),
                );
                ui.label("(0 = never)");
            });
            ui.checkbox(
                &mut m.signature.block_breaking_changes,
                "Block changes that would invalidate signatures",
            );
            ui.weak("The first .p12/.pfx in the ID folder is offered when signing; certificates in the trusted folder are trusted when validating signatures.");
        }
        "Tablet" => {
            ui.checkbox(
                &mut m.tablet.eraser_scales_with_zoom,
                "The eraser's size follows the zoom",
            );
            ui.horizontal(|ui| {
                ui.label("Eraser size");
                ui.add(
                    egui::DragValue::new(&mut m.tablet.eraser_px)
                        .range(2.0..=100.0)
                        .suffix(" px"),
                );
            });
            let t = &mut m.tablet;
            ui.checkbox(&mut t.pinch_zoom, "Pinch to zoom (touch screens, trackpads, tablets)");
            ui.horizontal(|ui| {
                ui.label("Pen cursor");
                ui.radio_value(&mut t.pen_cursor, "crosshair".to_string(), "Crosshair");
                ui.radio_value(&mut t.pen_cursor, "dot".to_string(), "A dot as wide as the pen");
            });
            ui.checkbox(
                &mut t.pen_text_highlight,
                "The Highlight pen over page text highlights the text",
            );
            ui.horizontal(|ui| {
                ui.label("Pen commit delay");
                ui.add(
                    egui::DragValue::new(&mut t.pen_commit_ms)
                        .range(0..=10_000)
                        .suffix(" ms"),
                );
                ui.label("(strokes within it join one markup; 0 = off)");
            });
            ui.checkbox(&mut t.ink_copy_picture, "Copy pen strokes as a picture too");
            ui.checkbox(&mut t.right_click_lasso, "Right-button drag lassoes markups");
            ui.checkbox(&mut t.pressure, "Pen pressure sets the stroke width");
            ui.checkbox(&mut t.touch_mode, "Touch input mode (larger handles and pick areas)");
        }
        "WebTab" => {
            let w = &mut m.webtab;
            ui.checkbox(&mut w.switch_to_new, "Switch to a new Web Tab");
            ui.horizontal(|ui| {
                ui.label("Links to web pages");
                ui.radio_value(&mut w.open_links_in, "browser".to_string(), "Open in the browser");
                ui.radio_value(&mut w.open_links_in, "capture".to_string(), "Capture as PDF");
            });
            folder(ui, "Browser to capture with", &mut w.browser_path);
            ui.horizontal(|ui| {
                ui.label("Capture time limit");
                ui.add(
                    egui::DragValue::new(&mut w.capture_timeout_secs)
                        .range(5..=600)
                        .suffix(" s"),
                );
            });
            ui.checkbox(&mut w.captures_in_split, "Open captured pages in a split view");
            ui.label(format!(
                "{} (manage them in the Web Tab)",
                crate::actions::plural(w.favorites.len(), "favourite")
            ));
        }
        "Sets" => {
            ui.checkbox(
                &mut m.sets.open_in_place,
                "Open a sheet in place of the Set sheet in the current tab",
            );
            ui.checkbox(&mut m.sets.latest_only, "Show only the latest revision of each sheet");
            sets_section(ui, &mut m.sets);
        }
        "Import/Export" => {
            let ie = &mut m.import_export;
            ui.horizontal(|ui| {
                ui.label("Export pages as images at");
                ui.add(egui::DragValue::new(&mut ie.image_dpi).range(36..=1200).suffix(" dpi"));
            });
            ui.horizontal(|ui| {
                ui.label("Scan at");
                ui.add(egui::DragValue::new(&mut ie.scan_dpi).range(50..=1200).suffix(" dpi"));
            });
            ui.horizontal(|ui| {
                ui.label("Camera and scanner pictures: longest side");
                ui.add(
                    egui::DragValue::new(&mut ie.capture_max_side)
                        .range(256..=16_384)
                        .suffix(" px"),
                );
            });
            folder(ui, "Network scanner (eSCL)", &mut ie.scanner_url);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Exporting").strong());
            ui.checkbox(&mut ie.ocr_on_export, "OCR pages without text before exporting them");
            ui.checkbox(&mut ie.open_after_export, "Open the exported file");
            ui.checkbox(
                &mut ie.word_page_pictures,
                "Word: a picture of each page above its text",
            );
            ui.checkbox(&mut ie.word_page_breaks, "Word: a page break between pages");
            ui.checkbox(&mut ie.excel_one_sheet, "Excel: all pages in one sheet");
            ui.horizontal(|ui| {
                ui.label("Numbers in tables use a");
                ui.radio_value(&mut ie.decimal_comma, false, "decimal point (1,234.5)");
                ui.radio_value(&mut ie.decimal_comma, true, "decimal comma (1.234,5)");
            });
            ui.horizontal(|ui| {
                ui.label("PowerPoint slides at");
                ui.add(egui::DragValue::new(&mut ie.slide_dpi).range(36..=300).suffix(" dpi"));
            });
            ui.horizontal(|ui| {
                ui.label("TIFF compression");
                for c in markupcraft_engine::convert::TiffCompression::ALL {
                    ui.radio_value(&mut ie.tiff_compression, c.name().to_string(), c.name());
                }
            });
            ui.checkbox(&mut ie.multi_page_tiff, "TIFF: all pages in one multi-page file");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Pictures to PDF").strong());
            ui.horizontal(|ui| {
                ui.label("Picture resolution");
                ui.add(
                    egui::DragValue::new(&mut ie.picture_dpi)
                        .range(36..=1200)
                        .suffix(" dpi"),
                );
                ui.checkbox(&mut ie.picture_grayscale, "Grayscale");
            });
        }
        "Advanced" => {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(egui::RichText::new("JavaScript and PDF/A conversion").strong())
                .id_salt("advanced-js-pdfa")
                .default_open(false)
                .show(ui, |ui| advanced_section(ui, &mut m.advanced));
        }
        "Integrations" => {
            let mut remove = None;
            egui::Grid::new("integrations")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Service");
                    ui.strong("Region");
                    ui.strong("Sign-in page");
                    ui.strong("On");
                    ui.end_row();
                    for (i, s) in m.integrations.services.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(110.0));
                        ui.add(egui::TextEdit::singleline(&mut s.region).desired_width(40.0));
                        ui.add(egui::TextEdit::singleline(&mut s.url).desired_width(170.0));
                        ui.checkbox(&mut s.enabled, "");
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(s.enabled && !s.url.is_empty(), egui::Button::new("Sign In"))
                                .clicked()
                            {
                                open = Some(s.url.clone());
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
            if let Some(i) = remove {
                m.integrations.services.remove(i);
            }
            if ui.button("Add Service").clicked() {
                m.integrations.services.push(Integration {
                    name: "Service".into(),
                    region: "us".into(),
                    url: String::new(),
                    enabled: true,
                });
            }
            ui.weak("Sign In opens the service's page in your browser.");
        }
        _ => {}
    }
    open
}
