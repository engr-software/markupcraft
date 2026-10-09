# Equivalence with Revu: how behavior is proven

MarkupCraft's goal is identical behavior to Revu 21 for the features in
[revu_features/](revu_features/). "Matches the help pages" is not proof. A feature
counts as **proven** only when a recording of real Revu and MarkupCraft agree.

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
