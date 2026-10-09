//! Page operation dialogs over the engine's page commands (each one undo step): Insert Blank
//! Pages, Insert Pages (position), Extract Pages, Delete Pages, Rotate Pages, Crop Pages, Page
//! Setup (resize) and Replace Pages, all with the shared page range picker (all, current,
//! selected thumbnails, even, odd, landscape, portrait, first, last, or a list like `1-3, 7`).

use egui::RichText;
use markupcraft_engine::boxes::{Anchor, BoxChange, PageBox};

use crate::dialogs::{self, Purpose};
use crate::{AppState, actions};

/// Which pages a dialog acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    All,
    #[default]
    Current,
    Selected,
    Even,
    Odd,
    Landscape,
    Portrait,
    First,
    Last,
    Custom,
}

impl Range {
    pub const ALL: [Range; 10] = [
        Range::All,
        Range::Current,
        Range::Selected,
        Range::Even,
        Range::Odd,
        Range::Landscape,
        Range::Portrait,
        Range::First,
        Range::Last,
        Range::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Range::All => "All pages",
            Range::Current => "Current page",
            Range::Selected => "Selected thumbnails",
            Range::Even => "Even pages",
            Range::Odd => "Odd pages",
            Range::Landscape => "Landscape pages",
            Range::Portrait => "Portrait pages",
            Range::First => "First page",
            Range::Last => "Last page",
            Range::Custom => "Pages:",
        }
    }
}

/// The page range picker's state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RangePick {
    pub range: Range,
    pub custom: String,
}

/// Page sizes for Insert Blank and Page Setup (points).
pub const SIZES: &[(&str, f64, f64)] = &[
    ("Letter (8.5 x 11 in)", 612.0, 792.0),
    ("Legal (8.5 x 14 in)", 612.0, 1008.0),
    ("Tabloid (11 x 17 in)", 792.0, 1224.0),
    ("ARCH C (18 x 24 in)", 1296.0, 1728.0),
    ("ARCH D (24 x 36 in)", 1728.0, 2592.0),
    ("ARCH E1 (30 x 42 in)", 2160.0, 3024.0),
    ("ARCH E (36 x 48 in)", 2592.0, 3456.0),
    ("A4 (210 x 297 mm)", 595.276, 841.89),
    ("A3 (297 x 420 mm)", 841.89, 1190.551),
    ("A1 (594 x 841 mm)", 1683.78, 2383.94),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    InsertBlank,
    InsertPages,
    Extract,
    Delete,
    Rotate,
    Crop,
    PageSetup,
    Replace,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Kind::InsertBlank => "Insert Blank Pages",
            Kind::InsertPages => "Insert Pages",
            Kind::Extract => "Extract Pages",
            Kind::Delete => "Delete Pages",
            Kind::Rotate => "Rotate Pages",
            Kind::Crop => "Crop Pages",
            Kind::PageSetup => "Page Setup",
            Kind::Replace => "Replace Pages",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageDialog {
    pub kind: Kind,
    pub pages: RangePick,
    /// Insert: how many blank pages.
    pub count: u32,
    /// Size preset index into [`SIZES`] (`None` = custom).
    pub size: Option<usize>,
    /// Custom width and height, inches.
    pub custom: (f64, f64),
    pub landscape: bool,
    /// Insert: after (else before) `at_page` (1-based).
    pub after: bool,
    pub at_page: usize,
    /// Rotate: degrees clockwise.
    pub degrees: i64,
    /// Crop margins, inches: left, bottom, right, top. `remove` clears the crop.
    pub margins: [f64; 4],
    pub remove_crop: bool,
    pub anchor: Anchor,
    /// Extract: delete the pages afterwards; open the new file.
    pub delete_after: bool,
    pub open_after: bool,
    /// Replace: first page of the source to use (1-based).
    pub source_start: usize,
    pub error: String,
}

impl PageDialog {
    pub fn new(kind: Kind, current: usize) -> Self {
        Self {
            kind,
            pages: RangePick::default(),
            count: 1,
            size: Some(0),
            custom: (8.5, 11.0),
            landscape: false,
            after: true,
            at_page: current + 1,
            degrees: 90,
            margins: [0.5; 4],
            remove_crop: false,
            anchor: Anchor::Center,
            delete_after: false,
            open_after: false,
            source_start: 1,
            error: String::new(),
        }
    }

    /// The chosen size in points (orientation applied).
    pub fn size_pt(&self) -> (f64, f64) {
        let (w, h) = match self.size.and_then(|i| SIZES.get(i)) {
            Some((_, w, h)) => (*w, *h),
            None => (self.custom.0 * 72.0, self.custom.1 * 72.0),
        };
        if self.landscape {
            (w.max(h), w.min(h))
        } else {
            (w.min(h), w.max(h))
        }
    }
}

/// The pages a range names (0-based), or why it names none.
pub fn resolve(app: &AppState, pick: &RangePick) -> Result<Vec<usize>, String> {
    let d = app.doc().ok_or("No document open")?;
    let n = d.session.page_count();
    let dims = |i: usize| {
        d.render
            .as_ref()
            .and_then(|r| r.page(i))
            .map_or((1.0, 1.0), |g| (g.width, g.height))
    };
    let v: Vec<usize> = match pick.range {
        Range::All => (0..n).collect(),
        Range::Current => vec![d.view.current.min(n.saturating_sub(1))],
        Range::Selected => {
            let mut s: Vec<usize> = app.shell.thumbs.selected.iter().copied().filter(|p| *p < n).collect();
            s.sort_unstable();
            s.dedup();
            if s.is_empty() { vec![d.view.current] } else { s }
        }
        Range::Even => (0..n).filter(|i| (i + 1) % 2 == 0).collect(),
        Range::Odd => (0..n).filter(|i| (i + 1) % 2 == 1).collect(),
        Range::Landscape => (0..n).filter(|i| dims(*i).0 > dims(*i).1).collect(),
        Range::Portrait => (0..n).filter(|i| dims(*i).0 <= dims(*i).1).collect(),
        Range::First => vec![0],
        Range::Last => vec![n.saturating_sub(1)],
        Range::Custom => {
            crate::pages_from_text(&pick.custom, d.view.current, n).ok_or("That page range names no pages")?
        }
    };
    if v.is_empty() || n == 0 {
        return Err("No pages match".into());
    }
    Ok(v)
}

/// The shared page range picker.
pub fn range_picker(ui: &mut egui::Ui, pick: &mut RangePick, id: &str) {
    ui.horizontal(|ui| {
        ui.label("Pages");
        egui::ComboBox::from_id_salt(("range", id))
            .selected_text(pick.range.label())
            .show_ui(ui, |ui| {
                for r in Range::ALL {
                    ui.selectable_value(&mut pick.range, r, r.label());
                }
            });
        if pick.range == Range::Custom {
            ui.add(
                egui::TextEdit::singleline(&mut pick.custom)
                    .hint_text("1-3, 7")
                    .desired_width(100.0),
            );
        }
    });
}

/// Open the dialog for a page command.
pub fn open(app: &mut AppState, id: &str) {
    let kind = match id {
        "document.insert_blank_pages" => Kind::InsertBlank,
        "pages.insert" => Kind::InsertPages,
        "document.extract_pages" => Kind::Extract,
        "document.delete_pages" => Kind::Delete,
        "document.rotate_pages" => Kind::Rotate,
        "document.crop_pages" => Kind::Crop,
        "document.page_setup" => Kind::PageSetup,
        "document.replace_pages" => Kind::Replace,
        _ => return,
    };
    let current = app.doc().map_or(0, |d| d.view.current);
    let mut d = PageDialog::new(kind, current);
    if kind == Kind::Rotate && app.shell.ui.extra.rotate_all_pages {
        d.pages.range = Range::All;
    }
    if !app.shell.thumbs.selected.is_empty() {
        d.pages.range = Range::Selected;
    }
    if kind == Kind::PageSetup
        && let Some(g) = app.doc().and_then(|d| d.render.as_ref()?.page(current).cloned())
    {
        d.size = None;
        d.custom = (f64::from(g.width) / 72.0, f64::from(g.height) / 72.0);
        d.landscape = g.width > g.height;
    }
    app.shell.page_dialog = Some(d);
}

/// Run the dialog's operation. Returns an error to show, or `None` when done.
pub fn apply(app: &mut AppState, dlg: &PageDialog) -> Option<String> {
    let threads = app.threads;
    let pages = match dlg.kind {
        Kind::InsertBlank | Kind::InsertPages => Vec::new(),
        _ => match resolve(app, &dlg.pages) {
            Ok(p) => p,
            Err(e) => return Some(e),
        },
    };
    let n = app.doc().map_or(0, |d| d.session.page_count());
    let at = if dlg.after {
        dlg.at_page
    } else {
        dlg.at_page.saturating_sub(1)
    }
    .min(n);
    let name = app
        .doc()
        .map(|d| d.name.trim_end_matches(".pdf").to_string())
        .unwrap_or_default();
    match dlg.kind {
        Kind::InsertPages => {
            app.dialogs.open(Purpose::InsertPages { at }, dialogs::PDF, false);
            return None;
        }
        Kind::Extract => {
            let tag = format!(
                "extract:{}:{}:{}",
                u8::from(dlg.delete_after),
                u8::from(dlg.open_after),
                pages.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
            );
            app.dialogs
                .save(Purpose::Shell { tag }, dialogs::PDF, &format!("{name} extract.pdf"));
            return None;
        }
        Kind::Replace => {
            let tag = format!(
                "replace:{}:{}",
                dlg.source_start.max(1) - 1,
                pages.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
            );
            app.dialogs.open(Purpose::Shell { tag }, dialogs::PDF, false);
            return None;
        }
        _ => {}
    }
    let d = app.doc_mut()?;
    let r: markupcraft_engine::Result<String> = match dlg.kind {
        Kind::InsertBlank => d
            .session
            .insert_blank_pages(at, dlg.count.clamp(1, 500) as usize, Some(dlg.size_pt()))
            .map(|_| format!("Inserted {}", actions::plural(dlg.count as usize, "blank page"))),
        Kind::Delete => {
            if pages.len() >= n {
                return Some("A document keeps at least one page".into());
            }
            d.session
                .delete_pages(&pages)
                .map(|_| format!("Deleted {}", actions::plural(pages.len(), "page")))
        }
        Kind::Rotate => d
            .session
            .rotate_pages(&pages, dlg.degrees)
            .map(|_| format!("Rotated {}", actions::plural(pages.len(), "page"))),
        Kind::Crop => {
            let change = if dlg.remove_crop {
                BoxChange::Remove
            } else {
                let m = dlg
                    .margins
                    .map(|v| if v.is_finite() { v.clamp(0.0, 200.0) * 72.0 } else { 0.0 });
                BoxChange::Margins(m)
            };
            d.session
                .set_page_box(&pages, PageBox::Crop, change)
                .map(|_| format!("Cropped {}", actions::plural(pages.len(), "page")))
        }
        Kind::PageSetup => {
            let (w, h) = dlg.size_pt();
            d.session
                .resize_pages(&pages, w, h, dlg.anchor)
                .map(|_| format!("Resized {}", actions::plural(pages.len(), "page")))
        }
        Kind::InsertPages | Kind::Extract | Kind::Replace => return None,
    };
    d.sync_pages(threads);
    match r {
        Ok(s) => {
            app.status = s;
            app.shell.thumbs.selected.clear();
            None
        }
        Err(e) => Some(e.to_string()),
    }
}

/// A file dialog answered for a page operation.
pub fn answer(app: &mut AppState, tag: &str, path: &std::path::Path) {
    let threads = app.threads;
    let mut parts = tag.split(':');
    let kind = parts.next().unwrap_or_default();
    let nums = |s: Option<&str>| -> Vec<usize> {
        s.unwrap_or_default()
            .split(',')
            .filter_map(|v| v.parse().ok())
            .collect()
    };
    match kind {
        "extract" => {
            let delete = parts.next() == Some("1");
            let open = parts.next() == Some("1");
            let pages = nums(parts.next());
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.extract_pages(&pages, path, delete);
            d.sync_pages(threads);
            app.status = actions::report(r, |n| {
                format!("Extracted {} to {}", actions::plural(n, "page"), path.display())
            });
            if open && path.is_file() {
                app.open_path(path);
            }
        }
        "replace" => {
            let start: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let targets = nums(parts.next());
            let src: Vec<usize> = (0..targets.len()).map(|i| start + i).collect();
            let Some(d) = app.doc_mut() else { return };
            let r = d.session.replace_pages(&targets, path, &src);
            d.sync_pages(threads);
            app.status = actions::report(r, |_| format!("Replaced {}", actions::plural(targets.len(), "page")));
        }
        _ => {}
    }
}

/// Show the open page dialog.
pub fn window(app: &mut AppState, ctx: &egui::Context) {
    let Some(mut dlg) = app.shell.page_dialog.take() else {
        return;
    };
    let n = app.doc().map_or(0, |d| d.session.page_count());
    if n == 0 {
        return;
    }
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new(dlg.kind.title())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            match dlg.kind {
                Kind::InsertBlank | Kind::InsertPages => {
                    if dlg.kind == Kind::InsertBlank {
                        ui.horizontal(|ui| {
                            ui.label("Number of pages");
                            ui.add(egui::DragValue::new(&mut dlg.count).range(1..=500));
                        });
                        size_picker(ui, &mut dlg);
                    }
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dlg.after, false, "Before");
                        ui.radio_value(&mut dlg.after, true, "After");
                        ui.label("page");
                        ui.add(egui::DragValue::new(&mut dlg.at_page).range(1..=n));
                        ui.label(format!("of {n}"));
                    });
                }
                Kind::Extract => {
                    range_picker(ui, &mut dlg.pages, "extract");
                    ui.checkbox(&mut dlg.delete_after, "Delete pages after extracting");
                    ui.checkbox(&mut dlg.open_after, "Open the new file");
                }
                Kind::Delete => range_picker(ui, &mut dlg.pages, "delete"),
                Kind::Rotate => {
                    range_picker(ui, &mut dlg.pages, "rotate");
                    ui.horizontal(|ui| {
                        ui.label("Rotate");
                        ui.radio_value(&mut dlg.degrees, 90, "90 clockwise");
                        ui.radio_value(&mut dlg.degrees, 180, "180");
                        ui.radio_value(&mut dlg.degrees, 270, "90 counterclockwise");
                    });
                }
                Kind::Crop => {
                    range_picker(ui, &mut dlg.pages, "crop");
                    ui.checkbox(&mut dlg.remove_crop, "Remove the crop (show the whole page)");
                    ui.add_enabled_ui(!dlg.remove_crop, |ui| {
                        egui::Grid::new("crop-margins").num_columns(4).show(ui, |ui| {
                            for (i, label) in ["Left", "Bottom", "Right", "Top"].iter().enumerate() {
                                ui.label(*label);
                                if let Some(v) = dlg.margins.get_mut(i) {
                                    ui.add(egui::DragValue::new(v).range(0.0..=100.0).speed(0.05).suffix(" in"));
                                }
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                }
                Kind::PageSetup => {
                    range_picker(ui, &mut dlg.pages, "setup");
                    size_picker(ui, &mut dlg);
                    ui.horizontal(|ui| {
                        ui.label("Keep the drawing at");
                        egui::ComboBox::from_id_salt("anchor")
                            .selected_text(format!("{:?}", dlg.anchor))
                            .show_ui(ui, |ui| {
                                for a in [
                                    Anchor::Center,
                                    Anchor::TopLeft,
                                    Anchor::Top,
                                    Anchor::TopRight,
                                    Anchor::Left,
                                    Anchor::Right,
                                    Anchor::BottomLeft,
                                    Anchor::Bottom,
                                    Anchor::BottomRight,
                                ] {
                                    ui.selectable_value(&mut dlg.anchor, a, format!("{a:?}"));
                                }
                            });
                    });
                }
                Kind::Replace => {
                    range_picker(ui, &mut dlg.pages, "replace");
                    ui.horizontal(|ui| {
                        ui.label("Starting at page");
                        ui.add(egui::DragValue::new(&mut dlg.source_start).range(1..=100_000));
                        ui.label("of the file chosen next");
                    });
                    ui.label(RichText::new("Markups, links and bookmarks stay on the replaced pages.").weak());
                }
            }
            if !dlg.error.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0xB0, 0x20, 0x20), &dlg.error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if ok {
        if let Some(e) = apply(app, &dlg) {
            dlg.error = e;
            app.shell.page_dialog = Some(dlg);
        }
        return;
    }
    if open && !cancel {
        app.shell.page_dialog = Some(dlg);
    }
}

fn size_picker(ui: &mut egui::Ui, dlg: &mut PageDialog) {
    ui.horizontal(|ui| {
        ui.label("Size");
        let text = dlg.size.and_then(|i| SIZES.get(i)).map_or("Custom", |s| s.0);
        egui::ComboBox::from_id_salt("page-size")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (i, s) in SIZES.iter().enumerate() {
                    ui.selectable_value(&mut dlg.size, Some(i), s.0);
                }
                ui.selectable_value(&mut dlg.size, None, "Custom");
            });
    });
    if dlg.size.is_none() {
        ui.horizontal(|ui| {
            ui.label("Width");
            ui.add(
                egui::DragValue::new(&mut dlg.custom.0)
                    .range(0.1..=200.0)
                    .speed(0.1)
                    .suffix(" in"),
            );
            ui.label("Height");
            ui.add(
                egui::DragValue::new(&mut dlg.custom.1)
                    .range(0.1..=200.0)
                    .speed(0.1)
                    .suffix(" in"),
            );
        });
    }
    ui.horizontal(|ui| {
        ui.radio_value(&mut dlg.landscape, false, "Portrait");
        ui.radio_value(&mut dlg.landscape, true, "Landscape");
    });
}
