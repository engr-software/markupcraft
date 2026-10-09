//! Embedded file attachments (the document's `/Names /EmbeddedFiles` tree, ISO 32000-1
//! §7.11.4): list, attach a file, save one out, remove one.

use std::path::Path;

use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, PdfString, Stream};

use crate::docutil::{name_tree_entries, names_tree, text_of, write_name_tree};
use crate::{EngineError, Result, Session, invalid};

/// Largest file attached or extracted.
pub const MAX_ATTACHMENT: usize = 256 << 20;

#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentInfo {
    /// The key in the attachments tree (what the other calls take).
    pub name: String,
    /// The file name it carries.
    pub file: String,
    pub description: String,
    /// Uncompressed size, when the file says.
    pub size: Option<u64>,
}

fn entries(cos: &CosDoc) -> Vec<(Vec<u8>, Object)> {
    names_tree(cos, b"EmbeddedFiles")
        .map(|t| name_tree_entries(cos, &t))
        .unwrap_or_default()
}

fn info(cos: &CosDoc, key: &[u8], spec: &Object) -> AttachmentInfo {
    let name = PdfString {
        bytes: key.to_vec(),
        hex: false,
    }
    .to_text();
    let d = cos.dict(spec).unwrap_or_default();
    let uf = text_of(cos, d.get(b"UF"));
    let file = if uf.is_empty() { text_of(cos, d.get(b"F")) } else { uf };
    let size = embedded(cos, &d)
        .and_then(|s| cos.dict(&s))
        .and_then(|sd| sd.get(b"Params").and_then(|p| cos.dict(p)))
        .and_then(|p| p.get(b"Size").and_then(|s| cos.resolve(s).as_int()))
        .and_then(|s| u64::try_from(s).ok());
    AttachmentInfo {
        name,
        file,
        description: text_of(cos, d.get(b"Desc")),
        size,
    }
}

/// The embedded file stream of a file specification (`/EF /UF` or `/EF /F`).
fn embedded(cos: &CosDoc, spec: &Dict) -> Option<Object> {
    let ef = cos.dict(spec.get(b"EF")?)?;
    ef.get(b"UF").or_else(|| ef.get(b"F")).cloned()
}

impl Session {
    pub fn attachments(&self) -> Vec<AttachmentInfo> {
        entries(&self.file.cos)
            .iter()
            .map(|(k, v)| info(&self.file.cos, k, v))
            .collect()
    }

    /// Attach the file at `path` under `name` (default: its file name); an attachment of that
    /// name is replaced. Returns the name used.
    pub fn add_attachment(&mut self, path: &Path, name: Option<&str>, description: &str) -> Result<String> {
        let meta = std::fs::metadata(path).map_err(|e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        if !meta.is_file() {
            return Err(invalid(format!("{} is not a file", path.display())));
        }
        if meta.len() > MAX_ATTACHMENT as u64 {
            return Err(invalid(format!(
                "{} is larger than {} MB",
                path.display(),
                MAX_ATTACHMENT >> 20
            )));
        }
        let data = std::fs::read(path).map_err(|e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".into());
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&file)
            .to_string();
        if name.chars().count() > 1_000 || description.chars().count() > 10_000 {
            return Err(invalid("the name or description is too long"));
        }
        let key = PdfString::text(&name).bytes;
        let out = name.clone();
        self.cos_edit("Add Attachment", |cos| {
            let mut params = Dict::new();
            params.set(b"Size".to_vec(), Object::Int(data.len() as i64));
            params.set(
                b"ModDate".to_vec(),
                Object::String(PdfString::literal(markupcraft_revu::pdf_date_now().into_bytes())),
            );
            let mut sd = Dict::new();
            sd.set(b"Type".to_vec(), Object::name("EmbeddedFile"));
            sd.set(b"Params".to_vec(), Object::Dict(params));
            let stream = cos.add(Object::Stream(Stream::flate(sd, &data)));
            let mut ef = Dict::new();
            ef.set(b"F".to_vec(), Object::Ref(stream));
            ef.set(b"UF".to_vec(), Object::Ref(stream));
            let mut spec = Dict::new();
            spec.set(b"Type".to_vec(), Object::name("Filespec"));
            spec.set(b"F".to_vec(), Object::String(PdfString::text(&file)));
            spec.set(b"UF".to_vec(), Object::String(PdfString::text(&file)));
            spec.set(b"EF".to_vec(), Object::Dict(ef));
            if !description.is_empty() {
                spec.set(b"Desc".to_vec(), Object::String(PdfString::text(description)));
            }
            let spec = cos.add(Object::Dict(spec));
            let mut list = entries(cos);
            list.retain(|(k, _)| *k != key);
            list.push((key, Object::Ref(spec)));
            write_name_tree(cos, b"EmbeddedFiles", list)?;
            Ok((out, true))
        })
    }

    /// The bytes of attachment `name`.
    pub fn attachment_data(&self, name: &str) -> Result<Vec<u8>> {
        let cos = &self.file.cos;
        let (_, spec) = self.find_attachment(name)?;
        let d = cos.dict(&spec).unwrap_or_default();
        let s = embedded(cos, &d).ok_or_else(|| invalid(format!("attachment {name:?} holds no file")))?;
        match &*cos.resolve(&s) {
            Object::Stream(st) => st.decoded_within(MAX_ATTACHMENT).map_err(EngineError::from),
            _ => Err(invalid(format!("attachment {name:?} holds no file"))),
        }
    }

    /// Save attachment `name` to `out` (atomic). Returns the byte count.
    pub fn extract_attachment(&self, name: &str, out: &Path) -> Result<usize> {
        let data = self.attachment_data(name)?;
        crate::write_atomic(out, &data)?;
        Ok(data.len())
    }

    pub fn delete_attachment(&mut self, name: &str) -> Result<()> {
        let (key, _) = self.find_attachment(name)?;
        self.cos_edit("Delete Attachment", |cos| {
            let mut list = entries(cos);
            list.retain(|(k, _)| *k != key);
            write_name_tree(cos, b"EmbeddedFiles", list)?;
            Ok(((), true))
        })
    }

    fn find_attachment(&self, name: &str) -> Result<(Vec<u8>, Object)> {
        let cos = &self.file.cos;
        entries(cos)
            .into_iter()
            .find(|(k, v)| {
                let i = info(cos, k, v);
                i.name == name
            })
            .ok_or_else(|| invalid(format!("no attachment named {name:?} (attachment_list shows them)")))
    }
}
