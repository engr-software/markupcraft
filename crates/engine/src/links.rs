//! Hyperlinks: Link annotations (ISO 32000-1 §12.5.6.5) that go to a page of this document, open
//! a web address, or open another file (a page of another PDF with GoToR, anything else with
//! Launch). Links are not markups: they live in the page's `/Annots` beside them and keep their
//! place when markups are saved. Each link carries an `/NM` id so it can be found again.

use markupcraft_model::{Color, Rect};
use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, PdfString};

use crate::docutil::{annots_of, item_page, page_objs, set_annots, text_of};
use crate::{EngineError, Result, Session, invalid};

/// Longest URL or file path accepted.
const MAX_TARGET: usize = 4_096;

/// Where a link goes.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// A page of this document (0-based).
    Page(usize),
    /// A web address.
    Url(String),
    /// Another file; `page` (0-based) opens a page of another PDF.
    File { path: String, page: Option<usize> },
    /// Something else (a named destination elsewhere, JavaScript...): kept as is.
    Other(String),
}

/// How a new link looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkLook {
    /// Border width in points; 0 = invisible.
    pub width: f64,
    pub color: Color,
}

impl Default for LinkLook {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Color::rgb(0.0, 0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkInfo {
    /// `/NM`, or `page:index` for links that have none.
    pub id: String,
    pub page: usize,
    pub rect: Rect,
    pub target: LinkTarget,
}

fn num(cos: &CosDoc, o: Option<&Object>) -> Option<f64> {
    o.map(|o| cos.resolve(o))
        .and_then(|o| o.as_f64())
        .filter(|v| v.is_finite())
}

fn rect_of(cos: &CosDoc, o: Option<&Object>) -> Option<Rect> {
    let a = cos.resolve(o?);
    let a = a.as_array()?;
    Some(
        Rect::new(
            num(cos, a.first())?,
            num(cos, a.get(1))?,
            num(cos, a.get(2))?,
            num(cos, a.get(3))?,
        )
        .normalized(),
    )
}

fn file_name(cos: &CosDoc, o: Option<&Object>) -> String {
    match o.map(|o| cos.resolve(o)).as_deref() {
        Some(Object::Dict(d)) => {
            let uf = text_of(cos, d.get(b"UF"));
            if uf.is_empty() { text_of(cos, d.get(b"F")) } else { uf }
        }
        Some(other) => text_of(cos, Some(other)),
        None => String::new(),
    }
}

fn target_of(cos: &CosDoc, d: &Dict, pages: &[markupcraft_revu::cos::ObjRef]) -> LinkTarget {
    if let Some(p) = item_page(cos, d, pages) {
        return LinkTarget::Page(p);
    }
    let Some(a) = d.get(b"A").and_then(|a| cos.dict(a)) else {
        return LinkTarget::Other("no action".into());
    };
    match a.name(b"S") {
        Some(b"URI") => LinkTarget::Url(text_of(cos, a.get(b"URI"))),
        Some(b"GoToR") => {
            let page = a
                .get(b"D")
                .map(|d| cos.resolve(d))
                .and_then(|d| {
                    d.as_array()
                        .and_then(|x| x.first())
                        .and_then(|p| cos.resolve(p).as_int())
                })
                .and_then(|p| usize::try_from(p).ok());
            LinkTarget::File {
                path: file_name(cos, a.get(b"F")),
                page,
            }
        }
        Some(b"Launch") => LinkTarget::File {
            path: file_name(cos, a.get(b"F")),
            page: None,
        },
        Some(s) => LinkTarget::Other(String::from_utf8_lossy(s).into_owned()),
        None => LinkTarget::Other("unknown action".into()),
    }
}

/// Every link on every page.
pub(crate) fn read(cos: &CosDoc) -> Vec<LinkInfo> {
    let pages = page_objs(cos).unwrap_or_default();
    let mut out = Vec::new();
    for (pi, p) in pages.iter().enumerate() {
        for (k, a) in annots_of(cos, *p).iter().enumerate() {
            let Some(d) = cos.dict(a) else { continue };
            if d.name(b"Subtype") != Some(b"Link") {
                continue;
            }
            let nm = text_of(cos, d.get(b"NM"));
            out.push(LinkInfo {
                id: if nm.is_empty() { format!("{}:{k}", pi + 1) } else { nm },
                page: pi,
                rect: rect_of(cos, d.get(b"Rect")).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                target: target_of(cos, &d, &pages),
            });
        }
    }
    out
}

fn filespec(path: &str) -> Object {
    let mut f = Dict::new();
    f.set(b"Type".to_vec(), Object::name("Filespec"));
    // `/F` is the portable form (forward slashes); `/UF` keeps the text as typed.
    f.set(b"F".to_vec(), Object::String(PdfString::text(&path.replace('\\', "/"))));
    f.set(b"UF".to_vec(), Object::String(PdfString::text(path)));
    Object::Dict(f)
}

impl Session {
    /// Every hyperlink in the document.
    pub fn links(&self) -> Vec<LinkInfo> {
        read(&self.file.cos)
    }

    /// Add a link over `rect` on `page`. Returns its id.
    pub fn add_link(&mut self, page: usize, rect: Rect, target: &LinkTarget, look: LinkLook) -> Result<String> {
        self.page(page)?;
        let r = rect.normalized();
        if !r.as_array().iter().all(|v| v.is_finite()) || r.width() < 1.0 || r.height() < 1.0 {
            return Err(invalid("the link rectangle must be at least 1 point wide and high"));
        }
        if !(look.width.is_finite() && (0.0..=12.0).contains(&look.width)) {
            return Err(invalid("the border width must be 0 to 12 points"));
        }
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("Annot"));
        d.set(b"Subtype".to_vec(), Object::name("Link"));
        d.set(
            b"Rect".to_vec(),
            Object::Array(r.as_array().iter().map(|v| Object::Real(*v)).collect()),
        );
        d.set(b"F".to_vec(), Object::Int(4));
        d.set(b"H".to_vec(), Object::name("I"));
        d.set(
            b"Border".to_vec(),
            Object::Array(vec![Object::Int(0), Object::Int(0), Object::Real(look.width)]),
        );
        let c = look.color;
        d.set(
            b"C".to_vec(),
            Object::Array(vec![Object::Real(c.r), Object::Real(c.g), Object::Real(c.b)]),
        );
        let text_target = |s: &str, what: &str| -> Result<String> {
            let s = s.trim();
            if s.is_empty() || s.chars().count() > MAX_TARGET {
                return Err(invalid(format!("give a {what} of 1 to {MAX_TARGET} characters")));
            }
            Ok(s.to_string())
        };
        let mut action = Dict::new();
        match target {
            LinkTarget::Page(p) => {
                self.page(*p)?;
            }
            LinkTarget::Url(u) => {
                let u = text_target(u, "web address")?;
                action.set(b"S".to_vec(), Object::name("URI"));
                action.set(b"URI".to_vec(), Object::String(PdfString::literal(u.into_bytes())));
            }
            LinkTarget::File { path, page } => {
                let path = text_target(path, "file path")?;
                match page {
                    Some(p) => {
                        action.set(b"S".to_vec(), Object::name("GoToR"));
                        action.set(
                            b"D".to_vec(),
                            Object::Array(vec![Object::Int(i64::try_from(*p).unwrap_or(0)), Object::name("Fit")]),
                        );
                    }
                    None => action.set(b"S".to_vec(), Object::name("Launch")),
                }
                action.set(b"F".to_vec(), filespec(&path));
                action.set(b"NewWindow".to_vec(), Object::Bool(true));
            }
            LinkTarget::Other(_) => return Err(invalid("a link goes to a page, a web address or a file")),
        }
        let id = self.new_id();
        d.set(b"NM".to_vec(), Object::String(PdfString::text(&id)));
        let target = target.clone();
        self.graph_edit("Add Link", |cos, _| {
            let pages = page_objs(cos)?;
            let page_ref = *pages.get(page).ok_or(EngineError::NoPage {
                page: page + 1,
                count: pages.len(),
            })?;
            match target {
                LinkTarget::Page(p) => {
                    let to = *pages.get(p).ok_or(EngineError::NoPage {
                        page: p + 1,
                        count: pages.len(),
                    })?;
                    d.set(
                        b"Dest".to_vec(),
                        Object::Array(vec![Object::Ref(to), Object::name("Fit")]),
                    );
                }
                _ => d.set(b"A".to_vec(), Object::Dict(action)),
            }
            d.set(b"P".to_vec(), Object::Ref(page_ref));
            let link = cos.add(Object::Dict(d));
            let mut list = annots_of(cos, page_ref);
            list.push(Object::Ref(link));
            set_annots(cos, page_ref, list)
        })?;
        Ok(id)
    }

    /// Delete links by id; returns how many went.
    pub fn delete_links(&mut self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Err(invalid("no link ids given (link_list shows them)"));
        }
        let all = self.links();
        for id in ids {
            if !all.iter().any(|l| &l.id == id) {
                return Err(invalid(format!("no link with id {id:?} (link_list shows the ids)")));
            }
        }
        self.graph_edit("Delete Links", |cos, _| {
            let pages = page_objs(cos)?;
            let mut removed = 0;
            for (pi, p) in pages.iter().enumerate() {
                let list = annots_of(cos, *p);
                let kept: Vec<Object> = list
                    .iter()
                    .enumerate()
                    .filter(|(k, a)| {
                        let Some(d) = cos.dict(a) else { return true };
                        if d.name(b"Subtype") != Some(b"Link") {
                            return true;
                        }
                        let nm = text_of(cos, d.get(b"NM"));
                        let id = if nm.is_empty() { format!("{}:{k}", pi + 1) } else { nm };
                        !ids.contains(&id)
                    })
                    .map(|(_, a)| a.clone())
                    .collect();
                if kept.len() != list.len() {
                    removed += list.len() - kept.len();
                    set_annots(cos, *p, kept)?;
                }
            }
            Ok(removed)
        })
    }
}
