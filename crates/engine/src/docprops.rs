//! Document properties: the document information dictionary (`/Info`, ISO 32000-1 §14.3.3)
//! with its standard and custom entries, kept in step with the XMP metadata packet (`/Metadata`
//! on the catalog, §14.3.2) that newer viewers prefer.
//!
//! On every change the XMP packet is rewritten from the information dictionary (Dublin Core
//! title, creator, description; PDF keywords and producer; XMP creator tool and dates). A PDF/A
//! identification found in the old packet is carried over.

use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, Stream};

use crate::docutil::{err, text_of};
use crate::{Result, Session, invalid};

/// The standard information keys, in the order Document Properties shows them.
pub const STANDARD_KEYS: [&str; 8] = [
    "Title",
    "Author",
    "Subject",
    "Keywords",
    "Creator",
    "Producer",
    "CreationDate",
    "ModDate",
];

const MAX_VALUE: usize = 32_000;
const MAX_XMP: usize = 4 << 20;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocProperties {
    /// Standard entries that are present, by key.
    pub standard: Vec<(String, String)>,
    /// Every other text entry (custom properties).
    pub custom: Vec<(String, String)>,
    pub pdf_version: String,
    pub pages: usize,
    pub encrypted: bool,
    /// The catalog has an XMP metadata stream.
    pub has_xmp: bool,
    pub file_size: usize,
}

fn info_dict(cos: &CosDoc) -> Dict {
    cos.trailer().get(b"Info").and_then(|o| cos.dict(o)).unwrap_or_default()
}

fn xmp_text(cos: &CosDoc) -> Option<String> {
    let root = cos.root()?;
    let m = cos.get(root).as_dict()?.get(b"Metadata")?.clone();
    match &*cos.resolve(&m) {
        Object::Stream(s) => s
            .decoded_within(MAX_XMP)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned()),
        _ => None,
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `D:YYYYMMDDHHmmSS...` to `YYYY-MM-DDTHH:mm:SSZ` (as far as the date goes).
fn xmp_date(pdf: &str) -> Option<String> {
    let d = pdf.trim().trim_start_matches("D:");
    let digits: String = d.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |a: usize, b: usize, dflt: &str| digits.get(a..b).unwrap_or(dflt).to_string();
    Some(format!(
        "{}-{}-{}T{}:{}:{}Z",
        part(0, 4, "1970"),
        part(4, 6, "01"),
        part(6, 8, "01"),
        part(8, 10, "00"),
        part(10, 12, "00"),
        part(12, 14, "00")
    ))
}

/// The value of a simple `<prefix:tag>value</prefix:tag>` or attribute in an XMP packet.
fn xmp_value(xmp: &str, tag: &str) -> Option<String> {
    if let Some(i) = xmp.find(&format!("<{tag}>")) {
        let rest = xmp.get(i + tag.len() + 2..)?;
        let end = rest.find(&format!("</{tag}>"))?;
        return rest.get(..end).map(|v| v.trim().to_string());
    }
    let i = xmp.find(&format!("{tag}=\""))?;
    let rest = xmp.get(i + tag.len() + 2..)?;
    rest.get(..rest.find('"')?).map(str::to_string)
}

/// An XMP packet carrying the information entries.
fn build_xmp(info: &Dict, cos: &CosDoc, old: Option<&str>) -> String {
    let get = |k: &str| text_of(cos, info.get(k.as_bytes()));
    let alt = |tag: &str, v: &str| {
        format!(
            "<{tag}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{tag}>\n",
            esc(v)
        )
    };
    let mut body = String::new();
    let title = get("Title");
    if !title.is_empty() {
        body.push_str(&alt("dc:title", &title));
    }
    let author = get("Author");
    if !author.is_empty() {
        body.push_str(&format!(
            "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>\n",
            esc(&author)
        ));
    }
    let subject = get("Subject");
    if !subject.is_empty() {
        body.push_str(&alt("dc:description", &subject));
    }
    let simple = |tag: &str, v: &str, body: &mut String| {
        if !v.is_empty() {
            body.push_str(&format!("<{tag}>{}</{tag}>\n", esc(v)));
        }
    };
    simple("pdf:Keywords", &get("Keywords"), &mut body);
    simple("pdf:Producer", &get("Producer"), &mut body);
    simple("xmp:CreatorTool", &get("Creator"), &mut body);
    if let Some(d) = xmp_date(&get("CreationDate")) {
        simple("xmp:CreateDate", &d, &mut body);
    }
    if let Some(d) = xmp_date(&get("ModDate")) {
        simple("xmp:ModifyDate", &d, &mut body);
        simple("xmp:MetadataDate", &d, &mut body);
    }
    if let Some(old) = old {
        if let Some(part) = xmp_value(old, "pdfaid:part") {
            simple("pdfaid:part", &part, &mut body);
        }
        if let Some(c) = xmp_value(old, "pdfaid:conformance") {
            simple("pdfaid:conformance", &c, &mut body);
        }
    }
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
         xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n{body}</rdf:Description>\n\
         </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>"
    )
}

/// Rewrite the catalog's `/Metadata` from the information dictionary.
fn sync_xmp(cos: &mut CosDoc) -> Result<()> {
    let Some(root) = cos.root() else { return Ok(()) };
    let old = xmp_text(cos);
    let xml = build_xmp(&info_dict(cos), cos, old.as_deref());
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), Object::name("Metadata"));
    d.set(b"Subtype".to_vec(), Object::name("XML"));
    // Metadata stays uncompressed so tools that scan for it find it.
    let stream = Stream::from_raw(d, xml.into_bytes());
    let existing = cos
        .get(root)
        .as_dict()
        .and_then(|c| c.reference(b"Metadata"))
        .filter(|r| matches!(&*cos.get(*r), Object::Stream(_)));
    match existing {
        Some(r) => cos.set(r, Object::Stream(stream)),
        None => {
            let r = cos.add(Object::Stream(stream));
            cos.update_dict(root, |c| c.set(b"Metadata".to_vec(), Object::Ref(r)))?;
        }
    }
    Ok(())
}

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 127 && k.bytes().all(|b| b.is_ascii_graphic() && !b"/()<>[]{}%#".contains(&b))
}

impl Session {
    pub fn doc_properties(&self) -> DocProperties {
        let cos = &self.file.cos;
        let info = info_dict(cos);
        let mut standard = Vec::new();
        let mut custom = Vec::new();
        for (k, v) in info.iter() {
            let key = String::from_utf8_lossy(k).into_owned();
            let value = match &*cos.resolve(v) {
                Object::String(s) => s.to_text(),
                Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
                _ => continue,
            };
            if STANDARD_KEYS.contains(&key.as_str()) {
                standard.push((key, value));
            } else if key != "Trapped" {
                custom.push((key, value));
            }
        }
        standard.sort_by_key(|(k, _)| STANDARD_KEYS.iter().position(|s| s == k));
        DocProperties {
            standard,
            custom,
            pdf_version: cos.version().to_string(),
            pages: self.page_count(),
            encrypted: cos.security().is_some(),
            has_xmp: xmp_text(cos).is_some(),
            file_size: cos.bytes().len(),
        }
    }

    /// Set (`Some`) or remove (`None` or `""`) information entries, standard or custom, and
    /// refresh the XMP packet to match.
    pub fn set_doc_properties(&mut self, changes: &[(String, Option<String>)]) -> Result<()> {
        if changes.is_empty() {
            return Err(invalid("no properties given"));
        }
        for (k, v) in changes {
            if !valid_key(k) {
                return Err(invalid(format!(
                    "{k:?} is not a property name (letters, digits and punctuation, no spaces)"
                )));
            }
            if matches!(k.as_str(), "CreationDate" | "ModDate") {
                return Err(invalid(format!("{k} is kept by the application")));
            }
            if v.as_ref().is_some_and(|v| v.chars().count() > MAX_VALUE) {
                return Err(invalid(format!("the value of {k} is too long")));
            }
        }
        self.cos_edit("Document Properties", |cos| {
            for (k, v) in changes {
                pdfcraft_organize::set_info(cos, k, v.as_deref().unwrap_or("")).map_err(err)?;
            }
            sync_xmp(cos)?;
            Ok(((), true))
        })
    }

    /// The XMP metadata packet, when the document has one.
    pub fn xmp(&self) -> Option<String> {
        xmp_text(&self.file.cos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_values() {
        assert_eq!(xmp_date("D:20261009153000Z").as_deref(), Some("2026-10-09T15:30:00Z"));
        assert_eq!(xmp_date("D:2026").as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(xmp_date("junk"), None);
        let x = "<pdfaid:part>2</pdfaid:part> pdfaid:conformance=\"B\"";
        assert_eq!(xmp_value(x, "pdfaid:part").as_deref(), Some("2"));
        assert_eq!(xmp_value(x, "pdfaid:conformance").as_deref(), Some("B"));
        assert!(valid_key("Project") && !valid_key("Two words") && !valid_key(""));
    }
}
