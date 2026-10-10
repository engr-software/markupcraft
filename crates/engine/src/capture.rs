//! File Attachment markups (a file embedded in the PDF, shown as a paperclip or pin icon on the
//! page), the capture summary of every file attached that way (and exporting them to a
//! folder), and snapshots: a page region, the whole page (Copy Page to Snapshot) or a Space's
//! box, copied as a Snapshot markup ready to paste.
//!
//! File Attachment annotations (ISO 32000-1 §12.5.6.15) are written straight into the page's
//! annotation list with our own icon appearance; they then load as `File Attachment` markups
//! (listed, moved by their file, deleted like any markup) and keep their appearance on save.

use std::path::{Path, PathBuf};

use markupcraft_geom::bbox;
use markupcraft_model::{Color, Kind, Markup, Point, Rect, SnapshotSource};
use markupcraft_revu::cos::{Dict, Object, PdfString, Stream};

use crate::attachments::MAX_ATTACHMENT;
use crate::docutil::{annots_of, page_objs, set_annots, text_of};
use crate::{EngineError, Result, Session, invalid};

/// Icon size in points.
const ICON: f64 = 20.0;

/// One File Attachment markup.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentMarkup {
    pub id: String,
    pub page: usize,
    pub rect: Rect,
    pub file: String,
    pub description: String,
    pub size: Option<u64>,
}

/// The icon a File Attachment shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachIcon {
    Paperclip,
    PushPin,
}

impl AttachIcon {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "paperclip" | "clip" => Some(Self::Paperclip),
            "pushpin" | "pin" => Some(Self::PushPin),
            _ => None,
        }
    }

    fn pdf_name(self) -> &'static str {
        match self {
            Self::Paperclip => "Paperclip",
            Self::PushPin => "PushPin",
        }
    }

    /// Our own drawing of the icon in a 20 x 20 box.
    fn drawing(self, c: Color) -> String {
        let (r, g, b) = (c.r, c.g, c.b);
        match self {
            // A paperclip: two nested rounded loops.
            Self::Paperclip => format!(
                "q {r} {g} {b} RG 1.6 w 1 J 1 j 7 4 m 7 15 l 7 18 13 18 13 15 c 13 6 l 13 3.5 9.5 3.5 9.5 6 c 9.5 14 l S Q"
            ),
            // A push pin: a head and a needle.
            Self::PushPin => {
                format!("q {r} {g} {b} rg {r} {g} {b} RG 1.2 w 6 12 8 6 re f 4 11 12 2 re f 10 11 m 10 2 l S Q")
            }
        }
    }
}

fn embedded_bytes(cos: &markupcraft_revu::cos::Document, fs: &Dict) -> Option<Vec<u8>> {
    let ef = cos.dict(fs.get(b"EF")?)?;
    let s = ef.get(b"UF").or_else(|| ef.get(b"F"))?;
    match &*cos.resolve(s) {
        Object::Stream(st) => st.decoded_within(MAX_ATTACHMENT).ok(),
        _ => None,
    }
}

impl Session {
    /// Tools > File Attachment: embed the file at `path` as a markup at `at` on `page` (its
    /// icon's lower-left corner). Returns the markup id. Undoable.
    pub fn add_file_attachment(
        &mut self,
        page: usize,
        at: Point,
        path: &Path,
        icon: AttachIcon,
        description: &str,
    ) -> Result<String> {
        self.page(page)?;
        if !(at.x.is_finite() && at.y.is_finite()) {
            return Err(invalid("the position must be numbers"));
        }
        let io = |e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        };
        let meta = std::fs::metadata(path).map_err(io)?;
        if !meta.is_file() || meta.len() > MAX_ATTACHMENT as u64 {
            return Err(invalid(format!(
                "{} is not a file of at most {} MB",
                path.display(),
                MAX_ATTACHMENT >> 20
            )));
        }
        if description.chars().count() > 10_000 {
            return Err(invalid("the description is too long"));
        }
        let data = markupcraft_revu::fsio::read(path).map_err(io)?;
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".into());
        let id = self.new_id();
        let author = self.author.clone();
        let description = description.to_string();
        let nm = id.clone();
        self.graph_edit("Add File Attachment", move |cos, _| {
            let pages = page_objs(cos)?;
            let pref = *pages.get(page).ok_or(EngineError::NoPage {
                page: page + 1,
                count: pages.len(),
            })?;
            let mut params = Dict::new();
            params.set(
                b"Size".to_vec(),
                Object::Int(i64::try_from(data.len()).unwrap_or(i64::MAX)),
            );
            let mut sd = Dict::new();
            sd.set(b"Type".to_vec(), Object::name("EmbeddedFile"));
            sd.set(b"Params".to_vec(), Object::Dict(params));
            let stream = cos.add(Object::Stream(Stream::flate(sd, &data)));
            let mut ef = Dict::new();
            ef.set(b"F".to_vec(), Object::Ref(stream));
            ef.set(b"UF".to_vec(), Object::Ref(stream));
            let mut fs = Dict::new();
            fs.set(b"Type".to_vec(), Object::name("Filespec"));
            fs.set(b"F".to_vec(), Object::String(PdfString::text(&file)));
            fs.set(b"UF".to_vec(), Object::String(PdfString::text(&file)));
            fs.set(b"EF".to_vec(), Object::Dict(ef));
            if !description.is_empty() {
                fs.set(b"Desc".to_vec(), Object::String(PdfString::text(&description)));
            }
            let color = Color::rgb(0.1, 0.3, 0.75);
            let r = Rect::new(at.x, at.y, at.x + ICON, at.y + ICON);
            let mut apd = Dict::new();
            apd.set(b"Type".to_vec(), Object::name("XObject"));
            apd.set(b"Subtype".to_vec(), Object::name("Form"));
            apd.set(
                b"BBox".to_vec(),
                Object::Array(vec![
                    Object::Int(0),
                    Object::Int(0),
                    Object::Real(ICON),
                    Object::Real(ICON),
                ]),
            );
            let ap = cos.add(Object::Stream(Stream::flate(apd, icon.drawing(color).as_bytes())));
            let mut aps = Dict::new();
            aps.set(b"N".to_vec(), Object::Ref(ap));
            let mut a = Dict::new();
            a.set(b"Type".to_vec(), Object::name("Annot"));
            a.set(b"Subtype".to_vec(), Object::name("FileAttachment"));
            a.set(b"NM".to_vec(), Object::String(PdfString::text(&nm)));
            a.set(
                b"Rect".to_vec(),
                Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect()),
            );
            a.set(b"FS".to_vec(), Object::Dict(fs));
            a.set(b"Name".to_vec(), Object::name(icon.pdf_name()));
            a.set(b"Contents".to_vec(), Object::String(PdfString::text(&file)));
            a.set(b"Subj".to_vec(), Object::String(PdfString::text("File Attachment")));
            a.set(b"T".to_vec(), Object::String(PdfString::text(&author)));
            a.set(
                b"C".to_vec(),
                Object::Array(vec![
                    Object::Real(color.r),
                    Object::Real(color.g),
                    Object::Real(color.b),
                ]),
            );
            a.set(b"F".to_vec(), Object::Int(4));
            a.set(
                b"M".to_vec(),
                Object::String(PdfString::literal(markupcraft_revu::pdf_date_now().into_bytes())),
            );
            a.set(b"AP".to_vec(), Object::Dict(aps));
            a.set(b"P".to_vec(), Object::Ref(pref));
            let ar = cos.add(Object::Dict(a));
            let mut list = annots_of(cos, pref);
            list.push(Object::Ref(ar));
            set_annots(cos, pref, list)
        })?;
        Ok(id)
    }

    /// Every File Attachment markup (the capture summary).
    pub fn attachment_markups(&self) -> Vec<AttachmentMarkup> {
        let cos = &self.file.cos;
        let mut out = Vec::new();
        for (pi, p) in page_objs(cos).unwrap_or_default().into_iter().enumerate() {
            for a in annots_of(cos, p) {
                let Some(d) = cos.dict(&a) else { continue };
                if d.name(b"Subtype") != Some(b"FileAttachment") {
                    continue;
                }
                let fs = d.get(b"FS").and_then(|f| cos.dict(f)).unwrap_or_default();
                let uf = text_of(cos, fs.get(b"UF"));
                let size = fs
                    .get(b"EF")
                    .and_then(|e| cos.dict(e))
                    .and_then(|e| e.get(b"F").or_else(|| e.get(b"UF")).and_then(|s| cos.dict(s)))
                    .and_then(|s| s.get(b"Params").and_then(|p| cos.dict(p)))
                    .and_then(|p| p.get(b"Size").and_then(|s| cos.resolve(s).as_int()))
                    .and_then(|s| u64::try_from(s).ok());
                let rect = d
                    .get(b"Rect")
                    .map(|r| cos.resolve(r))
                    .and_then(|r| {
                        let v: Vec<f64> = r.as_array()?.iter().filter_map(|x| cos.resolve(x).as_f64()).collect();
                        match v.as_slice() {
                            [a, b, c, d] => Some(Rect::new(*a, *b, *c, *d).normalized()),
                            _ => None,
                        }
                    })
                    .unwrap_or_default();
                out.push(AttachmentMarkup {
                    id: text_of(cos, d.get(b"NM")),
                    page: pi,
                    rect,
                    file: if uf.is_empty() { text_of(cos, fs.get(b"F")) } else { uf },
                    description: text_of(cos, fs.get(b"Desc")),
                    size,
                });
            }
        }
        out
    }

    /// The file a File Attachment markup holds.
    pub fn attachment_markup_data(&self, id: &str) -> Result<Vec<u8>> {
        let cos = &self.file.cos;
        for p in page_objs(cos)? {
            for a in annots_of(cos, p) {
                let Some(d) = cos.dict(&a) else { continue };
                if d.name(b"Subtype") == Some(b"FileAttachment") && text_of(cos, d.get(b"NM")) == id {
                    let fs = d.get(b"FS").and_then(|f| cos.dict(f)).unwrap_or_default();
                    return embedded_bytes(cos, &fs).ok_or_else(|| invalid("the attachment holds no file"));
                }
            }
        }
        Err(invalid(format!("no File Attachment markup {id:?}")))
    }

    /// Export Capture Media: every File Attachment markup's file saved into `dir` (names made
    /// unique). Returns the files written.
    pub fn export_attachment_markups(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        if !dir.is_dir() {
            return Err(invalid(format!("{} is not a folder", dir.display())));
        }
        let mut out: Vec<PathBuf> = Vec::new();
        for a in self.attachment_markups() {
            let data = self.attachment_markup_data(&a.id)?;
            let safe: String = a
                .file
                .chars()
                .map(|c| {
                    if "/\\:*?\"<>|".contains(c) || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            let safe = if safe.trim().is_empty() {
                "attachment".to_string()
            } else {
                safe
            };
            let mut p = dir.join(&safe);
            let mut k = 2;
            while out.contains(&p) || p.exists() {
                let stem = Path::new(&safe)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let ext = Path::new(&safe)
                    .extension()
                    .map(|s| format!(".{}", s.to_string_lossy()))
                    .unwrap_or_default();
                p = dir.join(format!("{stem} ({k}){ext}"));
                k += 1;
                if k > 10_000 {
                    return Err(invalid("too many files of one name"));
                }
            }
            crate::write_atomic(&p, &data)?;
            out.push(p);
        }
        Ok(out)
    }

    /// The capture summary as CSV: Page, File, Size, Description, Author.
    pub fn capture_summary_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("Page,File,Size,Description,Author\r\n");
        for a in self.attachment_markups() {
            let author = self.markup(&a.id).map(|m| m.author.clone()).unwrap_or_default();
            s.push_str(&format!(
                "{},{},{},{},{}\r\n",
                a.page + 1,
                q(&a.file),
                a.size.map(|v| v.to_string()).unwrap_or_default(),
                q(&a.description),
                q(&author)
            ));
        }
        s
    }

    /// A Snapshot markup of `rect` on `page` (captured from the page content on save), not
    /// added: paste it, or add it with `add_markup`.
    pub fn snapshot_markup(&self, page: usize, rect: Rect) -> Result<Markup> {
        self.page(page)?;
        let r = rect.normalized();
        if !(r.as_array().iter().all(|v| v.is_finite()) && r.width() >= 2.0 && r.height() >= 2.0) {
            return Err(invalid("a snapshot needs a box at least 2 points on each side"));
        }
        let mut m = Markup::new(Kind::Snapshot, page, r.corners().to_vec());
        m.rect = r;
        m.line_width = 0.0;
        m.subject = crate::props::default_subject(Kind::Snapshot);
        m.snapshot = Some(SnapshotSource {
            annot: None,
            page: Some(page),
            region: r,
        });
        Ok(m)
    }

    /// Snapshot to the clipboard: a region, the whole page (Copy Page to Snapshot, `rect`
    /// `None`), or a Space's box (`space` id). Paste places it.
    pub fn snapshot_to_clipboard(&mut self, page: usize, rect: Option<Rect>, space: Option<&str>) -> Result<Rect> {
        let r = match (rect, space) {
            (Some(r), None) => r,
            (None, Some(id)) => {
                let (p, sp) = self
                    .spaces(None)
                    .into_iter()
                    .find(|(_, s)| s.id == id)
                    .ok_or_else(|| invalid(format!("no space {id:?} (space_list shows them)")))?;
                let b = bbox(&sp.pts).ok_or_else(|| invalid("the space has no outline"))?;
                return {
                    let m = self.snapshot_markup(p, b)?;
                    self.set_clipboard(vec![m]);
                    Ok(b)
                };
            }
            (None, None) => self.page(page)?.crop.normalized(),
            (Some(_), Some(_)) => return Err(invalid("give a box or a space, not both")),
        };
        let m = self.snapshot_markup(page, r)?;
        self.set_clipboard(vec![m]);
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn file_attachments_round_trip_and_export() {
        let d = std::env::temp_dir().join(format!("markupcraft-capture-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let photo = d.join("site photo.jpg");
        std::fs::write(&photo, b"\xFF\xD8 not really a jpeg").unwrap();
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 50.0, 50.0))]),
            d.join("a.pdf"),
        )
        .unwrap();
        let id = s
            .add_file_attachment(0, Point::new(300.0, 400.0), &photo, AttachIcon::Paperclip, "north wall")
            .unwrap();
        let m = s.markup(&id).unwrap();
        assert_eq!(m.kind, Kind::Attachment);
        let list = s.attachment_markups();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].file.as_str(), list[0].size), ("site photo.jpg", Some(20)));
        assert!(s.capture_summary_csv().contains("\"north wall\""));
        let out = d.join("a.pdf");
        s.save_as(&out, false).unwrap();
        let t = Session::open(&out).unwrap();
        assert_eq!(t.attachment_markups().len(), 1, "kept on save");
        let media = d.join("media");
        std::fs::create_dir_all(&media).unwrap();
        let files = t.export_attachment_markups(&media).unwrap();
        assert_eq!(std::fs::read(&files[0]).unwrap(), std::fs::read(&photo).unwrap());
        s.undo().ok();
        assert!(
            s.add_file_attachment(9, Point::new(0.0, 0.0), &photo, AttachIcon::PushPin, "")
                .is_err()
        );
    }

    #[test]
    fn snapshots_of_a_region_a_page_and_a_space() {
        let mut s = Session::from_bytes(
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 50.0, 50.0))]),
            "s.pdf",
        )
        .unwrap();
        let r = s.snapshot_to_clipboard(0, None, None).unwrap();
        assert_eq!((r.width(), r.height()), (612.0, 792.0));
        assert_eq!(s.clipboard()[0].kind, Kind::Snapshot);
        let id = s
            .add_space(
                0,
                "Room 1",
                vec![
                    Point::new(90.0, 90.0),
                    Point::new(160.0, 90.0),
                    Point::new(160.0, 160.0),
                    Point::new(90.0, 160.0),
                ],
                None,
                None,
            )
            .unwrap();
        let b = s.snapshot_to_clipboard(0, None, Some(&id)).unwrap();
        assert_eq!(b, Rect::new(90.0, 90.0, 160.0, 160.0));
        assert!(
            s.snapshot_to_clipboard(0, Some(Rect::new(0.0, 0.0, 1.0, 1.0)), None)
                .is_err()
        );
    }
}
