# Revu 21 feature inventory: 01 Measurement and quantity takeoff

Clean-room inventory, built only from Bluebeam's public support site and the installed help PDFs
(`KeyboardShortcuts.pdf`, `Getting Started.pdf`). Descriptions are in our own words. No binaries,
no decomp, no running Revu. Compiled 2026-10-07.

Priority is for an estimator doing HVAC/mechanical takeoff: **P0** daily / must have, **P1** weekly,
**P2** occasional, **P3** rare or niche. MarkupCraft status is against the app as of today
(Calibrate, arch+eng presets, one page scale, Length/Polylength/Area/Perimeter/Count, Shift ortho,
vertex edit, undo, color, Markups List grouped by subject with totals, CSV export, Revu-compatible save).

Rows marked *(unverified)* are behaviour we believe exists but that the public page we read did not
state outright; confirm by making the markup by hand in Revu and diffing the saved PDF (our own file).

## Source key

| Key | Source |
|---|---|
| KS | `C:\Program Files\Bluebeam Software\Bluebeam Revu\21\Help\KeyboardShortcuts.pdf` |
| GS | `C:\Program Files\Bluebeam Software\Bluebeam Revu\21\Help\Getting Started.pdf` |
| TM | https://support.bluebeam.com/user-manual/menus/tools/takeoffs-measurements.html |
| MP | https://support.bluebeam.com/se/user-manual/menus/window/measurements-panel.html |
| MT | https://support.bluebeam.com/fi/user-manual/toolbars/measure-toolbar.html |
| SC | https://support.bluebeam.com/user-manual/menus/tools/set-page-scale.html |
| VP | https://support.bluebeam.com/se/user-manual/menus/window/create-manage-viewports.html ; https://support.bluebeam.com/revu/how-to/use-viewports.html |
| LN | https://support.bluebeam.com/nl/user-manual/menus/tools/length-measurement.html |
| PL | https://support.bluebeam.com/user-manual/menus/tools/polylength-measurement.html |
| PE | https://support.bluebeam.com/user-manual/menus/tools/perimeter-measurement.html |
| AR | https://support.bluebeam.com/user-manual/menus/tools/area-measurement.html |
| VO | https://support.bluebeam.com/nl/user-manual/menus/tools/volume-measurement.html |
| CU | https://support.bluebeam.com/user-manual/menus/tools/cutout-tool.html |
| CR | https://support.bluebeam.com/user-manual/menus/tools/center-radius-measurement.html |
| R3 | https://support.bluebeam.com/user-manual/menus/tools/3-point-radius-measurement.html |
| CT | https://support.bluebeam.com/user-manual/menus/tools/count-tool.html |
| CB | https://support.bluebeam.com/revu/how-to/tips-and-tricks/boost-takeoff-workflows-with-count-tool.html |
| DF | https://support.bluebeam.com/user-manual/menus/tools/dynamic-fill.html |
| TP | https://support.bluebeam.com/user-manual/menus/revu/tools-preferences.html (Measure, Sketch tabs) |
| GR | https://support.bluebeam.com/online-help/revu2017/Content/RevuHelp/11--Preferences/Grid-and-Snap-Preferences--V.htm ; https://support.bluebeam.com/revu/how-to/disable-snap-to-content-in-measurements-panel.html |
| BP | https://support.bluebeam.com/revu/how-to/follow-best-practices-for-measurement-markups.html |
| UN | https://support.bluebeam.com/revu/how-to/use-different-units-of-measurement-within-drawing.html |
| MV | https://support.bluebeam.com/revu/how-to/multiple-measurement-values.html ; https://support.bluebeam.com/revu/how-to/tips-and-tricks/add-captions-to-markups.html |
| CC | https://support.bluebeam.com/user-manual/tutorials/custom-columns-takeoffs.html ; https://support.bluebeam.com/revu/how-to/tips-and-tricks/calculate-costs-with-custom-columns-in-markups-list.html |
| QL | https://support.bluebeam.com/revu/how-to/tips-and-tricks/improve-takeoffs-with-quantity-link.html ; https://support.bluebeam.com/revu/how-to/enable-quantity-link.html |
| LG | https://support.bluebeam.com/user-manual/menus/tools/markups-legend.html ; https://support.bluebeam.com/revu/features/legends-overview.html |
| VS | https://support.bluebeam.com/revu/features/visual-search-overview.html ; https://support.bluebeam.com/se/user-manual/menus/window/search-panel.html |
| SK | https://support.bluebeam.com/fi/user-manual/toolbars/sketch-toolbar.html |
| SP | https://support.bluebeam.com/user-manual/menus/window/create-edit-spaces.html |
| SU | https://support.bluebeam.com/user-manual/menus/batch/summary.html |
| ML | https://support.bluebeam.com/revu/how-to/track-and-manage-markups-using-markups-list.html |
| TC | https://support.bluebeam.com/user-manual/menus/window/use-tool-chest.html ; https://support.bluebeam.com/revu/features/tool-chest-guide.html |
| AI | https://community.bluebeam.com/kb/articles/327-2-can-mcp-support-qto-and-measurements |

## Scale and calibration

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-001 | Calibrate | Draw a line over a known dimension, type its real length and units, and the page scale is derived from it. Shift constrains the calibration line; Snap to Content helps hit vector endpoints. | Tools > Measure > Set Scale / Measurements panel > Page Scale > Calibrate | none | P0 | have | SC, MP |
| M-002 | Preset scales | Pick a standard scale (architectural, engineering, metric ratio) from a dropdown instead of calibrating. | Measurements panel > Page Scale > Preset; status bar scale selector | none | P0 | have (arch, eng, metric 1:N) | SC |
| M-003 | Custom scale | Type "drawing length = real length" with independent units on each side, units may mix systems (e.g. mm on paper = ft real). | Measurements panel > Page Scale > Custom | none | P0 | have (Set Scale dialog, mixed units) | SC |
| M-004 | Save custom/calibrated scale as preset | Store a custom or calibrated scale; user presets list at the top of the dropdown and can be deleted (built-ins cannot). | Page Scale > + Add Preset; trash icon | none | P2 | missing | SC, MP |
| M-005 | Apply scale to page range | When setting a scale choose All pages, Current, Selected (thumbnails), or a custom range like "1-3, 5, 9". | Set Scale dialog > page range | none | P0 | partial (current / all / range text; not thumbnail selection) | SC |
| M-006 | Add Scale to More Pages | Copy an existing page's scale to other pages after the fact. | Measurements panel > Add Scale to More Pages | none | P1 | have (Tools > Measure > Add Scale to More Pages, Measurements panel; page range, optional recalculate) | SC, MP |
| M-007 | Per-page scale | Every page carries its own scale; measurements use the scale of the page (or viewport) they sit on. | Measurements panel > Page Scale (current page) | none | P0 | have | SC, MP |
| M-008 | Separate Y scale | Allow different horizontal and vertical scales (stretched scans, profiles). Turning it off affects only future measurements. | Page Scale > Separate Y Scale | none | P3 | missing | SC |
| M-009 | Precision | Choose display precision per scale, as decimal places or as a fraction (fraction choices for imperial ft-in style units). | Measurements panel > Precision | none | P0 | have (page scale and per measurement) | SC, MP |
| M-010 | Scale shown in status bar | The current page scale is shown and changeable from the bottom status/navigation bar. | Status bar | none | P1 | have (navigation bar scale button with the preset menu; "Scale Not Set") | SC |
| M-011 | Protected / temporary page scales | Scales can be locked; a temporary scale reverts when the document closes; panel shows when scale is protected. | Measurements panel > Page Scale | none | P3 | missing | MP |
| M-012 | Recalculate | After a scale change, push the new scale onto existing measurements so their values update. | Measurements panel > Recalculate | none | P1 | have (Tools > Measure > Recalculate Measurements: this page / all pages; keeps each measurement's units and precision) | MP |
| M-013 | "Scale not set" state | Pages without a scale show that no scale is set rather than giving bogus values. | Measurements panel / caption | none | P1 | partial (we always write a scale on save) | PLAN.md note, MP |

## Viewports

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-014 | Add Viewport | Drag a rectangle (Shift = square) around a detail to give that region its own scale; measurements started inside it use the viewport scale. Polygon-shaped viewports are reported in third-party write-ups *(unverified)*. | Measurements panel > Viewports > Add Viewport | none | P1 | missing | VP |
| M-015 | Viewport label | Each viewport has a name (Label); unnamed viewports are flagged as risky and should be recreated. | Viewports list | none | P1 | missing | VP |
| M-016 | Viewport scale by preset / custom / calibrate | Each viewport takes a preset, custom, or calibrated scale, with optional separate X/Y; "More Options" opens the full scale dialog. | Viewports list > menu | none | P1 | missing | VP |
| M-017 | Highlight Viewports | When a measurement tool is active, viewports are drawn highlighted (blue) with their scale so you see which scale applies; can also highlight from the list. | Measurements menu > Highlight Viewports | none | P2 | missing | MP, VP |
| M-018 | Delete / Clear All viewports | Remove one viewport, or clear all from the page or the whole document. | Viewports > Delete Viewport / Clear All | none | P2 | missing | VP |
| M-019 | Apply viewports to other pages | Copy a viewport (region + scale) to matching sheets *(unverified on Bluebeam pages; seen in third-party tips)*. | Viewport right-click | none | P2 | missing | VP (third-party) |

## Linear tools

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-020 | Length | Two-click straight-line measurement drawn like a dimension line; arrows flip inside/outside extension lines depending on room. | Tools > Measure > Length; Measure toolbar; Measurements panel | Shift+Alt+L | P0 | have | KS, TM, LN |
| M-021 | Polylength | Multi-segment open run whose total length is the sum of segments; click points, double-click or Enter to finish. Duct and pipe runs. | Tools > Measure > Polylength | Shift+Alt+Q | P0 | have | KS, PL |
| M-022 | Perimeter | Multi-segment line measured as a perimeter (open or closed); can carry Depth to produce wall area. | Tools > Measure > Perimeter | Shift+Alt+P | P1 | have | KS, PE |
| M-023 | Backspace removes last point | While placing a multi-point measurement, Backspace deletes the last vertex. | in tool | Backspace | P0 | missing *(check)* | PE, CU |
| M-024 | Enter / double-click to finish; click first point to close | Standard completion gestures for multi-point measurements. | in tool | Enter | P0 | have | PL, AR |
| M-025 | Rise/Drop (Polylength) | Adds a vertical run (floor-to-floor) to the length total in the same units. Mutually exclusive with Depth. | Measurements panel > Measurement Properties | none | P1 | have (Polylength, Measurements panel; saved in our key /PCRiseDrop) | MP, PL |
| M-026 | Slope (Length/Area) | Applies a slope given as Pitch, Degrees or Grade so the true sloped length/area is reported. | Measurement Properties > Slope | none | P2 | missing | MP, PL |
| M-027 | Show Segment Values | Show each segment's length on the segment and the running total opposite it; text can align to the segment or stay horizontal. | Properties > Show Segment Values | none | P1 | have (Properties: Tools > Measure / right-click / Measurements panel; values along each segment, total beside the last one; /PCSegmentValues) | PL, AR |
| M-028 | Convert segment to arc | Right-click a segment and turn it into an arc, then drag the yellow handle to bend it; measurement updates live. Ctrl-drag also curves a segment. | right-click segment > Convert to Arc | Ctrl+drag | P1 | have (right-click a segment > Convert to Arc / Convert to Line, drag the yellow handle; stored as a 32-chord polyline in /Vertices + /PCArcs; no Ctrl+drag) | TM, CU |
| M-029 | Add / delete control point | Insert or remove vertices on an existing measurement. | right-click markup | none | P1 | have (Shift+click an edge / vertex, or right-click > Add Vertex / Delete Vertex) | AR |

## Area and volume tools

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-030 | Area (polygon) | Click points to define a closed polygon; area is computed at page scale. Close with Enter, double-click, or clicking the first point. | Tools > Measure > Area | Shift+Alt+A | P0 | have | KS, AR |
| M-031 | Area by drag-rectangle | Click-drag places a rectangle area in one gesture; Shift forces a square. | Area tool | Shift+drag | P0 | have | AR |
| M-032 | Volume | Area polygon plus a Depth gives volume; can also show area, perimeter and wall area. | Tools > Measure > Volume | Shift+Alt+V | P2 | missing | KS, VO |
| M-033 | Depth | A third dimension on Perimeter/Area/Volume/Polylength that enables Wall Area and Volume columns. | Measurement Properties > Depth | none | P1 | missing | MP, VO |
| M-034 | Wall Area | Perimeter x Depth reported as wall area (shown as "WA"). Useful for duct wrap / wall surface. | Markups List column; caption | none | P2 | missing | TM |
| M-035 | Show All Measurements | One markup shows several values at once (Perimeter, Area, Wall Area, Volume as P/A/WA/V). | Properties > Show All Measurements | none | P1 | missing | TM, MV |
| M-036 | Show Centroid | Mark the geometric center of an area. | Properties > Show Centroid | none | P3 | missing | AR, MP |
| M-037 | Polygon Cutout | Hover a target Area/Volume (highlights blue), then draw a polygon hole that is subtracted. Multiple cutouts allowed; overlapping ones merge. | Tools > Measure > Polygon Cutout; Measurements panel | none | P0 | partial (multiple cutouts; overlaps not merged; Revu storage guessed) | CU, MT |
| M-038 | Ellipse Cutout | Drag an ellipse hole out of an Area/Volume; Shift makes a circle. | Tools > Measure > Ellipse Cutout | none | P1 | have (Tools > Measure > Ellipse Cutout, Shift = circle; a 48-point ring) | CU, MT |
| M-039 | Edit / delete cutout | Cutout vertices are editable like the parent; right-click a cutout edge > Delete Cutout. Curved cutouts may not overlap edges or other cutouts. | right-click cutout | none | P1 | have (cutout vertices drag, Shift+click adds / deletes; right-click > Delete Cutout) | CU |
| M-040 | Cutout to its own measurement | Turn an existing cutout into a separate Area/Volume by picking the tool and clicking inside the cutout (fill cursor). | Area/Volume tool over cutout | none | P3 | missing | CU |
| M-041 | Hatch fill | Area fill can be a hatch pattern with a scale (about 50-200%). | Properties > Hatch | none | P2 | missing | AR |
| M-042 | Rotate measurement | Rotate a shape by its orange handle (15-degree snap, Shift for 1-degree) or type an angle. | Properties > Rotation; handle | Shift while rotating | P3 | missing | AR, KS |

## Circular and angular tools

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-043 | Diameter | Measures the diameter of a circular feature (round duct, tank, curb). | Tools > Measure > Diameter | Shift+Alt+D | P2 | missing | KS, MT |
| M-044 | Center Radius | Click center then a point on the circumference; draws full circle by default or an arc (max 180 degrees) by dragging. | Tools > Measure > Center Radius | Shift+Alt+U (Radius) | P2 | missing | KS, CR |
| M-045 | 3-Point Radius | Three clicks on an arc (end, mid, end) give a pie shape and its radius; can be resized to a full circle. | Tools > Measure > 3-Point Radius | none | P3 | missing | R3 |
| M-046 | Angle | Three clicks define an angle and report degrees. | Tools > Measure > Angle | Shift+Alt+G | P3 | missing | KS, MT |

## Count

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-047 | Count | Click each item to drop a numbered symbol; the series keeps a running total; Esc ends. | Tools > Measure > Count | Shift+Alt+C | P0 | have | KS, CT |
| M-048 | Count symbol choice / scale | Pick the symbol shape, color, opacity and symbol scale for a count series. | Properties | none | P0 | partial (shape, size, color; no opacity UI) | CT |
| M-049 | Custom count symbol from any markup | Draw any markup and turn it into a count symbol via the Tool Chest. | Tool Chest | none | P1 | have (right-click any markup > Add to Tool Chest as Count Symbol: a Count item in My Tools, armed; /PCCountShape) | CT, CB |
| M-050 | Resume Count | Right-click any member of a series to keep adding to it later. | right-click > Resume Count | none | P0 | have | CT |
| M-051 | Delete one item from a count group | Right-click a single symbol > delete it from the group; the series renumbers. Ctrl-click for several. | right-click | none | P0 | have (right-click > Delete Count Item; renumbers) | CT |
| M-052 | Split / merge counts | Break a series into smaller groups or join separate series. | right-click | none | P2 | missing | CT, CB |
| M-053 | Count dimensions | A count can carry Height/Width/Depth for uniform items (diffusers, doors). | Measurements panel | none | P3 | missing | CT |
| M-054 | Count grouped per page or Space | Totals roll up per page or per Space; optional auto-split by Space in preferences. | Markups List; Preferences > Measure | none | P1 | partial (per page: Measurements panel counts per page, Markups List group by Page; no Spaces) | CT, TP |
| M-055 | Live count preview | A small live readout in the lower-right while placing; click to reopen it. | lower-right of view | none | P2 | missing | CB |
| M-056 | Count statuses | Assign statuses to counts (e.g. installed) and split to set per item; drives visual status reports. | Markups List > Status | none | P3 | missing | CB |

## Dynamic Fill and auto-detection

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-057 | Dynamic Fill | Click-and-hold inside a room; a flood fill grows to the drawn vector boundaries; Apply turns the region into a measurement. | Tools > Measure > Dynamic Fill | J | P1 | missing | KS, DF |
| M-058 | Dynamic Fill outputs | The filled region can become a Space, Area, Polylength, Perimeter, Volume, or plain Polygon, or several at once, using defaults or a Tool Chest tool. | Dynamic Fill toolbar | none | P1 | missing | DF |
| M-059 | Add Boundary | Draw temporary boundary lines (2+ points) to split a room or close an open one before filling. | Dynamic Fill toolbar > Add Boundary | none | P1 | missing | DF |
| M-060 | Clear fill / boundaries | Clear All, Clear Fill, or Clear Boundaries. | Dynamic Fill toolbar | none | P1 | missing | DF |
| M-061 | Drag-to-encircle fill | Moving the mouse while holding speeds filling and covers everything encircled, even across boundaries. | Dynamic Fill | none | P2 | missing | DF |
| M-062 | Dynamic Fill settings | Detection DPI, fill/boundary cursor size and color, fill speed, edge sensitivity, and temporarily hide markups while filling. | Dynamic Fill Settings; Preferences > Measure | none | P2 | missing | DF, TP |
| M-063 | Visual Search | Box a symbol; Revu finds similar occurrences, with a Sensitivity slider (lower = looser). | Search panel > Visual Search > Get Rectangle | Alt+1 (Search panel) | P1 | missing | VS |
| M-064 | Apply Count to Visual Search results | Check results then apply a chosen Count symbol to all of them at once (semi-automatic counting). | Search results > Actions > Apply Count Measurement to Checked | none | P0 | missing | VS, CB |
| M-065 | Count from text search | Create counts from text search hits (e.g. tag "VAV-"). | Search panel results actions | none | P1 | have (Search panel > Apply Count: a Count per page on every text result of the document) | CB *(described as text or Visual Search)* |
| M-066 | AI auto-count | Not a native Revu 21 feature: public material says Max AI/MCP can place counts but does not recognise symbols; no built-in "auto count". | n/a | n/a | P3 | missing | AI |

## Sketch to Scale

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-067 | Polyline Sketch to Scale | Draw a polyline by typing exact lengths and angles at page scale. | Sketch toolbar | none | P2 | missing | SK |
| M-068 | Polygon Sketch to Scale | Closed shape from typed lengths/angles. | Sketch toolbar | none | P2 | missing | SK |
| M-069 | Rectangle Sketch to Scale | Rectangle from typed width x height. | Sketch toolbar | none | P2 | missing | SK |
| M-070 | Ellipse Sketch to Scale | Ellipse from width x height, or circle from radius (preference). | Sketch toolbar; Preferences > Sketch | none | P3 | missing | SK, TP |
| M-071 | Sketch angle mode | Typed angles are relative to the previous segment or absolute to the page. | Preferences > Sketch > Rotation Input | none | P3 | missing | TP |

## Drawing aids and snapping

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-072 | Snap Orthogonal / Shift constraint | Constrain segments to 0/45/90 degrees; toggle in panel, or hold Shift temporarily. | Measurements menu > Snap Orthogonal | hold Shift | P0 | have (Shift) | MP, KS |
| M-073 | Snap to Content | Cursor snaps to vector geometry in the PDF (not text or images). | View / Measurements menu | Ctrl+Shift+F8 | P0 | have | KS, GR |
| M-074 | Snap to Markup | Snap to points of other markups, plus guide lines extended from nearby markup edges. | View | Ctrl+Shift+F7 | P1 | have (View > Snap to Markup, Ctrl+Shift+F7, status bar button: vertices, edges, midpoints, centres; no guide lines) | KS, GR |
| M-075 | Snap to Grid | Snap to a page grid; grid spacing sets the resolution. | View | Ctrl+Shift+F9 | P3 | missing | KS, GR |
| M-076 | Show Grid | Draw the grid over the page. | View | Shift+F9 | P3 | missing | KS |
| M-077 | Snap point types | Choose which point kinds snap (endpoints, midpoints, intersections, and similar) *(exact list per Revu 2017 page)*. | Preferences > Grid & Snap > Snap To | none | P1 | have (View > Snap Points: endpoints, midpoints, intersections, nearest, centres; for content and markups) | GR |
| M-078 | Snap sensitivity | Slider sets how big the snap capture radius is. | Preferences > Grid & Snap | none | P2 | missing | GR |
| M-079 | Rulers | Show page rulers. | View | Ctrl+R | P3 | missing | KS |
| M-080 | Pan while drawing | Hold Spacebar to pan without ending the measurement in progress. | in tool | Spacebar (hold) | P0 | have | KS |
| M-081 | Ctrl wheel toggle zoom/pan | Ctrl swaps mouse-wheel zoom and pan. | global | Ctrl+wheel | P1 | partial | KS |
| M-082 | Copy in straight line | Ctrl+Shift+drag duplicates a markup constrained to a line (repeat diffusers, counts). | global | Ctrl+Shift+click/drag | P1 | have (Select: Ctrl+Shift+drag) | KS |

## Measurement properties, captions and appearance

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-083 | Subject | Description that groups measurements in the Markups List; set before or after drawing. | Measurements panel / Properties | none | P0 | have | TM |
| M-084 | Label | Text shown on the markup itself alongside the value. | Measurements panel / Properties | none | P0 | have | TM |
| M-085 | Per-measurement units | Length, area and volume units chosen per markup independent of the page scale (e.g. LF for pipe, SF for area, CY for volume). | Measurement Properties; Properties toolbar units | none | P0 | partial (length / area units; no volume) | UN, BP |
| M-086 | Set as Default | Right-click a configured measurement to make its units/appearance the default for new ones of that tool. | right-click / bottom of Properties | none | P0 | have | UN, BP, TC |
| M-087 | Show Caption + Edit caption contents | Toggle the value text, and choose which values (and since 21.0.50, which Markups List custom columns) appear in it. | Properties > Show Caption > Edit | none | P1 | partial (fixed caption) | LN, MV |
| M-088 | Move caption independently | Shift-drag the caption to reposition it; right-click resets it. | on markup | Shift+drag caption | P1 | have (Shift+drag the caption; right-click or Measurements panel > Reset Caption Position; Area / Length in Revu's /CO) | KS, PL |
| M-089 | Caption leader line | Optional leader connecting a moved caption to its markup. | Properties > Show Caption Leader Line | none | P2 | missing | LN |
| M-090 | Caption default placement | Area/Volume values centre in the shape; Perimeter/Polylength values sit along the last segment. | automatic | none | P1 | partial | TM |
| M-091 | Line endpoints (Start/End) | Choose end styles (arrows, ticks, etc.) for Length/Polylength with a size scale that defaults to Auto (tracks line width). | Properties > Start / End | none | P2 | missing | LN, PL |
| M-092 | Line style / width / opacity / color / fill | Standard appearance: color, fill color, line width (pt), dash style incl. custom line styles, opacity 0-100, Highlight mode. | Properties | none | P1 | partial (color) | LN, AR |
| M-093 | Font for captions | Font, size (2-72, typed up to 144), color, bold/italic/underline/strike/super/sub. | Properties | none | P2 | missing | LN |
| M-094 | Live measure readout | While placing, a preview of key values shows bottom-right. | lower-right window | none | P1 | have (next to the cursor while drawing and while dragging vertices / cutouts; Measurements panel) | TM, CR |
| M-095 | Temporary vs persistent measurements | Toggle whether measurement tools create saved markups or just a temporary readout. | Measurements menu > Make Annotations from Measurements | none | P2 | missing | MP |
| M-096 | Measure tool (unified) | One tool that opens the Measurements panel and switches between measurement modes. | Measure toolbar > Measure | M | P2 | missing | KS, MT |
| M-097 | Measurements panel | Docked panel holding tool buttons, scale, precision, properties of the selected measurement, and viewports. | Window > Panels > Measurements | Alt+U | P0 | partial (scale, precision, snap, values, units; no viewports) | KS, MP |
| M-098 | Properties toolbar Totals | Select several measurements (even mixed types) to see summed totals in a detachable subpanel. | Properties toolbar > Totals | none | P1 | have (status bar totals of the selected measurements by unit) | TM |
| M-099 | Bulk edit selected measurements | Select many in canvas or list (box left-right = contained, right-left = touching) and change units/properties together. | canvas / Markups List | Ctrl+A in list | P0 | partial (select; not multi-edit) | BP |
| M-100 | Layer, Author, Comment fields | Standard markup fields that also act as filters downstream. | Properties | none | P2 | partial (read only) | QL |
| M-101 | Lock markup | Prevent accidental moves of finished takeoff. | right-click / Properties | Ctrl+Shift+L | P2 | missing | KS |
| M-102 | Legacy Subject/Label persist | Preference choosing whether the last-used Subject/Label carry over to new measurements. | Preferences > Measure | none | P3 | missing | TP |

## Tool Chest for takeoff

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-103 | Save measurement tool to Tool Chest | Store a configured measurement (subject, color, units, depth, custom column values) for reuse across files. | Tool Chest; right-click > Add to Tool Chest | Alt+X opens panel | P0 | have | TC, KS |
| M-104 | Properties Mode vs Drawing Mode | Properties mode reuses only the look/settings; drawing mode stamps the exact saved geometry. | Tool Chest item toggle | none | P1 | missing | TC |
| M-105 | Tool sets (e.g. one per system) | Group tools into named sets (Supply duct, Return duct, Diffusers) and share .btx files. | Tool Chest | none | P0 | have | TC |
| M-106 | Dynamic Tool Set Scaler | A tool set authored at one scale auto-resizes symbols when placed on pages/viewports of another scale. | Tool Chest set properties | none | P2 | missing | TC |
| M-107 | Takeoff profile | A UI profile that arranges panels for takeoff work. | Profiles | none | P3 | missing | GS |

## Markups List, custom columns and formulas

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-108 | Measurement column with totals | Each measurement's value appears in the list; the column header/top shows the running total by type. | Markups List | Alt+L opens list | P0 | have | TM |
| M-109 | Measurement-specific columns | Length, Area, Volume, Depth, Wall Area, Count, Space, Page Label, Layer etc. available as columns. | Markups List > Columns | none | P1 | partial | CC, SP |
| M-110 | Sort / group / filter list | Sort by any column, group, and filter by author, status, subject etc. | Markups List | none | P0 | partial (group by subject) | ML |
| M-111 | Custom column: Number | Numeric field per markup usable in formulas (e.g. unit price, gauge). | Markups List > Manage Columns | none | P1 | missing | CC |
| M-112 | Custom column: Choice with values | Dropdown of named choices, each carrying a numeric value (material + unit cost). | Manage Columns | none | P1 | missing | CC |
| M-113 | Custom column: Formula | Expression built from operators (+ - * / ^ %), functions (round, ceiling, floor, sqrt, trig, log), constants (pi, e) and variables (Measurement, Length, Area, Volume, Depth, other columns). | Manage Columns > Formula | none | P1 | missing | CC |
| M-114 | Custom column: Text / Date / Checkmark | Free text, date, and yes/no fields per markup. | Manage Columns | none | P2 | missing | CC |
| M-115 | Formula display format (e.g. currency) | Formula/number columns can display as currency etc. | Manage Columns | none | P2 | missing | CC |
| M-116 | Custom columns in captions | Show custom column data (material, cost) on the markup caption (21.0.50+). | Show Caption > Edit | none | P2 | missing | MV |
| M-117 | Statuses | Preset/custom status values on markups, filterable and reportable. | Markups List > Status | none | P3 | missing | CB |

## Spaces (location grouping)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-118 | Spaces | Named rectangle/polygon regions (rooms, floors, zones); measurements inside one get its name automatically. | Spaces panel > Add Space | Alt+S (panel) | P1 | missing | SP |
| M-119 | Space column / group by Space | Markups List shows the Space and can total by it. | Markups List > Columns > Space | none | P1 | missing | SP |
| M-120 | Recalculate Space | Re-assign Space membership for markups drawn before the Space existed. | right-click in Markups List | none | P2 | missing | SP |
| M-121 | Space from Dynamic Fill | Dynamic Fill can create a Space directly from a room outline. | Dynamic Fill | J | P2 | missing | DF |

## Legends

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-122 | Tool-set legend | Place a legend tied to a Tool Chest set; it lists matching markups with symbols and live quantities and picks up new ones automatically. | Tool Chest / Tools > Legend | none | P1 | missing | LG |
| M-123 | Ad-hoc legend | Legend built from a chosen selection of markups. | Tools > Legend | none | P2 | missing | LG |
| M-124 | Legend scope | Count markups from the current page, all pages, or a page range. | Legend properties | none | P1 | missing | LG |
| M-125 | Legend columns | Choose and order columns from the Markups List (including custom columns); extra columns split rows. | Edit Columns | none | P1 | missing | LG |
| M-126 | Legend appearance | Title on/off, fonts, line/fill color and opacity, line width, symbol size 1-5000%, alignment/margins, header row, table styles. | Properties | none | P2 | missing | LG |
| M-127 | Legend distribution | Copy to all pages at the same position, or snapshot to a static copy. | right-click | none | P3 | missing | LG |

## Export, reporting and Excel

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| M-128 | Markup Summary to CSV | Export markups (with measurement and custom columns) to CSV for Excel. | Markups List > Summary > CSV | none | P0 | have (basic CSV) | SU |
| M-129 | Markup Summary to XML / PDF / Print | Other summary outputs; PDF supports templates, logos, flow/table styles, page breaks by field, thumbnails, Spaces cover sheet, append to PDF with links. | Summary dialog | none | P2 | missing | SU |
| M-130 | Summary across files/folders | Summarise many PDFs (open files, folders, subfolders) in one report, with saved configurations. | Batch > Summary | none | P1 | missing | SU |
| M-131 | Summary filters and sort | Filter by Space, author, etc. and choose multi-level sort before export. | Summary dialog | none | P1 | partial | SU |
| M-132 | Quantity Link: link cell to total | In Excel, right-click a cell > Quantity Link > Create and bind it to a measurement total from one or more PDFs; updates live as markups change. | Excel add-in | none | P1 | missing | QL |
| M-133 | Quantity Link filters | Totals filtered by Subject, Label, Color, Author, Layer or any custom column; filters edited via Manage. | Excel add-in > Manage | none | P1 | missing | QL |
| M-134 | Quantity Link measurement types | Length, Area, Volume, Count and custom columns can be linked. | Excel add-in | none | P1 | missing | QL |

## Biggest gaps for takeoff (top 10)

1. **Cutouts / deductions** (M-037..M-039): PLAN.md's v1 "done" explicitly needs deductions; nothing exists.
2. **Per-page scale, page-range apply, custom and metric scales, precision** (M-003, M-005, M-007, M-009): one page-wide scale is wrong as soon as a set mixes 1/4" plans and 1/8" overall sheets.
3. **Snap to Content** (M-073, M-077): HVAC runs and room outlines are drawn on vector CAD lines; without snapping every point is eyeballed.
4. **Tool Chest measurement tools with Set as Default and per-markup units** (M-085, M-086, M-103, M-105): estimators work from a pre-built tool set (one tool per duct size/system), not by re-typing subjects.
5. **Visual Search + apply Count** (M-063, M-064): fastest way to count diffusers, VAVs, grilles across a sheet.
6. **Count management: Resume, delete-and-renumber, custom symbols** (M-049..M-051).
7. **Custom columns with Choice values and Formulas** (M-111..M-113): turns quantities into priced line items inside the list; prerequisite for Quantity Link and legends.
8. **Viewports** (M-014..M-016): details and enlarged mechanical rooms on the same sheet need their own scales.
9. **Depth / Rise-Drop / Wall Area / Volume** (M-025, M-032..M-035): vertical riser lengths and duct-wrap/wall surface areas.
10. **Dynamic Fill** (M-057..M-060) and **Spaces** (M-118, M-119): one-click room areas and per-floor/zone totals.
