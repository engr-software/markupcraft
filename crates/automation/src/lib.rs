//! markupcraft-automation: the headless tool table over the engine.
//!
//! [`Automation`] holds open documents (engine [`Session`]s) and runs tools by name with JSON
//! arguments, returning JSON. The same table serves `markupcraft-cli run`, the opt-in MCP
//! server ([`mcp`]) and the app's control channel.
//!
//! Conventions:
//! - Tool names are `snake_case` (MCP clients reject dots).
//! - Pages are **1-based**. Coordinates are PDF user space: points, origin at the bottom-left
//!   of the unrotated page (the space markups are stored in).
//! - `doc` is optional everywhere: it defaults to the document opened (or used) last.
//! - Markup tools act on `ids`, or on the document's selection when `ids` is omitted.
//! - Unknown arguments are rejected, so typos fail loudly. Errors say what to do instead.
//! - With a root ([`Automation::with_root`]) every path a tool reads or writes must resolve
//!   inside it.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod args;
pub mod mcp;
mod paths;
pub mod tools;

use std::path::{Path, PathBuf};

use markupcraft_engine::{EngineError, Markup, Session};
use serde_json::{Value, json};

pub use args::{Args, parse_range};
pub use tools::{TOOLS, Tool};

/// Why a tool call failed.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolError {
    /// No tool has this name.
    UnknownTool(String),
    /// The arguments do not match the tool's schema.
    InvalidArgs(String),
    /// The tool ran and failed (the message is for the agent to read).
    Failed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::UnknownTool(t) => write!(f, "unknown tool {t:?} (the `tools` command lists them)"),
            ToolError::InvalidArgs(m) | ToolError::Failed(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ToolError {}

impl From<EngineError> for ToolError {
    fn from(e: EngineError) -> Self {
        ToolError::Failed(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ToolError>;

pub(crate) fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(e.to_string())
}

pub(crate) fn bad_args(e: impl std::fmt::Display) -> ToolError {
    ToolError::InvalidArgs(e.to_string())
}

/// Most documents open at once.
const MAX_DOCS: usize = 64;

/// Open documents and the tools that drive them.
pub struct Automation {
    docs: Vec<(u64, Session)>,
    next_id: u64,
    current: Option<u64>,
    root: Option<PathBuf>,
    /// shared between documents, so markups paste from one into another
    clipboard: Vec<Markup>,
    author: String,
    /// preferences, profiles and the stamp library (None = the user's config folder)
    config_dir: Option<PathBuf>,
}

impl Default for Automation {
    fn default() -> Self {
        Self::new()
    }
}

impl Automation {
    pub fn new() -> Self {
        Self {
            docs: Vec::new(),
            next_id: 1,
            current: None,
            root: None,
            clipboard: Vec::new(),
            author: String::new(),
            config_dir: None,
        }
    }

    /// Confine every path the tools read or write to `root` (relative paths resolve inside it).
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into().canonicalize()?;
        if !root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the root must be a folder",
            ));
        }
        self.root = Some(root);
        Ok(self)
    }

    /// The author written on new markups.
    pub fn with_author(mut self, author: &str) -> Self {
        self.author = author.to_string();
        self
    }

    /// Keep preferences, profiles and the stamp library in `dir` instead of the user's config
    /// folder.
    pub fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dir = Some(dir.into());
        self
    }

    /// The config folder the preference and stamp tools use.
    pub fn config_dir(&self) -> Result<PathBuf> {
        match &self.config_dir {
            Some(d) => Ok(d.clone()),
            None => markupcraft_engine::prefs::default_config_dir()
                .ok_or_else(|| failed("no config folder (set MARKUPCRAFT_CONFIG_DIR)")),
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Run tool `name` with JSON `args` (an object; `null` means no arguments).
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value> {
        let empty = json!({});
        let args = if args.is_null() { &empty } else { args };
        let tool = tools::find(name).ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
        let a = Args::checked(tool, args)?;
        (tool.run)(self, &a)
    }

    /// Resolve a path argument (inside the root when one is set).
    pub fn resolve(&self, path: &str, for_write: bool) -> Result<PathBuf> {
        paths::resolve(self.root.as_deref(), path, for_write)
    }

    // ---- documents ---------------------------------------------------------------------------

    pub(crate) fn add_doc(&mut self, mut s: Session) -> Result<u64> {
        if self.docs.len() >= MAX_DOCS {
            return Err(failed(format!(
                "{MAX_DOCS} documents are open; close one first (doc_close)"
            )));
        }
        s.set_author(&self.author);
        let id = self.next_id;
        self.next_id += 1;
        self.docs.push((id, s));
        self.current = Some(id);
        Ok(id)
    }

    pub(crate) fn remove_doc(&mut self, id: u64) {
        self.docs.retain(|(d, _)| *d != id);
        if self.current == Some(id) {
            self.current = self.docs.last().map(|(d, _)| *d);
        }
    }

    pub(crate) fn docs(&self) -> &[(u64, Session)] {
        &self.docs
    }

    /// The document `doc` names, or the current one.
    pub(crate) fn doc_id(&self, a: &Args) -> Result<u64> {
        match a.opt_u64("doc")? {
            Some(id) if self.docs.iter().any(|(d, _)| *d == id) => Ok(id),
            Some(id) => Err(failed(format!("no open document {id} (doc_list shows the open ones)"))),
            None => self
                .current
                .ok_or_else(|| failed("no document is open; open one with doc_open or doc_new")),
        }
    }

    pub(crate) fn session(&mut self, a: &Args) -> Result<(u64, &mut Session)> {
        let id = self.doc_id(a)?;
        self.current = Some(id);
        let s = self
            .docs
            .iter_mut()
            .find(|(d, _)| *d == id)
            .map(|(_, s)| s)
            .ok_or_else(|| failed("document vanished"))?;
        Ok((id, s))
    }

    pub(crate) fn session_ref(&self, a: &Args) -> Result<(u64, &Session)> {
        let id = self.doc_id(a)?;
        let s = self
            .docs
            .iter()
            .find(|(d, _)| *d == id)
            .map(|(_, s)| s)
            .ok_or_else(|| failed("document vanished"))?;
        Ok((id, s))
    }

    pub(crate) fn clipboard(&self) -> &[Markup] {
        &self.clipboard
    }

    pub(crate) fn set_clipboard(&mut self, items: Vec<Markup>) {
        self.clipboard = items;
    }
}

/// A document's summary, returned by every tool that changes it.
pub(crate) fn summary(id: u64, s: &Session) -> Value {
    json!({
        "doc": id,
        "path": s.path().display().to_string(),
        "pages": s.page_count(),
        "markups": s.doc().markups.len(),
        "dirty": s.is_dirty(),
        "undo": s.undo_label(),
        "redo": s.redo_label(),
        "selection": s.selection(),
    })
}
