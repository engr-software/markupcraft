# Third-party software and assets

MarkupCraft is written in Rust; dependencies keep their own licenses (`cargo tree` lists them all).

| Component | Use | License |
|---|---|---|
| [PdfCraft](https://github.com/storytold/pdfcraft) `pdfcraft-cos`, `pdfcraft-render` (pinned commit) | PDF object layer, page rendering | MIT OR Apache-2.0 |
| [PdfCraft](https://github.com/storytold/pdfcraft) `pdfcraft-automation` (adapted source) | `--root` path confinement (`crates/automation/src/paths.rs`), MCP server (`crates/automation/src/mcp.rs`), tool-table conventions | MIT OR Apache-2.0 |
| [hayro](https://github.com/LaurenzV/hayro) (with PdfCraft's safety patches) | PDF interpretation and rasterization | MIT OR Apache-2.0 |
| [egui / eframe](https://github.com/emilk/egui) with wgpu | user interface, GPU canvas | MIT OR Apache-2.0 |
| en_US dictionary from [SCOWL](http://wordlist.aspell.net/) via [wooorm/dictionaries](https://github.com/wooorm/dictionaries) | spell check word list, `assets/dictionaries/en_US.*` | SCOWL permissive (MIT-like); text in `assets/dictionaries/en_US.LICENSE.txt` |
| MarkupCraft logo (`assets/logo*.svg`, `.png`, `.ico`) | app icon | contributor-original, MIT OR Apache-2.0 |
| [egui_dock](https://github.com/anhosh/egui_dock) | docking panels | MIT |
| [Lucide](https://lucide.dev) icons (`crates/ui-egui/assets/icons/*.svg`) | toolbar, menu and panel icons | ISC; text in `crates/ui-egui/assets/icons/LICENSE-lucide.txt` |
| Code adapted from PdfCraft's `ui-egui` (icon tinting in `icons.rs`, design tokens in `theme.rs`, the headless `shot` example, the tile scheduling in `canvas.rs`) and `apps/pdfcraft/build.rs` | interface | MIT OR Apache-2.0 |
| [winresource](https://github.com/BenjaminRi/winresource) (build only) | Windows icon and version resource | MIT |
