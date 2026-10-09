//! A small picture of a file's first page, for previews of recent files.

use std::path::Path;
use std::sync::Arc;

use crate::raster::Renderable;
use crate::{Result, invalid};

/// Largest file previewed, bytes.
const MAX_FILE: u64 = 512 << 20;

/// The first page of the PDF at `path`, its longer side at most `max_side` pixels, as grey
/// pixels (width, height, row-major).
pub fn first_page_preview(path: &Path, max_side: f32) -> Result<(usize, usize, Vec<u8>)> {
    let len = std::fs::metadata(path)
        .map_err(|e| invalid(format!("{}: {e}", path.display())))?
        .len();
    if len > MAX_FILE {
        return Err(invalid("the file is too large to preview"));
    }
    let bytes = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    let doc = Renderable::new(Arc::new(bytes), false)?;
    if doc.page_count() == 0 {
        return Err(invalid("the file has no pages"));
    }
    let img = doc.render(0, 4.0, max_side.clamp(16.0, 1024.0))?;
    Ok((img.gray.w, img.gray.h, img.gray.px))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn previews_the_first_page_small() {
        let p = std::env::temp_dir().join(format!("mc-preview-{}.pdf", std::process::id()));
        std::fs::write(
            &p,
            pdf(&[SyntheticPage::new(612.0, 792.0, rect(100.0, 100.0, 200.0, 200.0))]),
        )
        .unwrap();
        let (w, h, px) = first_page_preview(&p, 120.0).unwrap();
        assert!(w <= 120 && h <= 120 && h > w);
        assert_eq!(px.len(), w * h);
        assert!(px.iter().any(|v| *v < 128), "the box is drawn");
        let _ = std::fs::remove_file(&p);
        assert!(first_page_preview(Path::new("no such file.pdf"), 120.0).is_err());
    }
}
