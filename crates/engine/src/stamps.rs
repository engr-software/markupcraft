//! Stamps: our own text stamp designs (Approved, Rejected, Reviewed, Draft, For Construction,
//! Void ... drawn by `markupcraft_revu::kinds::draw`), dynamic fields filled in when a stamp is
//! placed, custom image stamps from a PNG or a PDF page, and a stamp library kept in the
//! config folder.
//!
//! Dynamic fields:
//!
//! ```text
//! {user}  {date}  {date:FMT}  {time}  {time:FMT}  {datetime:FMT}
//! {page} (label, else number)  {pagenum}  {pages}  {file}  {filename}  {path}
//! {prompt:Label}  {prompt:Label=Default}       answered when placing
//! FMT letters: yyyy yy MMMM MMM MM M dd d HH H hh h mm ss AP ap   (an unknown field stays as typed)
//! ```
//!
//! Library (`<config>/stamps/library.json`): `[{ "id", "name", "text", "color", "image" }]`,
//! image files copied beside it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use markupcraft_model::{Color, Kind, Markup, Point, Rect};
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, Stream};
use markupcraft_revu::kinds::draw::{STAMP_DESIGNS, find_stamp};
use serde::{Deserialize, Serialize};

use crate::docutil::{annots_of, page_objs, set_annots};
use crate::{EngineError, Result, Session, invalid};

/// Largest image (pixels per side) a stamp takes.
pub const MAX_IMAGE_SIDE: u32 = 4096;
/// Largest stamp source file read.
const MAX_SOURCE: u64 = 256 << 20;
/// Most objects copied from a PDF stamp source.
const MAX_OBJECTS: usize = 200_000;
/// Longest stamp text.
const MAX_TEXT: usize = 2_000;
/// Default width of an image stamp placed by its centre, points.
const IMAGE_WIDTH: f64 = 144.0;
/// Default size of a text stamp placed by its centre.
const TEXT_SIZE: (f64, f64) = (170.0, 60.0);

// ---- dynamic fields ------------------------------------------------------------------------

/// What the dynamic fields are filled with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StampContext {
    pub user: String,
    /// full path ("" = none)
    pub path: String,
    pub page_label: String,
    /// 1-based
    pub page_number: usize,
    pub page_count: usize,
    /// seconds since 1970 (UTC unless the caller passes local time)
    pub when: i64,
}

fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Format a time with the letters above.
pub fn format_time(secs: i64, fmt: &str) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    let month = MONTHS.get((mo as usize).saturating_sub(1)).copied().unwrap_or("");
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    let tokens: [(&str, String); 17] = [
        ("yyyy", format!("{y:04}")),
        ("yy", format!("{:02}", y.rem_euclid(100))),
        ("MMMM", month.to_string()),
        ("MMM", month.chars().take(3).collect()),
        ("MM", format!("{mo:02}")),
        ("M", mo.to_string()),
        ("dd", format!("{d:02}")),
        ("d", d.to_string()),
        ("HH", format!("{h:02}")),
        ("H", h.to_string()),
        ("hh", format!("{h12:02}")),
        ("h", h12.to_string()),
        ("mm", format!("{mi:02}")),
        ("ss", format!("{s:02}")),
        ("AP", if h < 12 { "AM" } else { "PM" }.to_string()),
        ("ap", if h < 12 { "am" } else { "pm" }.to_string()),
        ("m", mi.to_string()),
    ];
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (t, v) in &tokens {
            if let Some(r) = rest.strip_prefix(t) {
                out.push_str(v);
                rest = r;
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/// A `{prompt:...}` field: its label and default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampPrompt {
    pub label: String,
    pub default: String,
}

fn fields(tmpl: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(open) = tmpl.get(i..).and_then(|t| t.find('{')).map(|o| o + i) {
        let Some(close) = tmpl.get(open..).and_then(|t| t.find('}')).map(|c| c + open) else {
            break;
        };
        if let Some(inner) = tmpl.get(open + 1..close) {
            out.push((open, close + 1, inner));
        }
        i = close + 1;
    }
    out
}

/// The prompts of a template, in order, without duplicates.
pub fn stamp_prompts(tmpl: &str) -> Vec<StampPrompt> {
    let mut out: Vec<StampPrompt> = Vec::new();
    for (_, _, f) in fields(tmpl) {
        if let Some(p) = f.strip_prefix("prompt:") {
            let (label, default) = p.split_once('=').unwrap_or((p, ""));
            if !out.iter().any(|x| x.label == label) {
                out.push(StampPrompt {
                    label: label.to_string(),
                    default: default.to_string(),
                });
            }
        }
    }
    out
}

/// Fill every field of `tmpl`.
pub fn expand_fields(tmpl: &str, c: &StampContext, answers: &BTreeMap<String, String>) -> String {
    let file = Path::new(&c.path);
    let mut out = String::new();
    let mut last = 0;
    for (a, b, f) in fields(tmpl) {
        out.push_str(tmpl.get(last..a).unwrap_or_default());
        let (name, arg) = f.split_once(':').unwrap_or((f, ""));
        let v = match name {
            "user" => Some(c.user.clone()),
            "date" => Some(format_time(c.when, if arg.is_empty() { "yyyy-MM-dd" } else { arg })),
            "time" => Some(format_time(c.when, if arg.is_empty() { "h:mm AP" } else { arg })),
            "datetime" => Some(format_time(
                c.when,
                if arg.is_empty() { "yyyy-MM-dd h:mm AP" } else { arg },
            )),
            "page" => Some(if c.page_label.is_empty() {
                c.page_number.to_string()
            } else {
                c.page_label.clone()
            }),
            "pagenum" => Some(c.page_number.to_string()),
            "pages" => Some(c.page_count.to_string()),
            "file" => Some(
                file.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            "filename" => Some(
                file.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            "path" => Some(c.path.clone()),
            "prompt" => {
                let (label, default) = arg.split_once('=').unwrap_or((arg, ""));
                Some(answers.get(label).cloned().unwrap_or_else(|| default.to_string()))
            }
            _ => None,
        };
        match v {
            Some(v) => out.push_str(&v),
            None => out.push_str(tmpl.get(a..b).unwrap_or_default()),
        }
        last = b;
    }
    out.push_str(tmpl.get(last..).unwrap_or_default());
    out
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// ---- the library ------------------------------------------------------------------------------

/// A stamp in the library (built-in designs are listed too, `builtin: true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StampEntry {
    pub id: String,
    pub name: String,
    /// text template (text stamps); the first line is drawn big
    #[serde(default)]
    pub text: String,
    /// `#RRGGBB`
    #[serde(default)]
    pub color: String,
    /// image file name in the library folder (image / PDF stamps)
    #[serde(default)]
    pub image: Option<String>,
    #[serde(skip)]
    pub builtin: bool,
}

/// Our built-in designs.
pub fn builtin_stamps() -> Vec<StampEntry> {
    STAMP_DESIGNS
        .iter()
        .map(|d| StampEntry {
            id: d.id.to_string(),
            name: d.text.to_string(),
            text: format!("{}\r{{user}}  {{date}}", d.text),
            color: d.color.hex(),
            image: None,
            builtin: true,
        })
        .collect()
}

/// A stamp library folder.
#[derive(Debug, Clone, PartialEq)]
pub struct StampLibrary {
    pub dir: PathBuf,
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> EngineError + '_ {
    move |e| EngineError::Io {
        path: path.display().to_string(),
        source: e,
    }
}

impl StampLibrary {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The library in a config folder (`<config>/stamps`).
    pub fn in_config(config: &Path) -> Self {
        Self::new(config.join("stamps"))
    }

    fn file(&self) -> PathBuf {
        self.dir.join("library.json")
    }

    /// The user's stamps (not the built-in ones).
    pub fn custom(&self) -> Result<Vec<StampEntry>> {
        let f = self.file();
        if !f.exists() {
            return Ok(Vec::new());
        }
        if std::fs::metadata(&f).map_err(io(&f))?.len() > 16 << 20 {
            return Err(invalid("the stamp library file is too large"));
        }
        let text = std::fs::read_to_string(&f).map_err(io(&f))?;
        serde_json::from_str(&text).map_err(|e| invalid(format!("the stamp library is damaged: {e}")))
    }

    /// Built-in designs, then the user's stamps.
    pub fn all(&self) -> Result<Vec<StampEntry>> {
        let mut v = builtin_stamps();
        v.extend(self.custom()?);
        Ok(v)
    }

    pub fn find(&self, id: &str) -> Result<StampEntry> {
        self.all()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| invalid(format!("no stamp {id:?} (stamp_list shows them)")))
    }

    fn store(&self, list: &[StampEntry]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(io(&self.dir))?;
        let text = serde_json::to_string_pretty(list).map_err(|e| invalid(e.to_string()))?;
        crate::write_atomic(&self.file(), text.as_bytes())
    }

    /// Add a text stamp (Create Stamp); returns its id.
    pub fn add_text(&self, name: &str, text: &str, color: Color) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(invalid("a stamp name has 1 to 128 characters"));
        }
        if text.trim().is_empty() || text.chars().count() > MAX_TEXT {
            return Err(invalid(format!("a stamp's text has 1 to {MAX_TEXT} characters")));
        }
        let mut list = self.custom()?;
        let id = format!("custom-{}", markupcraft_revu::new_markup_id().to_lowercase());
        list.push(StampEntry {
            id: id.clone(),
            name: name.to_string(),
            text: text.to_string(),
            color: color.hex(),
            image: None,
            builtin: false,
        });
        self.store(&list)?;
        Ok(id)
    }

    /// Add an image stamp from a PNG or PDF file (copied into the library); returns its id.
    pub fn add_image(&self, name: &str, source: &Path) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(invalid("a stamp name has 1 to 128 characters"));
        }
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext != "png" && ext != "pdf" {
            return Err(invalid("an image stamp comes from a .png or a .pdf file"));
        }
        let bytes = read_source(source)?;
        // check it now, not when it is placed
        match ext.as_str() {
            "png" => {
                decode_png(&bytes)?;
            }
            _ => {
                CosDoc::open(Arc::new(bytes.clone()))?;
            }
        }
        let id = format!("custom-{}", markupcraft_revu::new_markup_id().to_lowercase());
        let file = format!("{id}.{ext}");
        std::fs::create_dir_all(&self.dir).map_err(io(&self.dir))?;
        crate::write_atomic(&self.dir.join(&file), &bytes)?;
        let mut list = self.custom()?;
        list.push(StampEntry {
            id: id.clone(),
            name: name.to_string(),
            text: String::new(),
            color: String::new(),
            image: Some(file),
            builtin: false,
        });
        self.store(&list)?;
        Ok(id)
    }

    /// Remove a user stamp (and its image).
    pub fn remove(&self, id: &str) -> Result<()> {
        let mut list = self.custom()?;
        let Some(i) = list.iter().position(|e| e.id == id) else {
            return Err(invalid(format!(
                "no user stamp {id:?} (built-in designs cannot be removed)"
            )));
        };
        let e = list.remove(i);
        if let Some(img) = e.image.filter(|f| !f.contains(['/', '\\']) && !f.starts_with('.')) {
            let _ = std::fs::remove_file(self.dir.join(img));
        }
        self.store(&list)
    }

    /// The image file of an image stamp (inside the library folder only).
    pub fn image_path(&self, e: &StampEntry) -> Option<PathBuf> {
        let f = e.image.as_ref()?;
        if f.contains(['/', '\\']) || f.starts_with('.') {
            return None;
        }
        Some(self.dir.join(f))
    }
}

// ---- images ----------------------------------------------------------------------------------

fn read_source(path: &Path) -> Result<Vec<u8>> {
    let len = std::fs::metadata(path).map_err(io(path))?.len();
    if len > MAX_SOURCE {
        return Err(invalid(format!("{} is too large for a stamp", path.display())));
    }
    std::fs::read(path).map_err(io(path))
}

struct Rgba {
    w: u32,
    h: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

fn decode_png(bytes: &[u8]) -> Result<Rgba> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| invalid(format!("not a readable PNG: {e}")))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    let mut alpha = Vec::with_capacity((w as usize) * (h as usize));
    for p in img.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p.0[3]);
    }
    let alpha = alpha.iter().any(|a| *a < 255).then_some(alpha);
    Ok(Rgba { w, h, rgb, alpha })
}

fn name(s: &str) -> Object {
    Object::name(s)
}

fn rect_obj(r: &Rect) -> Object {
    Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect())
}

/// Copies objects from another document, following references (never `/Parent`).
struct Importer<'a> {
    src: &'a CosDoc,
    map: std::collections::HashMap<ObjRef, ObjRef>,
    queue: Vec<ObjRef>,
    copied: usize,
}

impl Importer<'_> {
    fn rewrite(&mut self, dst: &mut CosDoc, o: &Object, depth: usize) -> Object {
        if depth > 64 {
            return Object::Null;
        }
        match o {
            Object::Ref(r) => match self.map.get(r) {
                Some(n) => Object::Ref(*n),
                None => {
                    let n = dst.add(Object::Null);
                    self.map.insert(*r, n);
                    self.queue.push(*r);
                    Object::Ref(n)
                }
            },
            Object::Array(a) => Object::Array(a.iter().map(|x| self.rewrite(dst, x, depth + 1)).collect()),
            Object::Dict(d) => Object::Dict(self.dict(dst, d, depth)),
            Object::Stream(s) => Object::Stream(Stream {
                dict: self.dict(dst, &s.dict, depth),
                raw: s.raw.clone(),
            }),
            other => other.clone(),
        }
    }

    fn dict(&mut self, dst: &mut CosDoc, d: &Dict, depth: usize) -> Dict {
        d.iter()
            .filter(|(k, _)| k.as_slice() != b"Parent")
            .map(|(k, v)| (k.clone(), self.rewrite(dst, v, depth + 1)))
            .collect()
    }

    fn drain(&mut self, dst: &mut CosDoc) -> Result<()> {
        while let Some(r) = self.queue.pop() {
            self.copied += 1;
            if self.copied > MAX_OBJECTS {
                return Err(invalid("the stamp's PDF page references too many objects"));
            }
            let Some(new) = self.map.get(&r).copied() else { continue };
            let obj = self.src.get(r);
            let out = self.rewrite(dst, &obj, 0);
            dst.set(new, out);
        }
        Ok(())
    }
}

/// What a stamp is drawn from, made ready to place: an XObject and its natural size.
enum Art {
    Image(Rgba),
    Page { src: CosDoc, page: usize },
}

fn load_art(path: &Path, page: usize) -> Result<Art> {
    let bytes = read_source(path)?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "png" {
        return Ok(Art::Image(decode_png(&bytes)?));
    }
    let src = CosDoc::open(Arc::new(bytes))?;
    let n = markupcraft_revu::pdf::pages(&src).len();
    if page >= n {
        return Err(invalid(format!(
            "page {} is not in {} ({n} pages)",
            page + 1,
            path.display()
        )));
    }
    Ok(Art::Page { src, page })
}

impl Art {
    /// Natural size in points (an image at 72 ppi scaled to [`IMAGE_WIDTH`] wide).
    fn size(&self) -> (f64, f64) {
        match self {
            Art::Image(i) => {
                let h = IMAGE_WIDTH * f64::from(i.h) / f64::from(i.w.max(1));
                (IMAGE_WIDTH, h)
            }
            Art::Page { src, page } => {
                let p = markupcraft_revu::pdf::pages(src);
                let b = p.get(*page).map(|p| p.crop.unwrap_or(p.media)).unwrap_or_default();
                (b.width().abs().max(1.0), b.height().abs().max(1.0))
            }
        }
    }

    /// Add the XObject to `dst`; returns it and the box it draws in (its own space).
    fn add_to(&self, dst: &mut CosDoc) -> Result<(ObjRef, Rect)> {
        match self {
            Art::Image(i) => {
                let mut d = Dict::new();
                d.set(b"Type".to_vec(), name("XObject"));
                d.set(b"Subtype".to_vec(), name("Image"));
                d.set(b"Width".to_vec(), Object::Int(i64::from(i.w)));
                d.set(b"Height".to_vec(), Object::Int(i64::from(i.h)));
                d.set(b"ColorSpace".to_vec(), name("DeviceRGB"));
                d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
                if let Some(a) = &i.alpha {
                    let mut m = Dict::new();
                    m.set(b"Type".to_vec(), name("XObject"));
                    m.set(b"Subtype".to_vec(), name("Image"));
                    m.set(b"Width".to_vec(), Object::Int(i64::from(i.w)));
                    m.set(b"Height".to_vec(), Object::Int(i64::from(i.h)));
                    m.set(b"ColorSpace".to_vec(), name("DeviceGray"));
                    m.set(b"BitsPerComponent".to_vec(), Object::Int(8));
                    let mr = dst.add(Object::Stream(Stream::flate(m, a)));
                    d.set(b"SMask".to_vec(), Object::Ref(mr));
                }
                let r = dst.add(Object::Stream(Stream::flate(d, &i.rgb)));
                Ok((r, Rect::new(0.0, 0.0, 1.0, 1.0)))
            }
            Art::Page { src, page } => {
                let pages = markupcraft_revu::pdf::pages(src);
                let p = pages.get(*page).ok_or_else(|| invalid("the stamp page vanished"))?;
                let b = p.crop.unwrap_or(p.media).normalized();
                let mut content = Vec::new();
                let contents = p.dict.get(b"Contents").map(|c| src.resolve(c));
                let mut streams = Vec::new();
                match contents.as_deref() {
                    Some(Object::Array(a)) => streams.extend(a.iter().map(|x| src.resolve(x))),
                    Some(Object::Stream(_)) => streams.extend(contents.clone()),
                    _ => {}
                }
                for s in streams {
                    if let Object::Stream(st) = &*s {
                        let left = (64usize << 20).saturating_sub(content.len());
                        content.extend(st.decoded_within(left)?);
                        content.push(b'\n');
                    }
                }
                let mut imp = Importer {
                    src,
                    map: std::collections::HashMap::new(),
                    queue: Vec::new(),
                    copied: 0,
                };
                let res = p
                    .dict
                    .get(b"Resources")
                    .map(|r| imp.rewrite(dst, r, 0))
                    .unwrap_or(Object::Dict(Dict::new()));
                imp.drain(dst)?;
                let mut d = Dict::new();
                d.set(b"Type".to_vec(), name("XObject"));
                d.set(b"Subtype".to_vec(), name("Form"));
                d.set(b"BBox".to_vec(), rect_obj(&b));
                d.set(b"Resources".to_vec(), res);
                let r = dst.add(Object::Stream(Stream::flate(d, &content)));
                Ok((r, b))
            }
        }
    }
}

/// A stamp to place.
#[derive(Debug, Clone, PartialEq)]
pub enum StampSource {
    /// a built-in design or a library stamp, by id
    Library(String),
    /// text (first line big) in a colour
    Text { text: String, color: Color },
    /// a PNG, or a page (0-based) of a PDF
    File { path: PathBuf, page: usize },
}

/// Where a stamp goes: a box, or its centre (default size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StampPlace {
    Rect(Rect),
    Center(Point),
}

impl Session {
    /// The dynamic-field context of page `page` now.
    pub fn stamp_context(&self, page: usize) -> Result<StampContext> {
        let info = self.page(page)?;
        Ok(StampContext {
            user: self.author.clone(),
            path: self.path().display().to_string(),
            page_label: info.label.clone(),
            page_number: page + 1,
            page_count: self.page_count(),
            when: now_secs(),
        })
    }

    /// Place a stamp. `library` resolves library ids; `answers` fill `{prompt:...}` fields;
    /// `when` overrides the time the date fields show. Returns the new markup's id. Undoable.
    pub fn place_stamp(
        &mut self,
        page: usize,
        at: StampPlace,
        source: &StampSource,
        library: Option<&StampLibrary>,
        answers: &BTreeMap<String, String>,
        when: Option<i64>,
    ) -> Result<String> {
        let mut ctx = self.stamp_context(page)?;
        if let Some(w) = when {
            ctx.when = w;
        }
        let (text, color, design, file) = match source {
            StampSource::Text { text, color } => (text.clone(), *color, "Custom".to_string(), None),
            StampSource::File { path, page } => (String::new(), Color::RED, String::new(), Some((path.clone(), *page))),
            StampSource::Library(id) => {
                let e = match library {
                    Some(l) => l.find(id)?,
                    None => builtin_stamps()
                        .into_iter()
                        .find(|e| e.id == *id)
                        .ok_or_else(|| invalid(format!("no stamp {id:?} (stamp_list shows them)")))?,
                };
                match (&e.image, library) {
                    (Some(_), Some(l)) => {
                        let p = l
                            .image_path(&e)
                            .ok_or_else(|| invalid("the stamp's image file name is not valid"))?;
                        (String::new(), Color::RED, String::new(), Some((p, 0)))
                    }
                    _ => {
                        let color = crate::props::parse_color(&e.color).unwrap_or(Color::RED);
                        let design = if find_stamp(&e.id).is_some() {
                            e.id.clone()
                        } else {
                            "Custom".into()
                        };
                        (e.text.clone(), color, design, None)
                    }
                }
            }
        };
        if let Some((path, src_page)) = file {
            return self.place_image_stamp(page, at, &path, src_page);
        }
        if text.chars().count() > MAX_TEXT {
            return Err(invalid(format!("a stamp's text has at most {MAX_TEXT} characters")));
        }
        let contents = expand_fields(&text, &ctx, answers);
        if contents.trim().is_empty() {
            return Err(invalid("the stamp has no text"));
        }
        let r = match at {
            StampPlace::Rect(r) => r.normalized(),
            StampPlace::Center(c) => Rect::new(
                c.x - TEXT_SIZE.0 / 2.0,
                c.y - TEXT_SIZE.1 / 2.0,
                c.x + TEXT_SIZE.0 / 2.0,
                c.y + TEXT_SIZE.1 / 2.0,
            ),
        };
        let mut m = Markup::new(Kind::Stamp, page, r.corners().to_vec());
        m.stamp = design;
        m.contents = contents;
        m.color = color;
        m.text.color = color;
        m.text.bold = true;
        m.line_width = 2.5;
        m.subject = "Stamp".into();
        self.add_markup(m)
    }

    fn place_image_stamp(&mut self, page: usize, at: StampPlace, path: &Path, src_page: usize) -> Result<String> {
        let art = load_art(path, src_page)?;
        let (w, h) = art.size();
        let r = match at {
            StampPlace::Rect(r) => r.normalized(),
            StampPlace::Center(c) => Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0),
        };
        if !r.as_array().iter().all(|v| v.is_finite()) || r.width() < 1.0 || r.height() < 1.0 {
            return Err(invalid("the stamp box must be at least 1 point wide and high"));
        }
        let id = self.new_id();
        let author = self.author.clone();
        let nm = id.clone();
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Image".into());
        let is_picture = !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        self.graph_edit("Add Stamp", move |cos, _| {
            let pages = page_objs(cos)?;
            let pref = *pages.get(page).ok_or(EngineError::NoPage {
                page: page + 1,
                count: pages.len(),
            })?;
            let (xo, b) = art.add_to(cos)?;
            // map the art's box onto the stamp's box
            let (sx, sy) = (r.width() / b.width().max(1e-9), r.height() / b.height().max(1e-9));
            let (tx, ty) = (r.x0 - b.x0 * sx, r.y0 - b.y0 * sy);
            let content = format!("q {sx:.6} 0 0 {sy:.6} {tx:.4} {ty:.4} cm /Art Do Q\n");
            let mut xobjs = Dict::new();
            xobjs.set(b"Art".to_vec(), Object::Ref(xo));
            let mut res = Dict::new();
            res.set(b"XObject".to_vec(), Object::Dict(xobjs));
            let mut sd = Dict::new();
            sd.set(b"Type".to_vec(), name("XObject"));
            sd.set(b"Subtype".to_vec(), name("Form"));
            sd.set(b"BBox".to_vec(), rect_obj(&r));
            sd.set(b"Resources".to_vec(), Object::Dict(res));
            let ap = cos.add(Object::Stream(Stream::flate(sd, content.as_bytes())));
            let mut apd = Dict::new();
            apd.set(b"N".to_vec(), Object::Ref(ap));
            let mut a = Dict::new();
            a.set(b"Type".to_vec(), name("Annot"));
            a.set(b"Subtype".to_vec(), name("Stamp"));
            a.set(b"NM".to_vec(), Object::String(PdfString::text(&nm)));
            a.set(b"Name".to_vec(), name(&title.replace([' ', '/', '#', '(', ')'], "_")));
            // a picture file is an Image markup (a PDF page stays a Stamp)
            a.set(
                b"Subj".to_vec(),
                Object::String(PdfString::text(if is_picture { "Image" } else { "Stamp" })),
            );
            a.set(b"Contents".to_vec(), Object::String(PdfString::text(&title)));
            a.set(b"T".to_vec(), Object::String(PdfString::text(&author)));
            a.set(b"F".to_vec(), Object::Int(4));
            a.set(b"P".to_vec(), Object::Ref(pref));
            a.set(b"Rect".to_vec(), rect_obj(&r));
            a.set(b"AP".to_vec(), Object::Dict(apd));
            let now = markupcraft_revu::pdf_date_now();
            a.set(b"CreationDate".to_vec(), Object::String(PdfString::text(&now)));
            a.set(b"M".to_vec(), Object::String(PdfString::text(&now)));
            let ar = cos.add(Object::Dict(a));
            let mut list = annots_of(cos, pref);
            list.push(Object::Ref(ar));
            set_annots(cos, pref, list)
        })?;
        self.selection = vec![id.clone()];
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_expand_dates_pages_files_and_prompts() {
        let c = StampContext {
            user: "Reviewer".into(),
            path: "/jobs/A-101 Plans.pdf".into(),
            page_label: "A-101".into(),
            page_number: 3,
            page_count: 12,
            // 2026-10-09 14:05:09 UTC
            when: 1_791_554_709,
        };
        let mut ans = BTreeMap::new();
        ans.insert("Job".to_string(), "1021".to_string());
        let t = expand_fields(
            "{user} {date} {time} {date:dd MMM yyyy} {page} {pagenum}/{pages} {filename} {prompt:Job} {prompt:Phase=Rough} {nope}",
            &c,
            &ans,
        );
        assert_eq!(
            t,
            "Reviewer 2026-10-09 2:05 PM 09 Oct 2026 A-101 3/12 A-101 Plans 1021 Rough {nope}"
        );
        assert_eq!(
            stamp_prompts("{prompt:Job}{prompt:Phase=Rough}{prompt:Job}"),
            vec![
                StampPrompt {
                    label: "Job".into(),
                    default: String::new()
                },
                StampPrompt {
                    label: "Phase".into(),
                    default: "Rough".into()
                }
            ]
        );
        assert_eq!(format_time(0, "yyyy-MM-dd HH:mm:ss ap"), "1970-01-01 00:00:00 am");
    }
}
