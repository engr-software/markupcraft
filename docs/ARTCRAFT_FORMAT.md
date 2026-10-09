# The ArtCraft app format, and how MarkupCraft follows it

The Crafting Apps of the ArtCraft family ([PdfCraft](https://github.com/storytold/pdfcraft),
PhotoCraft) share one shape: an open-source, clean-room desktop app in pure Rust, built so that
agents and scripts can drive everything a person can. MarkupCraft is an independent project that
adopts the same shape. This page lists the conventions and where MarkupCraft follows or departs
from them.

## The conventions

| Convention | What it means in the family | MarkupCraft |
|---|---|---|
| **Workspace layout** | One Cargo workspace: `apps/` (binaries), `crates/` (libraries, layered), `xtask/` (project automation via `cargo xtask`). | Follows. `apps/markupcraft`, `apps/markupcraft-cli`, `apps/markupcraft-web`; eight crates from `geom` (L0) to `ui-egui` (L6); `xtask`. |
| **UI stack** | egui + eframe on wgpu; the same UI compiled to WASM for the web. | Follows (egui 0.36, wgpu). The web build is `apps/markupcraft-web`. |
| **Headless engine** | Every feature is an engine call first; nothing below the UI crate depends on egui, winit or a file dialog. | Follows (`crates/engine`; rule in `docs/ARCHITECTURE.md`). |
| **Automation tools** | One tool table, JSON in and out, shared by the CLI's scripted runs, an MCP server and the app's control channel. A feature is done when its tool has an end-to-end test. | Follows (`crates/automation`). |
| **Opt-in MCP / control channel** | Never started by default; loopback only; token-protected. | Follows (AGENTS.md section 4). |
| **Never crash** | Every file, script, tool call and keystroke is untrusted input: `Result` everywhere, no `unwrap`/`panic!` outside tests, checked arithmetic, bounded recursion, `unsafe_code = "forbid"`, atomic saves. | Follows (AGENTS.md section 3). |
| **Parity tracking** | A TOML table with one row per feature of the reference app, a status, and the test that proves it; `cargo xtask parity` rejects evidence that does not exist. | Follows, with a stricter top status: `proven` means it matches a recording of the real reference app, not just its documentation (`docs/EQUIVALENCE.md`). |
| **Quality gates** | `cargo fmt --check`, `clippy -D warnings`, `cargo test`, plus project gates, bundled as `cargo xtask ci`. | Follows. Adds the compatibility scorecard (`cargo xtask scorecard`), which runs only when a reference set is available. |
| **Per-OS packaging** | Scripts under `packaging/<os>/` that CI runs; artifacts named `<app>-<version>-<os>-<arch>.<ext>` with `SHA256SUMS.txt`. | Follows, simplified (below). |
| **Licensing** | MIT OR Apache-2.0 for code; only openly licensed or original assets, each recorded; no GPL code. | Follows (`LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md`). |
| **Clean-room** | Behavior from public documentation and black-box observation only; nothing taken from the original product. | Follows (AGENTS.md section 1). |

## Where MarkupCraft differs

- **Shared PDF stack.** Instead of its own PDF layer, MarkupCraft depends on PdfCraft's
  `pdfcraft-cos` and `pdfcraft-render` at a pinned commit, with the same safety patches.
- **Proof against real files.** Beyond the parity table, compatibility is measured by a scorecard
  on a real marked-up drawing set (check 410/410, resave 410/410, markupcheck 187/187). The set is
  never committed; tests and the scorecard read it from `MARKUPCRAFT_REF_PDF` and skip without it.
- **Packaging.** PdfCraft builds signed, notarized installers for more platforms (universal macOS,
  MSI via WiX, deb/rpm/Flatpak, FreeBSD) from a `release` branch into a draft release.
  MarkupCraft keeps a smaller set and the release shape of its earlier C++ version:
  - Windows x64: portable `.zip` and an NSIS installer (`-setup.exe`).
  - macOS arm64: `MarkupCraft.app` (with Info.plist and icon) in a `.dmg`, ad-hoc signed.
  - Linux x86_64: `.AppImage` and `.tar.gz` with the `.desktop` entry and icons.
  - A version tag `vX.Y.Z` publishes three separate releases, `vX.Y.Z-windows`, `vX.Y.Z-macos`,
    `vX.Y.Z-linux`, each with its own notes and `SHA256SUMS.txt` (see `docs/releasing.md`).
- **Branding.** MarkupCraft uses its own name, logo and icons. It does not use the ArtCraft
  brand assets or community links, which belong to the ArtCraft team.
- **Fewer xtask commands for now.** `ci`, `wasm`, `scorecard`, `parity`, `package`, `version`.
  Layering, asset-licence and dependency-licence gates (`layers`, `assets`, `deny` in PdfCraft)
  are candidates to add as the workspace grows.
