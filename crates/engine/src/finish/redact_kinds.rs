//! Apply Redactions to text only or to images only (Revu's advanced choice). The redaction
//! engine removes every kind of content under a mark, so each marked page's content is split
//! first: the kinds to redact stay in the page while the rest (with the same graphics state, so
//! everything keeps its place) is set aside, the marks are applied, and the set-aside content
//! is put back underneath. Content inside form XObjects is kept in both modes.

use pdfcraft_content::{Op, parse, serialize_ops};

use crate::docutil::page_objs;
use crate::redact::{RedactMark, RedactReport};
use crate::{Result, Session, invalid};
use markupcraft_revu::cos::{Dict, Object, Stream};

/// What Apply Redactions removes under the marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactKinds {
    /// Text, images and vector graphics.
    #[default]
    All,
    TextOnly,
    ImagesOnly,
}

impl RedactKinds {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "all" => Self::All,
            "text" => Self::TextOnly,
            "images" => Self::ImagesOnly,
            _ => return None,
        })
    }
}

fn is_path_construction(op: &[u8]) -> bool {
    matches!(op, b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re")
}

fn is_path_paint(op: &[u8]) -> bool {
    matches!(
        op,
        b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n"
    )
}

/// Split `ops` into (the part to redact, the part to keep). State operators go to both.
fn split(ops: Vec<Op>, kinds: RedactKinds, is_image: &dyn Fn(&[u8]) -> bool) -> (Vec<Op>, Vec<Op>) {
    let (mut red, mut keep) = (Vec::new(), Vec::new());
    let mut path: Vec<Op> = Vec::new();
    let mut clip = false;
    let mut text: Option<Vec<Op>> = None;
    for op in ops {
        if let Some(block) = text.as_mut() {
            let end = op.is("ET");
            block.push(op);
            if end {
                let b = text.take().unwrap_or_default();
                if kinds == RedactKinds::TextOnly {
                    red.extend(b);
                } else {
                    keep.extend(b);
                }
            }
            continue;
        }
        let name = op.op.clone();
        if op.is("BT") {
            text = Some(vec![op]);
        } else if is_path_construction(&name) {
            path.push(op);
        } else if op.is("W") || op.is("W*") {
            clip = true;
            path.push(op);
        } else if is_path_paint(&name) {
            path.push(op);
            let p = std::mem::take(&mut path);
            if clip {
                red.extend(p.iter().cloned());
                keep.extend(p);
            } else {
                // vector graphics are never what these modes redact
                keep.extend(p);
            }
            clip = false;
        } else if op.inline.is_some() || (op.is("Do") && op.name(0).is_some_and(is_image)) {
            if kinds == RedactKinds::ImagesOnly {
                red.push(op);
            } else {
                keep.push(op);
            }
        } else if op.is("Do") || op.is("sh") {
            keep.push(op);
        } else {
            red.push(op.clone());
            keep.push(op);
        }
    }
    // an unterminated text block or path is kept as it was
    if let Some(b) = text {
        keep.extend(b);
    }
    keep.extend(path);
    (red, keep)
}

impl Session {
    /// Apply the redaction marks (on `pages`, or all) to the chosen kinds of content: text
    /// only, images only, or everything (`redact_apply_with`). Undoable until saved; the next
    /// save rewrites the whole file.
    pub fn redact_apply_kinds(
        &mut self,
        pages: Option<&[usize]>,
        kinds: RedactKinds,
        scrub_metadata: bool,
    ) -> Result<RedactReport> {
        if kinds == RedactKinds::All {
            return self.redact_apply_with(pages, scrub_metadata);
        }
        if let Some(p) = pages {
            for page in p {
                self.page(*page)?;
            }
        }
        let marks: Vec<RedactMark> = self
            .redact_marks()
            .into_iter()
            .filter(|m| pages.is_none_or(|p| p.contains(&m.page)))
            .collect();
        let mut marked: Vec<usize> = marks.iter().map(|m| m.page).collect();
        marked.sort_unstable();
        marked.dedup();
        let report = self.edit("Apply Redactions", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let objs = page_objs(&s.file.cos)?;
            let mut kept = Vec::new();
            for p in &marked {
                let Some(pref) = objs.get(*p).copied() else { continue };
                let pd = s.file.cos.dict(&Object::Ref(pref)).unwrap_or_default();
                let images: Dict = pd
                    .get(b"Resources")
                    .and_then(|r| s.file.cos.dict(r))
                    .and_then(|r| r.get(b"XObject").and_then(|x| s.file.cos.dict(x)))
                    .unwrap_or_default();
                let cos = &s.file.cos;
                let is_image = |n: &[u8]| -> bool {
                    images.get(n).map(|o| cos.resolve(o)).is_some_and(|o| match &*o {
                        Object::Stream(st) => st.dict.get(b"Subtype").and_then(Object::as_name) == Some(b"Image"),
                        _ => false,
                    })
                };
                let data = crate::overlay::page_content(cos, &pd)?;
                let (red, keep) = split(parse(&data).ops, kinds, &is_image);
                let red_ref = s
                    .file
                    .cos
                    .add(Object::Stream(Stream::flate(Dict::new(), &serialize_ops(&red))));
                s.file
                    .cos
                    .update_dict(pref, |d| d.set(b"Contents".to_vec(), Object::Ref(red_ref)))?;
                let mut kb = b"q\n".to_vec();
                kb.extend_from_slice(&serialize_ops(&keep));
                kb.extend_from_slice(b"\nQ\n");
                kept.push((pref, kb));
            }
            let r = pdfcraft_redact::apply(&mut s.file.cos, pages).map_err(|e| invalid(e.to_string()))?;
            for (pref, kb) in kept {
                let pre = String::from_utf8_lossy(&kb).into_owned();
                super::pages::wrap_content(&mut s.file.cos, pref, &pre, "")?;
            }
            s.file.cos.require_full_save();
            s.reload();
            Ok((
                RedactReport {
                    marks: r.marks,
                    pages: r.pages,
                    glyphs: r.glyphs,
                    images: r.images_removed + r.images_cleared,
                    paths: r.paths_removed + r.paths_clipped,
                    annotations: r.annotations,
                    fields: r.fields,
                    residue: Vec::new(),
                },
                true,
            ))
        })?;
        // text kept on purpose (images only) is not residue
        let residue = if kinds == RedactKinds::TextOnly {
            self.redact_residue(&marks)?
        } else {
            Vec::new()
        };
        if scrub_metadata {
            self.redact_apply_with(Some(&[]), true)?;
        }
        Ok(RedactReport { residue, ..report })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redact::MarkStyle;
    use crate::synthetic::{SyntheticPage, pdf, rect, text};
    use markupcraft_geom::Rect;

    fn page() -> Session {
        // text and a filled box under the same mark
        let content = format!(
            "{}{}",
            rect(60.0, 680.0, 300.0, 60.0),
            text(72.0, 700.0, 14.0, "SECRET PLAN")
        );
        Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, content)]), "r.pdf").unwrap()
    }

    #[test]
    fn text_only_keeps_the_graphics_and_images_only_keeps_the_text() {
        let mark = Rect::new(50.0, 670.0, 400.0, 760.0);
        let mut s = page();
        s.redact_mark(0, &[mark], &MarkStyle::default()).unwrap();
        let r = s.redact_apply_kinds(None, RedactKinds::TextOnly, false).unwrap();
        assert!(r.glyphs > 0, "{r:?}");
        assert!(r.residue.is_empty(), "{:?}", r.residue);
        assert!(!s.page_text(0).unwrap().contains("SECRET"));
        let data = String::from_utf8_lossy(&s.current_bytes().unwrap()).into_owned();
        assert!(!data.is_empty());

        let mut s = page();
        s.redact_mark(0, &[mark], &MarkStyle::default()).unwrap();
        let r = s.redact_apply_kinds(None, RedactKinds::ImagesOnly, false).unwrap();
        assert_eq!(r.glyphs, 0);
        assert!(s.page_text(0).unwrap().contains("SECRET"), "the text stays");
        assert_eq!(RedactKinds::from_name("text"), Some(RedactKinds::TextOnly));
        assert_eq!(RedactKinds::from_name("x"), None);
    }
}
