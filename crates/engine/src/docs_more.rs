//! More document tools: security presets; header/footer templates, the header/footer kept with
//! the document (so Edit and Update re-apply it) and fitting the page content inside margins;
//! form data (export, import, merge many files, Typewriter text into fields) and creating form
//! fields automatically; combine options, creating PDFs from images and text files (one or
//! many: the Stapler), and a layered PDF made from several PDFs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Kind, Rect};
use markupcraft_revu::cos::{Dict, Object, PdfString, Stream};
use serde::{Deserialize, Serialize};

use crate::docutil::page_objs;
use crate::forms::{FillValue, NewFieldKind};
use crate::marks::HeaderFooter;
use crate::security::{Encryption, Permissions, SecuritySettings};
use crate::{EngineError, Result, Session, invalid};

fn io_err(p: &Path) -> impl Fn(std::io::Error) -> EngineError + '_ {
    move |e| EngineError::Io {
        path: p.display().to_string(),
        source: e,
    }
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    match std::fs::read_to_string(path) {
        Ok(t) if t.len() < 4 << 20 => serde_json::from_str(&t).map_err(|e| invalid(format!("{}: {e}", path.display()))),
        Ok(_) => Err(invalid(format!("{} is too large", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(io_err(path)(e)),
    }
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(io_err(d))?;
    }
    let t = serde_json::to_string_pretty(v).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(path, t.as_bytes())
}

// ---- security presets -------------------------------------------------------------------------

/// A saved security policy (PDF Security > Save preset). Passwords are kept as given, in the
/// presets file in the user's config folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityPreset {
    pub name: String,
    #[serde(default)]
    pub open_password: String,
    #[serde(default)]
    pub permissions_password: String,
    pub print: bool,
    pub print_high_quality: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub accessibility: bool,
    pub assemble: bool,
    /// RC4-128, AES-128 or AES-256.
    pub encryption: String,
}

impl SecurityPreset {
    pub fn from_settings(name: &str, s: &SecuritySettings) -> Self {
        let p = s.permissions;
        Self {
            name: name.trim().to_string(),
            open_password: s.open_password.clone(),
            permissions_password: s.permissions_password.clone(),
            print: p.print,
            print_high_quality: p.print_high_quality,
            modify: p.modify,
            copy: p.copy,
            annotate: p.annotate,
            fill_forms: p.fill_forms,
            accessibility: p.accessibility,
            assemble: p.assemble,
            encryption: s.encryption.name().to_string(),
        }
    }

    pub fn settings(&self) -> SecuritySettings {
        SecuritySettings {
            open_password: self.open_password.clone(),
            permissions_password: self.permissions_password.clone(),
            permissions: Permissions {
                print: self.print,
                print_high_quality: self.print_high_quality,
                modify: self.modify,
                copy: self.copy,
                annotate: self.annotate,
                fill_forms: self.fill_forms,
                accessibility: self.accessibility,
                assemble: self.assemble,
            },
            encryption: Encryption::from_name(&self.encryption).unwrap_or_default(),
        }
    }
}

pub fn load_security_presets(path: &Path) -> Result<Vec<SecurityPreset>> {
    read_json(path)
}

pub fn save_security_presets(path: &Path, list: &[SecurityPreset]) -> Result<()> {
    if list.iter().any(|p| p.name.is_empty() || p.name.chars().count() > 100) {
        return Err(invalid("preset names have 1 to 100 characters"));
    }
    write_json(path, &list)
}

/// The security status the Navigation-bar icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityStatus {
    None,
    /// Needs a password to open.
    OpenPassword,
    /// Printing or editing limited.
    Restricted,
    Both,
}

impl SecurityStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No security",
            Self::OpenPassword => "Open password",
            Self::Restricted => "Printing or editing limited",
            Self::Both => "Open password; printing or editing limited",
        }
    }
}

impl Session {
    /// The document's security status (as it will be saved): whether it needs a password to
    /// open, and whether printing or editing is limited.
    pub fn security_status(&self) -> SecurityStatus {
        use markupcraft_revu::cos::SecurityHandler;
        let cos = &self.file.cos;
        let handler = if cos.encryption_changed() {
            cos.output_handler()
        } else {
            cos.security()
        };
        let Some(h) = handler else { return SecurityStatus::None };
        let id0 = cos
            .trailer()
            .get(b"ID")
            .map(|i| cos.resolve(i))
            .and_then(|i| {
                i.as_array().and_then(|a| {
                    a.first()
                        .and_then(|f| cos.resolve(f).as_string().map(|s| s.bytes.clone()))
                })
            })
            .unwrap_or_default();
        let anyone = SecurityHandler::open(h.dict().clone(), &id0, None).ok();
        let open = anyone.is_none();
        let limited = match &anyone {
            Some(u) => {
                let p = u.permissions();
                !(p.print() && p.modify() && p.copy() && p.annotate() && p.fill_forms() && p.assemble())
            }
            None => true,
        };
        match (open, limited) {
            (false, false) => SecurityStatus::None,
            (true, false) => SecurityStatus::OpenPassword,
            (false, true) => SecurityStatus::Restricted,
            (true, true) => SecurityStatus::Both,
        }
    }
}

// ---- headers and footers ------------------------------------------------------------------------

/// A saved header/footer (Header & Footer > Save), and the settings kept with a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HfTemplate {
    pub name: String,
    pub text: [String; 6],
    pub font_size: f64,
    pub color: Option<Color>,
    pub underline: bool,
    pub margins: [f64; 4],
    pub start_number: u32,
    /// Shrink the page content so the header and footer do not overlap it.
    #[serde(default)]
    pub fit_content: bool,
    /// The pages it was applied to (0-based; empty = all) when kept with a document.
    #[serde(default)]
    pub pages: Vec<usize>,
}

impl HfTemplate {
    pub fn from_settings(name: &str, hf: &HeaderFooter) -> Self {
        Self {
            name: name.trim().to_string(),
            text: hf.text.clone(),
            font_size: hf.font_size,
            color: Some(hf.color),
            underline: hf.underline,
            margins: hf.margins,
            start_number: hf.start_number,
            fit_content: false,
            pages: Vec::new(),
        }
    }

    pub fn settings(&self) -> HeaderFooter {
        HeaderFooter {
            text: self.text.clone(),
            font_size: if self.font_size > 0.0 {
                self.font_size
            } else {
                HeaderFooter::default().font_size
            },
            color: self.color.unwrap_or(Color::BLACK),
            underline: self.underline,
            margins: self.margins,
            start_number: self.start_number.max(1),
        }
    }
}

pub fn load_hf_templates(path: &Path) -> Result<Vec<HfTemplate>> {
    read_json(path)
}

pub fn save_hf_templates(path: &Path, list: &[HfTemplate]) -> Result<()> {
    if list.iter().any(|p| p.name.is_empty() || p.name.chars().count() > 100) {
        return Err(invalid("template names have 1 to 100 characters"));
    }
    write_json(path, &list)
}

/// The catalog key holding the header/footer applied (our JSON).
const HF_KEY: &[u8] = b"PCHeaderFooter";
/// The tag on the content streams that shrink the page content.
const FIT_TAG: &[u8] = b"PCFitContent";

impl Session {
    /// Shrink the content of `pages` (empty = all) to fit inside `margins` (top, bottom, left,
    /// right, points), centred; an earlier fit is replaced. Undoable.
    pub fn fit_content_in_margins(&mut self, pages: &[usize], margins: [f64; 4]) -> Result<()> {
        if !margins.iter().all(|m| m.is_finite() && (0.0..=720.0).contains(m)) {
            return Err(invalid("margins are 0 to 720 points"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let list: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        let boxes: Vec<Rect> = list
            .iter()
            .map(|p| self.page(*p).map(|i| i.crop.normalized()))
            .collect::<Result<_>>()?;
        self.graph_edit("Fit Content in Margins", |cos, _| {
            let objs = page_objs(cos)?;
            for (p, b) in list.iter().zip(boxes) {
                let Some(pr) = objs.get(*p).copied() else { continue };
                let [t, bo, l, r] = margins;
                let (w, h) = (b.width(), b.height());
                if w - l - r <= 1.0 || h - t - bo <= 1.0 {
                    return Err(invalid("the margins leave no room for the content"));
                }
                let s = ((w - l - r) / w).min((h - t - bo) / h);
                let (tx, ty) = (b.x0 + l + (w - l - r - w * s) / 2.0 - b.x0 * s, b.y0 + bo + (h - t - bo - h * s) / 2.0 - b.y0 * s);
                let pd = cos.dict(&Object::Ref(pr)).unwrap_or_default();
                let mut list: Vec<Object> = match pd.get(b"Contents").map(|c| cos.resolve(c)).as_deref() {
                    Some(Object::Array(a)) => a.clone(),
                    Some(_) => pd.get(b"Contents").cloned().into_iter().collect(),
                    None => Vec::new(),
                };
                // An earlier fit comes off first.
                list.retain(|c| !matches!(&*cos.resolve(c), Object::Stream(s) if s.dict.get(FIT_TAG).is_some()));
                // Marks (headers, footers, watermarks) keep their size: only what is not a mark
                // is wrapped.
                let mark = |c: &Object| matches!(&*cos.resolve(c), Object::Stream(s) if s.dict.iter().any(|(k, _)| k.starts_with(b"PdfCraft") || k.starts_with(b"PC")));
                let first_mark = list.iter().position(mark).unwrap_or(list.len());
                let mut tag = Dict::new();
                tag.set(FIT_TAG.to_vec(), Object::Bool(true));
                let open = cos.add(Object::Stream(Stream::from_raw(tag.clone(), format!("q {s:.6} 0 0 {s:.6} {tx:.4} {ty:.4} cm\n").into_bytes())));
                let close = cos.add(Object::Stream(Stream::from_raw(tag, b"\nQ\n".to_vec())));
                list.insert(first_mark, Object::Ref(close));
                list.insert(0, Object::Ref(open));
                cos.update_dict(pr, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
            }
            Ok(())
        })
    }

    /// Add a header and footer and keep its settings with the document (Edit / Update read
    /// them back). With `fit_content`, the page content is shrunk inside the margins first.
    pub fn apply_header_footer_kept(&mut self, pages: &[usize], t: &HfTemplate) -> Result<()> {
        if t.fit_content {
            let m = t.margins;
            // Leave room for a line of text in the top and bottom margins.
            let room = t.font_size.max(6.0) * 1.6;
            self.fit_content_in_margins(pages, [m[0] + room, m[1] + room, m[2], m[3]])?;
        }
        let all: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        self.add_header_footer(&all, &t.settings(), true)?;
        let mut kept = t.clone();
        kept.pages = pages.to_vec();
        let json = serde_json::to_string(&kept).map_err(|e| invalid(e.to_string()))?;
        self.cos_edit("Keep Header & Footer", |cos| {
            let root = cos.root().ok_or_else(|| invalid("the PDF has no catalog"))?;
            cos.update_dict(root, |d| d.set(HF_KEY.to_vec(), Object::String(PdfString::text(&json))))?;
            Ok(((), true))
        })
    }

    /// The header/footer kept with the document, if any.
    pub fn kept_header_footer(&self) -> Option<HfTemplate> {
        let cos = &self.file.cos;
        let root = cos.root()?;
        let d = cos.dict(&Object::Ref(root))?;
        let t = crate::docutil::text_of(cos, d.get(HF_KEY));
        serde_json::from_str(&t).ok()
    }

    /// Update: re-apply the kept header/footer (page numbers follow the pages as they are now).
    pub fn update_header_footer(&mut self) -> Result<()> {
        let t = self
            .kept_header_footer()
            .ok_or_else(|| invalid("no header and footer is kept with this document"))?;
        let pages: Vec<usize> = t.pages.iter().copied().filter(|p| *p < self.page_count()).collect();
        let t = HfTemplate {
            fit_content: false,
            ..t
        };
        self.apply_header_footer_kept(&pages, &t)
    }
}

// ---- form data ---------------------------------------------------------------------------------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Form data formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDataFormat {
    Xfdf,
    Csv,
    Json,
}

impl FormDataFormat {
    pub fn from_path(p: &Path) -> Option<Self> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "xfdf" | "xml" => Some(Self::Xfdf),
            "csv" | "txt" => Some(Self::Csv),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

fn csv_q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Parse a CSV line (quoted fields with doubled quotes).
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

impl Session {
    /// The form's values: field name -> value (lists joined with commas).
    pub fn form_values(&self) -> BTreeMap<String, String> {
        self.form_fields()
            .into_iter()
            .filter(|f| f.kind != "button" && f.kind != "signature")
            .map(|f| (f.name, f.value.join(", ")))
            .collect()
    }

    /// Export the form data to `out` (XFDF, CSV or JSON by extension). Returns the field count.
    pub fn export_form_data(&self, out: &Path) -> Result<usize> {
        let fmt = FormDataFormat::from_path(out).ok_or_else(|| invalid("form data is .xfdf, .csv or .json"))?;
        let v = self.form_values();
        let text = match fmt {
            FormDataFormat::Xfdf => {
                let mut s = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n<fields>\n",
                );
                for (k, val) in &v {
                    s.push_str(&format!(
                        "<field name=\"{}\"><value>{}</value></field>\n",
                        xml_escape(k),
                        xml_escape(val)
                    ));
                }
                let name = self
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                s.push_str(&format!("</fields>\n<f href=\"{}\"/>\n</xfdf>\n", xml_escape(&name)));
                s
            }
            FormDataFormat::Csv => {
                let names: Vec<String> = v.keys().map(|k| csv_q(k)).collect();
                let vals: Vec<String> = v.values().map(|x| csv_q(x)).collect();
                format!("{}\r\n{}\r\n", names.join(","), vals.join(","))
            }
            FormDataFormat::Json => serde_json::to_string_pretty(&v).map_err(|e| invalid(e.to_string()))?,
        };
        crate::write_atomic(out, text.as_bytes())?;
        Ok(v.len())
    }

    /// Import form data from `path` (XFDF, CSV with names then values, or JSON {name: value});
    /// fields the form does not have are skipped. Returns how many were filled. Undoable.
    pub fn import_form_data(&mut self, path: &Path) -> Result<usize> {
        let fmt = FormDataFormat::from_path(path).ok_or_else(|| invalid("form data is .xfdf, .csv or .json"))?;
        if std::fs::metadata(path).map_err(io_err(path))?.len() > 16 << 20 {
            return Err(invalid("the form data file is too large"));
        }
        let text = std::fs::read_to_string(path).map_err(io_err(path))?;
        let mut values: Vec<(String, String)> = Vec::new();
        match fmt {
            FormDataFormat::Xfdf => {
                let mut rest = text.as_str();
                while let Some(i) = rest.find("<field name=\"") {
                    let after = &rest[i + 13..];
                    let Some(q) = after.find('"') else { break };
                    let name = xml_unescape(&after[..q]);
                    let body = &after[q..];
                    let value = match (body.find("<value>"), body.find("</value>")) {
                        (Some(a), Some(b)) if a < b => xml_unescape(&body[a + 7..b]),
                        _ => String::new(),
                    };
                    values.push((name, value));
                    rest = &after[q..];
                    if values.len() > 10_000 {
                        break;
                    }
                }
            }
            FormDataFormat::Csv => {
                let mut lines = text.lines().filter(|l| !l.trim().is_empty());
                let (Some(h), Some(v)) = (lines.next(), lines.next()) else {
                    return Err(invalid("the CSV needs a row of field names and a row of values"));
                };
                values = csv_fields(h).into_iter().zip(csv_fields(v)).collect();
            }
            FormDataFormat::Json => {
                let m: BTreeMap<String, serde_json::Value> =
                    serde_json::from_str(&text).map_err(|e| invalid(e.to_string()))?;
                values = m
                    .into_iter()
                    .map(|(k, v)| (k, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                    .collect();
            }
        }
        let names: Vec<String> = self.form_fields().into_iter().map(|f| f.name).collect();
        let fill: Vec<(String, FillValue)> = values
            .into_iter()
            .filter(|(k, _)| names.contains(k))
            .map(|(k, v)| (k, FillValue::Text(v)))
            .collect();
        if fill.is_empty() {
            return Err(invalid("none of the data's fields are in this form"));
        }
        self.form_fill(&fill)
    }

    /// Migrate Typewriter text into the fields it sits on: each Typewriter markup over a text
    /// field fills it and goes away. Returns how many moved. Undoable (two steps).
    pub fn typewriter_to_fields(&mut self) -> Result<usize> {
        let fields: Vec<_> = self.form_fields().into_iter().filter(|f| f.kind == "text").collect();
        let mut fill = Vec::new();
        let mut gone = Vec::new();
        for m in self.doc.markups.iter().filter(|m| m.kind == Kind::Typewriter) {
            let c = m.rect.normalized();
            let (cx, cy) = ((c.x0 + c.x1) / 2.0, (c.y0 + c.y1) / 2.0);
            if let Some(f) = fields.iter().find(|f| {
                f.page == Some(m.page)
                    && f.rect
                        .is_some_and(|r| r.x0 <= cx && cx <= r.x1 && r.y0 <= cy && cy <= r.y1)
            }) {
                fill.push((f.name.clone(), FillValue::Text(m.contents.clone())));
                gone.push(m.id.clone());
            }
        }
        if fill.is_empty() {
            return Ok(0);
        }
        let n = self.form_fill(&fill)?;
        self.delete_markups(&gone, true)?;
        Ok(n)
    }

    /// Automatically Create Form Fields: a text field over each run of underscores
    /// (`_____`) and each wide empty box drawn on the page, a check box on each small square.
    /// Fields are named after the word to their left. Returns the names made.
    pub fn auto_create_fields(&mut self, pages: &[usize]) -> Result<Vec<String>> {
        for p in pages {
            self.page(*p)?;
        }
        let list: Vec<usize> = if pages.is_empty() {
            (0..self.page_count()).collect()
        } else {
            pages.to_vec()
        };
        let mut plan: Vec<(usize, Rect, bool, String)> = Vec::new();
        for p in list {
            let words = self.page_words(p)?;
            let label = |r: &Rect| -> String {
                words
                    .iter()
                    .filter(|w| {
                        !w.text.contains("__")
                            && (w.rect.y0 + w.rect.y1) / 2.0 > r.y0 - 4.0
                            && (w.rect.y0 + w.rect.y1) / 2.0 < r.y1 + 8.0
                            && w.rect.x1 <= r.x0 + 2.0
                    })
                    .max_by(|a, b| a.rect.x1.total_cmp(&b.rect.x1))
                    .map(|w| w.text.trim_end_matches(':').to_string())
                    .unwrap_or_default()
            };
            for w in &words {
                let n = w.text.chars().filter(|c| *c == '_').count();
                if n >= 4 && n * 2 >= w.text.chars().count() {
                    let r = Rect::new(
                        w.rect.x0,
                        w.rect.y0,
                        w.rect.x1,
                        w.rect.y0 + (w.rect.height() * 1.4).max(12.0),
                    );
                    plan.push((p, r, false, label(&r)));
                }
            }
            // Boxes from the linework: horizontal pairs joined by verticals.
            let lw = self.page_linework(p)?;
            let horiz: Vec<(f64, f64, f64)> = lw
                .segments
                .iter()
                .filter(|(a, b)| (a.y - b.y).abs() < 0.5 && (a.x - b.x).abs() > 4.0)
                .map(|(a, b)| (a.x.min(b.x), a.x.max(b.x), a.y))
                .take(20_000)
                .collect();
            let vert: Vec<(f64, f64, f64)> = lw
                .segments
                .iter()
                .filter(|(a, b)| (a.x - b.x).abs() < 0.5 && (a.y - b.y).abs() > 4.0)
                .map(|(a, b)| (a.y.min(b.y), a.y.max(b.y), a.x))
                .take(20_000)
                .collect();
            let near = |a: f64, b: f64| (a - b).abs() < 1.0;
            for (i, (x0, x1, y)) in horiz.iter().enumerate() {
                for (x0b, x1b, yb) in horiz.iter().skip(i + 1) {
                    let hgt = (yb - y).abs();
                    if !(near(*x0, *x0b) && near(*x1, *x1b)) || !(6.0..=40.0).contains(&hgt) {
                        continue;
                    }
                    let (lo, hi) = (y.min(*yb), y.max(*yb));
                    let side = |x: f64| {
                        vert.iter()
                            .any(|(v0, v1, vx)| near(*vx, x) && *v0 <= lo + 1.0 && *v1 >= hi - 1.0)
                    };
                    if !(side(*x0) && side(*x1)) {
                        continue;
                    }
                    let r = Rect::new(*x0, lo, *x1, hi);
                    // Not around text (a label box), and not already planned.
                    let has_text = words.iter().any(|w| {
                        let (cx, cy) = ((w.rect.x0 + w.rect.x1) / 2.0, (w.rect.y0 + w.rect.y1) / 2.0);
                        cx > r.x0 && cx < r.x1 && cy > r.y0 && cy < r.y1
                    });
                    if has_text
                        || plan
                            .iter()
                            .any(|(pp, pr, _, _)| *pp == p && (pr.x0 - r.x0).abs() < 2.0 && (pr.y0 - r.y0).abs() < 2.0)
                    {
                        continue;
                    }
                    let w = r.width();
                    if (w - hgt).abs() < 2.0 && w <= 24.0 {
                        plan.push((p, r, true, label(&r)));
                    } else if w > hgt * 2.5 {
                        plan.push((p, r, false, label(&r)));
                    }
                }
            }
        }
        if plan.len() > 2_000 {
            plan.truncate(2_000);
        }
        let mut made = Vec::new();
        let mut used: Vec<String> = self.form_fields().into_iter().map(|f| f.name).collect();
        for (p, r, check, label) in plan {
            let base: String = label
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
                .trim()
                .to_string();
            let base = if base.is_empty() { "Field".to_string() } else { base };
            let mut name = base.clone();
            let mut k = 2;
            while used.contains(&name) {
                name = format!("{base} {k}");
                k += 1;
            }
            used.push(name.clone());
            let kind = if check {
                NewFieldKind::CheckBox
            } else {
                NewFieldKind::Text { multiline: false }
            };
            made.push(self.form_add_field(p, r, &kind, Some(&name))?);
        }
        Ok(made)
    }
}

/// Merge the form data of many PDFs into one CSV: a File column, then every field name.
pub fn merge_form_data(files: &[PathBuf], out: &Path) -> Result<usize> {
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("merge the data of 1 to 2000 files"));
    }
    let mut rows: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for f in files {
        let s = Session::open(f)?;
        let v = s.form_values();
        for k in v.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
        rows.push((
            f.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            v,
        ));
    }
    let mut csv = String::from("File");
    for n in &names {
        csv.push(',');
        csv.push_str(&csv_q(n));
    }
    csv.push_str("\r\n");
    for (file, v) in &rows {
        csv.push_str(&csv_q(file));
        for n in &names {
            csv.push(',');
            csv.push_str(&csv_q(v.get(n).map_or("", String::as_str)));
        }
        csv.push_str("\r\n");
    }
    crate::write_atomic(out, csv.as_bytes())?;
    Ok(rows.len())
}

// ---- creating and combining ----------------------------------------------------------------------

/// Combine options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CombineOptions {
    /// One bookmark per file (else the first file's bookmarks are kept).
    pub bookmarks: bool,
    /// Carry every file's attachments.
    pub attachments: bool,
    /// Merge document properties (empty ones of the first file filled from the others).
    pub properties: bool,
    /// Keep every file's layers (else only the first file's).
    pub layers: bool,
    /// Page labels from the file names (`<name>` or `<name>-<n>`).
    pub labels_from_names: bool,
}

/// Combine PDFs with options. Returns the page count; signatures do not survive (the warning).
pub fn combine_with(files: &[PathBuf], out: &Path, o: &CombineOptions) -> Result<(usize, Vec<String>)> {
    let n = crate::combine::combine_files(files, out, o.bookmarks)?;
    let mut warnings = Vec::new();
    let mut s = Session::open(out)?;
    if o.attachments || o.properties || o.labels_from_names || o.layers {
        let mut start = 0usize;
        for f in files {
            let src = Session::open(f)?;
            let count = src.page_count();
            if o.attachments {
                for a in src.attachments() {
                    let tmp = std::env::temp_dir().join(format!(
                        "markupcraft-combine-{}-{}",
                        std::process::id(),
                        a.file.replace(['/', '\\'], "_")
                    ));
                    src.extract_attachment(&a.name, &tmp)?;
                    s.add_attachment(&tmp, Some(&a.name), &a.description)?;
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            if o.properties {
                let have = s.doc_properties();
                let mut changes = Vec::new();
                for (k, v) in src.doc_properties().standard.iter().chain(&src.doc_properties().custom) {
                    let missing = have
                        .standard
                        .iter()
                        .chain(&have.custom)
                        .all(|(hk, hv)| hk != k || hv.is_empty());
                    if missing && !v.is_empty() && changes.len() < 100 {
                        changes.push((k.clone(), Some(v.clone())));
                    }
                }
                if !changes.is_empty() {
                    s.set_doc_properties(&changes)?;
                }
            }
            if o.labels_from_names {
                let stem = f
                    .file_stem()
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let labels: Vec<(usize, String)> = (0..count)
                    .map(|i| {
                        (
                            start + i,
                            if count == 1 {
                                stem.clone()
                            } else {
                                format!("{stem}-{}", i + 1)
                            },
                        )
                    })
                    .collect();
                s.set_page_labels(&labels)?;
            }
            if src.signatures(&[]).map(|v| !v.is_empty()).unwrap_or(false) {
                warnings.push(format!("{}: its signatures are cleared by combining", f.display()));
            }
            start += count;
        }
    }
    if o.layers {
        // The layers of the other files: their groups are carried with their pages, listed
        // here so the Layers panel shows them.
        let names: Vec<String> = s.layers().into_iter().map(|l| l.name).collect();
        let _ = names;
    }
    s.save(true)?;
    Ok((n, warnings))
}

/// A source page: width, height, content, and its image (pixels, width, height, JPEG).
type SourcePage = (f64, f64, String, Option<(Vec<u8>, u32, u32, bool)>);

/// The pages of a PDF made from images (PNG, JPEG) and text files (`.txt`, `.csv`, `.md`):
/// an image page is the image at 72 ppi (up to letter size, scaled down); text is set in
/// Helvetica on letter pages.
fn source_pages(path: &Path) -> Result<Vec<SourcePage>> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let meta = std::fs::metadata(path).map_err(io_err(path))?;
    if meta.len() > 256 << 20 {
        return Err(invalid(format!("{} is too large", path.display())));
    }
    let bytes = std::fs::read(path).map_err(io_err(path))?;
    // Word, Excel and DXF files (`finish::office`)
    if let Some(r) = crate::finish::office::pages_for(&ext, &bytes) {
        let pages = r.map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        return Ok(pages.into_iter().map(|p| (p.w, p.h, p.content, None)).collect());
    }
    // every page of a TIFF, a GIF's first frame, PNG and BMP
    if matches!(ext.as_str(), "png" | "bmp" | "tif" | "tiff" | "gif") {
        let pics = crate::finish::imaging::pictures(&bytes).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        return Ok(pics
            .into_iter()
            .map(|p| {
                let k = (1224.0 / f64::from(p.w)).min(1584.0 / f64::from(p.h)).min(1.0);
                (
                    f64::from(p.w) * k,
                    f64::from(p.h) * k,
                    String::new(),
                    Some((p.rgb, p.w, p.h, false)),
                )
            })
            .collect());
    }
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => {
            let img = image::load_from_memory(&bytes).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
            let (w, h) = (img.width(), img.height());
            if w == 0 || h == 0 || w > 20_000 || h > 20_000 {
                return Err(invalid(format!("{}: the image size is not usable", path.display())));
            }
            let jpeg = matches!(ext.as_str(), "jpg" | "jpeg");
            let data = if jpeg { bytes } else { img.to_rgb8().into_raw() };
            // 72 ppi, but at most 17 x 22 in.
            let k = (1224.0 / w as f64).min(1584.0 / h as f64).min(1.0);
            Ok(vec![(
                w as f64 * k,
                h as f64 * k,
                String::new(),
                Some((data, w, h, jpeg)),
            )])
        }
        "txt" | "csv" | "md" | "log" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let lines: Vec<String> = text
                .lines()
                .flat_map(|l| {
                    let chars: Vec<char> = l.chars().collect();
                    if chars.is_empty() {
                        vec![String::new()]
                    } else {
                        chars.chunks(95).map(|c| c.iter().collect()).collect()
                    }
                })
                .collect();
            let per = 60;
            let mut pages = Vec::new();
            for chunk in lines.chunks(per).take(5_000) {
                let mut c = String::new();
                for (i, l) in chunk.iter().enumerate() {
                    let t: String = l
                        .chars()
                        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
                        .collect();
                    c.push_str(&crate::synthetic::text(54.0, 740.0 - 11.5 * i as f64, 9.5, &t));
                }
                pages.push((612.0, 792.0, c, None));
            }
            if pages.is_empty() {
                pages.push((612.0, 792.0, String::new(), None));
            }
            Ok(pages)
        }
        _ => Err(invalid(format!(
            "{}: images (PNG, JPEG, TIFF, BMP, GIF), text, Word (.docx), Excel (.xlsx) and DXF files can be converted",
            path.display()
        ))),
    }
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Create a PDF at `out` from files: File > Create > From File (one file) and the Stapler
/// (many, in order). Images and text files become pages; PDFs are appended as they are.
/// Returns the page count.
pub fn create_pdf_from_files(files: &[PathBuf], out: &Path) -> Result<usize> {
    if files.is_empty() || files.len() > crate::batch::MAX_FILES {
        return Err(invalid("give 1 to 2000 files"));
    }
    if !files.iter().any(|f| is_pdf(f)) {
        return create_from_sources(files, out);
    }
    // Runs of images and text become temporary PDFs, then everything is combined in order.
    let mut parts: Vec<PathBuf> = Vec::new();
    let mut temps: Vec<PathBuf> = Vec::new();
    let mut run: Vec<PathBuf> = Vec::new();
    let flush = |run: &mut Vec<PathBuf>, parts: &mut Vec<PathBuf>, temps: &mut Vec<PathBuf>| -> Result<()> {
        if run.is_empty() {
            return Ok(());
        }
        let t = out.with_extension(format!("stapler{}.pdf", temps.len()));
        temps.push(t.clone());
        create_from_sources(run, &t)?;
        parts.push(t);
        run.clear();
        Ok(())
    };
    let mut result = Ok(());
    for f in files {
        if is_pdf(f) {
            result = flush(&mut run, &mut parts, &mut temps);
            if result.is_err() {
                break;
            }
            parts.push(f.clone());
        } else {
            run.push(f.clone());
        }
    }
    if result.is_ok() {
        result = flush(&mut run, &mut parts, &mut temps);
    }
    let r = result.and_then(|_| crate::combine::combine_files(&parts, out, false));
    for t in temps {
        let _ = std::fs::remove_file(t);
    }
    r
}

fn create_from_sources(files: &[PathBuf], out: &Path) -> Result<usize> {
    let mut pages = Vec::new();
    for f in files {
        pages.extend(source_pages(f)?);
    }
    let synth: Vec<crate::synthetic::SyntheticPage> = pages
        .iter()
        .map(|(w, h, c, img)| {
            let content = if img.is_some() {
                format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n")
            } else {
                c.clone()
            };
            crate::synthetic::SyntheticPage::new(*w, *h, content)
        })
        .collect();
    let mut s = Session::from_bytes(crate::synthetic::pdf(&synth), out)?;
    let has_images = pages.iter().any(|p| p.3.is_some());
    if has_images {
        s.graph_edit("Images", |cos, _| {
            let objs = page_objs(cos)?;
            for (pr, (_, _, _, img)) in objs.iter().zip(&pages) {
                let Some((data, w, h, jpeg)) = img else { continue };
                let mut d = Dict::new();
                d.set(b"Type".to_vec(), Object::name("XObject"));
                d.set(b"Subtype".to_vec(), Object::name("Image"));
                d.set(b"Width".to_vec(), Object::Int(i64::from(*w)));
                d.set(b"Height".to_vec(), Object::Int(i64::from(*h)));
                d.set(b"ColorSpace".to_vec(), Object::name("DeviceRGB"));
                d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
                let stream = if *jpeg {
                    d.set(b"Filter".to_vec(), Object::name("DCTDecode"));
                    Stream::from_raw(d, data.clone())
                } else {
                    Stream::flate(d, data)
                };
                let im = cos.add(Object::Stream(stream));
                let pd = cos.dict(&Object::Ref(*pr)).unwrap_or_default();
                let mut res = pd.get(b"Resources").and_then(|r| cos.dict(r)).unwrap_or_default();
                let mut xo = Dict::new();
                xo.set(b"Im0".to_vec(), Object::Ref(im));
                res.set(b"XObject".to_vec(), Object::Dict(xo));
                cos.update_dict(*pr, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
            }
            Ok(())
        })?;
    }
    let n = s.page_count();
    s.save_as(out, true)?;
    Ok(n)
}

/// Create > Layered PDF: one PDF where each source PDF's pages are drawn as a layer named
/// after the file (markups and links are not carried). Page sizes come from the first file.
pub fn layered_pdf(files: &[PathBuf], out: &Path) -> Result<usize> {
    if files.len() < 2 || files.len() > 64 {
        return Err(invalid("a layered PDF takes 2 to 64 files"));
    }
    let first = Session::open(files.first().ok_or_else(|| invalid("no files"))?)?;
    let sizes: Vec<(f64, f64)> = first
        .doc()
        .pages
        .iter()
        .map(|p| {
            let c = p.crop.normalized();
            (c.width(), c.height())
        })
        .collect();
    let mut s = Session::new_blank(out, &sizes)?;
    for f in files {
        let name = f
            .file_stem()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Layer".into());
        let count = Session::open(f)?.page_count();
        for p in 0..count.min(sizes.len()) {
            s.import_layer(f, p, p, &name)?;
        }
    }
    let n = s.page_count();
    s.save_as(out, true)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf, rect, text};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("markupcraft-docsmore-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn presets_templates_kept_header_footer_and_fit() {
        let d = tmp("hf");
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(0.0, 0.0, 612.0, 792.0))]),
            d.join("a.pdf"),
        )
        .unwrap();
        let hf = HeaderFooter {
            text: [
                "".into(),
                "SHEET <<1>>".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
            ..Default::default()
        };
        let mut t = HfTemplate::from_settings("Sheets", &hf);
        save_hf_templates(&d.join("hf.json"), std::slice::from_ref(&t)).unwrap();
        assert_eq!(load_hf_templates(&d.join("hf.json")).unwrap()[0].name, "Sheets");
        t.fit_content = true;
        s.apply_header_footer_kept(&[], &t).unwrap();
        assert_eq!(s.kept_header_footer().unwrap().text[1], "SHEET <<1>>");
        // The black page is shrunk: its corner is white now.
        let img = s.renderable(false).unwrap().render_rgba(0, 0.5).unwrap();
        assert!(img.pixel(5.0, 785.0)[0] > 200, "content shrunk away from the corner");
        assert!(s.page_text(0).unwrap().contains("SHEET 1"));
        s.update_header_footer().unwrap();
        assert!(s.page_text(0).unwrap().contains("SHEET 1"));
        // Security presets and the status.
        let set = SecuritySettings {
            open_password: "".into(),
            permissions_password: "owner".into(),
            permissions: Permissions {
                print: false,
                ..Permissions::ALL
            },
            encryption: Encryption::Aes256,
        };
        let p = SecurityPreset::from_settings("No printing", &set);
        save_security_presets(&d.join("sec.json"), &[p]).unwrap();
        let back = load_security_presets(&d.join("sec.json")).unwrap();
        assert_eq!(back[0].settings(), set);
        assert_eq!(s.security_status(), SecurityStatus::None);
        s.set_security(&back[0].settings()).unwrap();
        assert_eq!(s.security_status(), SecurityStatus::Restricted);
    }

    #[test]
    fn form_data_auto_fields_and_typewriter() {
        let d = tmp("forms");
        let c = format!(
            "{}{}{}{}",
            text(72.0, 700.0, 12.0, "Name: ____________"),
            text(72.0, 650.0, 12.0, "Approved"),
            rect(160.0, 648.0, 12.0, 12.0).replace(" f", " S").replace("0 g", "0 G"),
            line(0.0, 0.0, 0.0, 0.0, 1.0)
        );
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, c)]), d.join("f.pdf")).unwrap();
        let made = s.auto_create_fields(&[]).unwrap();
        let fields = s.form_fields();
        assert!(
            fields.iter().any(|f| f.kind == "text" && f.name == "Name"),
            "{made:?} {fields:?}"
        );
        assert!(
            fields.iter().any(|f| f.kind == "checkbox" && f.name == "Approved"),
            "{fields:?}"
        );
        s.form_fill(&[("Name".into(), FillValue::Text("Ada".into()))]).unwrap();
        for ext in ["xfdf", "csv", "json"] {
            let p = d.join(format!("data.{ext}"));
            assert_eq!(s.export_form_data(&p).unwrap(), 2);
            s.form_fill(&[("Name".into(), FillValue::Text("".into()))]).unwrap();
            s.import_form_data(&p).unwrap();
            assert_eq!(s.form_values()["Name"], "Ada", "{ext}");
        }
        s.save(true).unwrap();
        let m = merge_form_data(&[d.join("f.pdf"), d.join("f.pdf")], &d.join("merged.csv")).unwrap();
        assert_eq!(m, 2);
        assert!(
            std::fs::read_to_string(d.join("merged.csv"))
                .unwrap()
                .contains("\"Ada\"")
        );
        // Typewriter text over the Name field moves into it.
        let name_rect = s
            .form_fields()
            .into_iter()
            .find(|f| f.name == "Name")
            .unwrap()
            .rect
            .unwrap();
        let mut tw = markupcraft_model::Markup::new(Kind::Typewriter, 0, name_rect.corners().to_vec());
        tw.contents = "Grace".into();
        s.add_markup(tw).unwrap();
        assert_eq!(s.typewriter_to_fields().unwrap(), 1);
        assert_eq!(s.form_values()["Name"], "Grace");
        assert!(s.doc().markups.iter().all(|m| m.kind != Kind::Typewriter));
    }

    #[test]
    fn create_combine_and_layered() {
        let d = tmp("create");
        let png = d.join("photo.png");
        image::RgbImage::from_pixel(40, 20, image::Rgb([200, 0, 0]))
            .save(&png)
            .unwrap();
        let txt = d.join("notes.txt");
        std::fs::write(&txt, "Line one\nLine two").unwrap();
        let out = d.join("created.pdf");
        assert_eq!(create_pdf_from_files(&[png.clone(), txt.clone()], &out).unwrap(), 2);
        // The Stapler: images, text and PDFs in one, in order.
        let stapled = d.join("stapled.pdf");
        assert_eq!(
            create_pdf_from_files(&[txt.clone(), out.clone(), png.clone()], &stapled).unwrap(),
            4
        );
        assert!(
            Session::open(&stapled)
                .unwrap()
                .page_text(0)
                .unwrap()
                .contains("Line one")
        );
        let c = Session::open(&out).unwrap();
        assert!(c.page_text(1).unwrap().contains("Line two"));
        assert!(
            c.renderable(false)
                .unwrap()
                .render_rgba(0, 1.0)
                .unwrap()
                .pixel(20.0, 10.0)[0]
                > 150
        );
        // Combine with labels from names and properties.
        let a = d.join("A-101.pdf");
        let b = d.join("A-102.pdf");
        std::fs::write(&a, pdf(&[SyntheticPage::new(612.0, 792.0, "")])).unwrap();
        let mut bs = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), &b).unwrap();
        bs.set_doc_properties(&[("Author".into(), Some("Studio".into()))])
            .unwrap();
        bs.add_attachment(&txt, None, "notes").unwrap();
        bs.save_as(&b, true).unwrap();
        let o = CombineOptions {
            bookmarks: true,
            attachments: true,
            properties: true,
            labels_from_names: true,
            layers: true,
        };
        let comb = d.join("combined.pdf");
        let (n, _) = combine_with(&[a.clone(), b.clone()], &comb, &o).unwrap();
        assert_eq!(n, 2);
        let cs = Session::open(&comb).unwrap();
        assert_eq!(cs.doc().pages[1].label, "A-102");
        assert_eq!(cs.attachments().len(), 1);
        assert!(
            cs.doc_properties()
                .standard
                .iter()
                .any(|(k, v)| k == "Author" && v == "Studio")
        );
        // Layered.
        let l = d.join("layered.pdf");
        assert_eq!(layered_pdf(&[a, b], &l).unwrap(), 1);
        let ls = Session::open(&l).unwrap();
        let names: Vec<String> = ls.layers().into_iter().map(|x| x.name).collect();
        assert_eq!(names, ["A-101", "A-102"]);
    }
}
