//! Extract one file per page with the links between the extracted pages updated: a link on
//! one extracted page to another extracted page becomes a link to that page's new file
//! (a relative path, so the set of files can move together).

use std::path::{Path, PathBuf};

use crate::links::LinkTarget;
use crate::{Result, Session};

impl Session {
    /// [`Session::extract_each`] with `update_links`: links between the extracted pages point
    /// to the new files. Returns the files written.
    #[allow(clippy::too_many_arguments)]
    pub fn extract_each_linked(
        &mut self,
        pages: &[usize],
        dir: &Path,
        stem: &str,
        by_label: bool,
        overwrite: bool,
        delete: bool,
        update_links: bool,
    ) -> Result<Vec<PathBuf>> {
        let files = self.extract_each(pages, dir, stem, by_label, overwrite, false)?;
        if update_links {
            let links = self.links();
            for (p, file) in pages.iter().zip(&files) {
                let mut s = Session::open(file)?;
                let mine = s.links();
                let mut changed = false;
                for l in links.iter().filter(|l| l.page == *p) {
                    let target = match &l.target {
                        LinkTarget::Page(q) | LinkTarget::Zoomed { page: q, .. } => *q,
                        _ => continue,
                    };
                    let Some(i) = pages.iter().position(|x| *x == target) else {
                        continue;
                    };
                    let Some(name) = files
                        .get(i)
                        .and_then(|f| f.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                    else {
                        continue;
                    };
                    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
                    let to = LinkTarget::File {
                        path: name,
                        page: Some(0),
                    };
                    match mine.iter().find(|x| {
                        let (r, s2) = (x.rect.normalized(), l.rect.normalized());
                        near(r.x0, s2.x0) && near(r.y0, s2.y0) && near(r.x1, s2.x1) && near(r.y1, s2.y1)
                    }) {
                        Some(m) => s.edit_link(&m.id, Some(&to), None, None)?,
                        // the link was dropped with its page target: made again
                        None => {
                            s.add_link(0, l.rect, &to, Default::default())?;
                        }
                    }
                    changed = true;
                }
                if changed {
                    s.save(true)?;
                }
            }
        }
        if delete {
            self.delete_pages(pages)?;
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};
    use markupcraft_geom::Rect;

    #[test]
    fn links_between_extracted_pages_point_to_the_new_files() {
        let pages: Vec<SyntheticPage> = (0..3)
            .map(|i| SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, &format!("Sheet {i}"))))
            .collect();
        let mut s = Session::from_bytes(pdf(&pages), "set.pdf").unwrap();
        s.add_link(
            0,
            Rect::new(100.0, 100.0, 200.0, 150.0),
            &LinkTarget::Page(2),
            Default::default(),
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("mc-extract-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files = s
            .extract_each_linked(&[0, 1, 2], &dir, "sheet", false, true, false, true)
            .unwrap();
        let first = Session::open(&files[0]).unwrap();
        let name = files[2].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            first
                .links()
                .iter()
                .any(|l| matches!(&l.target, LinkTarget::File { path, .. } if *path == name)),
            "{:?}",
            first.links()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
