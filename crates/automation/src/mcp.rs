//! A Model Context Protocol server over the tool table. Adapted from PdfCraft's
//! `pdfcraft-automation` (MIT OR Apache-2.0; see THIRD_PARTY.md).
//!
//! **Opt-in only.** Nothing starts it on its own: it runs while `markupcraft-cli mcp` runs
//! (an agent launches it from its MCP configuration) and stops when its input closes. It opens
//! no network port; the transport is newline-delimited JSON-RPC 2.0 over stdin/stdout.
//!
//! Implemented: `initialize`, `ping`, `tools/list`, `tools/call`, and the `notifications/*` a
//! client sends. Tool failures come back as results with `isError: true`, so the agent can read
//! them and recover; protocol errors use JSON-RPC error codes.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::{Automation, ToolError};

/// Protocol revisions spoken, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Longest request line read (a defence against unbounded input).
const MAX_LINE: usize = 64 << 20;

const INSTRUCTIONS: &str = "MarkupCraft edits PDF markups and takeoffs (Revu-compatible). Open a file with doc_open \
(or doc_new), inspect it (doc_info, markup_list, measure), edit markups (markup_add, markup_edit, markup_transform, \
markup_delete ...), set scales (scale_set, scale_calibrate) and pages (page_*). Edits are undoable (edit_undo) and stay \
in memory until doc_save. Pages are 1-based; coordinates are PDF points with the origin at the bottom-left.";

pub struct McpServer {
    automation: Automation,
}

impl McpServer {
    pub fn new(automation: Automation) -> Self {
        Self { automation }
    }

    pub fn automation(&self) -> &Automation {
        &self.automation
    }

    /// Serve until `input` reaches end of file.
    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let n = std::io::Read::take(input.by_ref(), MAX_LINE as u64).read_until(b'\n', &mut buf)?;
            if n == 0 {
                return Ok(());
            }
            if buf.last() != Some(&b'\n') && n >= MAX_LINE {
                // Too long: skip the rest of the line and say so.
                let mut skip = Vec::new();
                while input.read_until(b'\n', &mut skip)? > 0 && skip.last() != Some(&b'\n') {
                    skip.clear();
                }
                writeln!(output, "{}", error(Value::Null, INVALID_REQUEST, "request too large"))?;
                output.flush()?;
                continue;
            }
            let line = String::from_utf8_lossy(&buf);
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = self.handle_line(&line) {
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
        }
    }

    /// Handle one JSON-RPC message; returns the serialized reply, if one is due.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.handle(&msg),
            Err(e) => Some(error(Value::Null, PARSE_ERROR, &format!("parse error: {e}"))),
        };
        reply.map(|r| r.to_string())
    }

    /// Handle one parsed message. Notifications (no `id`) get no reply.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "expected a JSON-RPC request object",
            ));
        };
        let id = obj.get("id").cloned();
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            return id.map(|id| error(id, INVALID_REQUEST, "missing method"));
        };
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;
        Some(match self.dispatch(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str);
                let version = asked
                    .filter(|v| PROTOCOL_VERSIONS.contains(v))
                    .or(PROTOCOL_VERSIONS.first().copied())
                    .unwrap_or("2025-06-18");
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "markupcraft", "title": "MarkupCraft", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": crate::TOOLS.iter().map(|t| tool_json(t)).collect::<Vec<_>>() })),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or((INVALID_PARAMS, "tools/call needs a tool name".to_string()))?;
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                match self.automation.call(name, &args) {
                    Ok(v) => Ok(call_result(v)),
                    Err(ToolError::UnknownTool(t)) => Err((INVALID_PARAMS, format!("unknown tool {t:?}"))),
                    Err(e) => Ok(tool_error(&e.to_string())),
                }
            }
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_json(t: &crate::Tool) -> Value {
    json!({
        "name": t.name,
        "title": t.title,
        "description": t.description,
        "inputSchema": (t.schema)(),
        "annotations": { "title": t.title, "readOnlyHint": t.read_only, "destructiveHint": t.destructive, "openWorldHint": false },
    })
}

fn call_result(v: Value) -> Value {
    let text = serde_json::to_string_pretty(&v).unwrap_or_default();
    let mut out = json!({ "content": [{ "type": "text", "text": text }], "isError": false });
    if v.is_object() {
        out["structuredContent"] = v;
    }
    out
}

fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}
