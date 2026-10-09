//! The acceptance harness itself works: tools, files and the headless app.
use markupcraft_acceptance::*;

#[test]
fn harness_drives_tools_and_the_app() {
    let dir = temp_dir("smoke");
    let pdf = sample_pdf(&dir, "plan.pdf");
    let mut a = automation(&dir);
    let v = call(
        &mut a,
        "doc_open",
        json!({ "path": pdf.file_name().unwrap().to_str().unwrap() }),
    );
    assert!(v.to_string().contains("pages"), "{v}");
    let h = app();
    assert!(!markups(&h).is_empty());
}
