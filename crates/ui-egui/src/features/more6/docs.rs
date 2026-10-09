//! File > Create PDF Package, Document > Insert Pages with Options and Insert Layered Pages,
//! Document > Stitching, Bookmarks from Structure, File > File Manager Integration, and links
//! dragged out from the File Access list.

use std::path::{Path, PathBuf};

use markupcraft_engine::insert_more::InsertOptions;
use markupcraft_engine::links::{LinkLook, LinkTarget};
use markupcraft_engine::package::create_pdf_package;
use markupcraft_engine::shell_integration::{TargetOs, write_integration};
use markupcraft_engine::stitch::StitchLayout;
use markupcraft_geom::Point;

use super::{Ask6, Pick6, doc_step};
use crate::AppState;
use crate::dialogs::Purpose;
use crate::features::{ANY, Ask, start_pick};

pub struct DocsState {
    pub package_open: bool,
    pub package_files: Vec<PathBuf>,
    /// Insert with options: the chosen file, the options, the position (1-based) text.
    pub insert_file: Option<PathBuf>,
    pub insert: InsertOptions,
    pub insert_at: String,
    pub layered_file: Option<PathBuf>,
    pub layered_first: usize,
    pub layered_name: String,
    pub stitch_open: bool,
    pub stitch_pages: String,
    pub stitch_columns: usize,
    pub stitch_overlap: f64,
    pub shell_open: bool,
    pub shell_os: TargetOs,
    pub shell_cli: String,
    pub shell_written: Vec<PathBuf>,
    /// The file a link from the File Access list goes to.
    pub link_file: Option<PathBuf>,
    pub message: String,
}

impl Default for DocsState {
    fn default() -> Self {
        Self {
            package_open: false,
            package_files: Vec::new(),
            insert_file: None,
            insert: InsertOptions::default(),
            insert_at: String::new(),
            layered_file: None,
            layered_first: 1,
            layered_name: String::new(),
            stitch_open: false,
            stitch_pages: String::new(),
            stitch_columns: 0,
            stitch_overlap: 0.0,
            shell_open: false,
            shell_os: TargetOs::current(),
            shell_cli: default_cli(),
            shell_written: Vec::new(),
            link_file: None,
            message: String::new(),
        }
    }
}

/// `markupcraft-cli` beside the running program.
fn default_cli() -> String {
    let name = if cfg!(windows) {
        "markupcraft-cli.exe"
    } else {
        "markupcraft-cli"
    };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(name)))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// Document > Bookmarks from Structure.
pub fn bookmarks_from_source(app: &mut AppState) {
    doc_step(app, |s| {
        s.bookmarks_from_source(true).map(|(n, kind)| {
            format!(
                "Made {} from the {}",
                crate::actions::plural(n, "bookmark"),
                if kind.name() == "structure" {
                    "document structure"
                } else {
                    "headings"
                }
            )
        })
    });
    app.show_panel("bookmarks");
}

/// File dialogs answered.
pub fn answer(app: &mut AppState, ask: &Ask6, paths: &[PathBuf]) {
    let Some(first) = paths.first().cloned() else { return };
    let st = &mut app.features.more6.docs;
    match ask {
        Ask6::PackageAdd => {
            for p in paths {
                if !st.package_files.contains(p) {
                    st.package_files.push(p.clone());
                }
            }
        }
        Ask6::PackageOut => {
            let files = st.package_files.clone();
            match create_pdf_package(&files, &first) {
                Ok(n) => {
                    st.package_open = false;
                    st.package_files.clear();
                    super::open_written(app, &first);
                    app.status = format!("PDF Package created with {}", crate::actions::plural(n, "file"));
                }
                Err(e) => st.message = e.to_string(),
            }
        }
        Ask6::InsertFile => {
            st.insert_file = Some(first);
            st.insert_at.clear();
        }
        Ask6::LayeredFile => {
            st.layered_name = first
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            st.layered_file = Some(first);
            let current = app.doc().map_or(1, |d| d.view.current + 1);
            app.features.more6.docs.layered_first = current;
        }
        Ask6::StitchOut => stitch_to(app, &first),
        Ask6::ShellDir => {
            let cli = PathBuf::from(st.shell_cli.trim());
            match write_integration(&first, &cli, st.shell_os) {
                Ok(files) => {
                    st.message = format!(
                        "Wrote {} to {}: follow its README.txt to install them (nothing on your system changed)",
                        crate::actions::plural(files.len(), "file"),
                        first.display()
                    );
                    st.shell_written = files;
                }
                Err(e) => st.message = e.to_string(),
            }
        }
        _ => {}
    }
}

fn stitch_to(app: &mut AppState, out: &Path) {
    let st = &app.features.more6.docs;
    let layout = StitchLayout {
        columns: st.stitch_columns,
        overlap: st.stitch_overlap,
    };
    let text = st.stitch_pages.clone();
    let Some(d) = app.doc() else { return };
    let count = d.session.page_count();
    let Some(pages) = crate::features::parse_pages(&text, count) else {
        app.features.more6.docs.message = "Pages: a range like 1-4".into();
        return;
    };
    match d.session.stitch_pages(&pages, layout, out) {
        Ok(r) => {
            app.features.more6.docs.stitch_open = false;
            super::open_written(app, out);
            app.status = format!(
                "Stitched {} into one {:.0} x {:.0} pt page",
                crate::actions::plural(r.pages, "page"),
                r.width,
                r.height
            );
        }
        Err(e) => app.features.more6.docs.message = e.to_string(),
    }
}

/// The File Access list's Link: drag the link's box on the page.
pub fn start_link_file(app: &mut AppState, file: PathBuf) {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    app.features.more6.docs.link_file = Some(file);
    start_pick(
        app,
        Pick6::LinkFile.into(),
        &format!("Drag the link area that opens {name}"),
    );
}

/// The link's box was dragged.
pub fn link_file_picked(app: &mut AppState, page: usize, pts: &[Point]) {
    let Some(r) = crate::features::rect_of(pts) else { return };
    let Some(file) = app.features.more6.docs.link_file.take() else {
        return;
    };
    let target = LinkTarget::File {
        path: file.display().to_string(),
        page: None,
    };
    doc_step(app, |s| {
        s.add_link(page, r, &target, LinkLook::default())
            .map(|_| format!("Linked to {}", file.display()))
    });
}

pub fn window(app: &mut AppState, ctx: &egui::Context) {
    package_window(app, ctx);
    insert_window(app, ctx);
    layered_window(app, ctx);
    stitch_window(app, ctx);
    shell_window(app, ctx);
}

fn package_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.docs.package_open {
        return;
    }
    let mut open = true;
    let (mut add, mut create) = (false, false);
    let st = &mut app.features.more6.docs;
    crate::features::window("Create PDF Package")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("A PDF Package holds other files (PDFs, drawings, spreadsheets) behind a cover page.");
            let mut remove = None;
            for (i, f) in st.package_files.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        f.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                st.package_files.remove(i);
            }
            ui.horizontal(|ui| {
                add = ui.button("Add Files...").clicked();
                create = ui.button("Create...").clicked();
            });
            if st.package_files.is_empty() {
                ui.weak("With no files: an empty package to add files to later.");
            }
            if !st.message.is_empty() {
                ui.label(&st.message);
            }
        });
    app.features.more6.docs.package_open = open;
    if add {
        app.dialogs
            .open(Purpose::Feature(Ask::More6(Ask6::PackageAdd)), ANY, true);
    }
    if create {
        app.dialogs.save(
            Purpose::Feature(Ask::More6(Ask6::PackageOut)),
            crate::dialogs::PDF,
            "Package.pdf",
        );
    }
}

fn insert_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(file) = app.features.more6.docs.insert_file.clone() else {
        return;
    };
    let mut open = true;
    let mut go = false;
    let st = &mut app.features.more6.docs;
    crate::features::window("Insert Pages").open(&mut open).show(ctx, |ui| {
        ui.label(format!("From {}", file.display()));
        ui.horizontal(|ui| {
            ui.label("Before page");
            ui.add(
                egui::TextEdit::singleline(&mut st.insert_at)
                    .desired_width(50.0)
                    .hint_text("end"),
            );
        });
        let o = &mut st.insert;
        ui.checkbox(&mut o.bookmarks, "Carry over bookmarks");
        ui.checkbox(&mut o.attachments, "Carry over attachments");
        ui.checkbox(&mut o.properties, "Merge document properties");
        ui.checkbox(&mut o.layers, "Keep layers");
        ui.checkbox(&mut o.labels_from_name, "Use the file name as page labels");
        ui.checkbox(&mut o.interleave, "Interleave pages (rejoin odd and even scans)");
        ui.add_enabled(
            o.interleave,
            egui::Checkbox::new(&mut o.reverse, "Reverse the inserted pages"),
        );
        go = ui.button("Insert").clicked();
        if !st.message.is_empty() {
            ui.label(&st.message);
        }
    });
    if !open {
        app.features.more6.docs.insert_file = None;
    }
    if go {
        let st = &app.features.more6.docs;
        let o = st.insert;
        let at_text = st.insert_at.trim().to_string();
        let count = app.doc().map_or(0, |d| d.session.page_count());
        let at = match at_text.parse::<usize>() {
            Ok(n) if (1..=count + 1).contains(&n) => n - 1,
            Ok(_) => {
                app.features.more6.docs.message = format!("Before page 1 to {}", count + 1);
                return;
            }
            Err(_) => count,
        };
        app.features.more6.docs.insert_file = None;
        doc_step(app, |s| {
            s.insert_pages_with(at, &file, None, &o).map(|r| {
                format!(
                    "Inserted {}",
                    crate::actions::plural(r.pages_after.saturating_sub(r.pages_before), "page")
                )
            })
        });
    }
}

fn layered_window(app: &mut AppState, ctx: &egui::Context) {
    let Some(file) = app.features.more6.docs.layered_file.clone() else {
        return;
    };
    let count = app.doc().map_or(1, |d| d.session.page_count());
    let mut open = true;
    let mut go = false;
    let st = &mut app.features.more6.docs;
    crate::features::window("Insert Layered Pages")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(format!(
                "Each page of {} becomes a layer on a page here.",
                file.display()
            ));
            ui.horizontal(|ui| {
                ui.label("Starting at page");
                ui.add(egui::DragValue::new(&mut st.layered_first).range(1..=count.max(1)));
            });
            ui.horizontal(|ui| {
                ui.label("Layer name");
                ui.text_edit_singleline(&mut st.layered_name);
            });
            go = ui.button("Insert").clicked();
        });
    if !open {
        app.features.more6.docs.layered_file = None;
    }
    if go {
        let st = &mut app.features.more6.docs;
        let first = st.layered_first.saturating_sub(1);
        let name = (!st.layered_name.trim().is_empty()).then(|| st.layered_name.trim().to_string());
        st.layered_file = None;
        doc_step(app, |s| {
            s.insert_layered_pages(&file, None, first, name.as_deref())
                .map(|n| format!("Layered {}", crate::actions::plural(n, "page")))
        });
        app.show_panel("layers");
    }
}

fn stitch_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.docs.stitch_open {
        return;
    }
    let mut open = true;
    let mut go = false;
    let st = &mut app.features.more6.docs;
    crate::features::window("Stitching").open(&mut open).show(ctx, |ui| {
        ui.label("Join drawing pages edge to edge into one large page (a new PDF).");
        crate::features::pages_field(ui, &mut st.stitch_pages);
        ui.horizontal(|ui| {
            ui.label("Pages per row");
            ui.add(egui::DragValue::new(&mut st.stitch_columns).range(0..=64));
            ui.weak("(0 = one row)");
        });
        ui.horizontal(|ui| {
            ui.label("Overlap at match lines");
            ui.add(
                egui::DragValue::new(&mut st.stitch_overlap)
                    .range(0.0..=2000.0)
                    .suffix(" pt"),
            );
        });
        go = ui.button("Stitch...").clicked();
        if !st.message.is_empty() {
            ui.label(&st.message);
        }
    });
    app.features.more6.docs.stitch_open = open;
    if go {
        app.dialogs.save(
            Purpose::Feature(Ask::More6(Ask6::StitchOut)),
            crate::dialogs::PDF,
            "Stitched.pdf",
        );
    }
}

fn shell_window(app: &mut AppState, ctx: &egui::Context) {
    if !app.features.more6.docs.shell_open {
        return;
    }
    let mut open = true;
    let mut go = false;
    let st = &mut app.features.more6.docs;
    crate::features::window("File Manager Integration").open(&mut open).show(ctx, |ui| {
        ui.label("Adds \"Combine in MarkupCraft\" and \"Convert to PDF with MarkupCraft\" to the right-click menu of your file manager. MarkupCraft writes the files; you install them (each comes with a README), so nothing on your system changes until you do.");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut st.shell_os, TargetOs::Windows, "Windows Explorer");
            ui.selectable_value(&mut st.shell_os, TargetOs::MacOs, "macOS Finder");
            ui.selectable_value(&mut st.shell_os, TargetOs::Linux, "Linux");
        });
        ui.horizontal(|ui| {
            ui.label("markupcraft-cli");
            ui.add(egui::TextEdit::singleline(&mut st.shell_cli).desired_width(300.0));
        });
        go = ui.button("Write Files to a Folder...").clicked();
        for f in &st.shell_written {
            ui.weak(f.display().to_string());
        }
        if !st.message.is_empty() {
            ui.label(&st.message);
        }
    });
    app.features.more6.docs.shell_open = open;
    if go {
        app.dialogs.folder(Purpose::Feature(Ask::More6(Ask6::ShellDir)));
    }
}
