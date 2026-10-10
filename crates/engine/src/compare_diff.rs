//! Compare Documents into a separate result file: the clouds go on a copy of the newer
//! revision saved beside it as `<name>_Diff.pdf` (or a path given), so neither revision
//! changes. [`Session::compare_with`] clouds the open document instead.

use std::path::{Path, PathBuf};

use crate::compare::{CompareOptions, CompareReport};
use crate::{Result, Session, invalid};

/// `<folder>/<stem>_Diff.pdf` beside `newer`.
pub fn diff_path(newer: &Path) -> PathBuf {
    let stem = newer
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Compare".into());
    newer.with_file_name(format!("{stem}_Diff.pdf"))
}

impl Session {
    /// Compare this (newer) document with `old` and write the clouded copy to `out` (atomic);
    /// this document is left as it is. Returns the report (markup ids are those in `out`).
    pub fn compare_to_file(
        &self,
        old: std::sync::Arc<Vec<u8>>,
        opts: &CompareOptions,
        out: &Path,
    ) -> Result<CompareReport> {
        if crate::docutil::same_file(out, self.path()) {
            return Err(invalid("the result file must not be the newer document itself"));
        }
        let bytes = self.current_bytes()?;
        let mut copy = Session::from_bytes(bytes.as_ref().clone(), out)?;
        copy.set_author(self.author());
        let report = copy.compare_with(old, opts)?;
        copy.save_as(out, true)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, line, pdf};

    #[test]
    fn diff_file_beside_the_newer_revision() {
        assert_eq!(diff_path(Path::new("a/Plan B.pdf")), PathBuf::from("a/Plan B_Diff.pdf"));
        let dir = std::env::temp_dir().join(format!("mc-diff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = pdf(&[SyntheticPage::new(612.0, 792.0, line(100.0, 100.0, 300.0, 100.0, 2.0))]);
        let new = pdf(&[SyntheticPage::new(
            612.0,
            792.0,
            format!(
                "{}{}",
                line(100.0, 100.0, 300.0, 100.0, 2.0),
                line(100.0, 400.0, 300.0, 400.0, 2.0)
            ),
        )]);
        let newer = dir.join("new.pdf");
        std::fs::write(&newer, &new).unwrap();
        let s = Session::open(&newer).unwrap();
        let out = diff_path(&newer);
        let r = s
            .compare_to_file(std::sync::Arc::new(old), &CompareOptions::default(), &out)
            .unwrap();
        assert!(!r.regions.is_empty());
        assert!(s.doc().markups.is_empty(), "the open document is untouched");
        assert_eq!(std::fs::read(&newer).unwrap(), new, "the newer file is untouched");
        let d = Session::open(&out).unwrap();
        assert_eq!(
            d.doc().markups.len(),
            r.regions.len(),
            "the clouds are in the result file"
        );
        assert!(
            s.compare_to_file(std::sync::Arc::new(Vec::new()), &CompareOptions::default(), &newer)
                .is_err()
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
