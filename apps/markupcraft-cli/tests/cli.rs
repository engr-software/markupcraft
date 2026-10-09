//! The CLI end to end: tools, run --script, run <tool>, mcp, pages, and the scorecard commands
//! on files built here.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

fn exe() -> Command {
    Command::new(env!("CARGO_BIN_EXE_markupcraft-cli"))
}

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "markupcraft-cli-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Output {
    exe().args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn script(d: &Path, steps: Value) -> Output {
    let s = d.join("steps.json");
    std::fs::write(&s, steps.to_string()).unwrap();
    run(&["run", "--script", s.to_str().unwrap(), "--root", d.to_str().unwrap()])
}

/// A 3-page file with one area per page at 1/8" = 1'-0".
fn sample(d: &Path) -> PathBuf {
    let sq = |x: f64| json!([[x, 100.0], [x + 90.0, 100.0], [x + 90.0, 190.0], [x, 190.0]]);
    let o = script(
        d,
        json!([
            { "tool": "doc_new", "params": { "path": "in.pdf", "pages": 3 } },
            { "tool": "scale_set", "params": { "scale": { "kind": "architectural", "paper_inches": 0.125, "real_feet": 1 } } },
            { "tool": "markup_add", "params": { "page": 1, "kind": "Area", "points": sq(100.0), "label": "one" } },
            { "tool": "markup_add", "params": { "page": 2, "kind": "Area", "points": sq(200.0), "label": "two" } },
            { "tool": "markup_add", "params": { "page": 3, "kind": "Area", "points": sq(300.0), "label": "three" } },
            { "tool": "doc_save", "params": {} }
        ]),
    );
    assert!(o.status.success(), "{}", text(&o));
    d.join("in.pdf")
}

#[test]
fn tools_lists_the_table() {
    let o = run(&["tools"]);
    assert!(o.status.success());
    let t = text(&o);
    assert!(t.contains("markup_add") && t.contains("page_extract"), "{t}");
    let o = run(&["tools", "--json"]);
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v.as_array().unwrap().iter().any(|t| t["name"] == "doc_open"));
}

#[test]
fn run_script_and_single_tool() {
    let d = dir();
    sample(&d);
    let o = script(
        &d,
        json!([
            { "tool": "doc_open", "params": { "path": "in.pdf" } },
            { "tool": "markup_list", "args": { "page": 2 } },
            { "tool": "measure", "params": { "page": 1, "points": [[0, 0], [9, 0]] } }
        ]),
    );
    assert!(o.status.success(), "{}", text(&o));
    let lines: Vec<Value> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[1]["result"]["markups"][0]["label"], "two");
    assert_eq!(lines[2]["result"]["text"], "1'-0\"");
    // A failing step stops the script and names the step.
    let o = script(&d, json!([{ "tool": "doc_open", "params": { "path": "../x.pdf" } }]));
    assert!(!o.status.success());
    assert!(
        text(&o).contains("step 1 (doc_open)") && text(&o).contains("outside"),
        "{}",
        text(&o)
    );
    // One tool with key=value arguments.
    let o = run(&["run", "doc_open", "path=in.pdf", "--root", d.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["markups"], 3);
}

#[test]
fn mcp_over_stdio() {
    let d = dir();
    sample(&d);
    let mut child = exe()
        .args(["mcp", "--root", d.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for m in [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "doc_open", "arguments": { "path": "in.pdf" } } }),
        ] {
            writeln!(stdin, "{m}").unwrap();
        }
    }
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let replies: Vec<Value> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies[1]["result"]["structuredContent"]["markups"], 3);
}

#[test]
fn pages_operations() {
    let d = dir();
    let input = sample(&d);
    let i = input.to_str().unwrap();
    let out = |name: &str| d.join(name).to_str().unwrap().to_string();
    let markups_on = |path: &str| -> Vec<(u64, String)> {
        let o = script(
            &d,
            json!([{ "tool": "doc_open", "params": { "path": path } }, { "tool": "markup_list", "params": {} }]),
        );
        let last: Value = serde_json::from_str(String::from_utf8_lossy(&o.stdout).lines().last().unwrap()).unwrap();
        last["result"]["markups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["page"].as_u64().unwrap(), m["label"].as_str().unwrap().to_string()))
            .collect()
    };

    let o = run(&["pages", i, &out("rot.pdf"), "rotate", "1,3", "90"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("3 pages, 3 markups"), "{}", text(&o));

    let o = run(&["pages", i, &out("del.pdf"), "delete", "2"]);
    assert!(text(&o).contains("2 pages, 2 markups"), "{}", text(&o));

    let o = run(&["pages", i, &out("mv.pdf"), "move", "3", "1"]);
    assert!(o.status.success(), "{}", text(&o));
    let got = markups_on(&out("mv.pdf"));
    assert!(
        got.contains(&(1, "three".into())) && got.contains(&(2, "one".into())),
        "{got:?}"
    );

    let o = run(&["pages", i, &out("blank.pdf"), "blank", "2", "2"]);
    assert!(text(&o).contains("5 pages, 3 markups"), "{}", text(&o));

    let o = run(&["pages", i, &out("ins.pdf"), "insert", "1", i, "2-3"]);
    assert!(text(&o).contains("5 pages, 5 markups"), "{}", text(&o));

    let o = run(&["pages", i, &out("ext.pdf"), "extract", "2-"]);
    assert!(text(&o).contains("2 pages, 2 markups"), "{}", text(&o));

    let o = run(&["pages", i, &out("bad.pdf"), "rotate", "1-9", "90"]);
    assert!(!o.status.success() && text(&o).contains("1-3"), "{}", text(&o));
    let o = run(&["pages", i, &out("bad.pdf"), "spin", "1"]);
    assert!(
        !o.status.success() && text(&o).contains("unknown page operation"),
        "{}",
        text(&o)
    );
}

#[test]
fn scorecard_commands_still_work() {
    let d = dir();
    let input = sample(&d);
    let i = input.to_str().unwrap();
    let csv = d.join("list.csv");
    let o = run(&["list", i, "--csv", csv.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("3 pages, 3 markups"), "{}", text(&o));
    assert!(csv.exists());
    let o = run(&["check", i]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("value agrees 3/3"), "{}", text(&o));
    let o = run(&["resave", i, d.join("re.pdf").to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    let o = run(&["markupcheck", i, d.join("mc.pdf").to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    let o = run(&["demo", i, d.join("demo.pdf").to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    let o = run(&[]);
    assert_eq!(o.status.code(), Some(2));
}
