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
| a drawing tool | `crates/ui-egui/src/tools/<name>.rs`, one line in the tool registry |
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
  `/PCArcs`, `/PCColumns`, `/PCColumnData`, `/PCStamp`.
