//! End-to-end test of the Markup Summary tool (CSV, XML, Excel and PDF reports).

use markupcraft_automation::Automation;
use serde_json::json;

#[test]
fn summary_export_writes_csv_xml_xlsx_and_pdf() {
    let d = std::env::temp_dir().join(format!("markupcraft-summary-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("plan.pdf"), markupcraft_render::synthetic::sample_pdf()).unwrap();
    let mut a = Automation::new().with_root(&d).unwrap();
    a.call("doc_open", &json!({ "path": "plan.pdf" })).unwrap();
    for (name, magic) in [
        ("s.csv", &b"Subject"[..]),
        ("s.xml", &b"<?xml"[..]),
        ("s.xlsx", &b"PK"[..]),
        ("s.pdf", &b"%PDF"[..]),
    ] {
        let v = a
            .call("summary_export", &json!({ "out": name, "group_by": ["type"] }))
            .unwrap();
        assert_eq!(v["markups"], 6, "{name}: {v}");
        assert!(std::fs::read(d.join(name)).unwrap().starts_with(magic), "{name}");
    }
    let v = a
        .call(
            "summary_export",
            &json!({ "out": "p2.csv", "pages": [2], "columns": ["subject", "measurement"] }),
        )
        .unwrap();
    assert!(v["markups"].as_u64().unwrap() < 6);
    let csv = std::fs::read_to_string(d.join("p2.csv")).unwrap();
    assert!(csv.starts_with("Subject,Measurement"), "{csv}");
    assert!(a.call("summary_export", &json!({ "out": "x.doc" })).is_err());
    let _ = std::fs::remove_dir_all(&d);
}
