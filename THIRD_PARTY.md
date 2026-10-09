# Third-party software and assets

MarkupCraft is written in Rust; dependencies keep their own licenses (`cargo tree` lists them all).

| Component | Use | License |
|---|---|---|
| [PdfCraft](https://github.com/storytold/pdfcraft) `pdfcraft-cos`, `pdfcraft-render` (pinned commit) | PDF object layer, page rendering | MIT OR Apache-2.0 |
| [hayro](https://github.com/LaurenzV/hayro) (with PdfCraft's safety patches) | PDF interpretation and rasterization | MIT OR Apache-2.0 |
| [egui / eframe](https://github.com/emilk/egui) with wgpu | user interface, GPU canvas | MIT OR Apache-2.0 |
| en_US dictionary from [SCOWL](http://wordlist.aspell.net/) via [wooorm/dictionaries](https://github.com/wooorm/dictionaries) | spell check word list, `assets/dictionaries/en_US.*` | SCOWL permissive (MIT-like); text in `assets/dictionaries/en_US.LICENSE.txt` |
| MarkupCraft logo (`assets/logo*.svg`, `.png`, `.ico`) | app icon | contributor-original, MIT OR Apache-2.0 |
| Advance widths of Helvetica, Helvetica-Bold and Times-Roman from the Adobe Core 14 AFM metrics | approximate text layout, `crates/geom/src/text.rs` | Adobe AFM license (free use, copying and distribution) |
