# Architecture

```
apps/
  markupcraft/          desktop app (eframe + wgpu): window, menus, docking panels, canvas
  markupcraft-cli/      scorecard, list/CSV, `run --script`, `mcp` (opt-in)
  markupcraft-web/      the same UI compiled to WASM (trunk)
crates/
  geom/                 L0  points, rects, paths, hit tests, clouds, line endings
  measure/              L1  /Measure /RL scales, number formats, units, presets, calibration
  model/                L2  Markup, Kind, PageInfo, Viewport, Document, columns, captions
  revu/                 L3  Revu-compatible PDF read/write on pdfcraft-cos (kinds registry, /AP)
  render/               L3  page rendering (pdfcraft-render / hayro), tiles, thumbnails, text
  engine/               L4  an open document session: edits, undo/redo, selection, tools, search
  automation/           L5  the tool table: JSON in/out, shared by CLI run, MCP, control channel
  ui-egui/              L6  the Revu-style interface
xtask/                  ci, scorecard, parity, package
parity/                 revu-features.toml: one row per Revu feature, status + evidence
```

Rule: nothing below `ui-egui` depends on egui/winit/eframe/rfd. `geom`, `measure` and `model`
have no PDF dependency. The file IS the project: markups live in the model while editing and are
written back as standard PDF annotations on save (incremental by default).

## PDF stack

`pdfcraft-cos` (PdfCraft's object layer, f64 reals, copy-on-write, incremental/full saves) and
`pdfcraft-render` (hayro-based page renderer with a tile pool) are pinned git dependencies,
with the same safety patches PdfCraft carries (`[patch.crates-io]` in `Cargo.toml`).

## Registration points (one line each, so parallel work merges cleanly)

| To add | Where |
|---|---|
| a markup kind's PDF form | a `pub static FOO: AnnotKind` in `crates/revu/src/kinds/<family>.rs`, one line in `REGISTRY` (`kinds/mod.rs`) |
| a model field | `crates/model/src/lib.rs` `Markup` + its `Default` |
| a Markups List extra (columns, status, replies, groups) | `crates/revu/src/extras.rs` hooks |
| an engine command | `crates/engine/src/commands/<area>.rs`, one line in the command table |
| a headless tool | `crates/automation/src/tools/<area>.rs`, one line in `TOOLS` |
| a UI panel | `crates/ui-egui/src/panels/<name>.rs`, one line in the panel registry |
| a drawing tool | a `pub static` `ToolDef` in `crates/ui-egui/src/tools/<family>.rs` (gesture + markup kind), one line in the tool registry; `tools::new_markup` builds its markup |
| a menu / toolbar command | `crates/ui-egui/src/commands.rs` table (id, label, shortcut, icon) |
| a parity row status | `parity/revu-features.toml` |

## Revu file format (what is known)

Measurements are ordinary annotations plus Revu keys:

| Kind | Subtype | /IT | /MeasurementTypes |
|---|---|---|---|
| Area | Polygon | PolygonDimension | 129 |
| Perimeter | Polygon | PolygonDimension | 130 |
| Length | Line | LineDimension | 130 |
| Polylength | PolyLine | PolyLineDimension | 130 |
| Count | PolyLine | PolyLineCount (guess) | 128 |
| Volume | Polygon | PolygonDimension | 132 |

- `/Measure /RL` with `/X /D /A /V` number formats. Value = geometry × `/X /C` × `/D /C`.
  Revu always reduces fractions and strips trailing decimal zeros, even with `/FD true`.
- Page scale: `/VP [<< /Type /Viewport /BBox /Measure /NM >>]`, BBox relative to the media box corner.
- `/CO` caption offset (Area/Perimeter: from the vertex mean; Length: [along, perpendicular]).
- `/AP /N` is required, or Revu shows nothing. Revu keys written on measurements: `/DS`,
  `/DepthUnit`, `/Cap`, `/SlopeType`, `/PitchRun`, `/AlignOnSegment`, `/LE`, `/LL`, `/LLE`.
- MarkupCraft's own keys (when Revu's storage is not known yet): `/PCCutouts`, `/PCCountSymbol`,
  `/PCSymbolScale`, `/PCCountShape`, `/PCSegmentValues`, `/PCRiseDrop`, `/PCCaptionOffset`,
  `/PCArcs`, `/PCColumns`, `/PCColumnData`, `/PCStamp`, `/PCSlope` (with Revu's `/SlopeType`), `/PCCaption`
  (caption contents), `/PCCapLeader`, `/PCCentroid`; our intents `/PCDimension` (a `/Line` with `/LL` `/LLE`)
  and `/PCArc` (a three-point `/PolyLine`).

## The interface (`crates/ui-egui`)

- Every edit is a `markupcraft_engine::Session` call, the session automation drives, so undo,
  the command table, the clipboard and page operations are shared. A gesture that edits many
  times (a slider drag, typing) runs under a merge key and is one undo step
  (`Session::set_merge_key`, `Session::seal`).
- `interact.rs` runs the active tool: drafts (points, drags, freehand), the text editor over the
  page, Select-tool gestures shown as live previews and committed on release, the context menu.
- Snapping (`snapping.rs`): page linework from `markupcraft_render::snap` indexed per page in
  `markupcraft_geom::snap::SnapIndex` on a worker thread, plus markup vertices and the grid.
- Tool Chest (`chest.rs`): My Tools, the user's tool sets and Set as Default looks in
  `<config folder>/MarkupCraft/toolchest.json` (`MARKUPCRAFT_CONFIG_DIR` overrides the folder);
  Recent Tools live for the session. The desktop app loads it; tests and the headless `shot`
  example use an in-memory chest.
- File dialogs run on their own threads (`dialogs.rs`); the frame loop polls for answers.
- The shell (`shell/`): bars and toolbars, full screen and presentation, view history, rotate
  view and page layout modes (`canvas.rs` lays out one or two pages to a row), rulers and the
  crosshair, the dimmer, split views (the second pane keeps its own `DocView` and renderer and
  swaps them in to draw), document tabs, recent files and the last session
  (`<config>/recent.json`), page dialogs. `AppState::run`, `enabled` and `checked` ask
  `shell::run` / `enabled` / `checked` first, so shell commands are rows in the command table
  plus an arm there. Preferences (`prefs_ui.rs`) edit the engine's `prefs` (per profile) plus
  the interface preferences in `<config>/ui/<profile>.json` (`shell::UiPrefs`, including the
  dock layout).
- More markup and takeoff work: `more.rs` (its own command table joined to the menus: the
  Measure tool, Recalculate, Review Text, Import / Export Markups, Unflatten, Edit Action,
  temporary measurements, display options; Count series and arc / cutout rows of the canvas
  menu), `gestures.rs` (the Eraser, Lasso, Select Text, Insert / Replace Text, Flag and File
  Attachment tools, `ToolKind::Special`), `chest_more.rs` (Tool Chest options and the user's
  library, saved in the Tool Chest file: scale presets, line styles, profile columns, custom
  statuses, pinned and locked sets) and `panels/properties_more.rs` (Properties sections for
  dimensions, captions, slope, counts, attachments, text layout and replies). Their engine
  calls live in `markup_ops.rs`, `unflatten.rs` and `summary_links.rs`.
- Document and review features (`features/`, one file each: search, compare, overlay, summary,
  print, document operations, redaction, forms, spelling, stamps, batch, Dynamic Fill, spaces,
  links, signatures, sets) keep their state in `AppState::features`, add menu rows in
  `features::COMMANDS`, and ask for files with `Purpose::Feature`. A feature that needs a point,
  box or outline on the page starts a pick (`features::start_pick`); `features/canvas.rs` runs it
  ahead of the active tool and draws the features' highlights over the pages.
  `/PCArcs`, `/PCColumns`, `/PCColumnData`, `/PCStamp`, `/PCHatch`, `/PCLegend`, page `/PCSpaces`.
