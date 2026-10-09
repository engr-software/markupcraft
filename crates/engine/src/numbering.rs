//! Editing page labels (`/PageLabels`): Number Pages over a range (style, prefix, start), a
//! label typed for one page (text only), clearing every label, and labels taken from bookmarks.
//! Each edit rewrites the label runs from one label per page (`labels::write` merges runs).

use crate::labels::{self, LabelSpec, LabelStyle};
use crate::{Result, Session, invalid};

/// Longest label text accepted.
const MAX_LABEL: usize = 1_000;

impl Session {
    /// Each page's current label spec; pages without one read as their decimal page number.
    fn label_specs(&self) -> Vec<LabelSpec> {
        let n = self.page_count();
        let read = labels::read(&self.file.cos, n);
        (0..n)
            .map(|i| {
                read.as_ref()
                    .and_then(|l| l.get(i).cloned().flatten())
                    .unwrap_or_else(|| LabelSpec::decimal(i as i64 + 1))
            })
            .collect()
    }

    fn write_labels(&mut self, label: &str, specs: Vec<LabelSpec>) -> Result<()> {
        self.cos_edit(label, |cos| {
            labels::write(cos, &specs);
            Ok(((), true))
        })?;
        self.refresh_labels();
        Ok(())
    }

    /// Re-read the labels into the page model (after an undo-able label edit).
    pub(crate) fn refresh_labels(&mut self) {
        let n = self.page_count();
        let read = labels::read(&self.file.cos, n);
        for (i, p) in self.doc.pages.iter_mut().enumerate() {
            p.label = read
                .as_ref()
                .and_then(|l| l.get(i).cloned().flatten())
                .map(|s| s.text())
                .unwrap_or_default();
        }
    }

    /// Number Pages: `pages` (sorted) are numbered `start`, `start + 1`, ... in `style`
    /// (`None` = the prefix alone) after `prefix`.
    pub fn number_pages(&mut self, pages: &[usize], style: Option<LabelStyle>, prefix: &str, start: i64) -> Result<()> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        if !(1..=1_000_000).contains(&start) {
            return Err(invalid("the first number must be 1 to 1000000"));
        }
        if prefix.chars().count() > MAX_LABEL {
            return Err(invalid("the prefix is too long"));
        }
        for p in pages {
            self.page(*p)?;
        }
        let mut sorted = pages.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut specs = self.label_specs();
        for (k, p) in sorted.iter().enumerate() {
            if let Some(s) = specs.get_mut(*p) {
                *s = LabelSpec {
                    style,
                    prefix: prefix.to_string(),
                    number: start.saturating_add(k as i64),
                };
            }
        }
        self.write_labels("Number Pages", specs)
    }

    /// Give pages a typed label each (text only); `""` puts the page number back.
    pub fn set_page_labels(&mut self, labels: &[(usize, String)]) -> Result<()> {
        if labels.is_empty() {
            return Err(invalid("no labels given"));
        }
        let mut specs = self.label_specs();
        for (p, text) in labels {
            self.page(*p)?;
            if text.chars().count() > MAX_LABEL {
                return Err(invalid(format!("the label for page {} is too long", p + 1)));
            }
            if let Some(s) = specs.get_mut(*p) {
                *s = if text.is_empty() {
                    LabelSpec::decimal(*p as i64 + 1)
                } else {
                    LabelSpec {
                        style: None,
                        prefix: text.clone(),
                        number: 1,
                    }
                };
            }
        }
        self.write_labels("Page Label", specs)
    }

    /// Remove every page label (viewers then show page numbers). Returns whether there were any.
    pub fn clear_page_labels(&mut self) -> Result<bool> {
        let had = labels::read(&self.file.cos, self.page_count()).is_some();
        self.cos_edit("Clear Page Labels", |cos| {
            if let Some(root) = cos.root() {
                cos.update_dict(root, |d| {
                    d.remove(b"PageLabels");
                })?;
            }
            Ok(((), had))
        })?;
        self.refresh_labels();
        Ok(had)
    }

    /// Label each page with the title of the first bookmark that goes to it (pages without a
    /// bookmark keep their label). Returns how many pages were labelled.
    pub fn labels_from_bookmarks(&mut self) -> Result<usize> {
        let titles = crate::bookmarks::titles_by_page(&self.file.cos, self.page_count());
        let set: Vec<(usize, String)> = titles
            .into_iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|t| (i, t.chars().take(MAX_LABEL).collect())))
            .collect();
        if set.is_empty() {
            return Err(invalid("no bookmark goes to a page of this document"));
        }
        let n = set.len();
        self.set_page_labels(&set)?;
        Ok(n)
    }
}
