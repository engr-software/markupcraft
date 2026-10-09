# AGENTS.md: rules for every agent and contributor

MarkupCraft is a clean-room, open-source, Rust-native PDF markup and takeoff application that
works like Bluebeam Revu and reads and writes Revu-compatible markups. It runs natively on
Windows, macOS and Linux, and on the web via WASM. It follows the conventions of the Crafting
Apps family (PhotoCraft, PdfCraft) and builds on PdfCraft's PDF object layer and renderer.

## 1. Clean-room

- Behaviour comes only from public documentation (ISO 32000, Revu's public help and manuals),
  from PDFs we own read with a PDF parser, and from black-box observation of the running app.
- Never decompile, disassemble or copy anything from a Bluebeam installation, and never use
  Bluebeam icons, artwork, fonts, sample files or stamps. Our icons and visual design are our own.
- Never copy GPL/AGPL code.
- Never commit project drawings, client files, names of real people or companies, or local
  paths. Tests that need a real Revu-marked set read it from `MARKUPCRAFT_REF_PDF` and skip
  when it is not set.

## 2. Assets

Only openly licensed (MIT, Apache-2.0, BSD, ISC, OFL, CC0, CC-BY) or contributor-original assets.
Record each in `THIRD_PARTY.md`.

## 3. Never crash

Every PDF, script, tool call, settings file and keystroke is untrusted input.

- Return `Result`; no `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!` outside tests unless
  provably infallible (comment why). New crates start with the `#![deny(clippy::unwrap_used, ...)]` line.
- No indexing with input-derived positions (use `get`), checked arithmetic on input numbers,
  bounded recursion, capped allocations. No `unsafe` (`unsafe_code = "forbid"`).
- Never lose the user's work: saves write a temporary file, then rename.

## 4. Everything is reachable headlessly

Every user-facing feature is an engine call first (`crates/engine`), then a tool in
`crates/automation` (the same table serves `markupcraft-cli run`, the MCP server and the app's
control channel), then UI. A feature is done when its tool has an end-to-end test.

The MCP server and the control channel are opt-in: never started by default, loopback only,
token-protected.

## 5. Track parity honestly

Every Revu feature has a row in `parity/revu-features.toml` (from `docs/revu_features/`).
Status: `missing`, `partial`, `have` (built, tested from documentation) or `proven` (matches a
recording of real Revu, see `docs/EQUIVALENCE.md`). `have`/`proven` rows cite a test in
`evidence`; `cargo xtask parity` rejects evidence that does not exist.

## 6. Quality gates (before every commit)

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- the scorecard when `MARKUPCRAFT_REF_PDF` is set: `cargo xtask scorecard`
  (check 410/410, resave 410/410, markupcheck 187/187 on the reference set).

## 7. Working in parallel

Each agent works in its own git worktree with its own `CARGO_TARGET_DIR`. Registration points
are one-line additions (see `docs/ARCHITECTURE.md`) so parallel work merges cleanly. Commit
only green states. Commit as `engr-software <339296073+engr-software@users.noreply.github.com>`
with no co-author trailers.
