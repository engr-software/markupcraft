# Hardening: real-world takeoff, performance, fuzzing

Three tools keep MarkupCraft honest on real drawing sets:

| What | How to run | Where |
|---|---|---|
| End-to-end takeoff on a real Revu-marked set | `MARKUPCRAFT_REF_PDF=<set.pdf> cargo test -p markupcraft-acceptance --test real_world` (skips when unset) | `crates/acceptance/tests/real_world.rs` |
| Performance bench | `cargo xtask bench [--synthetic PAGES] [--markups N] [--no-ref]` | `xtask/src/bench.rs`, `crates/ui-egui/examples/bench.rs` |
| Mutation fuzzer | `cargo xtask fuzz --time SECS [--jobs N] [--timeout S] [--debug]`, local seeds from `MARKUPCRAFT_FUZZ_CORPUS` | `xtask/src/fuzz.rs`, `crates/ui-egui/examples/fuzz_one.rs` |

The reference set and any corpus stay local: nothing from them (names, paths, text, findings
derived from them) is committed. Every bug they find becomes a small synthetic regression test.

## 1. End-to-end takeoff on the reference set

`estimator_workflow_on_the_reference_set` drives an estimator's day through the automation
tools (the same table as the CLI and MCP) and the real interface (headless): open the
121-sheet set, read text across it, set 1/8" = 1'-0" on three empty sheets and check the page
scales, take off an area with a cutout, a length, a polylength and a count on each (175 sf,
10 ft, 20 ft, 4 ea), add a length on a sheet Revu scaled (it takes that sheet's scale and
unit), add a Unit Cost column and a `Measurement * [Unit Cost]` formula total, export the
summary grouped by subject as CSV (the grand total is checked) and XLSX, save incrementally and
in full, reopen both files and check every quantity and custom value survived and that every
one of Revu's own markups is exactly as it was (all Markups List fields, and every measurement
still measures), compare two copies (no false changes), flatten one sheet of a copy (only that
sheet's markups go), then open the saved set in the interface: walk first / last / next page,
group the Markups List by subject on one sheet (group subtotals and the total shown), and draw
one more area with the Area tool (about 100 sf at the sheet's scale).

Findings and fixes:

| Finding | Fix | Regression test (synthetic) |
|---|---|---|
| Compare Documents counted the **text of markups** (measurement labels, notes) as text changes: two copies of the same sheet, one with a takeoff, showed "removed" text although markups are left out of comparisons by default. | The text layer of a comparison follows `include_markups` (`TextExtractor::with_markups`). | `compare::tests::markup_text_is_not_a_text_change_unless_markups_are_included` |
| Compare, batch compare and export re-parsed the **whole PDF for every page's text** (`Renderable::text` built a new extractor per call). | `Renderable` keeps one extractor and its per-page cache. | covered by the compare and export tests |

Everything else in the flow passed first time: scales, quantities, cutouts, formulas, totals,
CSV / XLSX, incremental and full saves, reopen, flatten, the interface. The scorecard stayed at
check 410/410, resave 410/410, markupcheck 187/187.

## 2. Performance

`cargo xtask bench` runs the release `bench` example once on a large synthetic set built in
memory (120 sheets of 36 x 24 in linework and text, 600 measurements and markups: runs in CI)
and once on the reference set when `MARKUPCRAFT_REF_PDF` is set, and prints one table
(milliseconds, wall time; also written to `target/bench/last.tsv`). Open is the app's own path
(parse, model, renderer and its hidden-annotation copy); renders use the app's pool (6 threads
here); the Markups List rows are what the panel builds per frame; the UI frame rows step the
real interface headlessly; search is the whole document.

Windows 10, 12 logical cores, release build. Before = the code before the two optimizations
below, after = with them; the two binaries ran interleaved, three times each, and the table
shows the medians (other builds were running on the machine, so rows that did not change move
by up to about 20% between runs). The Markups List is shown in the default layout, so the
default-layout frame pays for it too.

| measurement (ms, median of 3) | synthetic before | synthetic after | reference before | reference after |
|---|---:|---:|---:|---:|
| open | 93 | 138 | 332 | 200 |
| first page render (fit width, 1600 px) | 34 | 31 | 222 | 153 |
| render every page (fit width) | 783 | 812 | 7,304 | 6,280 |
| thumbnails (200 px, every page) | 211 | 247 | 6,115 | 5,578 |
| Markups List build (one full table) | 5.7 | 6.4 | 5.6 | 5.5 |
| **UI frame, default layout** | **12.1** | **3.3** | **26.5** | **2.3** |
| **UI frame, Markups List grouped by subject** | **13.0** | **2.7** | **26.1** | **4.9** |
| **search all pages (first)** | **1,116** | **1,197** | **5,294** | **1,528** |
| **search again** | **990** | **26** | **4,520** | **33** |
| **search after a markup edit** | **801** | **27** | **3,932** | **34** |
| full save | 86 | 70 | 253 | 273 |
| incremental save | 11 | 9 | 112 | 110 |
| reopen (model only) | 48 | 44 | 88 | 87 |
| peak memory (MB) | 324 | 324 | 1,235 | 1,272 |

The slowest paths and what changed:

- **Search** read every page's text again on every search, on one thread, and after any markup
  edit it first wrote the whole document out (a 100 MB rewrite) although search leaves markups
  out. Now page text is read once on worker threads and cached per page for the bytes it came
  from (capped at 4 M glyphs); while only markups changed, search reads the file's own bytes,
  so a takeoff in progress keeps the cache. A content change (watermark, header, text edit,
  page operation) reads it again. Test: `search::tests::search_text_is_cached_until_the_page_content_changes`.
- **Markups List** computed every cell of every markup twice per frame (once for the toolbar's
  column names, once for the table). Now one table per frame, and its cells are kept across
  frames until the document changes (keyed by the session's state version and saved state;
  edits, undo and save all change it). Test:
  `panels::markups_list::tests::cells_are_kept_until_the_document_changes`.
- Rendering and thumbnails are the renderer's (PdfCraft / hayro) time: the reference set's
  slowest sheet takes 2.5 to 6 s at fit width and the average sheet about 50 ms; the app renders
  on a background pool and in tiles, so it never blocks the interface. Left as is. The first
  search of a large set (1.5 s here) is text extraction on worker threads; the synthetic set's
  first search is dominated by its 10,000 hits, not by reading text.

## 3. Fuzzing

`cargo xtask fuzz` mutates PDFs (byte flips, PDF syntax, splices, extreme numbers, corrupt
streams, and tokens aimed at the keys MarkupCraft and Revu read on markups: `/Measure`, `/IT`,
`/Vertices`, `/PCCutouts`, `/PCColumns`, `/RC`, `/DS`, ...) and runs each in a child process
(`fuzz_one`) with a timeout: open as the app does (model, Markups, renderer), render every page
small, read text and search, build the Markups List grouped two ways and export it, add a
measurement, move and delete markups, save in full and incrementally, reopen both and list
again. A panic, abort or stack overflow is a crash; running past the timeout is a hang. Crashes
are minimized and kept in `fuzz-out/findings/` (git-ignored).

Seeds: the sample plan, a measured set saved by MarkupCraft (areas with cutouts, lengths,
counts, polylengths, clouds, text), a plain page, plus (locally) single sheets extracted from
the reference set via `MARKUPCRAFT_FUZZ_CORPUS`.

### Round 1: 25 minutes, 24,465 runs (16 per second, 5 jobs, dev profile with overflow checks)

| Finding | Where | Fix | Regression test (synthetic) |
|---|---|---|---|
| Crash: an object numbered 4294967295 made the object layer's next free object number overflow while opening (a panic in debug builds; in release it wrapped to 0, so the next markup would have been written as object 0) | PdfCraft object layer, reached from `markupcraft_revu::open_bytes` | `markupcraft_revu::open_cos`: refuse a file whose object numbers or trailer `/Size` reach 2^30 (ISO 32000 allows 8,388,607 objects), and turn a panic in the object layer into an error | `revu/tests/hostile.rs`: `an_object_numbered_u32_max_is_refused_not_a_crash` |
| Crash: a trailer `/Size 99999999999999999999` opened, then the first new object of a save took number 4294967295 and the next overflowed | same | same | `an_absurd_trailer_size_is_refused_not_saved_broken` |
| Crash: a damaged `/Measure` made a quantity so large that formatting its label overflowed an `i64` | `markupcraft-measure` `format_last` | values from 1e15 up (no fractional digits left) are shown as whole numbers, capped | `measure` `huge_values_format_without_overflow` |
| Hang (3 inputs): a dashed line 4 billion points long makes the renderer stroke hundreds of millions of dashes | PdfCraft renderer (hayro) | The app renders on a worker pool whose watchdog already gives up on a page after 20 s. MarkupCraft's own synchronous renders (`Renderable`: compare, overlay, export to images, OCR, visual search) rendered inline and never returned; they now render on one worker with the same watchdog and a deadline, so the feature gets an error. The renderer itself is left to PdfCraft. | `raster::tests::a_page_that_never_finishes_rendering_is_an_error_not_a_hang` |

### Round 2 (after the fixes): 25 minutes, 36,311 runs, another seed

No crashes and no hangs. With renders on workers (as the app does), `fuzz_one` only reports a
hang when MarkupCraft's own steps (open, list, edit, save, reopen) run past the timeout.

Together: 50 minutes, 60,776 runs, 4 distinct bugs found (one of them three hanging inputs),
all fixed with synthetic regression tests. The minimized inputs stay in the local, git-ignored
`fuzz-out/`.
