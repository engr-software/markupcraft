//! Snapshot Content > Cut: the region's page content goes to the clipboard as a snapshot (kept
//! vector) and is removed from the page. The snapshot's appearance is captured first (in a
//! snapshot annotation that is then taken off the page, so its content outlives the cut), and
//! the region is cleared with the redaction engine without a box (other redaction marks on the
//! page are left alone). One undoable step.

use markupcraft_geom::Rect;
use markupcraft_model::{Kind, SnapshotSource};
use markupcraft_revu::cos::Object;
use pdfcraft_annot::{Meta, NewAnnotation, OverlayLook, Shape, Style, add_annotation, rect_quad};

use crate::docutil::{annots_of, page_objs, set_annots};
use crate::{Result, Session, invalid};

const HOLD: &[u8] = b"PCRedactHeld";

impl Session {
    /// Cut the content of `rect` on `page` to the clipboard as a snapshot. Returns the region.
    pub fn snapshot_cut(&mut self, page: usize, rect: Rect) -> Result<Rect> {
        let mut m = self.snapshot_markup(page, rect)?;
        let r = m.rect;
        let id = m.id.clone();
        // the snapshot is captured into the file (its /AP made from the page) and taken off
        // the page again, so the content survives the cut for Paste
        m.flags |= markupcraft_model::flags::HIDDEN;
        let held_id = self.add_markup(m)?;
        let author = self.author.clone();
        let src = self.edit("Cut Snapshot", |s| {
            let vp = std::mem::take(&mut s.vp_changed);
            Session::flush(&mut s.file.cos, &mut s.doc, &vp);
            let obj = s
                .doc
                .markups
                .iter()
                .find(|x| x.id == held_id)
                .map(|x| x.obj)
                .ok_or_else(|| invalid("the snapshot could not be captured"))?;
            let pref = *page_objs(&s.file.cos)?
                .get(page)
                .ok_or_else(|| invalid("no such page"))?;
            // off the page (the object stays, holding the captured content)
            let keep: Vec<Object> = annots_of(&s.file.cos, pref)
                .into_iter()
                .filter(|o| !matches!(o, Object::Ref(r) if (r.num, r.generation) == obj))
                .collect();
            set_annots(&mut s.file.cos, pref, keep)?;
            // other redaction marks on the page wait, the cut's own mark is applied
            let mut held = Vec::new();
            for o in annots_of(&s.file.cos, pref) {
                let Object::Ref(ar) = o else { continue };
                let is_redact = s
                    .file
                    .cos
                    .dict(&Object::Ref(ar))
                    .and_then(|d| d.get(b"Subtype").and_then(Object::as_name).map(|n| n == b"Redact"))
                    .unwrap_or(false);
                if is_redact {
                    s.file.cos.update_dict(ar, |d| {
                        d.set(b"Subtype".to_vec(), Object::Name(HOLD.to_vec()));
                    })?;
                    held.push(ar);
                }
            }
            let shape = Shape::Redact {
                quads: vec![rect_quad([r.x0, r.y0, r.x1, r.y1])],
                overlay: String::new(),
                look: OverlayLook::default(),
            };
            let mut st = Style::default_for(&shape);
            st.fill = None;
            let meta = Meta {
                date: Some(markupcraft_revu::pdf_date_now()),
                id: markupcraft_revu::new_markup_id(),
            };
            let new = NewAnnotation {
                page,
                shape,
                style: st,
                contents: String::new(),
                author: author.clone(),
            };
            add_annotation(&mut s.file.cos, &new, &meta).map_err(|e| invalid(e.to_string()))?;
            let applied = pdfcraft_redact::apply(&mut s.file.cos, Some(&[page])).map_err(|e| invalid(e.to_string()));
            for ar in &held {
                s.file.cos.update_dict(*ar, |d| {
                    d.set(b"Subtype".to_vec(), Object::Name(b"Redact".to_vec()));
                })?;
            }
            applied?;
            s.file.cos.require_full_save();
            s.reload();
            Ok((obj, true))
        })?;
        let mut clip = self.snapshot_markup(page, r)?;
        clip.id = id;
        clip.kind = Kind::Snapshot;
        clip.snapshot = Some(SnapshotSource {
            annot: Some(src),
            page: None,
            region: r,
        });
        self.set_clipboard(vec![clip]);

        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};
    use markupcraft_model::Point;

    #[test]
    fn cut_removes_the_content_and_paste_brings_it_back_as_a_snapshot() {
        let content = format!(
            "{}{}",
            text(72.0, 700.0, 14.0, "DETAIL 4"),
            text(72.0, 400.0, 14.0, "KEEP ME")
        );
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, content)]), "c.pdf").unwrap();
        let before = s.doc().markups.len();
        s.snapshot_cut(0, Rect::new(60.0, 680.0, 300.0, 730.0)).unwrap();
        let t = s.page_text(0).unwrap();
        assert!(!t.contains("DETAIL"), "{t}");
        assert!(t.contains("KEEP ME"));
        assert_eq!(
            s.doc().markups.len(),
            before,
            "the captured snapshot is not left on the page"
        );
        let ids = s.paste(Some(0), Some(Point::new(300.0, 300.0))).unwrap();
        assert_eq!(ids.len(), 1);
        let m = s.doc().find(&ids[0]).unwrap().clone();
        assert_eq!(m.kind, Kind::Snapshot);
        let bytes = s.current_bytes().unwrap();
        let back = Session::from_bytes(bytes.to_vec(), "c2.pdf").unwrap();
        assert!(
            back.doc().markups.iter().any(|x| x.kind == Kind::Snapshot),
            "{:?}",
            back.doc()
                .markups
                .iter()
                .map(|x| (x.kind, x.subject.clone()))
                .collect::<Vec<_>>()
        );
        // undo puts the content back
        s.undo().unwrap();
        s.undo().unwrap();
        assert!(s.page_text(0).unwrap().contains("DETAIL"));
    }
}
