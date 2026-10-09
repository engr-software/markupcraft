# Revu 21 feature inventory, part 2: Markup tools, Properties, Tool Chest, Layers, Markups List

Clean-room inventory written from public documentation only. Behaviour is described in our own words.
No Revu binaries, decompiled material or running Revu were used.

**Source keys**

| Key | Source |
|---|---|
| KS | Installed `Help\KeyboardShortcuts.pdf` (Revu 21, English). Its two-column layout extracts misaligned; shortcuts were re-paired by list order, so a few (marked `?`) need a check. |
| MLP | support.bluebeam.com/user-manual/menus/window/markups-list.html |
| CC | support.bluebeam.com/user-manual/menus/window/custom-columns.html |
| MLS | support.bluebeam.com/user-manual/menus/window/markups-list-summary.html |
| CCT | support.bluebeam.com/revu/how-to/tips-and-tricks/calculate-costs-with-custom-columns-in-markups-list.html |
| TCP | support.bluebeam.com/user-manual/menus/window/tool-chest-panel.html |
| TCG | support.bluebeam.com/revu/features/tool-chest-guide.html and .../use-tool-chest.html |
| LP | support.bluebeam.com/user-manual/menus/window/layers-panel.html and .../pdf-layers.html |
| CM | support.bluebeam.com/user-manual/tutorials/customize-markups.html |
| LS | support.bluebeam.com/user-manual/tutorials/custom-line-style-set.html |
| ST | support.bluebeam.com/user-manual/menus/tools/stamp-tool.html, .../edit-stamps.html, tutorials/create-stamps.html |
| CCP | support.bluebeam.com/user-manual/menus/edit/cut-copy-paste.html |
| UM | Other support.bluebeam.com user-manual tool pages (menus/tools/*) found by search, not fetched in full; treat the detail as "verify against the page before implementing" |

**Priority** is for a construction estimator / PM doing takeoff and drawing review: P0 daily, P1 weekly, P2 occasional, P3 rare.
**MarkupCraft** status is against the 2026-10-07 build: open/view, Revu markup read (line/polygon/polyline drawn natively, rest by PDFium),
Length/Polylength/Area/Perimeter/Count/Calibrate, select/move/vertex edit/delete/undo/redo, color dialog, Markups List grouped by subject
with totals, editable label/subject, measurement filter, CSV export, Revu-compatible save.

---

## A. Text and comment markups

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-001 | Text Box | Drag a rectangle and type rich text inside a bordered/filled box (PDF FreeText). Box can grow with text or stay fixed. | Tools > Text > Text Box; Markup toolbar | T | P0 | have (T, in-place editing) | KS, UM |
| K-002 | Autosize Text Box | Shrinks/grows the selected text box frame to fit its text exactly. | Right-click text box; Markup menu | Alt+Z | P1 | partial (box grows with text) | KS |
| K-003 | Typewriter | Click and type plain text with no border or fill, like filling in a form by hand. | Tools > Text > Typewriter | W | P1 | have (W) | KS, UM |
| K-004 | Note | Drops a sticky-note icon whose comment opens in a pop-up; icon style selectable (comment, help, key, etc.). | Tools > Text > Note | N | P1 | have (N; 7 icons from the right-click menu; note window opens on placing and on double-click; /Text + /Popup) | KS, UM |
| K-005 | Callout | Text box with a leader line and arrowhead pointing at a spot; leader knee and box move independently. | Tools > Text > Callout | Q | P0 | have (Q) | KS, UM |
| K-006 | Rich text editing in text markups | Bold/italic/underline/strikethrough, font, size, color, alignment per character run while editing a text box/callout. | In-place edit + Properties | Ctrl+B/I/U while editing | P1 | partial (whole-box bold/italic/underline; no mixed runs) | UM |
| K-007 | Spell check | Checks spelling of text in markups (and red-underlines while typing). | Tools > Check Spelling | F7 | P2 | have (squiggles + right-click suggestions in the text editor, Check Spelling dialog over text boxes / callouts / typewriter / notes, user dictionary, Ignore All, Ignore UPPERCASE; Hunspell en_US, optional at build) | KS |
| K-008 | Review Text (text-edit view) | Opens a list view to review/edit the text of markups in sequence. | Markup menu | Shift+Alt+R | P3 | missing | KS |
| K-009 | Text Highlight | Select document text and highlight it (text-based highlight annotation). | Tools > Text markup; Highlight tool over text | H (Highlight) | P2 | have (Highlight Text tool; or Select Text (Shift+T) then H. H alone stays the freehand highlighter) | KS, UM |
| K-010 | Underline text | Underline selected PDF text. | Document/Tools > Text markup | U | P2 | have (U) | KS |
| K-011 | Strikethrough text | Strike through selected PDF text. | Document/Tools > Text markup | D | P2 | have (D) | KS |
| K-012 | Squiggly text | Wavy underline on selected PDF text. | Document/Tools > Text markup | Shift+U | P3 | have (Shift+U) | KS |
| K-013 | Insert/Replace text markup (caret) | Marks a text insertion or replacement point for document review. | Tools > Text markup | - | P3 | have (click = caret, drag = strikethrough + caret; Acrobat-style /Caret, Revu's own format unchecked) | UM |
| K-014 | Sequences / incrementing text | Tool Chest items whose text auto-increments (A1, A2...) each time they are placed, for tags and keynotes. | Tool Chest > Sequences & Actions set | - | P1 | missing | TCP |
| K-015 | Text font properties | Font family, size, color, bold/italic/underline/strike, text alignment (L/C/R/justify), vertical alignment. | Properties > Appearance/Font | - | P0 | have (font, size, B/I/U, align, color) | CM |
| K-016 | Text box margins / line spacing | Inner padding between frame and text; line spacing for multi-line text. | Properties (text box) | - | P2 | missing | UM |
| K-017 | Callout leader/leader end | Callout leader line style and arrowhead type; adding a knee point. | Properties (callout) | - | P1 | have (leader + line end) | UM |

## B. Line and shape markups

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-018 | Line | Straight segment between two clicks (non-measuring). Shift snaps to 0/45/90 degrees. | Tools > Shapes > Line | L (Revu) | P0 | have (L) | KS |
| K-019 | Arrow | Line with an arrowhead at the end. | Tools > Shapes > Arrow | A | P0 | have (A) | KS |
| K-020 | Dimension | Line with arrowheads at both ends and extension/leader styling, used to annotate dimensions (not measured). | Tools > Shapes > Dimension | Shift+L | P2 | missing | KS |
| K-021 | Polyline | Open multi-segment line; finish by double-click, Enter or right-click. | Tools > Shapes > Polyline | Shift+N | P1 | have (Shift+N) | KS |
| K-022 | Polygon | Closed multi-vertex shape with optional fill. | Tools > Shapes > Polygon | Shift+P | P1 | have (Shift+P) | KS |
| K-023 | Rectangle | Drag a rectangle; Shift makes a square. Optional fill and rounded corners via line style. | Tools > Shapes > Rectangle | R | P0 | have (R) | KS |
| K-024 | Ellipse | Drag an ellipse; Shift makes a circle. | Tools > Shapes > Ellipse | E | P1 | have (E) | KS |
| K-025 | Arc | Three-point arc drawn as a curved line. | Tools > Shapes > Arc | Shift+C | P2 | missing | KS, UM |
| K-026 | Cloud | Polygon drawn with a scalloped revision-cloud border. | Tools > Shapes > Cloud | C | P0 | have (C, intensity) | KS |
| K-027 | Cloud+ | A cloud with an attached callout/leader and text box, in one markup; standard revision-callout markup. | Tools > Shapes > Cloud+ | K | P0 | partial (K: cloud + callout, not grouped) | KS |
| K-028 | Rectangle/polygon cloud modes | Clouds can be drawn as rectangle (drag) or polygon (click vertices) shapes. | Cloud tool options | - | P1 | have (Cloud tool: drag = rectangle, click points = polygon) | UM |
| K-029 | Pen (freehand ink) | Freehand ink stroke; Shift constrains to horizontal/vertical. | Tools > Pen | P | P1 | have (P) | KS |
| K-030 | Highlighter (freehand) | Wide translucent freehand stroke, multiply-blended so text stays readable. | Tools > Highlight | H | P1 | have (H, multiply) | KS |
| K-031 | Eraser | Erases parts of Pen/Highlight ink strokes by dragging over them. | Tools > Eraser | Shift+E | P2 | missing | KS |
| K-032 | Shift-constrain while drawing | Holding Shift locks line/arrow/polyline/polygon/measure segments to 0/45/90 deg; Pen/Highlight to H/V. | Modifier key | Shift | P0 | have | KS |
| K-033 | Spacebar pan while drawing | Hold Space to pan without cancelling the markup being drawn. | Modifier key | Space | P1 | have | KS |
| K-034 | Shape fill hatch patterns | Shapes can use a hatch pattern instead of solid fill; hatch sets are editable. | Properties > Fill/Hatch | - | P2 | missing | CM, UM |
| K-035 | Flag | Places a small flag/marker icon on the page. | Markup toolbar | Ctrl+Alt+V? | P3 | missing | KS (pairing uncertain) |

## C. Stamps, images, links, attachments, capture

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-036 | Stamp tool | Place a predefined stamp (Approved, Reviewed, Void, etc.) from a stamp list; scale by dragging. | Tools > Stamp | S | P1 | partial (S: built-in text stamps) | KS, ST |
| K-037 | Stamp library management | Change stamp folder, import stamps from PDF, edit, delete, and manage the stamp list. | Tools > Stamp > menu | - | P2 | missing | ST |
| K-038 | Create Stamp | Build a stamp from a template (Blank, Text, Text with Border, Text with Date and Border) with subject name. | Tools > Stamp > Create Stamp | - | P2 | missing | ST |
| K-039 | Dynamic stamp text | Stamp text fields that fill at placement: current user, date/time, file name, file name w/o extension, full path, etc. | Stamp editor > Dynamic dropdown | - | P1 | missing | ST |
| K-040 | Interactive stamps | Stamps containing form-like fields (checkboxes, text) the reviewer fills in after placement. | Stamp editor | - | P2 | missing | ST (add-and-edit-interactive-stamps) |
| K-041 | Stamp from image/PDF | Use an image or PDF page as a stamp (e.g. a signature or company logo). | Create Stamp / Import | - | P2 | missing | ST |
| K-042 | Image markup | Insert a raster image file as a resizable markup. | Tools > Image | I | P2 | missing | KS |
| K-043 | Image From Scanner | Insert an image acquired from a scanner. | Tools > Image > From Scanner | Shift+I | P3 | missing | KS |
| K-044 | Snapshot | Copy a rectangular region of the page (vector content) to the clipboard to paste as a markup elsewhere. | Edit > Snapshot | G | P1 | have (G: picture + vector snapshot on the clipboard; Ctrl+V pastes a /StampSnapshot shaped like Revu's) | KS |
| K-045 | Copy Page to Snapshot | Copy the whole current page as a snapshot. | Edit menu | Ctrl+Alt+C | P3 | have | KS |
| K-046 | Snapshot Content (cut/copy region of page) | Captures page content in a region for paste/reuse, keeping vector form. | Document menu | Shift+G | P2 | partial (Shift+G copies and places the snapshot where it was taken; no cut) | KS |
| K-047 | Hyperlink | Region or markup that jumps to a page, view, file, URL or snapshot; also an Action on any markup. | Tools > Hyperlink | Shift+H | P1 | missing | KS, UM |
| K-048 | Edit Action | Attach/edit the action (jump to page, open URL, open file, etc.) fired when a markup is clicked. | Markup right-click > Edit Action | Ctrl+Shift+E | P2 | missing | KS |
| K-049 | File Attachment | Embed a file in the PDF, shown as a paperclip/pin icon on the page. | Tools > File Attachment | F | P2 | missing | KS |
| K-050 | Capture (camera) | Attach photos, video or audio captured from the device camera/mic to a markup. | Tools > Capture | Ctrl+Alt+I? | P2 | missing | KS, MLP |
| K-051 | Capture Summary / Export Capture Media | Report of all captured media; save embedded images/videos to a folder. | Markups List menu | - | P3 | missing | MLP |
| K-052 | Symbols (Tool Chest items as symbols) | Reusable graphic symbols (doors, outlets, valves) stored in tool sets and placed by click. | Tool Chest | - | P0 | have (Tool Chest drawing mode, scaled) | TCP |
| K-053 | Sketch tools | Drafting aids (sketch-to-scale, drawing by typed lengths/angles) to construct shapes precisely. | Tools > Sketch (customizable shortcuts) | - | P2 | missing | KS (custom shortcuts note), UM |
| K-054 | Legend | Generated table that lists markups by subject/type with counts and totals, placed on the page and updated live. | Tool set Properties > Legend; Markups List right-click > Legend | - | P1 | missing | TCP, MLP |
| K-055 | Spaces (markup-related) | Named regions (rooms, zones) that markups inside inherit as a "Space" column for grouping. | Spaces panel | - | P2 | missing | MLP (Space column) |
| K-056 | Dynamic Fill | Paint-bucket style fill that finds an enclosed region bounded by linework to create an area. | Measure > Dynamic Fill | J | P1 | missing | KS |

## D. Properties panel

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-057 | Properties panel (markup context) | Shows editable properties of the selected markup(s); with nothing selected shows document metadata. | Window > Panels > Properties | Alt+P | P0 | have (Properties panel) | KS, CM |
| K-058 | Multi-select property editing | Changes made with several markups selected apply to all; mixed values shown blank. | Properties panel | - | P0 | have (wave 2: mixed values shown blank / "Mixed") | CM |
| K-059 | General: Subject | The category/name of the markup, used for grouping and totals in the list. | Properties > General | - | P0 | partial (editable in list only) | CM |
| K-060 | General: Author | User who created the markup; set from the identity in preferences. | Properties > General | - | P1 | missing | CM |
| K-061 | General: Date / modified | Creation and modified timestamps (read only). | Properties > General | - | P2 | missing | CM |
| K-062 | General: Label | Short display label (for measurements shown on the caption). | Properties > General | - | P0 | partial (editable in list) | CM |
| K-063 | General: Comments | Free-text comment body attached to the markup; shown in list and pop-up. | Properties > General | - | P0 | missing | CM |
| K-064 | General: Reply count / status | Shows number of replies and the review status. | Properties > General | - | P2 | missing | CM |
| K-065 | Appearance: Color (stroke) | Line/border color with palette and custom picker. | Properties > Appearance | - | P0 | partial (color dialog, no panel) | CM |
| K-066 | Appearance: Fill color | Interior fill color for closed shapes, none allowed. | Properties > Appearance | - | P0 | missing | CM |
| K-067 | Appearance: Opacity | Overall opacity percentage of the markup (separate stroke/fill opacity on some types). | Properties > Appearance | - | P0 | missing | CM |
| K-068 | Appearance: Fill opacity | Separate transparency for the fill only, so lines stay solid over a see-through fill. | Properties > Appearance | - | P0 | missing | UM |
| K-069 | Appearance: Line width | Stroke thickness in points. | Properties > Appearance | - | P0 | missing | CM |
| K-070 | Appearance: Line style | Solid, dashed, dotted, and custom styles from line style sets (including symbol/text line types). | Properties > Appearance | - | P1 | missing | CM, LS |
| K-071 | Custom line style editor / line style sets | Define new dash/symbol line styles grouped in sets; share sets. | Line style dropdown > Manage | - | P2 | missing | LS |
| K-072 | Appearance: Line start / end | Line ending style at each end: none, open/closed arrow, circle, square, diamond, slash, etc. | Properties > Appearance (lines, arrows, polylines, callouts) | - | P1 | missing | CM |
| K-073 | Appearance: Cloud style / intensity | Turns border into cloud scallops and sets scallop size/intensity. | Properties > Appearance (cloud, cloud+, shapes) | - | P0 | missing | UM |
| K-074 | Appearance: Blend mode | Normal vs Multiply so fills/highlights do not hide drawing content. | Properties > Appearance | - | P1 | missing | UM |
| K-075 | Appearance: Hatch | Hatch pattern, scale and color as fill. | Properties > Appearance | - | P2 | missing | UM |
| K-076 | Appearance: Font | Font family/size/style/color for text-bearing markups. | Properties > Appearance | - | P0 | missing | CM |
| K-077 | Appearance: Icon (note/attachment) | Choose the icon shape for Note and File Attachment markups. | Properties > Appearance | - | P3 | missing | UM |
| K-078 | Layout: X, Y | Exact position of the markup on the page, editable numerically. | Properties > Layout | - | P2 | missing | CM |
| K-079 | Layout: Width, Height | Exact size, editable numerically. | Properties > Layout | - | P2 | missing | CM |
| K-080 | Layout: Rotation | Rotation angle up to 360 degrees, editable numerically. | Properties > Layout | - | P2 | missing | CM |
| K-081 | Interactive rotation handle | Top handle rotates markup; snaps to 15 deg, Shift for 1 deg steps. | Selected markup handles | Shift | P2 | missing | KS |
| K-082 | Layer property | Assigns the markup to a PDF layer (OCG); flattening lands on that layer. | Properties > General | - | P1 | missing | MLP |
| K-084 | Options: Set as Default | Saves this markup's appearance as the default for new markups of the same type. | Properties > Options; right-click | - | P0 | have | CM |
| K-085 | Options: Add to Tool Chest | Saves this markup (with properties) as a reusable tool in a tool set. | Properties > Options; right-click | - | P0 | have | CM |
| K-083 | Lock property | Locks the markup so it cannot be moved, edited or deleted until unlocked. | Properties / right-click > Lock | Ctrl+Shift+L | P1 | have (wave 2: /F Locked bit; Properties checkbox, Ctrl+Shift+L, right-click) | KS, MLP |
| K-086 | Measurement properties | Scale, units, precision, caption display, depth/height, slope, etc. for measure markups. | Properties > Measurement | - | P0 | partial (scale/calibrate, no panel) | MLP (Depth, Slope cols) |
| K-087 | Caption move (measurement) | Shift-click a measurement caption to move it separately from the shape. | Modifier on caption | Shift+drag | P1 | have (Shift+drag; see M-088) | KS |
| K-088 | Format Painter | Copy appearance from one markup and click others to apply it. | Edit > Format Painter | Ctrl+Shift+C | P0 | have (wave 2: look = colour, fill, opacities, line width) | KS |
| K-089 | Flip Horizontal / Vertical | Mirror the selected markup. | Markup menu / right-click | Ctrl+Alt+H / Ctrl+Alt+V | P3 | have (wave 2) | KS |
| K-090 | Hidden / no-view / no-print flags | Markups can be hidden, printable or non-printable per PDF annotation flags. | Properties / list | - | P2 | missing | MLP |

## E. Selection, arrangement and clipboard

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-091 | Select tool | Click/drag to select markups; Shift adds. | Tools > Select | V | P0 | have | KS |
| K-092 | Marquee multi-select | Drag a box to select all markups inside; right-drag also multi-selects. | Select tool / right button drag | Shift+drag | P0 | have (wave 2: left-drag on empty page; right-drag not yet) | KS |
| K-093 | Lasso select | Freeform loop selection. | Tools > Lasso | Shift+O | P2 | missing | KS |
| K-094 | Select All | Select all markups on the page. | Edit > Select All | Ctrl+A | P0 | have (wave 2) | KS |
| K-095 | Cut / Copy / Paste | Clipboard operations on markups, within and across documents. | Edit menu, right-click | Ctrl+X/C/V | P0 | have (wave 2: MarkupCraft clipboard format + text fallback) | KS, CCP |
| K-096 | Paste in Place | Paste at the exact same page coordinates as the source (e.g. onto another sheet). | Edit > Paste in Place | Ctrl+Shift+V | P0 | have (wave 2) | KS, CCP |
| K-097 | Apply to All Pages / Paste to pages | Copies the markup to the same location on many pages; dialog offers all, range, odd/even, portrait/landscape. | Right-click > Apply to All Pages | - | P1 | have (wave 2: Paste to Pages / Apply to Pages; all, range, odd/even; no portrait/landscape filter) | CCP |
| K-098 | Ctrl+Shift drag-copy | Ctrl+Shift+drag copies a markup and constrains the move to a straight line. | Modifier on select | Ctrl+Shift+drag | P1 | have (wave 2) | KS |
| K-099 | Delete | Delete selected markups. | Edit / right-click | Del | P0 | have (locked markups are kept) | KS |
| K-100 | Undo / Redo | Undo/redo markup edits. | Edit menu | Ctrl+Z / Ctrl+Y | P0 | have | KS |
| K-101 | Nudge with arrow keys | Move selected markups by small steps with arrow keys. | Keyboard | Arrows | P1 | have (wave 2: 1 screen px, Shift = 10) | UM |
| K-102 | Group | Bind several markups so they select/move as one. | Markup > Group; right-click | Ctrl+G | P1 | have (Ctrl+G, Edit > Group, right-click; one click selects and moves the group; saved as /IRT + /RT /Group; wave 20) | KS |
| K-103 | Ungroup | Break a group into individual markups. | Markup > Ungroup | Ctrl+Shift+G | P1 | have (Ctrl+Shift+G; Remove From Group Ctrl+Shift+Alt+G; wave 20) | KS |
| K-104 | Remove From Group | Pull one markup out of a group without ungrouping the rest. | Right-click | Ctrl+Shift+Alt+G | P3 | have (Ctrl+Shift+Alt+G, Edit > Group, right-click; wave 20) | KS |
| K-105 | Align Left/Center/Right | Align selected markups horizontally to the reference markup. | Markup > Alignment | Ctrl+Alt+L / Ctrl+Alt+E? / Ctrl+Alt+R | P2 | have (wave 2: reference = last selected) | KS |
| K-106 | Align Top/Middle/Bottom | Align selected markups vertically. | Markup > Alignment | Ctrl+Alt+T / Ctrl+Alt+M / Ctrl+Alt+B | P2 | have (wave 2: reference = last selected) | KS |
| K-107 | Distribute horizontally/vertically | Space three or more markups evenly. | Markup > Alignment | - | P3 | have (wave 2) | UM |
| K-108 | Bring Forward / to Front | Raise markup in z-order one step / to top. | Markup > Arrange | Ctrl+] / Ctrl+Shift+] | P2 | have (wave 2: saved as /Annots order) | KS |
| K-109 | Send Backward / to Back | Lower markup in z-order one step / to bottom. | Markup > Arrange | Ctrl+[ / Ctrl+Shift+[ | P2 | have (wave 2: saved as /Annots order) | KS |
| K-110 | Edit vertices | Drag vertices; add/delete vertices on polylines/polygons via right-click. | Selected markup | - | P0 | partial (move vertices; add/delete verify) | UM |
| K-111 | Snap to Content / Grid / Markup | Cursor snaps to PDF linework endpoints, grid, or existing markup points while drawing. | View > Snap | Ctrl+Shift+F8 / F9 / F7 | P0 | missing | KS |
| K-112 | Grid and rulers | Show a page grid and rulers to guide placement. | View | Ctrl+F5 (grid?), Ctrl+R (rulers?) | P3 | missing | KS (pairing uncertain) |
| K-113 | Right-click markup context menu | Per-markup menu: cut/copy/paste, delete, properties, Set as Default, Add to Tool Chest, lock, layer, status, reply, group, arrange, flip, apply to all pages, edit action, flatten. | Right-click on markup | Shift+F10 (context) | P0 | partial (wave 2: cut/copy/paste, delete, properties, arrange, align, flip, lock, Format Painter, duplicate, apply to pages; Set as Default placeholder; no layer/status/reply/group/flatten) | MLP, CCP |
| K-114 | Hide Markups | Temporarily hides all markups on screen. | Markups List menu | - | P1 | have (View > Hide Markups: view only; Hide / Show selected sets the PDF Hidden flag, saved; Markups List right-click Hide / Show; wave 20) | MLP |

## F. Tool Chest

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-115 | Tool Chest panel | Panel of saved, preconfigured markup tools organized in collapsible tool sets. | Window > Panels > Tool Chest | Alt+X | P0 | have | TCP |
| K-116 | Tool sets | Named collections of tools (e.g. "Concrete takeoff", "Review stamps"); show/hide per set. | Tool Chest | - | P0 | have | TCP |
| K-117 | My Tools | Permanent personal tool set kept across sessions. | Tool Chest | - | P0 | have | TCP, TCG |
| K-118 | Recent Tools | Auto-populated set of recently used markups, cleared on exit; can promote to My Tools. | Tool Chest | - | P1 | have | TCP, TCG |
| K-119 | Recent Tools options | Max number of recents, clear recents, default Properties Mode for recents. | Recent Tools gear menu | - | P3 | missing | TCP |
| K-120 | Properties Mode vs Drawing Mode | Properties mode: tool draws a new markup with saved appearance. Drawing mode: places an exact copy (shape, text, size) by click. | Tool right-click toggle | - | P0 | have | TCG |
| K-121 | Single-click vs double-click (sticky) tool | One click uses the tool once; double-click keeps it active for repeated placement. | Tool Chest item | - | P0 | have | TCG |
| K-122 | Tool item properties / edit | Edit a saved tool's properties, subject, label; reorder or delete items. | Tool right-click | - | P1 | have | TCG |
| K-123 | Update Tool Set Item on Reuse | When enabled, property changes on a placed markup write back to the originating tool. | Tool Chest settings | - | P2 | missing | TCP |
| K-124 | Comment persistence setting | Whether saved comment text carries into new markups: No / except text boxes (default) / all markups. | Tool Chest settings | - | P3 | missing | TCP |
| K-125 | Tool set scale | A tool set can carry a baseline scale so its symbols resize to the drawing scale; set/remove scale, toggle. | Tool set Properties menu | - | P1 | partial | TCP |
| K-126 | Symbol view / Detail view | Show tool set items as icons only or icons with names/details. | Tool set Properties menu | - | P3 | missing | TCP |
| K-127 | Icon size slider | Changes the size of tool icons in the panel. | Tool Chest panel | - | P3 | missing | TCP |
| K-128 | Manage Tool Sets | Dialog to add, remove, import, reorder tool sets and choose which are shown. | Tool Chest menu | - | P1 | partial | TCP |
| K-129 | Import / Export tool set (.btx) | Share a tool set as a .btx file; import a colleague's or vendor's set. | Tool set Properties > Export; Manage Tool Sets > Import | - | P0 | partial | TCP |
| K-130 | Pin tool set to toolbar | Shows a tool set as a toolbar for one-click access. | Tool set Properties > Pin | - | P2 | missing | TCP |
| K-131 | Collapsed set flyout | Collapsed tool sets show a flyout to pick tools without expanding. | Tool Chest | - | P3 | missing | TCP |
| K-132 | Shared tool set lock/checkout | Network-shared tool sets are read-only until checked out for editing. | Tool set header indicator | - | P3 | missing | TCP |
| K-133 | Profiles | Saved workspace configurations (panels, toolbars, tool sets, columns, shortcuts); import/export .bpx. | Revu > Profiles | - | P2 | missing | TCP, CC |
| K-134 | Add markup to Tool Chest by drag | Drag a markup from the page into a tool set to save it. | Drag onto Tool Chest | - | P1 | missing | TCG |

## G. Layers (PDF optional content)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-135 | Layers panel | Lists PDF layers (OCGs) with visibility toggles and tools to create/delete layers. | Window > Panels > Layers | Alt+Y? | P1 | missing | LP |
| K-136 | Toggle layer visibility | Eye box per layer shows/hides everything on it (drawing content and markups). | Layers panel | - | P1 | missing | LP |
| K-137 | Add New Layer | Create an empty layer before/after/as child of another. | Layers toolbar | - | P2 | missing | LP |
| K-138 | Layer hierarchy by drag | Drag to reorder or nest layers as children. | Layers panel | - | P3 | missing | LP |
| K-139 | Markup Layer | Designate the layer new markups are created on. | Layers right-click | - | P1 | missing | LP |
| K-140 | Isolate layer | Show only the chosen layer (and its children). | Layers right-click | - | P1 | missing | LP |
| K-141 | Show All / Reset Layers | Turn every layer on, or restore the saved configuration. | Layers menu | - | P2 | missing | LP |
| K-142 | Layer configurations | Save named visibility presets and switch between them. | Layers toolbar dropdown | - | P2 | missing | LP |
| K-143 | Show Print / Export layers | Preview only layers set to print / to export. | Layers menu | - | P3 | missing | LP |
| K-144 | Show layers on page only / alphabetical | Filter to layers used on the current page; sort by name. | Layers menu | - | P3 | missing | LP |
| K-145 | Rename / Delete layer | Rename or remove a layer. | Layers right-click | - | P2 | missing | LP |
| K-146 | Layer Properties | Title, default state, locked, view/print/export state, zoom-range visibility. | Layers right-click > Properties | - | P3 | missing | LP |
| K-147 | Import / Export layer to page | Bring content from another PDF in as a layer; export a layer to a PDF. | Layers right-click / toolbar | - | P3 | missing | LP |
| K-148 | Flatten/Unflatten markups on a layer | Flatten markups into a layer's content, or restore them. | Layers right-click | - | P3 | missing | LP |

## H. Markups List

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-149 | Markups List panel | Table of every markup in the document with author, date, color, comments, measurements; click to zoom to markup. | Window > Panels > Markups | Alt+L | P0 | have | KS, MLP |
| K-150 | Standard columns: identity | Subject, Label, Page Label, Page Index, Author, Date, Creation Date, Color, Comments, Layer, Space, Sequence. | Markups List > Columns | - | P0 | have (Subject, Label, Page Label, Page Index, Author, Date, Creation Date, Color, Comments, Layer, Space (blank), Markup ID) | MLP |
| K-151 | Standard columns: measurement | Measurement, Length, Area, Wall Area, Volume, Count, Depth, Height, Width, Rise/Drop, Slope, Unit. | Markups List > Columns | - | P0 | partial (Measurement, Length, Area, Perimeter, Count, Width, Height; Volume / Depth / Wall Area / Slope columns exist but stay blank: no depth or slope data yet) | MLP |
| K-152 | Standard columns: geometry | X, Y, X Center, Y Center, Document Width, Document Height. | Markups List > Columns | - | P3 | missing | MLP |
| K-153 | Standard columns: review | Status, Checkmark, Lock, Capture, Legend, 3D View. | Markups List > Columns | - | P1 | partial (Status, Checkmark, Lock; no Capture / Legend / 3D) | MLP |
| K-154 | Show/hide columns | Pick which columns are visible from a menu. | Markups List > Columns | - | P0 | have (header right-click, Columns button) | MLP |
| K-155 | Manage Columns dialog | Add/remove/reorder columns and create custom columns. | Markups List > Manage Columns | - | P1 | have (Manage Custom Columns dialog; drag headers to reorder) | MLP, CC |
| K-156 | Custom column: Text | Free text field, optional multiline wrap and default value. | Manage Columns | - | P1 | have (default value; no multiline wrap) | CC |
| K-157 | Custom column: Number | Numeric field with normal/currency/percent format, decimals, currency symbol, min/max, default. | Manage Columns | - | P1 | have (Number, Currency, Percent; decimals, symbol, default; no min/max) | CC |
| K-158 | Custom column: Date | Date field with chosen format and default (none/current/custom). | Manage Columns | - | P2 | partial (yyyy-MM-dd, literal default) | CC |
| K-159 | Custom column: Checkmark | Checkbox column with default state. | Manage Columns | - | P2 | have | CC |
| K-160 | Custom column: Choice | Dropdown of predefined items, optionally linked to subject and carrying a numeric value (e.g. unit cost); importable from CSV (item, subject, value); can allow custom text. | Manage Columns | - | P0 | have (items with subject + value, CSV import, custom text) | CC, CCT |
| K-161 | Custom column: Formula | Computed column from other columns using + - * / ^ %, negation, constants (pi, e) and functions (sqrt, trig, log...). Used for cost = quantity * unit price. | Manage Columns | - | P0 | have (core/Formula: + - * / ^ %, pi, e, sqrt/trig/log/round/min/max/if) | CC, CCT |
| K-162 | Totals for custom columns | Number/formula columns can be summed in group (section divider) totals. | Manage Columns option | - | P0 | have | CC |
| K-163 | Save columns to profile | Make custom columns appear on all future documents; delete from profile. | Manage Columns | - | P1 | partial (Markups List right-click > Column Layouts: save / load / delete the column layout by name; custom column definitions stay per document; wave 20) | CC |
| K-164 | Sort by column | Click a header to sort; secondary sort by creation date. | Column header | - | P0 | have (secondary sort by creation date) | MLP |
| K-165 | Group by column (section dividers) | Group rows into collapsible sections by a column with per-section totals. | Column header / list menu | - | P0 | have (any column, nested two levels, subtotals) | MLP, MLS |
| K-166 | Column filters | Filter rows by values in any column; non-matching markups are dimmed on the page too. | Filter List toggle + header dropdowns | - | P0 | partial (per-column value filters; markups are not dimmed on the page yet) | MLP, MLS |
| K-167 | Search markups | Type text to filter the list. | Markups List > Search | - | P1 | have | MLP |
| K-168 | Saved filter configurations | Save/load filter setups; clear all filters. | Filter menu | - | P2 | missing | MLP |
| K-169 | Inline cell editing | Edit subject, label, comments, custom fields directly in the grid. | Markups List cells | - | P0 | have (text, number, date, choice, check boxes) | MLP |
| K-170 | Status | Set review status: Accepted, Rejected, Completed, Cancelled, None; custom status sets possible. | Right-click > Set Status | - | P1 | have (None/Accepted/Rejected/Cancelled/Completed as /StateModel /Review replies; no custom status sets) | MLP |
| K-171 | Checkmark | Private per-user check box for tracking which markups you have handled. | Right-click > Check / column | - | P1 | have (/StateModel /Marked replies) | MLP |
| K-172 | Replies | Threaded replies under a markup, shown in the list and properties. | Right-click > Reply | - | P1 | partial (read, list, add, delete; no threads of threads) | MLP |
| K-173 | Select-in-list syncs page | Selecting a row selects and zooms to the markup; selecting on page highlights row. | Markups List | - | P0 | have | MLP |
| K-174 | Copy rows | Copy selected rows to clipboard as table data. | Right-click > Copy | - | P1 | have (Ctrl+C, tab-separated) | MLP |
| K-175 | Summary: CSV | Export the list (visible columns, filters) as CSV. | Markups List > Summary | - | P0 | have (visible columns, filters, grouping, totals) | MLS |
| K-176 | Summary: XML | Export the list as XML for other systems. | Markups List > Summary | - | P2 | have | MLS |
| K-177 | Summary: PDF report | Report with thumbnails of each markup (small/medium/large), flow or table style, page range, filter/sort, column widths. | Markups List > Summary | - | P1 | partial (table style, totals, optional thumbnails; no flow style / page range) | MLS |
| K-178 | Summary appended with links | Append the PDF summary to the document with hyperlinks back to each markup. | Summary dialog | - | P2 | missing | MLS |
| K-179 | Import markups | Load markups from another PDF, XML, BAX or FDF. | Markups List > Import | Ctrl+F3 | P1 | have (File > Import Markups Ctrl+F3 and Markups List: from a PDF with a page mapping, XFDF, FDF; same /NM replaces; one undo step; no BAX; wave 20) | MLP, KS |
| K-180 | Export markups | Save markups to BAX or FDF for exchange. | Markups List > Export | Ctrl+F2 | P2 | missing | MLP, KS |
| K-181 | Flatten markups | Burn markups into page content (optionally by type/selection/pages) so they can no longer be edited. | Document > Flatten | Ctrl+Shift+M (Ctrl+Alt+F also listed) | P1 | partial (Document > Flatten Markups Ctrl+Shift+M: selected markups or all on a page range, burned in from their appearance; undo until closed; no unflatten, no filter by type; wave 20) | KS |
| K-182 | Unflatten | Restore flattened markups to editable annotations (works for markups Revu flattened). | Document > Unflatten | Ctrl+Shift+U | P2 | missing | KS |
| K-183 | Lock / unlock from list | Lock column toggles lock per markup. | Lock column / right-click | Ctrl+Shift+L | P1 | have (Lock column and list right-click set the same /F Locked flag as the canvas Lock; locked markups are not moved, edited or deleted) | MLP |
| K-184 | Layer from list | Assign selected markups to a layer from the list. | Right-click > Layer | - | P2 | missing | MLP |
| K-185 | Legend from list | Create a legend from selected list rows. | Right-click > Legend | - | P2 | missing | MLP |
| K-186 | Properties from list | Open Properties for the selected markups. | Right-click > Properties | - | P1 | have (Markups List right-click > Properties; wave 20) | MLP |
| K-187 | Delete from list | Delete selected markups from the list. | Right-click > Delete | - | P0 | have | MLP |

## I. Measurement-adjacent markup tools (listed here for shortcut completeness; full detail belongs in the measure inventory)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-188 | Length / Polylength / Area / Perimeter / Count | Core takeoff tools. | Measure | Shift+Alt+L / Q / A / P / C | P0 | have | KS |
| K-189 | Angle, Radius, Diameter, Volume | Further measure types. | Measure | Shift+Alt+G / U / D / V | P2 | missing | KS |
| K-190 | Measure Tool (generic) | Opens the measurement toolset. | Measure | M | P1 | partial | KS |

## J. Customization

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| K-191 | Custom keyboard shortcuts | Remap or add shortcuts for any tool or command (including sketch and batch tools); reassign conflicts. | Revu > Keyboard Shortcuts | - | P2 | missing | KS |
| K-192 | Markup display preferences | Show/hide markup pop-ups, rollover comments, line weight scaling with zoom. | Preferences > Markup | - | P3 | missing | UM |

---

## Biggest gaps (top 10 for an estimator / PM)

1. **Properties panel** (K-057 to K-086): there is nowhere to set fill, opacity, line width, line style, font or cloud style. Almost every other markup feature depends on it.
2. **Cloud and Cloud+** (K-026, K-027, K-073): these are the standard markups for drawing review and RFIs, and MarkupCraft has neither.
3. **Text Box, Callout and Arrow** (K-001, K-005, K-019, K-015): needed for any review comment. MarkupCraft cannot create text yet.
4. **Tool Chest with .btx import/export, Properties/Drawing modes and scaled symbols** (K-115 to K-134): estimators keep their takeoff standards here. Reading .btx is key to adoption.
5. **Custom columns, especially Choice + Formula, with totals** (K-155 to K-163): this is how Revu users price a takeoff (quantity times unit cost) inside the Markups List.
6. **Markups List grid power**: show/hide columns, sort, group by any column, per-column filters that dim markups on the page (K-154, K-164 to K-168).
7. **Clipboard: copy/paste, Paste in Place, Apply to pages** (K-095 to K-097): needed to carry markups between revisions and sheets.
8. **Snap to content/markup** (K-111): accurate takeoff and review drawing depend on it.
9. **Review workflow: Status, Checkmark, Replies, Lock** (K-170 to K-172, K-083) plus the **PDF summary report** (K-177).
10. **Rectangle/Ellipse/Pen/Highlighter plus Format Painter and Set as Default** (K-023, K-024, K-029, K-030, K-084, K-088): basic markup tools that users expect on day one.

Notes: shortcuts marked `?` came from re-pairing the misaligned columns in KeyboardShortcuts.pdf. Confirm them before binding. UM-sourced detail (cloud intensity values, blend modes, line-end list, distribute) comes from tool-page summaries and should be checked against the full support page during implementation.
