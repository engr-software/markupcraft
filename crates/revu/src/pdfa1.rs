//! PDF/A-1 files are PDF 1.4: saved with a classic cross-reference table (no object or
//! cross-reference streams) and a `%PDF-1.4` header.

use pdfcraft_cos::{Document as CosDoc, Object};

/// Largest XMP packet read.
const MAX_XMP: usize = 4 << 20;

/// Whether the catalog's XMP metadata declares PDF/A part 1 (`pdfaid:part` 1).
pub fn declares_part1(cos: &CosDoc) -> bool {
    let Some(root) = cos.root() else { return false };
    let Some(cat) = cos.dict(&Object::Ref(root)) else {
        return false;
    };
    let Some(m) = cat.get(b"Metadata") else {
        return false;
    };
    let Object::Stream(s) = &*cos.resolve(m) else {
        return false;
    };
    let Ok(b) = s.decoded_within(MAX_XMP) else {
        return false;
    };
    let x = String::from_utf8_lossy(&b);
    let compact: String = x.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("<pdfaid:part>1</pdfaid:part>")
        || compact.contains("pdfaid:part=\"1\"")
        || compact.contains("pdfaid:part='1'")
}

/// Rewrite the header of a written file to `%PDF-1.4` (same length, so no offset moves).
pub fn header_14(bytes: &mut [u8]) {
    if let Some(h) = bytes.get_mut(..8)
        && h.starts_with(b"%PDF-")
        && h.get(6) == Some(&b'.')
    {
        h.copy_from_slice(b"%PDF-1.4");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_becomes_14_without_moving_bytes() {
        let mut b = b"%PDF-1.7\n%x".to_vec();
        header_14(&mut b);
        assert_eq!(&b, b"%PDF-1.4\n%x");
        let mut short = b"%PDF".to_vec();
        header_14(&mut short);
        assert_eq!(&short, b"%PDF");
    }
}
