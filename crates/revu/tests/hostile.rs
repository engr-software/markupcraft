//! Damaged and hostile files found by `cargo xtask fuzz`, rebuilt as small synthetic PDFs: each
//! panicked (or, in a release build, silently broke the next save) before its fix.

use std::path::Path;
use std::sync::Arc;

/// A one-page PDF whose page object is numbered `page_num` and whose trailer says `/Size size`,
/// with a cross-reference table that does not lead to a catalog (so the reader rebuilds it from
/// the object headers).
fn damaged(page_num: u64, size: &str) -> Vec<u8> {
    format!(
        "%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
         2 0 obj\n<< /Type /Pages /Kids [{page_num} 0 R] /Count 1 >>\nendobj\n\
         {page_num} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
         xref\n0 3\n0000000000 65535 f \n0000000015 00000 n \n0000000064 00000 n \n\
         trailer\n<<  obj /Size {size} /Root 1 0 R >>\nstartxref\n9\n%%EOF\n"
    )
    .into_bytes()
}

fn open(bytes: Vec<u8>) -> Result<(), String> {
    markupcraft_revu::open_bytes(Arc::new(bytes), Path::new("x.pdf"))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// fuzz: an object numbered 4294967295 (u32::MAX) made the object layer's next free number
/// overflow while opening (a panic in debug builds; in release it wrapped to 0, so the first
/// markup added would have been written as object 0).
#[test]
fn an_object_numbered_u32_max_is_refused_not_a_crash() {
    let e = open(damaged(4_294_967_295, "6")).expect_err("refused");
    assert!(e.contains("out of range") || e.contains("could not read"), "{e}");
    // Large but sane numbers still open.
    open(damaged(70_000, "6")).unwrap();
}

/// fuzz: a trailer /Size of 99999999999999999999 opened fine, then the first new object of a
/// save was numbered 4294967295 and the next one overflowed (a panic in debug builds, object 0
/// in release).
#[test]
fn an_absurd_trailer_size_is_refused_not_saved_broken() {
    for size in ["99999999999999999999", "4294967295", "2147483648"] {
        assert!(open(damaged(4, size)).is_err(), "/Size {size}");
    }
    open(damaged(4, "5")).unwrap();
}
