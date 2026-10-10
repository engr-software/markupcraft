//! More Preferences pages (part of [`crate::prefs::Preferences`] as `more`): Interface >
//! Markups List and Layers, Tools > Measure, Forms and Signature, Window > Tablet and WebTab,
//! Sets, Import/Export and Integrations. Every value here is read by the feature it names
//! (the interface reads them when they change); `prefs_set {"more": {...}}` changes them
//! headlessly.

use serde::{Deserialize, Serialize};

use crate::{Result, invalid};

/// Interface > Markups List.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkupsListPrefs {
    /// Selecting a markup in the list scrolls the page to it (else it is only selected).
    pub zoom_to_selected: bool,
    /// A group's measurement shows on its first (dominant) markup only.
    pub dominant_only: bool,
    /// Comments show their rich text (bold, italic, colour) in the list.
    pub rich_comments: bool,
    /// Long comments wrap onto more lines (else they are cut at the column's edge).
    pub wrap_comments: bool,
    /// Exports leave out the markups the list's filters hide.
    pub exclude_filtered_from_export: bool,
    /// Markups the list's filters hide are dimmed on the page by this much, percent (0 = off).
    pub dim_filtered_pct: u8,
}

impl Default for MarkupsListPrefs {
    fn default() -> Self {
        Self {
            zoom_to_selected: true,
            dominant_only: false,
            rich_comments: false,
            wrap_comments: false,
            exclude_filtered_from_export: true,
            dim_filtered_pct: 0,
        }
    }
}

/// Interface > Layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LayersPrefs {
    /// Hiding (or showing) a layer in the Layers panel does the same to its child layers.
    pub hide_children_with_parent: bool,
    /// The Layers panel lists only the layers used on the current page.
    pub current_page_only: bool,
}

/// Tools > Measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeasurePrefs {
    /// Dynamic Fill: gaps in linework up to this many points still bound a region (its edge
    /// sensitivity).
    pub fill_gap_pt: f64,
    /// Dynamic Fill: islands inside a filled region become cutouts.
    pub fill_cutouts: bool,
    /// A Count placed across several spaces becomes one Count per space.
    pub split_counts_by_space: bool,
    /// Dynamic Fill detects on the rendered page image (scans) instead of the linework.
    pub fill_raster: bool,
    /// Dynamic Fill on the page image: resolution, dots per inch (36 to 300).
    pub fill_dpi: f64,
    /// Dynamic Fill on the page image: grey level below which a pixel is a wall (1 to 254).
    pub fill_sensitivity: u8,
    /// Dynamic Fill on the page image: markups are left out of the image.
    pub fill_hide_markups: bool,
    /// The fill cursor's ring: diameter on screen (8 to 64) and colour (`#RRGGBB`).
    pub fill_cursor_px: f32,
    pub fill_cursor_color: String,
    /// Dynamic Fill speed on the page image: `fast` (half the resolution), `balanced` or
    /// `accurate` (twice the resolution, at most 300 dpi).
    pub fill_speed: String,
    /// New measurements take the last one's subject and label (Keep Last Subject and Label).
    pub keep_subject_label: bool,
}

impl Default for MeasurePrefs {
    fn default() -> Self {
        Self {
            fill_gap_pt: 2.0,
            fill_cutouts: true,
            split_counts_by_space: false,
            fill_raster: false,
            fill_dpi: 100.0,
            fill_sensitivity: 160,
            fill_hide_markups: true,
            fill_cursor_px: 18.0,
            fill_cursor_color: "#0096DC".into(),
            fill_speed: "balanced".into(),
            keep_subject_label: false,
        }
    }
}

/// Tools > Forms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormsPrefs {
    /// Form fields are highlighted on the page.
    pub highlight_fields: bool,
    /// `#RRGGBB`.
    pub highlight_color: String,
    /// Highlight opacity, percent.
    pub highlight_opacity_pct: u8,
    /// Single-key shortcuts of the form tools (off while filling forms with the keyboard).
    pub single_key_shortcuts: bool,
}

impl Default for FormsPrefs {
    fn default() -> Self {
        Self {
            highlight_fields: true,
            highlight_color: "#C8DCFF".into(),
            highlight_opacity_pct: 60,
            single_key_shortcuts: true,
        }
    }
}

/// Tools > Signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SignaturePrefs {
    /// Where digital IDs (.p12, .pfx) are looked for: the first one is offered when signing.
    pub digital_id_folder: String,
    /// Certificates (.pem, .cer, .crt, .der) in this folder are trusted when validating.
    pub trusted_folder: String,
    /// Minutes a digital ID's password is remembered after signing (0 = forget at once).
    pub password_minutes: u32,
    /// Page edits that would invalidate a signed document's signatures are refused (instead
    /// of asked about).
    pub block_breaking_changes: bool,
}

/// Window > Tablet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabletPrefs {
    /// The eraser's size follows the zoom (a size on the page) instead of staying a fixed size
    /// on screen.
    pub eraser_scales_with_zoom: bool,
    /// The eraser's radius, screen points at 100%.
    pub eraser_px: f32,
    /// Pinching (touch screens, trackpads, pen tablets) zooms the page.
    pub pinch_zoom: bool,
    /// The pointer while drawing with the Pen or Highlight: `crosshair` or `dot` (a small dot
    /// the size of the pen's line).
    pub pen_cursor: String,
    /// The Highlight pen dragged over page text highlights the text (a text highlight).
    pub pen_text_highlight: bool,
    /// Pen strokes made within this many milliseconds of the last one join its markup (0 =
    /// every stroke is its own markup).
    pub pen_commit_ms: u32,
    /// Copying pen strokes also puts a picture of them on the clipboard (for other programs).
    pub ink_copy_picture: bool,
    /// A right-button drag draws a lasso that selects markups, whatever the tool.
    pub right_click_lasso: bool,
    /// Pen pressure (touch force) sets the width of a stroke.
    pub pressure: bool,
    /// Touch input: larger handles and pick areas for fingers.
    pub touch_mode: bool,
}

impl Default for TabletPrefs {
    fn default() -> Self {
        Self {
            eraser_scales_with_zoom: false,
            eraser_px: 8.0,
            pinch_zoom: true,
            pen_cursor: "crosshair".into(),
            pen_text_highlight: false,
            pen_commit_ms: 0,
            ink_copy_picture: false,
            right_click_lasso: false,
            pressure: false,
            touch_mode: false,
        }
    }
}

/// A saved web address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Favorite {
    pub name: String,
    pub url: String,
}

/// Window > WebTab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebTabPrefs {
    /// Show a new Web Tab when one opens (else it opens behind).
    pub switch_to_new: bool,
    /// What a link to a web page does: `browser` (the system browser) or `capture` (print the
    /// page to PDF and open it).
    pub open_links_in: String,
    /// The browser used to capture pages (empty = found automatically).
    pub browser_path: String,
    /// Seconds a capture may take.
    pub capture_timeout_secs: u32,
    pub favorites: Vec<Favorite>,
    /// A captured web page opens in a split view beside the document (else as a tab).
    pub captures_in_split: bool,
}

impl Default for WebTabPrefs {
    fn default() -> Self {
        Self {
            switch_to_new: true,
            open_links_in: "browser".into(),
            browser_path: String::new(),
            capture_timeout_secs: 60,
            favorites: Vec::new(),
            captures_in_split: false,
        }
    }
}

/// Sets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetsPrefs {
    /// A sheet opened from a Set replaces the Set sheet open in the current tab (unless it has
    /// unsaved changes) instead of adding a tab.
    pub open_in_place: bool,
    /// The Sets panel shows only each sheet's latest revision.
    pub latest_only: bool,
    /// Set files store their PDFs relative to the set file's folder (else as full paths).
    pub relative_paths: bool,
    /// How earlier revisions show in the Sets panel: 0 shown, 1 hidden, 2 greyed, 3 crossed out.
    pub earlier_revisions: u8,
    /// Earlier revisions are stacked under their latest version (in a fold).
    pub stack_revisions: bool,
    /// Categories by default: `off`, `file_name` or `sheet_number`.
    pub categories: String,
    /// Category templates (sheet key prefix to category); empty = the NCS disciplines.
    pub category_rules: Vec<crate::sets_more::CategoryRule>,
    /// Sort rule by default: `file`, `sheet` or `file_sheet`.
    pub sort: String,
    /// Revision filter by default (a wildcard for the sheet key in file names).
    pub revision_filter: String,
    /// A new revision added to a Set gets the previous revision's markups.
    pub copy_markups_to_revision: bool,
    /// The previous revision is stamped SUPERSEDED when a new one is added.
    pub stamp_superseded: bool,
    /// Discipline and sheet type are tagged from the sheet number.
    pub auto_tags: bool,
}

impl Default for SetsPrefs {
    fn default() -> Self {
        Self {
            open_in_place: false,
            latest_only: true,
            relative_paths: true,
            earlier_revisions: 2,
            stack_revisions: false,
            categories: "off".into(),
            category_rules: Vec::new(),
            sort: "file".into(),
            revision_filter: String::new(),
            copy_markups_to_revision: false,
            stamp_superseded: false,
            auto_tags: true,
        }
    }
}

impl SetsPrefs {
    /// The category templates in use.
    pub fn rules(&self) -> Vec<crate::sets_more::CategoryRule> {
        if self.category_rules.is_empty() {
            crate::sets_more::default_categories()
        } else {
            self.category_rules.clone()
        }
    }
}

/// Import/Export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportExportPrefs {
    /// Pages exported as images, dots per inch.
    pub image_dpi: u32,
    /// Camera and scanner pictures: longest side in pixels kept.
    pub capture_max_side: u32,
    /// Scanner resolution, dots per inch.
    pub scan_dpi: u32,
    /// The eSCL address of the network scanner last used.
    pub scanner_url: String,
    /// Exports to Word, Excel, PowerPoint, HTML, RTF and text read pages without text with OCR
    /// first (on a copy).
    pub ocr_on_export: bool,
    /// An exported file opens (with the program the system uses for it) once written.
    pub open_after_export: bool,
    /// Excel: every page in one sheet (else a sheet per page).
    pub excel_one_sheet: bool,
    /// Numbers in exported tables use a decimal comma (`1.234,5`).
    pub decimal_comma: bool,
    /// PowerPoint: slides rendered at this many dots per inch (36 to 300).
    pub slide_dpi: u32,
    /// Pictures made into PDF pages: pixels per inch (36 to 1200; 72 = a pixel a point).
    pub picture_dpi: u32,
    /// Pictures made into PDF pages are stored in grey.
    pub picture_grayscale: bool,
    /// TIFF export: `none`, `lzw`, `deflate` or `packbits`.
    pub tiff_compression: String,
    /// TIFF export: every page in one multi-page file.
    pub multi_page_tiff: bool,
    /// Word export: a picture of each page goes in above its text.
    pub word_page_pictures: bool,
    /// Word export: a page break between pages (else one flow of text).
    pub word_page_breaks: bool,
}

impl Default for ImportExportPrefs {
    fn default() -> Self {
        Self {
            image_dpi: 150,
            capture_max_side: 4096,
            scan_dpi: 300,
            scanner_url: String::new(),
            ocr_on_export: false,
            open_after_export: false,
            excel_one_sheet: false,
            decimal_comma: false,
            slide_dpi: 150,
            picture_dpi: 72,
            picture_grayscale: false,
            tiff_compression: "none".into(),
            multi_page_tiff: false,
            word_page_pictures: false,
            word_page_breaks: true,
        }
    }
}

impl ImportExportPrefs {
    /// The reconstruction options of a document export.
    pub fn export_options(&self) -> crate::convert::ExportOptions {
        crate::convert::ExportOptions {
            ocr: self.ocr_on_export,
            excel_one_sheet: self.excel_one_sheet,
            decimal_comma: self.decimal_comma,
            slide_dpi: f64::from(self.slide_dpi.clamp(36, 300)),
            word_page_pictures: self.word_page_pictures,
            word_page_breaks: self.word_page_breaks,
        }
    }

    /// How pictures become PDF pages.
    pub fn image_to_pdf(&self) -> crate::docs_more::ImageToPdf {
        crate::docs_more::ImageToPdf {
            dpi: f64::from(self.picture_dpi.clamp(36, 1200)),
            grayscale: self.picture_grayscale,
        }
    }

    /// The TIFF compression (none when the setting is not a known one).
    pub fn tiff(&self) -> crate::convert::TiffCompression {
        crate::convert::TiffCompression::from_name(&self.tiff_compression).unwrap_or_default()
    }
}

/// One third-party service MarkupCraft links to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Integration {
    pub name: String,
    /// The region whose server is used (`us`, `eu`, `au`...; free text).
    pub region: String,
    /// The sign-in page, opened in the browser.
    pub url: String,
    pub enabled: bool,
}

/// Integrations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IntegrationsPrefs {
    pub services: Vec<Integration>,
}

/// Advanced > JavaScript and PDF/A.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedPrefs {
    /// A document's own JavaScript runs when it opens (in the sandbox).
    pub js_enabled: bool,
    /// Only documents in a trusted location run their JavaScript.
    pub js_trusted_only: bool,
    /// Trusted folders (a document inside one, at any depth, is trusted).
    pub trusted_locations: Vec<String>,
    /// Archive as PDF/A: flatten the markups into the pages first.
    pub pdfa_flatten_markups: bool,
    /// Archive as PDF/A: remove embedded files first (PDF/A-1 and -2 restrict them).
    pub pdfa_remove_attachments: bool,
    /// Archive as PDF/A: make transparent markups opaque first (PDF/A-1 forbids transparency).
    pub pdfa_opaque_markups: bool,
}

impl Default for AdvancedPrefs {
    fn default() -> Self {
        Self {
            js_enabled: false,
            js_trusted_only: true,
            trusted_locations: Vec::new(),
            pdfa_flatten_markups: false,
            pdfa_remove_attachments: false,
            pdfa_opaque_markups: false,
        }
    }
}

impl AdvancedPrefs {
    /// Whether the document at `path` may run its JavaScript when it opens.
    pub fn js_allowed(&self, path: Option<&std::path::Path>) -> bool {
        if !self.js_enabled {
            return false;
        }
        if !self.js_trusted_only {
            return true;
        }
        let Some(p) = path else { return false };
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        self.trusted_locations.iter().any(|t| {
            let t = t.trim();
            if t.is_empty() {
                return false;
            }
            let dir = std::path::Path::new(t);
            let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            p.starts_with(&dir)
        })
    }

    /// The PDF/A conversion steps asked for.
    pub fn pdfa_conversion(&self) -> crate::archive::PdfaConversion {
        crate::archive::PdfaConversion {
            flatten_markups: self.pdfa_flatten_markups,
            remove_attachments: self.pdfa_remove_attachments,
            opaque_markups: self.pdfa_opaque_markups,
        }
    }
}

/// Every page above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MorePrefs {
    pub markups_list: MarkupsListPrefs,
    pub layers: LayersPrefs,
    pub measure: MeasurePrefs,
    pub forms: FormsPrefs,
    pub signature: SignaturePrefs,
    pub tablet: TabletPrefs,
    pub webtab: WebTabPrefs,
    pub sets: SetsPrefs,
    pub import_export: ImportExportPrefs,
    pub integrations: IntegrationsPrefs,
    pub advanced: AdvancedPrefs,
}

fn hex_ok(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s.chars().skip(1).all(|c| c.is_ascii_hexdigit())
}

fn text_ok(s: &str, n: usize) -> bool {
    s.chars().count() <= n && !s.chars().any(char::is_control)
}

impl MorePrefs {
    pub fn validate(&self) -> Result<()> {
        let me = &self.measure;
        if !(me.fill_gap_pt.is_finite() && (0.0..=72.0).contains(&me.fill_gap_pt)) {
            return Err(invalid("more.measure.fill_gap_pt: 0 to 72"));
        }
        if !(me.fill_dpi.is_finite() && (36.0..=300.0).contains(&me.fill_dpi))
            || me.fill_sensitivity == 0
            || me.fill_sensitivity == 255
            || !(me.fill_cursor_px.is_finite() && (8.0..=64.0).contains(&me.fill_cursor_px))
            || !hex_ok(&me.fill_cursor_color)
        {
            return Err(invalid(
                "more.measure: fill dpi 36 to 300, sensitivity 1 to 254, cursor 8 to 64 px, colour #RRGGBB",
            ));
        }
        if !matches!(me.fill_speed.as_str(), "fast" | "balanced" | "accurate") {
            return Err(invalid("more.measure.fill_speed: fast, balanced or accurate"));
        }
        if self.markups_list.dim_filtered_pct > 95 {
            return Err(invalid("more.markups_list.dim_filtered_pct: 0 to 95"));
        }
        if self.signature.password_minutes > 24 * 60 {
            return Err(invalid("more.signature.password_minutes: 0 to 1440"));
        }
        if !hex_ok(&self.forms.highlight_color) {
            return Err(invalid("more.forms.highlight_color is #RRGGBB"));
        }
        if self.forms.highlight_opacity_pct > 100 {
            return Err(invalid("more.forms.highlight_opacity_pct: 0 to 100"));
        }
        let s = &self.signature;
        if !text_ok(&s.digital_id_folder, 1024) || !text_ok(&s.trusted_folder, 1024) {
            return Err(invalid("more.signature: folders up to 1024 characters"));
        }
        let t = &self.tablet;
        if !(t.eraser_px.is_finite() && (2.0..=100.0).contains(&t.eraser_px)) {
            return Err(invalid("more.tablet.eraser_px: 2 to 100"));
        }
        if !matches!(t.pen_cursor.as_str(), "crosshair" | "dot") || t.pen_commit_ms > 10_000 {
            return Err(invalid(
                "more.tablet: pen_cursor crosshair or dot, pen_commit_ms 0 to 10000",
            ));
        }
        let w = &self.webtab;
        if !matches!(w.open_links_in.as_str(), "browser" | "capture") {
            return Err(invalid("more.webtab.open_links_in: browser or capture"));
        }
        if !(5..=600).contains(&w.capture_timeout_secs) || !text_ok(&w.browser_path, 1024) {
            return Err(invalid(
                "more.webtab: capture timeout 5 to 600 s, browser path up to 1024 characters",
            ));
        }
        if w.favorites.len() > 500
            || w.favorites
                .iter()
                .any(|f| !text_ok(&f.name, 200) || crate::webtab::check_url(&f.url).is_err())
        {
            return Err(invalid(
                "more.webtab.favorites: up to 500, each with an http(s) address",
            ));
        }
        let st = &self.sets;
        if st.earlier_revisions > 3
            || !matches!(st.categories.as_str(), "off" | "file_name" | "sheet_number")
            || !matches!(st.sort.as_str(), "file" | "sheet" | "file_sheet")
            || !text_ok(&st.revision_filter, 64)
            || st.category_rules.len() > 200
            || st
                .category_rules
                .iter()
                .any(|r| r.prefix.trim().is_empty() || !text_ok(&r.prefix, 16) || !text_ok(&r.name, 64))
        {
            return Err(invalid(
                "more.sets: earlier_revisions 0 to 3, categories off / file_name / sheet_number, sort file / sheet / file_sheet, up to 200 category rules",
            ));
        }
        let ie = &self.import_export;
        if !(36..=1200).contains(&ie.image_dpi)
            || !(50..=1200).contains(&ie.scan_dpi)
            || !(256..=16_384).contains(&ie.capture_max_side)
        {
            return Err(invalid(
                "more.import_export: image dpi 36 to 1200, scan dpi 50 to 1200, capture size 256 to 16384",
            ));
        }
        if !(36..=300).contains(&ie.slide_dpi)
            || !(36..=1200).contains(&ie.picture_dpi)
            || crate::convert::TiffCompression::from_name(&ie.tiff_compression).is_none()
        {
            return Err(invalid(
                "more.import_export: slide dpi 36 to 300, picture dpi 36 to 1200, tiff compression none, lzw, deflate or packbits",
            ));
        }
        if !ie.scanner_url.is_empty() && crate::devices::parse_http_url(&ie.scanner_url).is_err() {
            return Err(invalid("more.import_export.scanner_url: an http:// eSCL address"));
        }
        let a = &self.advanced;
        if a.trusted_locations.len() > 100 || a.trusted_locations.iter().any(|t| !text_ok(t, 1024)) {
            return Err(invalid("more.advanced.trusted_locations: up to 100 folders"));
        }
        let i = &self.integrations;
        if i.services.len() > 100
            || i.services.iter().any(|x| {
                !text_ok(&x.name, 100)
                    || !text_ok(&x.region, 40)
                    || (!x.url.is_empty() && crate::webtab::check_url(&x.url).is_err())
            })
        {
            return Err(invalid(
                "more.integrations.services: up to 100, each with an http(s) address",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Preferences;

    #[test]
    fn more_prefs_merge_and_validate() {
        let p = Preferences::default();
        let q = p
            .merged(&serde_json::json!({ "more": { "markups_list": { "zoom_to_selected": false }, "webtab": { "favorites": [{ "name": "Spec", "url": "https://example.com/spec" }] } } }))
            .unwrap();
        assert!(!q.more.markups_list.zoom_to_selected);
        assert_eq!(q.more.webtab.favorites.len(), 1);
        assert!(
            p.merged(&serde_json::json!({ "more": { "forms": { "highlight_opacity_pct": 400 } } }))
                .is_err()
        );
        assert!(
            p.merged(&serde_json::json!({ "more": { "webtab": { "open_links_in": "tv" } } }))
                .is_err()
        );
        assert!(
            p.merged(
                &serde_json::json!({ "more": { "webtab": { "favorites": [{ "name": "x", "url": "file:///c:/" }] } } })
            )
            .is_err()
        );
        assert!(
            p.merged(&serde_json::json!({ "more": { "import_export": { "scanner_url": "https://x" } } }))
                .is_err()
        );
        assert!(p.merged(&serde_json::json!({ "more": { "nope": 1 } })).is_err());
        // Old settings files without `more` still load.
        let old: Preferences = serde_json::from_str(r#"{"author":"A"}"#).unwrap();
        assert_eq!(old.more, MorePrefs::default());
    }
}
