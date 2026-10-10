# Equivalence with Revu: how behavior is proven

MarkupCraft's goal is identical behavior to Revu 21 for the features in
[revu_features/](revu_features/). "Matches the help pages" is not proof. A feature
counts as **proven** only when a recording of real Revu and MarkupCraft agree.

There are three levels of evidence, weakest first:

| Level | What it shows | Where |
|---|---|---|
| **have** | built, with an end-to-end test written by whoever built it | `evidence` in `parity/revu-features.toml` |
| **accepted** | a second, blind test written from the inventory text by someone who did not build it | `accepted` / `acceptance` in the same table; tests in `crates/acceptance` |
| **proven** | a recording of real Revu and MarkupCraft agree | the scorecard, `tests/revu_recordings/` |

Every in-scope row has a blind acceptance verdict; only measurement quantities and page scales
are **proven** so far. The file-format choices still waiting for a recording are listed under
[Guesses still needing a Revu recording](#guesses-still-needing-a-revu-recording).

## Status per feature

Each inventory row gets one of:

| Status | Meaning |
|---|---|
| proven | A Revu recording exists and MarkupCraft's test passes against it |
| differs | Recorded; MarkupCraft differs (the difference is written down) |
| unrecorded | Built from documentation only; not yet compared with Revu |

Already proven (from Revu-made files, no live session needed):

| What | Evidence |
|---|---|
| Length / Area quantities and label text | 410/410 measurements on a 121-sheet Revu-marked set (`markupcraft-cli check`) |
| Page scale storage (`/VP` viewport, media-box-relative BBox) | all 40 viewports in the same set |
| Re-saving Revu markups (geometry, text, colors, style) | 187/187 non-measurement markups, 410/410 measurements (`resave`, `markupcheck`) |
| Appearance of re-saved markups | PDFium render of 32 marked pages vs Revu's own appearance streams |
| Measurement caption offset `/CO` on areas | 37 captions in the reference set |

## Blind acceptance testing

Builders cannot audit their own work: a test written by the person who built a feature tends to
test what was built, not what was asked for. So every in-scope row of the parity table also has an
**acceptance test**, written in a separate pass:

1. **Blind.** The tester reads only the inventory row (`docs/revu_features/*.md`: Revu's public
   help, in our words) and writes down what a Revu user would expect: the command, its
   defaults, the numbers it should produce. Expected quantities are worked out by hand from the
   scale rules (72 pt = 1 paper inch), not copied from MarkupCraft's output.
2. **Through the front doors.** Tests drive the product the way a user or an agent would: the
   automation tool table (`Automation::call`, the same table as `markupcraft-cli run` and the
   MCP server) and the real interface, headlessly (`egui_kittest`: menus, keys, clicks and drags
   on the canvas, dialogs, panels).
3. **Synthetic inputs only.** Tests build their documents in code (the synthetic sample plan,
   blank pages from `doc_new`) and write only to a fresh temporary folder, so they run on any
   machine and never touch a project drawing.
4. **A verdict per row.** The row records its tests and the outcome:

   ```toml
   accepted = ["calibrate_derives_the_page_scale_from_a_known_length"]
   acceptance = "pass"     # or "fixed", or "gap"
   ```

   | Verdict | Meaning |
   |---|---|
   | `pass` | the feature behaved as the inventory describes the first time |
   | `fixed` | the test found a bug; it was fixed, with a regression test, and the row's notes say what changed ("Acceptance fix: ...") |
   | `gap` | the behavior still differs from the inventory; the row's notes say how ("Acceptance: ...") |

`cargo xtask parity --check` rejects an `accepted` test name that does not exist and any verdict
other than these three. [FEATURES.md](../FEATURES.md) shows the verdict on every row and the
pass / fixed / gap totals per area.

| File in `crates/acceptance/` | Covers |
|---|---|
| `src/lib.rs` | shared helpers: a tool table in a temporary folder, the sample plan, a headless app |
| `tests/measurement.rs` | scales, viewports, every measurement tool, Dynamic Fill, snapping, Spaces, legends, reports |
| `tests/markups.rs` | every markup tool, Properties, Tool Chest, layers, Markups List |
| `tests/documents_a.rs`, `tests/documents_b.rs` | opening, tabs, pages, bookmarks, combining; batch, summary, print, stamps, OCR, forms, signatures, security, redaction, flatten, export |
| `tests/review.rs` | compare, overlay, search, visual search, Spaces, links, signatures |
| `tests/interface.rs` | the interface, preferences, views, the mouse and every default shortcut |
| `tests/real_world.rs` | an estimator's whole takeoff day on a real Revu-marked set, read from `MARKUPCRAFT_REF_PDF` (skipped when unset; see [hardening.md](hardening.md)) |
| `tests/smoke.rs` | the harness itself |

Run them with `cargo test -p markupcraft-acceptance`. Interface tests take turns on one GPU
context, so they are slower than the unit tests.

An acceptance verdict is not a recording. `pass` means MarkupCraft does what Revu's
documentation says; whether Revu itself does exactly that, down to the stored keys, is known
only once a recording exists.

## Guesses still needing a Revu recording

Where Revu's storage or on-screen behavior is neither in its public help nor in any Revu-made
file we have read, MarkupCraft made a choice. Each choice is consistent (MarkupCraft reads what it
writes, and the scorecard confirms it never disturbs Revu's own markups), but it may not be what
Revu writes, so a markup made in MarkupCraft may look or list differently when the file is opened
in Revu. Each needs a recording (see the queue below) before its row can be **proven**.

**Measurement captions**

- **Label-first captions.** A number format with `/O /P` (ISO 32000: the unit label before the
  number) is read and written, and our caption then puts the label first. Whether Revu's
  caption follows `/O` is not recorded.
- **Perimeter and Polylength captions on the last segment.** Their value is drawn along the last
  segment (below it without Show Segment Values, beside the running total with them). Area and
  Volume center at the vertex mean, which *is* proven (`/CO` on 37 Area captions). Asking an Area
  or Volume for its caption on the last segment is stored as `/PCCaptionLastSeg`.
- **Moved captions of Polylength and Count** are stored as `/PCCaptionOffset`; Revu's `/CO` is
  proven only for Area, Perimeter and Length. Caption templates (`/PCCaption`), caption leaders
  (`/PCCapLeader`) and the centroid mark (`/PCCentroid`) are ours too.

**Measurement storage**

| Markup | What MarkupCraft writes | Status |
|---|---|---|
| Perimeter | `/Polygon` `/IT /PolygonDimension`, `/MeasurementTypes 130` | guess (Area's layout with the length code) |
| Count | `/PolyLine` `/IT /PolyLineCount`, code 128; symbol in `/PCCountSymbol`, `/PCCountShape`, `/PCSymbolScale`, item sizes in `/PCCountDims` | guess |
| Volume | `/Polygon` `/IT /PolygonDimension`, code 132, depth in `/PCDepth` beside Revu's `/DepthUnit` | layout seen, depth key ours |
| Diameter | `/Circle`, code 384, `/Vertices` = the two ends of a diameter | guess |
| Radius | `/Circle` `/IT /CircleRadius`, code 384, `/Vertices` = [center, a point on the circle] | guess |
| Angle | `/PolyLine` `/IT /PolyLineAngle`, code 1152, `/Vertices` = [arm end, vertex, arm end] | guess |
| Dimension | `/Line` `/IT /PCDimension`, `/LL` the offset, `/LLE` the extension past it | our own intent |
| Arc | `/PolyLine` `/IT /PCArc`, `/Vertices` = [start, a point on the arc, end] | our own intent |
| Arc segments inside a measurement | `/PCArcs` | ours |
| Cutouts (holes in an Area or Volume) | `/PCCutouts` | ours |
| Show Segment Values, Rise/Drop, slope value | `/PCSegmentValues`, `/PCRiseDrop`, `/PCSlope` (beside Revu's `/SlopeType`) | ours |

**Other markups and document data**

| What | What MarkupCraft writes |
|---|---|
| Custom column definitions and values | catalog `/PCColumns`, annotation `/PCColumnData` (Revu's `/BSIColumnData` is undocumented; it is kept untouched when present) |
| Text box vertical alignment, margin, line spacing | `/PCVAlign`, `/PCTextMargin`, `/PCLineSpacing` |
| Text stamps, hatch patterns, legends | `/PCStamp`, `/PCHatch`, `/PCLegend` |
| Spaces | page `/PCSpaces` |
| Tool sets | MarkupCraft's own `.mctools` JSON (Revu's `.btx` has no public specification) |
| Markup export | XFDF and FDF (Revu's BAX is not written) |

Review status and checkmarks (`/Text` replies with `/StateModel`) and groups (`/IRT` with
`/RT /Group`) follow ISO 32000-1; whether Revu stores them the same way is items 8 and 9 of the
queue.

## Recording a feature in Revu

One recording = one small, scripted action in Revu on a copy of a test sheet, captured as:

1. **The saved PDF** after the action (the file format: keys, values, appearance).
2. **Screenshots** before / during / after (on-screen behavior: preview, snapping, captions, highlights).
3. **UI facts**: the exact right-click menu items, dialog fields and defaults, Properties panel rows,
   Markups List columns and values, status bar text.
4. **A one-line script** of what was done, so MarkupCraft's test repeats the same steps.

Recordings live outside the repo (they contain Revu output and may contain project drawings);
the tests use extracted facts (key/value tables, coordinates, expected strings) checked into
`tests/revu_recordings/` as small JSON files with no drawing content.

## Recording queue (daily-use first)

| # | Feature | Script in Revu | Compare |
|---|---|---|---|
| 1 | Count markup format | Count tool, 3 clicks, Esc, save | annotation dict vs ours |
| 2 | Perimeter format | Perimeter tool, 4 points, save | dict + quantity + label |
| 3 | Polylength format | Polylength 4 points, save | dict + label |
| 4 | Area with cutout | Area, then Polygon Cutout inside, save | how holes are stored |
| 5 | Caption Shift-drag | Shift-drag an area's caption, save | `/CO` value vs our offset for the same drag |
| 6 | Cloud+ | Cloud+ around a region, type text, save | grouped structure |
| 7 | Typewriter, Note, Text Highlight, Strikethrough, Caret | one of each, save | intents, keys |
| 8 | Custom column value + status | add a Number column, set a value, set status Accepted | `/BSIColumnData` and status storage |
| 9 | Group | group two markups, save | group storage |
| 10 | Default colors / widths / fonts | draw each tool with defaults | default property values |
| 11 | Right-click menus | right-click each markup kind | menu item list and order |
| 12 | Markups List columns | default column set and order | column list |
| 13 | Markup summary CSV | Summary > CSV on the all-kinds test sheet | row-by-row diff with our CSV |
| ... | every remaining inventory row | | |

The reverse check (MarkupCraft writes, Revu reads) uses `revu_check/markupcraft_all_kinds.pdf`:
open it in Revu, export the Markups List summary CSV, and diff it with `markupcraft-cli list --csv`.

## Rules

- Never use a project's live file for a recording: always a copy, in a scratch folder.
- Never close, save or change a document the owner has open in Revu.
- A test fixture records facts, not copied drawings or Bluebeam content.
