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
}

impl Default for TabletPrefs {
    fn default() -> Self {
        Self {
            eraser_scales_with_zoom: false,
            eraser_px: 8.0,
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
}

impl Default for SetsPrefs {
    fn default() -> Self {
        Self {
            open_in_place: false,
            latest_only: true,
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
}

impl Default for ImportExportPrefs {
    fn default() -> Self {
        Self {
            image_dpi: 150,
            capture_max_side: 4096,
            scan_dpi: 300,
            scanner_url: String::new(),
        }
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
        let ie = &self.import_export;
        if !(36..=1200).contains(&ie.image_dpi)
            || !(50..=1200).contains(&ie.scan_dpi)
            || !(256..=16_384).contains(&ie.capture_max_side)
        {
            return Err(invalid(
                "more.import_export: image dpi 36 to 1200, scan dpi 50 to 1200, capture size 256 to 16384",
            ));
        }
        if !ie.scanner_url.is_empty() && crate::devices::parse_http_url(&ie.scanner_url).is_err() {
            return Err(invalid("more.import_export.scanner_url: an http:// eSCL address"));
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
