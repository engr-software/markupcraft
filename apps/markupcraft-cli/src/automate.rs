//! `tools`, `run` and `mcp`: the automation tool table from the command line.

use std::io::Write;

use markupcraft_automation::{Automation, TOOLS, mcp, tools};
use serde_json::{Value, json};

type Res = Result<bool, Box<dyn std::error::Error>>;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Arguments that are not flags or flag values.
fn positional(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if matches!(a.as_str(), "--root" | "--script" | "--author") {
            skip = true;
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        out.push(a.as_str());
    }
    out
}

fn automation(args: &[String]) -> Result<Automation, Box<dyn std::error::Error>> {
    let mut a = Automation::new();
    if let Some(root) = flag(args, "--root") {
        a = a.with_root(root).map_err(|e| format!("--root {root}: {e}"))?;
    }
    if let Some(author) = flag(args, "--author") {
        a = a.with_author(author);
    }
    Ok(a)
}

fn print(v: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{v}")?;
    out.flush()?;
    Ok(())
}

/// `tools [--json]`: the tool table.
pub fn list(args: &[String]) -> Res {
    if args.iter().any(|a| a == "--json") {
        print(&tools::list_json())?;
        return Ok(true);
    }
    let mut out = std::io::stdout().lock();
    for t in TOOLS {
        writeln!(out, "{:<20} {}", t.name, t.title)?;
    }
    writeln!(
        out,
        "\n{} tools; `markupcraft-cli tools --json` prints their schemas",
        TOOLS.len()
    )?;
    Ok(true)
}

/// `run --script steps.json [--root DIR]` or `run <tool> key=value ... [--root DIR]`.
pub fn run(args: &[String]) -> Res {
    let mut auto = automation(args)?;
    if let Some(script) = flag(args, "--script") {
        let text = std::fs::read_to_string(script).map_err(|e| format!("{script}: {e}"))?;
        let steps: Vec<Value> = serde_json::from_str(&text)
            .map_err(|e| format!("{script}: expected a JSON array of {{\"tool\", \"params\"}} steps ({e})"))?;
        for (i, step) in steps.iter().enumerate() {
            let n = i + 1;
            let tool = step["tool"]
                .as_str()
                .ok_or_else(|| format!("step {n}: missing \"tool\""))?;
            let params = if step.get("params").is_some() {
                &step["params"]
            } else {
                &step["args"]
            };
            let result = auto.call(tool, params).map_err(|e| format!("step {n} ({tool}): {e}"))?;
            print(&json!({ "step": n, "tool": tool, "result": result }))?;
        }
        return Ok(true);
    }
    let pos = positional(args);
    let tool = *pos
        .first()
        .ok_or("run: give --script steps.json, or a tool name and key=value arguments")?;
    let mut obj = serde_json::Map::new();
    for kv in pos.iter().skip(1) {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("run: expected key=value, got {kv:?}"))?;
        let value = serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string()));
        obj.insert(k.to_string(), value);
    }
    let result = auto.call(tool, &Value::Object(obj))?;
    print(&result)?;
    Ok(true)
}

/// `mcp [--root DIR]`: serve MCP on stdin/stdout until stdin closes. Opt-in only.
pub fn serve_mcp(args: &[String]) -> Res {
    let mut server = mcp::McpServer::new(automation(args)?);
    eprintln!(
        "markupcraft-cli: MCP server on stdio (protocol {}); close stdin to stop",
        mcp::PROTOCOL_VERSIONS.first().copied().unwrap_or("")
    );
    server.serve(std::io::stdin().lock(), std::io::stdout().lock())?;
    Ok(true)
}
