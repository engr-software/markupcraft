# Changelog

All notable changes to MarkupCraft. Versions follow [Semantic Versioning](https://semver.org);
each version is published as three releases, one per operating system
(`vX.Y.Z-windows`, `vX.Y.Z-macos`, `vX.Y.Z-linux`), on the
[Releases page](https://github.com/engr-software/markupcraft/releases).
Feature status per row is in [FEATURES.md](FEATURES.md).

## Unreleased

### Interface

- The default window layout follows the one Revu users know ([docs/UI_LAYOUT.md](docs/UI_LAYOUT.md)):
  the MarkupCraft menu, then File, Edit, View, Document, Batch, Tools, Window and Help, with the
  Markup and Measure commands under Tools; a document bar (documents, name, page count,
  properties); a panel bar on the left edge that opens one panel at a time in the left panel
  area; the markup and measurement tools in a strip on the right edge; one bottom bar (Markups
  List toggle, thumbnail size, snapping, view layout, navigation tools, page box "label (n of N)",
  theme switch, page size and scale); the Markups List under the canvas; and a dropdown of open
  documents at the end of the tab strip. The icon toolbars are hidden by default (Window >
  Toolbars). Command ids and shortcuts are unchanged.
- Dark theme by default; the light theme is in Preferences and on the bottom bar.
- Saved profiles take the new layout once (layout version 2); Window > Reset Panel Layout
  restores it later.

## 0.4.0 (2026-10-09)

Every in-scope Revu feature now has a **blind acceptance verdict**: a second test, written from
the feature inventory by someone who did not build it, driving the app through its tools and its
real interface (see [docs/EQUIVALENCE.md](docs/EQUIVALENCE.md#blind-acceptance-testing)). Of 905
rows in scope, 777 passed as built, 112 found a bug that is now fixed, and 16 remain known gaps.

### Acceptance testing

- New `crates/acceptance` harness and `accepted` / `acceptance` fields in the parity table;
  `cargo xtask parity --check` verifies both.
- Acceptance passes over measurement, markups, documents and pages, batch and output, review
  (compare, overlay, search) and the whole interface, including every default keyboard shortcut.
- FEATURES.md shows the verdict on every row and pass / fixed / gap totals per area.

### Fixed by acceptance testing (highlights)

- Full saves re-point markups at their renumbered annotations, so replies, groups and status
  survive a full rewrite.
- A page scale stored in a full-page viewport is recognized by the Measurements panel and the
  thumbnails; the status bar says "Scale Not Set" as Revu does.
- Clipboard shortcuts work in the desktop app (Copy, Cut, Paste, Format Painter, Paste in Place,
  Copy Page to Snapshot), and the Delete Pages / Extract Pages keys open their dialogs.
- Long menus scroll; tests and development runs keep their settings out of the user's
  configuration folder.

### Gaps closed

- Split views: up to 16 panes, each with its own document tabs; proportional corner resize.
- A background job queue for long batch work; slip sheets with pairing, superseded pages and
  reports; Batch Link places and URLs; batch apply stamp; combine with page ranges; split by size.
- Summary straight to the printer, PDF/A-1b output, header and footer file tokens, any-angle page
  rotation, text alignment, the markup layer, align to reference, Dynamic Fill clear.
- MarkupCraft's own help (F1), shipped profiles with rename and bundles, named toolbars, actions on
  search results, vector visual search limits, split counts by space, a links filter.
- Spanish interface, Enhance Thin Lines and the 2D rendering preferences, Sets, Import / Export,
  Tablet, Markup and Admin preferences, and document JavaScript from trusted folders.
- Compare can write `_Diff` files.

### Hardening

- An end-to-end takeoff test drives an estimator's whole day on a real Revu-marked set when one
  is supplied (`MARKUPCRAFT_REF_PDF`); it skips otherwise.
- `cargo xtask bench` (performance) and `cargo xtask fuzz` (mutation fuzzing) with
  [docs/hardening.md](docs/hardening.md).
- Search text and Markups List cells are cached; hostile object numbers, huge measurement values
  and renders that never finish are reported as errors instead of hanging.
- Compare ignores the text of markups unless asked to include markups.

## 0.3.0 (2026-10-09)

The first public release: a clean-room, Revu-compatible PDF markup and takeoff app in Rust, with
a desktop interface, a command-line tool and an opt-in MCP server, on Windows, macOS and Linux.

### File format and measurement

- Revu-compatible read and write of every measurement (Length, Polylength, Area, Perimeter,
  Count, Volume, Diameter, Radius, Angle) and every other markup kind (shapes, clouds, text boxes,
  callouts, pen, stamps, text markups, notes, snapshots), with appearance streams.
- Page scales and viewports (`/Measure`, `/VP`), unit and scale presets, calibration, captions,
  cutouts, arcs, slope, segment values; quantities and labels checked against a real Revu-marked
  set (scorecard: check 410/410, resave 410/410, markupcheck 187/187).
- Markups List data: grouping, sorting, filters, per-unit totals, formula columns, custom columns,
  status and checkmarks, replies, groups, page labels; CSV, XML and Excel summaries.

### Engine, automation and command line

- A headless engine session with undo; one automation tool table shared by
  `markupcraft-cli run`, the MCP server (`markupcraft-cli mcp`, opt-in, `--root` confinement) and
  the app; `markupcraft-cli` subcommands for listing, summaries, page operations and the scorecard.

### Interface

- A Revu-style egui + wgpu desktop shell: tabs, split views, rulers, crosshair, view modes, full
  screen, Preferences, recent files, thumbnails, saved layouts and Revu's default shortcuts.
- Every drawing tool with Revu's interaction and snapping, Properties, the Markups List, the
  Measurements panel, context menus and the Tool Chest; align, distribute, flip, rotate, Format
  Painter, rich text and live spell check.
- Feature panels and dialogs: search and visual search, compare, overlay, Spaces, layers, links,
  signatures, Sets, bookmarks, summary, print, OCR, redaction, forms, stamps, Dynamic Fill, batch.

### Documents and review

- Bookmarks, page labels, text search, links, attachments, properties, headers and footers,
  watermarks, Bates numbering, flatten and unflatten, XFDF / FDF, security, print-ready PDF,
  split, combine, replace pages and page boxes.
- Compare documents, overlay pages, visual search, OCR on a worker, redaction, forms (including
  XFA), digital signatures, spell check, Quantity Link, Batch Link, legends, PDF packages,
  stitching, Smart Overlay, Batch Sign and Seal, and Office, DXF, GIF and TIFF import.

### Packaging

- Per-OS packages from `cargo xtask package`: a Windows installer and portable zip, a macOS disk
  image and a Linux AppImage and tarball, each release with `SHA256SUMS.txt`.
