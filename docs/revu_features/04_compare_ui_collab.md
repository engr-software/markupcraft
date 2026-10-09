# Revu 21 feature inventory, part 4: compare, overlay, search, interface, collaboration

Clean-room inventory for MarkupCraft. Built only from public documentation: Bluebeam's online
Revu 21 user manual and how-to pages, and the installed help PDFs (KeyboardShortcuts.pdf,
Getting Started.pdf). Descriptions are our own wording. No Revu binaries, decompiled code or
screen captures were used. Compiled 2026-10-07.

Priority is for a construction estimator / PM: **P0** daily, **P1** weekly, **P2** occasional,
**P3** rare. MarkupCraft status is against the 2026-10-07 build (Qt window, File/Edit/View/Tools
menus, one tools toolbar, Pages + Markups List docks, status bar with coords and scale, saved
window layout, wheel zoom, middle-drag pan, Shift ortho, Esc/Enter/Backspace while drawing,
single-letter tool keys, Fit Page/Width/Actual Size on Ctrl+9/0/8, page navigation keys).

Shortcuts are Revu 21 defaults as printed in KeyboardShortcuts.pdf; "-" means no default.

### Source key

| Code | Source |
|---|---|
| KS | Installed `Help\KeyboardShortcuts.pdf` (default shortcuts, mouse navigation, modifier keys) |
| GS | Installed `Help\Getting Started.pdf` (interface, Profiles, MultiView, Studio overview) |
| CMP | support.bluebeam.com/user-manual/menus/document/compare-documents.html |
| CVO | support.bluebeam.com/revu/features/compare-documents-vs-overlay-pages.html |
| OVL | support.bluebeam.com/user-manual/menus/document/overlay-pages.html |
| BCMP | support.bluebeam.com/user-manual/menus/batch/batch-compare-documents.html |
| BOVL | support.bluebeam.com/user-manual/menus/batch/batch-overlay-pages.html |
| SRCH | support.bluebeam.com/user-manual/menus/window/search-panel.html |
| VS | support.bluebeam.com/revu/features/visual-search-overview.html |
| SPC | support.bluebeam.com/user-manual/menus/window/spaces-panel.html |
| LNK | support.bluebeam.com/user-manual/menus/window/links-panel.html |
| SIG | support.bluebeam.com/user-manual/menus/window/signatures-panel.html |
| UI | support.bluebeam.com/user-manual/welcome/revu-interface.html |
| RM | support.bluebeam.com/user-manual/menus/revu/revu-menu.html |
| VM | support.bluebeam.com/user-manual/menus/view/view-menu.html |
| WM | support.bluebeam.com/user-manual/menus/window/window-menu.html |
| NB | support.bluebeam.com/user-manual/toolbars/navigation-bar.html |
| SB | support.bluebeam.com/user-manual/toolbars/status-bar.html |
| MV | support.bluebeam.com/user-manual/multiview.html |
| TB | support.bluebeam.com/revu/how-to/customize-toolbars.html |
| PRF | support.bluebeam.com/revu/how-to/export-import-and-manage-profiles.html and user-manual/menus/revu/import-export-profile.html |
| MOD | support.bluebeam.com/revu/how-to/tips-and-tricks/use-modifier-keys.html |
| P-GEN | support.bluebeam.com/user-manual/menus/revu/general.html |
| P-INT | .../menus/revu/interface-preferences.html |
| P-TLS | .../menus/revu/tools-preferences.html |
| P-WIN | .../menus/revu/window-preferences.html |
| P-ADV | .../menus/revu/advanced-preferences.html |
| P-ADM | .../menus/revu/admin-preferences.html |
| P-IE | .../menus/revu/import-export-preferences.html |
| P-INTG | .../menus/revu/integrations-preferences.html |
| P-SET | .../menus/revu/sets-preferences.html |
| P-STU | .../menus/revu/studio-preferences.html |
| STP | .../menus/window/studio-panel.html |
| STS | .../menus/window/studio-panel-session.html |
| SHOST | .../menus/window/session-host.html |
| SPERM | .../menus/window/manage-sessions-permissions.html |
| STPR | .../menus/window/studio-panel-project.html |
| SOFF | .../menus/window/studio-offline.html |
| CLD | support.bluebeam.com/bluebeam-cloud/bluebeam-cloud.html |
| PLG | support.bluebeam.com/revu/resources/compatibility-system-requirements-21.html |
| HELP | Installed `Help\` folder listing (plugin, scripting and Script Reference help files exist) |

---

## 1. Compare Documents

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-001 | Compare Documents | Raster-diffs an original and a revised PDF page and drops cloud markups on every region that changed, so revisions can be reviewed and tracked like any markup. | Document > Compare Documents | - | P1 | missing | CMP, CVO |
| U-002 | Document A / Document B pickers | Choose the original and the revised file, from open tabs or a file browser. | Compare dialog | - | P1 | missing | CVO |
| U-003 | Page range per side | Compare all pages, the current page, or a custom list like `1-3, 5`. | Compare dialog | - | P1 | missing | CVO |
| U-004 | Alignment method | Page Align (stack as-is), Auto Align (find matching features), or Manual Align (pick matching anchor points on both sheets). | Compare dialog | - | P1 | missing | CVO |
| U-005 | Compare a selected window | Restrict comparison to a dragged rectangle instead of the whole sheet. | Compare dialog > Select Window | - | P2 | missing | CVO |
| U-006 | Change clouds output | Differences are marked with (by default orange) cloud markups on a result copy of the revised drawing, so the original files stay untouched. | Result `_Diff` PDF | - | P1 | missing | CVO |
| U-007 | Difference markup appearance | Subject name, line color, fill color, opacity, width, cloud on/off and lock-on-place for the generated difference markups. | Compare > Advanced > Markup Options | - | P2 | missing | CMP |
| U-008 | Comparison presets | Three built-in tuning presets (same printer, different printer, scanned) plus user-saved custom presets and Restore Defaults. | Compare > Advanced > Type | - | P3 | missing | CMP |
| U-009 | Grid size / pixel density | The page is cut into grid cells; a cell counts as changed when enough pixels differ inside it. Both thresholds are tunable. | Compare > Advanced | - | P3 | missing | CMP |
| U-010 | Color sensitivity | Threshold for how different two pixel colors must be to count as a change. | Compare > Advanced | - | P3 | missing | CMP |
| U-011 | Rasterization DPI | Resolution used to rasterize both PDFs for comparison (low default, raise for fine detail). | Compare > Advanced | - | P3 | missing | CMP |
| U-012 | Ignore margin | Skip a border band so title-block edges and crop marks do not register as changes. | Compare > Advanced | - | P2 | missing | CMP |
| U-013 | Include markups / flattened markups | By default existing annotations and recoverable flattened markups are ignored; toggles include them in the diff. | Compare > Advanced | - | P3 | missing | CMP |
| U-014 | Auto-alignment / manual offset | Automatic registration (aimed at scans) or a known X/Y offset between the two sheets. | Compare > Advanced > Page Align | - | P2 | missing | CMP |
| U-015 | Results in Markups List | Every difference is an ordinary markup, so the Markups List acts as the results list: sort, filter by subject, click to zoom, set status, export. | Markups List | Alt+L (panel) | P1 | partial (Markups List exists, no compare) | CMP, CVO |
| U-016 | Review with split view + dimmer | Result can be opened side by side with the original in synced split view, with the base drawing dimmed so clouds stand out. | View > Split / Dimmer | Ctrl+2, Ctrl+F5 | P1 | missing | CVO |
| U-017 | Batch Compare Documents | Wizard that pairs many current sheets with their revisions and compares every pair in one run. | Batch > Compare Documents | - | P2 | missing | BCMP |
| U-018 | Batch: add sources | Add files, all open files, a folder, or a folder with subfolders to the Current and Revised lists. | Batch Compare wizard | - | P2 | missing | BCMP |
| U-019 | Batch: match pages by | Auto-pair by file name + page index, by page label, by text read from a defined title-block region, or pair manually. | Batch Compare wizard | - | P2 | missing | BCMP |
| U-020 | Batch: wildcard match filter | Custom filter syntax (non-digit run, number run, letter run, separator, escape) to make auto-matching precise. | Batch wizard > Advanced | - | P3 | missing | BCMP |
| U-021 | Batch: drag to re-pair | Matched pairs are shown in rows; drag a sheet to another row or remove it before running. | Batch Compare wizard | - | P2 | missing | BCMP |
| U-022 | Saved batch file | Save the file lists and matching as a reusable batch job file shared by batch compare, overlay and slip sheet. | Batch wizard > Save Batch | - | P3 | missing | BCMP |
| U-023 | Batch summary and report | After running, a summary shows pairs and difference counts; can export a PDF or CSV report with hyperlinks to each result, date/time stamp and page size options. | Batch wizard > Summary | - | P2 | missing | BCMP |

## 2. Overlay Pages

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-024 | Overlay Pages | Recolors two or more sheets and stacks them as transparent layers in a new PDF; shared linework blends dark, added and removed linework stays in each sheet's color. | Document > Overlay Pages | - | P1 | missing | OVL |
| U-025 | Add overlay sources | Add a file from open tabs, browse for files, or add all open files; each becomes one layer. | Overlay dialog > Add Files | - | P1 | missing | OVL |
| U-026 | Per-layer color | Each layer gets an auto-assigned contrasting color (red/green style pair by default), changeable per layer. | Overlay dialog > Edit Layer | - | P1 | missing | OVL |
| U-027 | Per-layer background color | Color applied to the whitespace of a layer; transparent by default. | Edit Layer | - | P3 | missing | OVL |
| U-028 | Per-layer opacity | 0-100% opacity of a layer's content. | Edit Defaults | - | P2 | missing | OVL |
| U-029 | Blend mode | How overlapping colors combine (the default darkens where both layers have ink). | Edit Defaults > Default Blend | - | P2 | missing | OVL |
| U-030 | Advanced color shading | Higher-quality color blending that preserves more detail in the overlay. | Edit Defaults | - | P3 | missing | OVL |
| U-031 | Page Align | Stack sheets exactly as positioned on the page; for revisions at the same scale and location. | Overlay dialog | - | P1 | missing | OVL |
| U-032 | Auto Align | Revu finds matching features to register sheets that differ in size or scale; an Auto Align Limit drops small objects to speed it up. | Overlay dialog; Edit Defaults | - | P2 | missing | OVL |
| U-033 | Manual Align (3 points) | User clicks the same three anchor points, in the same order, on every sheet; snap to content is recommended for precision. | Overlay dialog | - | P1 | missing | OVL |
| U-034 | Select region per layer | Overlay only a dragged region of a sheet instead of the full page. | Edit Layer > Select Region | - | P2 | missing | OVL |
| U-035 | Page range per layer | All, current, selected, or custom page list per layer. | Edit Layer > Pages | - | P2 | missing | OVL |
| U-036 | Layer name | Rename a layer independently of its file name. | Edit Layer | - | P3 | missing | OVL |
| U-037 | Layer position defaults | Default rotation (0-360), scale factor and X/Y offset applied to layers. | Edit Defaults > Position | - | P3 | missing | OVL |
| U-038 | Include flattened markups | Include recoverable flattened markups in the overlay. | Edit Defaults | - | P3 | missing | OVL |
| U-039 | Layer visibility in result | The result PDF keeps each source as a toggleable PDF layer, so one revision can be hidden. | Layers panel on result | Alt+Y (Layers) | P2 | missing | OVL |
| U-040 | Batch Overlay | Wizard version: current vs new files (also from an open Set or folders), same matching, re-pairing, saved batch and PDF/CSV report as Batch Compare. | Batch > Overlay Pages | - | P2 | missing | BOVL |
| U-041 | Smart Overlay (Max plan) | Higher-tier variant that overlays whole sets and reports a match score per sheet/discipline. Note only. | Overlay (Max plan) | - | P3 | missing | OVL |

## 3. Search panel (text) and Visual Search

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-042 | Search panel | Panel that runs text or visual searches and lists hits. | Window > Panels > Search | Alt+1 or Ctrl+F | P0 | have (text; no visual search) | SRCH, KS |
| U-043 | Search scope | Current page, current document, all open documents, current Set, recent files, a folder (optionally with subfolders), or the current Studio Project (file names only). | Search panel, scope field | - | P1 | partial (current page, document, all open documents) | SRCH |
| U-044 | Search in page text | Search the PDF content text layer. | Search options > Search Pages | - | P0 | have | SRCH |
| U-045 | Search in markups | Include markup text (comments, subjects) in the search. | Search options > Search Markups | - | P1 | missing | SRCH |
| U-046 | Search file names / properties / form fields | Optional search targets: file names (folder/recents), document metadata, and form field values. | Search options | - | P2 | missing | SRCH |
| U-047 | Case sensitive / whole words | Standard match modifiers. | Search options | - | P1 | have | SRCH |
| U-048 | Results list | Hits grouped by document, page number shown, hit word highlighted in context; click jumps to it. | Search panel > Results | - | P0 | have | SRCH |
| U-049 | Next / previous result | Step through hits from the keyboard. | - | F3 / Shift+F3 | P0 | have | KS |
| U-050 | Search selected text | Select text on the page, right-click > Search to search the current PDF for it. | Context menu | - | P2 | missing | SRCH |
| U-051 | Check results + bulk actions | Tick some or all hits, then hyperlink, highlight, underline, squiggly, strikethrough, or mark for redaction every checked hit. | Results > Check Options | - | P2 | missing | SRCH |
| U-052 | Apply Count to checked | Drop a Count measurement (chosen count symbol) on every checked hit; turns a text search for a tag into a takeoff. | Results > Check Options | - | P1 | missing | SRCH |
| U-053 | Replace checked | Replace found text in the content layer (with a font fallback choice when the font is not editable). | Results > Replace Checked | - | P3 | missing | SRCH |
| U-054 | Clear results | Empty the results list. | Results toolbar | - | P2 | missing | SRCH |
| U-055 | Visual Search | Drag a rectangle around a symbol; Revu finds every graphically similar instance on the page/document/set. Works on scans too. | Search panel > Visual | - | P1 | missing | SRCH, VS |
| U-056 | Visual: sensitivity | Slider for how strict a match must be; low finds more variants but more false hits. | Visual search options | - | P1 | missing | SRCH |
| U-057 | Visual: multiple rotations | Also search the symbol rotated in 45-degree steps. | Visual search options | - | P1 | missing | SRCH |
| U-058 | Visual: filter by color | Restrict matching to the colors in the selection (histogram refine). | Visual search options | - | P2 | missing | SRCH |
| U-059 | Visual: limit by selection | On vector PDFs, ignore vector objects that run outside the selection box. | Visual search options | - | P2 | missing | SRCH |
| U-060 | Visual results thumbnails | Each hit is listed with a small image of the match; click to zoom to it. | Search panel > Results | - | P1 | missing | SRCH |
| U-061 | Visual: count / markup checked results | Same bulk actions as text search (count, highlight, hyperlink, bookmark) on visual hits; the main estimator use is auto-counting fixtures/diffusers. | Results > Check Options | - | P1 | missing | SRCH, VS |

## 4. Spaces, Links, Signatures panels

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-062 | Spaces panel | Lists named regions (rooms, zones, floors) defined on pages; markups inside a space are tagged with it so quantities can be sorted and totalled by space. | Window > Panels > Spaces | Alt+S | P1 | missing | SPC |
| U-063 | Add Space | Draw a polygon/rectangle region and name it (spaces can nest). | Spaces panel > Add Space | - | P1 | missing | SPC |
| U-064 | Highlight / edit spaces | Toggle display of space outlines; edit their shape and properties. | Spaces panel toolbar | - | P1 | missing | SPC |
| U-065 | Space column in Markups List | Markups carry the space they fall in, usable for grouping/filtering takeoff by room or zone. | Markups List | - | P1 | missing | SPC |
| U-066 | Split counts by space | Preference: a count placed across several spaces is automatically split per space. | Preferences > Tools > Measure | - | P2 | missing | P-TLS |
| U-067 | Snapshot from space | Take a snapshot of a space region (markups included or not by preference). | Spaces context menu | - | P3 | missing | P-TLS |
| U-068 | Links panel: Places | Named destinations (page + view) listed per page; click to jump; add, edit, filter. | Window > Panels > Links | Alt+N | P2 | missing | LNK |
| U-069 | Links panel: Hyperlinks list | Lists all link areas in the document; click jumps to the link; multi-select to edit several; filter box. | Links panel | - | P2 | missing | LNK |
| U-070 | Hyperlink tool / Edit Action | Draw a link rectangle and set its action (go to page, view, other file, URL). | Links panel; Tools | - | P2 | missing | LNK |
| U-071 | Signatures panel | Lists certifications and digital signatures on the document with their validation status and details. | Window > Panels > Signatures | Alt+4 | P3 | missing | SIG |
| U-072 | Sign / add field / certify / validate | Panel toolbar: sign document, add empty signature field, certify document, validate all signatures. | Signatures panel toolbar | - | P3 | missing | SIG |

## 5. Application layout: menus, toolbars, panels, tabs

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-073 | Menu bar (not a ribbon) | Revu 21 uses a classic menu bar: Revu, File, Edit, View, Document, Batch, Tools, Window, Help. | Top of window | Menu Bar toggle F9 | P0 | partial (File/Edit/View/Tools) | UI, KS |
| U-074 | Alt-key menu accelerators | Optional keyboard access to menus via Alt + letter (preference). | Preferences > General > Navigation | Alt | P2 | partial (Qt & accelerators on some items) | P-GEN |
| U-075 | Toolbars in toolstrips | Many named toolbars (File, Edit, Shapes, Text, Measure, Markup, Sketch, Order, Alignment, Rotation, Document, etc.) dockable top/left/right; drag by handle; new rows on drop. | Tools > Toolbars | - | P1 | partial (one tools toolbar) | TB |
| U-076 | Show/hide toolbars | Check/uncheck each toolbar in a menu. | Tools > Toolbars | - | P2 | missing | TB |
| U-077 | Customize toolbars | Dialog to add, remove and reorder tools on any toolbar and create new toolbars. | Tools > Toolbars > Customize | - | P2 | missing | TB |
| U-078 | Lock toolbars | Freeze all toolbars in place (all or none). | Tools > Toolbars > Lock | - | P3 | missing | TB |
| U-079 | Properties toolbar | Context toolbar under the menu bar that shows editable properties (color, line, font, scale) of the active tool or selected markup. | Under menu bar | - | P0 | missing | UI, TB |
| U-080 | Navigation bar | Bar between workspace and bottom panel: split buttons, page-mode, select/pan/zoom/lasso, first/prev/page box/next/last, prev/next view, dark mode, page size, page scale, profile picker, dimmer, document properties, security. | Bottom of workspace | F4 (toggle) | P0 | partial (page keys, scale in status bar) | NB, KS |
| U-081 | Status bar | Bottom bar: grid/snap toggles, reuse-tool toggle, view sync toggle, page size and cursor coordinates, scale. | Bottom | F8 (toggle) | P0 | partial (coords + scale) | SB, KS |
| U-082 | Page scale on bar | Shows page scale or "not set"; clicking starts calibration. | Navigation/status bar | - | P0 | partial (scale shown, combo box) | NB, SB |
| U-083 | Side and bottom panels | Left, right and bottom pull-out panels hosting tabbed panels (Thumbnails, Bookmarks, Tool Chest, Properties, Search, Studio, Markups List, Measurements, Layers, Spaces, Links, Sets, Signatures, Forms, File Access...). | Window > Panels | Hide Panels Shift+F4 | P0 | partial (Pages, Markups List docks) | UI, GS, WM |
| U-084 | Panel access bars | Icon strips beside the workspace that open panels; optional auto-hide. | Edges of workspace | - | P2 | missing | UI, WM |
| U-085 | Panel tab context menu | Right-click a panel tab: show another tab, hide this one, attach to left/right/bottom. | Panel tab right-click | - | P2 | partial (Qt dock float/move) | UI |
| U-086 | Split panels with position wheel | Drag a panel tab into a panel to stack it above/below/beside another tab so both show at once. | Drag panel tab | - | P2 | partial (Qt dock splitting) | UI |
| U-087 | Bottom panel overlap toggle | Let the bottom panel extend under the side panels to show more Markups List columns. | Panel border arrows | - | P2 | missing | UI |
| U-088 | Collapse panel by edge click | Click a panel's edge to slide it shut/open. | Panel edges | - | P2 | missing | UI |
| U-089 | Layout persistence | Panel and toolbar arrangement is restored on next launch. | Automatic | - | P0 | have | UI |
| U-090 | Document tabs | Each open PDF (and WebTab, Project) is a tab in the workspace; Ctrl+Tab cycles documents. | Workspace tabs | Ctrl+Tab / Ctrl+Shift+Tab | P0 | missing (single document) | KS, UI |
| U-091 | Tab truncation | Long tab names truncated at start or end (preference). | Preferences > General | - | P3 | missing | P-GEN |
| U-092 | Auto-hide tabs | Hide document tabs until the mouse reaches the top of the workspace. | Window > Auto-Hide Tabs | - | P3 | missing | WM |
| U-093 | Detach tab to window / monitor | Drag or right-click Detach a tab into its own window (e.g. second monitor); detached windows can split and sync. | Tab right-click > Detach | - | P1 | partial (right-click a tab > Detach to New Window, opens on a second screen when there is one; Reattach; no split / sync inside it yet; wave 20) | MV, GS |
| U-094 | Close / Close All | Close current tab or all tabs. | File / Window | Ctrl+F4 / Ctrl+Shift+W | P0 | partial (open replaces) | KS |
| U-095 | Context menu key | Open the right-click menu for the current selection from the keyboard. | - | Shift+F10 | P3 | missing | KS |
| U-096 | Always on Top | Keep the Revu window above other applications. | Window > Always on Top | Ctrl+F12 | P3 | missing | WM, KS |
| U-097 | WebTab | Browser tab inside Revu (favorites, GPU option, PDFs from links open in WebTabs or split). | Window > WebTab | Ctrl+T | P3 | missing | WM, P-WIN, KS |
| U-098 | File Access panel | Recent files list with previews, plus DMS integration. | Window > Panels > File Access | Alt+A | P1 | partial (recents with thumbnails, pinned files and folders; no DMS integration; wave 20) | P-INT, KS |
| U-099 | Revu menu | About, Preferences, Profiles, Keyboard Shortcuts, Administrator, Exit. | Revu menu | - | P1 | missing | RM |

## 6. Profiles and keyboard shortcut customization

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-100 | Profiles | Named interface configurations (toolbars, menus, panels, display settings) for different jobs; several are shipped (e.g. Takeoff, Construction, Design Review, Simple). | Revu > Profiles; profile button on navigation bar | - | P1 | missing | GS, PRF, NB |
| U-101 | Switch profile | Pick a profile from the list; the interface rearranges. | Revu > Profiles | - | P1 | missing | RM, NB |
| U-102 | Save profile | Save current toolbar/panel arrangement into the active profile. | Revu > Profiles > Save Profile | - | P2 | missing | TB |
| U-103 | Manage profiles | Create, rename, delete profiles. | Revu > Profiles > Manage Profiles | - | P2 | missing | PRF |
| U-104 | Export / import profile | Share a profile as a file (`.bpx`); optional "include dependencies" bundles tool sets, bookmark structures, hatch patterns and line styles. Double-click imports. | Manage Profiles | - | P2 | missing | PRF |
| U-105 | Keyboard Shortcuts dialog | List every command with its shortcut; select a command, type a keystroke, Add; Reassign steals a key already in use; menus update to show new keys. | Revu > Keyboard Shortcuts | - | P1 | missing | RM, KS |
| U-106 | Single-key tool shortcuts | Most markup/measure tools have single-letter or Shift+Alt+letter defaults (Select V, Pan Shift+V, Zoom Z, Lasso Shift+O). | Default keymap | various | P0 | partial (V,H,K,L,P,A,R,C; Shift+Alt+Q polylength) | KS |
| U-107 | Printable shortcut reference | Help PDF of default shortcuts, mouse map and modifier keys. | Help folder | - | P3 | missing | KS |

## 7. View modes, navigation, MultiView

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-108 | Fit Page / Fit Width / Actual Size | Standard zoom presets. | View | Ctrl+9 / Ctrl+0 / Ctrl+8 | P0 | have | VM, KS |
| U-109 | Zoom in / out keys | Step zoom. | Selection | Plus / Minus | P0 | have | KS |
| U-110 | Zoom tool / toggle zoom | Click to zoom in, Ctrl-click out, drag rectangle to zoom to area; Shift+Z toggles temporarily. | Navigation bar | Z / Shift+Z | P1 | have (Z / Shift+Z; wave 20) | NB, KS |
| U-111 | Page layout modes | Single Page, Continuous, Side by Side, Continuous Side by Side, with optional cover page alone. | View | Ctrl+4/5/6/7 | P1 | partial (Single Page Ctrl+4, Continuous Ctrl+5, Continuous Side by Side Ctrl+7; no plain Side by Side, no cover page; wave 20) | VM, KS |
| U-112 | Page navigation | First/previous/next/last page, page number box. | Navigation bar | Home / Ctrl+Left / Ctrl+Right / End | P0 | have | NB, KS |
| U-113 | Previous / next view | Back/forward through view history (page + zoom). | Navigation bar | Alt+Left / Alt+Right | P1 | missing | NB, KS |
| U-114 | Rotate view | Rotate the display 90 degrees without changing the file. | View > Rotate View | Ctrl+Shift+Plus / Minus | P2 | missing | VM, KS |
| U-115 | Refresh | Redraw the current view / document. | View | F5 | P3 | missing | KS |
| U-116 | Split vertical / horizontal (MultiView) | Split the workspace into panes, each showing any open document; can split repeatedly (up to 16). | View > Split | Ctrl+2 / Ctrl+H | P1 | missing | VM, MV, GS, KS |
| U-117 | Toggle split / switch / balance / unsplit | Flip split orientation, move tab to last split, equalize sizes, remove split. | View | Ctrl+I / Ctrl+1 / Shift+F12 / Ctrl+Shift+2 | P2 | missing | VM, KS |
| U-118 | Synchronize views | Linked pan/zoom across splits, either by page index (Document mode) or by relative position across different pages (Page mode). Key for comparing revisions. | View; status bar Sync | - | P1 | missing | VM, SB, P-GEN |
| U-119 | Full screen | Hide chrome, small floating toolbar, tabs on hover. | Window > Full Screen | F11 | P2 | missing | WM, KS |
| U-120 | Presentation mode | Slideshow of pages, arrow/page keys to advance, Esc exits; options in Preferences. | Window > Presentation | Ctrl+Enter | P3 | missing | WM, P-WIN, KS |
| U-121 | Dimmer | Fade the PDF content (0-100%) so markups stand out. | View > Dimmer; navigation bar | Ctrl+F5 | P1 | have (View > Dimmer Ctrl+F5 and a navigation bar button; amount 5-95% in View > Dimmer Amount; markups PDFium draws from their own appearance fade with the page; wave 20) | VM, P-ADV, KS |
| U-122 | Dark mode (workspace) | Dark color treatment of the drawing workspace (separate from the UI theme). | View > Dark Mode; navigation bar | - | P2 | missing | VM, NB |
| U-123 | Dark / light UI theme | Application chrome theme choice. | Preferences > General > Theme | - | P2 | missing | P-GEN |
| U-124 | Reply indicators | Optional on-page indicators on markups that have replies; hover previews them. | View > Always Show Reply Indicators | - | P3 | missing | VM |

## 8. Rulers, grid, snap, crosshair, line weights

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-125 | Rulers | Top/left rulers showing page bounds, cursor position, the extent of the selected markup, and snap position; scale with zoom. | View > Rulers | Ctrl+R | P2 | missing | VM, KS |
| U-126 | Ruler units | Right-click a ruler to choose inches, cm, mm, points or picas. | Ruler right-click | - | P3 | missing | VM |
| U-127 | Show grid | Draw a grid over the page. | View; status bar | Shift+F9 | P2 | missing | VM, SB, KS |
| U-128 | Snap to grid | Force markup points onto grid intersections even if grid is hidden. | View; status bar | Ctrl+Shift+F9 | P2 | missing | VM, SB, KS |
| U-129 | Grid spacing + units | Grid pitch and unit system (also drives rulers). | Preferences > General > Grid & Snap | - | P2 | missing | P-GEN |
| U-130 | Snap to content | Snap to vector linework of the drawing (endpoints, intersections, etc.; not text or images). Most important snap for takeoff. | View; status bar | Ctrl+Shift+F8 | P0 | have | VM, SB, KS |
| U-131 | Snap to markup | Snap to points on other markups and show alignment guidelines while drawing. | View; status bar | Ctrl+Shift+F7 | P1 | have (View > Snap to Markup, Ctrl+Shift+F7, status bar; no alignment guide lines) | VM, SB, KS |
| U-132 | Snap-to element filters | Choose which content/markup features (endpoints, midpoints, intersections, etc.) are snap targets. | Preferences > Grid & Snap > Snap to | - | P2 | missing | P-GEN |
| U-133 | Snap sensitivity | Snap capture radius (a 5-25 range). | Preferences > Grid & Snap | - | P2 | missing | P-GEN |
| U-134 | Snap indicator color | Color of the snap box; also the crosshair color. | Preferences > Grid & Snap | - | P3 | missing | P-GEN, VM |
| U-135 | Override snap with Ctrl | Hold Ctrl while placing a point to ignore all snaps. | While drawing | Ctrl | P1 | missing | MOD |
| U-136 | Full-screen crosshair | Replace cursor with lines spanning the workspace. | View; Preferences > General | - | P2 | missing | VM, P-GEN |
| U-137 | Disable line weights | Show all linework thin (screen-optimal) instead of PDF line weights; helps reading dense sheets. | View; Preferences > Advanced | - | P2 | missing | VM, P-ADV |
| U-138 | Enhance thin lines / fill anti-aliasing | Rendering aids that keep hairlines visible when zoomed out. | Preferences > Advanced > 2D Rendering | - | P3 | missing | P-ADV |

## 9. Preferences dialog (every page and its notable options)

Opened from Revu > Preferences (Ctrl+K). The online Revu 21 manual groups the pages into
General, Interface, Tools, Window, Sets, Studio, Import/Export, Advanced, Admin and
Integrations, with sub-tabs as listed. MarkupCraft has no Preferences dialog (only remembered
layout and last folder in QSettings).

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-139 | Preferences dialog | Single dialog with a page list on the left; settings persist per user. | Revu > Preferences | Ctrl+K | P1 | missing | RM, KS |
| U-140 | General > Options: user name | Author name stamped on markups and replies. | General > Options | - | P0 | missing | P-GEN |
| U-141 | General > Options: language, theme | UI language (restart) and Dark/Light theme. | General > Options | - | P2 | missing | P-GEN |
| U-142 | General > Options: startup | Startup mode (markup/view/last), open a PDF on start, reopen last session's files, open home WebTab, show recents, full screen with view mode, usage data, reset hidden messages. | General > Options | - | P2 | missing | P-GEN |
| U-143 | General > Document: recovery and save mode | Document crash recovery; default save mode (keep revisions / publish / publish compressed). | General > Document | - | P1 | partial (document recovery on/off and autosave; no save-mode choice; wave 20) | P-GEN |
| U-144 | General > Document: default layout and fit | Default page layout (four modes, document-based, auto by page size) and default fit for single and continuous modes; maximum zoom. | General > Document | - | P2 | missing | P-GEN |
| U-145 | General > Document: misc | Rotate all pages by default, auto-reorder bookmarks, redirect links on slip-sheet, locked-file prompt, auto-detect URLs, remember last page. | General > Document | - | P2 | missing | P-GEN |
| U-146 | General > Navigation: wheel = zoom or scroll | Separate wheel behavior for single-page and continuous modes. | General > Navigation | - | P1 | partial (wheel always zooms) | P-GEN |
| U-147 | General > Navigation: wheel tuning | Reverse zoom direction, zoom sensitivity per notch, horizontal tilt-wheel panning. | General > Navigation | - | P2 | missing | P-GEN |
| U-148 | General > Navigation: scrollbars | Horizontal / vertical scrollbars on or off, vertical on the left, lock panning in Fit Width. | General > Navigation | - | P3 | missing | P-GEN |
| U-149 | General > Navigation: sync, 3D mouse, accelerators | Default view synchronization and mode, 3D mouse support, Alt-menu accelerators. | General > Navigation | - | P3 | missing | P-GEN |
| U-150 | General > Grid & Snap | Units, show grid, grid spacing, snap to grid/content/markup, snap-to targets, sensitivity, snap color. | General > Grid & Snap | - | P1 | missing | P-GEN |
| U-151 | General > Spelling | Auto-complete with managed lists, spell check on/off, upper-case words, underline color, dictionary, custom words. | General > Spelling | - | P3 | missing | P-GEN |
| U-152 | Interface > File Access | Recents on/off, how many (0-100), preview, history age, clear; DMS/SharePoint behavior and configured DMS list. | Interface | - | P3 | missing | P-INT |
| U-153 | Interface > Markups List | Zoom to markup when selected, measurement value on group's dominant markup only, rich-text comments, wrap comments, exclude filtered markups from export, dim percent for filtered-out markups. | Interface | - | P1 | missing | P-INT |
| U-154 | Interface > Layers | Hide child layers with parent, list only layers on current page, Surface Dial behavior. | Interface | - | P3 | missing | P-INT |
| U-155 | Tools > Markup | Remember last properties per tool, reuse tools (stay in tool), autosize text boxes, scale grouped appearance, embed fonts, author/date in pop-ups, print pop-ups, copy highlighted text into comment, image encoding, drag behavior for shapes, snapshot DPI/vector/markups options. | Tools | - | P1 | missing | P-TLS |
| U-156 | Tools > Measure | Split counts by space, dynamic fill DPI / cursor sizes / speed / colors / edge sensitivity, hide markups during fill, legacy subject/label persistence. | Tools | - | P1 | missing | P-TLS |
| U-157 | Tools > Sketch | Rotation input relative vs absolute; ellipse sketch by width x height or radius. | Tools | - | P3 | missing | P-TLS |
| U-158 | Tools > Forms | Field highlight color/opacity; single-key shortcuts on/off. | Tools | - | P3 | missing | P-TLS |
| U-159 | Tools > Signature | Password timeout, digital ID and trusted identity folders, block changes that break signatures. | Tools | - | P3 | missing | P-TLS |
| U-160 | Window > Tablet | Tablet zoom, pen cursor, text highlighting, pen commit delay, ink copy, right-click lasso, eraser scales with zoom, pressure sensitivity, handwriting recognition, touch input mode. | Window | - | P3 | missing | P-WIN |
| U-161 | Window > Presentation | Loop, auto-advance interval, transition type/direction, background color, cursor visibility. | Window | - | P3 | missing | P-WIN |
| U-162 | Window > WebTab | GPU acceleration, switch to new WebTabs, open PDF links in WebTabs or split view, script errors, favorites. | Window | - | P3 | missing | P-WIN |
| U-163 | Sets | How set documents open (in place / new tab), relative paths, display, categories and templates, sort rules, revision filters and stacking, copy markups to new revision, stamp old revisions superseded, auto-tag discipline/revision from sheet number. | Sets | - | P2 | missing | P-SET |
| U-164 | Studio | Auto check-out on open, toolbar-only Studio access, force proxy, close project docs with tab, open project docs in split view, server list, sign in/out, notification settings, offline PIN. | Studio | - | P3 | missing | P-STU |
| U-165 | Import/Export | OCR handling during export, open after export, Word/Excel/PowerPoint reconstruction options, number separators, camera/video/image resolutions, image-to-PDF color space and DPI, TIFF compression and multi-page TIFF. | Import/Export | - | P3 | missing | P-IE |
| U-166 | Advanced > 2D Rendering | Rendering engine and mode (wait vs progressive), low-res preview while panning, enhance thin lines, disable line weights, fill anti-aliasing, blend modes, CMYK calibration, dimmer amount, max print DPI, screen DPI. | Advanced | - | P2 | missing | P-ADV |
| U-167 | Advanced > 3D Rendering | 3D engine, double-sided, occlusion hiding, fit on select, axis display, navigation mode, frame rate, transitions. | Advanced | - | P3 | missing | P-ADV |
| U-168 | Advanced > JavaScript / PDF/A | Enable document JavaScript, trusted documents/locations; PDF/A open-locked and conversion handling of annotations, transparency, attachments. | Advanced | - | P3 | missing | P-ADV |
| U-169 | Admin | Default PDF viewer, log folder, extended debugging, embedded browser engine, sign-in method, reset / backup / restore all settings, network stamp and email template folders. | Admin | - | P2 | missing | P-ADM |
| U-170 | Admin > settings backup/restore | Back up and restore all Revu settings (useful when moving machines). | Admin | - | P2 | missing | P-ADM |
| U-171 | Admin > AI assistant (MCP) connectors | Current manual lists one-click connections from Revu to external AI assistants over MCP. Note only. | Admin > MCP | - | P3 | missing | P-ADM |
| U-172 | Integrations | Third-party integration sign-in (field-forms service, by region). | Integrations | - | P3 | missing | P-INTG |

## 10. Mouse, gestures and modifier keys

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-173 | Middle-drag pan | Hold the wheel button and drag to pan in any tool. | Mouse | Middle drag | P0 | have | KS |
| U-174 | Middle double-click re-center | Double-click the wheel to re-center the view. | Mouse | Middle dbl-click | P2 | missing | KS |
| U-175 | Wheel zoom vs scroll | Wheel zooms or scrolls depending on the per-mode preference; holding Ctrl swaps the behavior. | Mouse | Wheel / Ctrl+wheel | P0 | partial (zoom only, no Ctrl toggle) | KS, MOD, P-GEN |
| U-176 | Spacebar temporary pan | Hold Space to pan mid-draw or mid-edit without losing the markup in progress; release to resume. | Keyboard | Space | P0 | have | KS, MOD |
| U-177 | Right-click context menus | Right-click a markup, page, text or panel item for its commands. | Mouse | Right click | P0 | partial (page and markup menus with registered feature items, e.g. Resume Count / Delete Count Item; right-click finishes a measurement while drawing; Markups List row and header menus; no text or other panel menus) | KS |
| U-178 | Right-drag multi-select | Right-button drag draws a selection box. | Mouse | Right drag | P2 | missing | KS |
| U-179 | Shift+drag multi-select; Shift+click add | Box-select or add individual markups to the selection. | Mouse | Shift+drag / Shift+click | P0 | have (wave 2) | KS, MOD |
| U-180 | Shift ortho / 45-degree lock | Hold Shift while drawing lines, polylines, polygons and measurements to lock horizontal, vertical or 45 degrees (pen/highlight: H/V only). | While drawing | Shift | P0 | partial (H/V only, no 45) | KS, MOD |
| U-181 | Shift: square / circle | Shift constrains rectangle to square, ellipse to circle, arc to circular. | While drawing | Shift | P1 | have (Rectangle / Ellipse / Area rectangle / Ellipse Cutout) | MOD |
| U-182 | Alt: draw from center / 3-point arc | Alt draws an ellipse from its center; Alt+clicks build a three-point arc. | While drawing | Alt | P2 | missing | MOD |
| U-183 | Shift-drag move in straight line | Constrain a markup move to horizontal/vertical. | While editing | Shift+drag | P1 | have (wave 2) | MOD |
| U-184 | Ctrl-drag copy; Ctrl+Shift-drag copy straight | Duplicate a markup by dragging with Ctrl (Ctrl+Shift keeps it on an axis). | While editing | Ctrl / Ctrl+Shift | P1 | have (wave 2) | MOD, KS |
| U-185 | Paste in place | Paste a copied markup at the same page position, e.g. onto another sheet. | Edit | Ctrl+Shift+V | P1 | have (wave 2) | MOD, KS |
| U-186 | Shift-click vertex add/delete | Shift-click a segment to add a vertex; Shift-click a vertex to remove it on polylines/polygons/areas. | While editing | Shift+click | P1 | have (polylines, polygons, measurements and their cutouts) | MOD |
| U-187 | Ctrl-click vertex to curve | Toggle a vertex between straight and arc; Ctrl on a handle moves it independently. | While editing | Ctrl+click | P3 | missing | MOD |
| U-188 | Rotation snap 15 degrees | Rotating with the top handle snaps to 15 degrees; Shift frees to 1 degree. | While editing | Shift | P2 | missing | KS, MOD |
| U-189 | Shift-drag measurement caption | Move a length/area/perimeter/volume label separately from its markup. | While editing | Shift+drag | P1 | have | KS, MOD |
| U-190 | Shift breaks aspect ratio | When resizing polyline/polygon/image markups from a corner. | While editing | Shift | P3 | missing | MOD |
| U-191 | Alt-drag callout as whole | Move a callout with all its parts. | While editing | Alt+drag | P3 | missing | MOD |
| U-192 | Ctrl-click link opens in background | Open a hyperlink or File Access entry in a background tab. | Mouse | Ctrl+click | P3 | missing | MOD |
| U-193 | Esc behavior | Cancels the point/markup in progress, ends multi-click tools, leaves presentation/full screen. | Keyboard | Esc | P0 | have (drawing); n/a (no full screen) | WM |
| U-194 | Enter / double-click to finish | Completes polyline/polygon/area-type tools. | Keyboard / mouse | Enter / dbl-click | P0 | have | - (MarkupCraft parity note) |
| U-195 | Reuse markup tool | Toggle: after placing a markup the tool stays active for the next one. | Status bar; Preferences > Tools | - | P0 | partial (tools stay active) | SB, P-TLS |
| U-196 | Tab / panel focus cycling | Ctrl+Tab cycles document tabs; dialog/panel fields follow normal Windows Tab order. (No other Tab-specific drawing behavior is documented in our sources.) | Keyboard | Ctrl+Tab | P2 | missing | KS |

## 11. Studio Sessions (real-time collaboration)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-197 | Studio panel | Panel for signing in to a Studio server and listing Projects and Sessions (filter mine/attended, sort by recent/name/ID/status, joined/not joined/deleted tabs). | Window > Panels > Studio | Alt+C | P2 | missing | STP, KS |
| U-198 | Start a Session | Name it, add PDFs (or all open files), upload them to the server, then invite. | Studio > Add > New Session | - | P2 | missing | SHOST |
| U-199 | Invite attendees | Invite by typed/pasted emails, Studio groups or address book; invitations are emailed by the server; copy invitation text; send reminders. | Studio panel > Invite | - | P2 | missing | SHOST, STS |
| U-200 | Join a Session | Join by Session ID or from the Not Joined list. | Studio > Add > Join | - | P2 | missing | STP |
| U-201 | Real-time markup sync | All attendees mark up the same PDFs; each markup appears for everyone, tagged with author; works asynchronously too. | Session | - | P2 | missing | GS, SHOST |
| U-202 | Attendee list | Joined / not joined attendees; host tools for access and invite. | Studio panel in Session | - | P2 | missing | STS |
| U-203 | Follow attendee / request follow | Mirror another attendee's view and cursor; host can ask everyone to follow them (also by typing a chat command). | Attendee list | - | P3 | missing | STS, SHOST |
| U-204 | Filter / select by attendee | Dim everyone else's markups, or select all of one attendee's markups. | Attendee right-click | - | P2 | missing | STS |
| U-205 | Record | Running log of chat, joins/leaves, and every markup add/change/delete with attendee and time; filterable by type; entries copyable. | Studio panel > Record | - | P2 | missing | STS |
| U-206 | Chat | Message field at the bottom of the Record; messages become Record entries. | Studio panel > Record | - | P3 | missing | STS |
| U-207 | Markup alerts / notifications | Notifications tab and email notifications when others add markups. | Studio panel > Notifications | - | P3 | missing | STS |
| U-208 | My status | Set your status in the Session (custom statuses allowed). | Session menu | - | P3 | missing | STS |
| U-209 | Session documents | Documents download in order; hover preview; add/delete documents (permission-gated). | Studio panel > Documents | - | P2 | missing | STS, SHOST |
| U-210 | Session permissions | Per user, group and everyone; permissions such as Add Documents and Full Control; individual beats group beats general. | Session Settings > Permissions | - | P3 | missing | SPERM |
| U-211 | Private session / block attendee | Restrict to invited users; block someone and decide what happens to their markups. | Attendee Access | - | P3 | missing | SHOST |
| U-212 | Session expiration | Optional end date with reminder emails at 7 days, 2 days and 24 hours. | Session Settings | - | P3 | missing | SPERM |
| U-213 | Session report | PDF record summary, or a package/combined PDF with all documents, with hyperlinks from each record line to its markup; options for attendees, documents, record columns. | Record > Report | - | P2 | missing | SHOST |
| U-214 | Finish Session | Host chooses whose markups to keep and saves files back over the originals (or to a folder, or discards). | Session menu > Finish | - | P2 | missing | SHOST |
| U-215 | Session lifecycle | Sessions are color-coded by state and are archived/deleted after inactivity; deleted items can be restored. | Studio panel | - | P3 | missing | STP |

## 12. Studio Projects, Bluebeam Cloud, plugins, scripting

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| U-216 | Studio Projects | Cloud (or on-prem) file store for any file type; opens in its own tab with folders, thumbnails/list view, search, sort. | Studio panel > Projects | - | P2 | missing | STPR, GS |
| U-217 | Project upload / folders | New folder, upload files, upload folder (permission-gated). | Project tab > Add | - | P2 | missing | STPR |
| U-218 | Check out / check in | Lock a file for editing and check it back in; auto check-out on open is a preference. | Project file actions | - | P2 | missing | STPR, P-STU |
| U-219 | Revision history / restore | View and restore earlier versions of a project file. | Project file actions | - | P2 | missing | dashboard nav: revision-history |
| U-220 | Project permissions and invites | Invite users, groups, folder-level permissions, shared links, notification settings. | Project Settings | - | P3 | missing | STPR |
| U-221 | Offline sync | Tag files/folders for sync, work offline, then check in or update server copy; conflict handling. | Project > Sync | - | P3 | missing | SOFF |
| U-222 | Add project PDFs to a Session | Bring project files into a live markup Session. | Project file actions | - | P3 | missing | dashboard nav |
| U-223 | Bluebeam Cloud integration (high level) | Web/mobile companion under the same sign-in: open/save cloud project files from Revu, markups shared across desktop, browser and tablet, plus field workflows (punch, RFIs, submittals). | Sign-in; File open/save | - | P2 | missing | CLD |
| U-224 | DMS / SharePoint integration | Open and save through document management systems from a toolbar or file dialogs; batch check-in. | Document Management toolbar; Preferences > Interface | - | P3 | missing | P-INT |
| U-225 | CAD and Office plugins (high level) | Add-ins that publish PDFs (with layers, metadata, batch) from AutoCAD (and verticals), Revit, Navisworks, SolidWorks, and Word/Excel/PowerPoint/Outlook. | Inside host apps | - | P3 | missing | PLG, HELP |
| U-226 | Scripting (high level) | Command-line/batch script engine with a script editor and reference for automating Revu operations on files; document JavaScript also supported. | Script editor; Preferences > Advanced > JavaScript | - | P3 | missing | HELP, P-ADV |

---

## Biggest gaps (top 10 for an estimator / PM)

1. **Snap to content (U-130) with Ctrl override (U-135).** Every takeoff click in Revu lands on
   real linework; MarkupCraft points are free-floating. This is the biggest accuracy and speed gap.
2. **Spacebar temporary pan and Ctrl+wheel toggle (U-176, U-175).** Daily muscle memory for
   Revu users; panning mid-polyline without losing the markup is expected.
3. **Multi-select and copy/move modifiers (U-179, U-183, U-184, U-185).** Shift-select, Ctrl-drag
   copy and paste-in-place are how repeated takeoff items get placed fast.
4. **Text Search panel with results list and Apply Count (U-042 to U-052).** Finding every tag
   or room name across a set, then counting the hits, is a core quantity workflow.
5. **Visual Search with count-checked (U-055 to U-061).** Auto-counting symbols (diffusers,
   fixtures, VAVs) is the most valuable "smart" feature for an HVAC estimator.
6. **Overlay Pages with 3-point manual align (U-024 to U-033).** The standard way to see what
   changed between bid sets; vector-friendly and simpler to build than raster compare.
7. **Compare Documents with change clouds (U-001 to U-016) and batch matching (U-017 to U-023).**
   Revision tracking on addenda; the result is just cloud markups, which fits MarkupCraft's model.
8. **Document tabs, split view and synchronized views (U-090, U-093, U-116, U-118).** Reviewing a
   revised sheet next to the old one, or plan next to schedule, needs more than one document open.
9. **Properties toolbar, right-click context menus and Preferences dialog (U-079, U-177, U-139
   with U-140 author, U-146 wheel mode, U-150 grid/snap, U-153 Markups List options).** Basic
   configurability Revu users expect on day one.
10. **Spaces (U-062 to U-066).** Grouping takeoff by room/zone/floor feeds directly into pricing
    breakouts; low cost on top of the existing markup model.

Collaboration (Studio Sessions/Projects, Cloud) is broad but P2-P3 for v1; a file-based
markup merge (import/export markups) would cover most of the practical need before any server.
