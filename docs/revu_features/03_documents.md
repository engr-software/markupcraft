# Revu 21 feature inventory, part 3: Documents, pages and files

Clean-room inventory written from public documentation only. Behaviour is described in our own words.
No Revu binaries, decompiled material or running Revu were used.

Scope: opening and tabs, views and navigation, Thumbnails, page labels, bookmarks, page operations,
creating and combining PDFs, batch tools, Sets, headers/footers, stamps and watermarks, OCR, search,
forms, digital signatures, security and redaction, print, flatten, file size, export, properties and
attachments, links, recent files, File Access, Studio (summary only), 3D (basics only).
Compare/Overlay, markup tools and the Markups List are covered in parts 1, 2 and 4; they appear here only as pointers.

Priority is for a construction estimator / PM: P0 daily, P1 weekly, P2 occasional, P3 rare.
MarkupCraft column reflects the app on 2026-10-07 (one PDF open, zoom/fit/actual size, wheel zoom, pan,
page up/down, Home/End, Pages panel with thumbnails and page labels, save / save as, markup CSV export, undo/redo).

**Source keys**

All `H:` keys are pages under `https://support.bluebeam.com/online-help/revu21/Content/RevuHelp/`.

| Key | Source |
|---|---|
| KS | Installed `Help\KeyboardShortcuts.pdf` (English). Shortcuts were re-paired with their command names by word x/y position (PyMuPDF), so pairings are reliable. |
| GS | Installed `Help\Getting Started.pdf` |
| DOC | H: Menus/Document/Document-Menu--M.htm |
| BAT | H: menus/batch/batch-menu.htm |
| PAN | H: Menus/Window/Panels/Tabs-Intro--MV.htm |
| FILE | H: Menus/File/File-Menu--MV.htm |
| FTB | H: Menus/Tools/Toolbars/File/File-Toolbar--V.htm |
| VIEW | H: Menus/View/View-Menu--MV.htm |
| NAV | H: Menus/Tools/Toolbars/Navigation-bar/Navigation-Bar--MTV.htm |
| MV | H: Unsorted/MultiView--MTV.htm |
| GEN | H: Menus/Revu/Preferences/General--MV.htm |
| INTF | H: Menus/Revu/Preferences/Interface-Preferences--MV.htm |
| THM | H: Menus/Window/Panels/Thumbnails/Thumbnails-Tab--MTV.htm |
| PLB | H: Menus/Window/Panels/Thumbnails/Page-Labels-Page-Numbering.htm |
| BMT | H: Menus/Window/Panels/Bookmarks/Bookmarks-Tab--MV.htm |
| BMW | H: Menus/Window/Panels/Bookmarks/Working-with-Bookmarks--MT.htm |
| BMS | H: Menus/Window/Panels/Bookmarks/Bookmarks-Structure.htm |
| INS | H: Menus/Document/Page-Setup_Manipulation/Insert-Pages--MT.htm |
| EXT | H: Menus/Document/Page-Setup_Manipulation/Extract-Pages--M.htm |
| REP | H: Menus/Document/Page-Setup_Manipulation/Replace-Pages--MT.htm |
| DEL | H: Menus/Document/Page-Setup_Manipulation/Delete-Pages--MT.htm |
| ROT | H: Menus/Document/Page-Setup_Manipulation/Rotate-Pages--T.htm |
| SPL | H: Menus/Document/Page-Setup_Manipulation/Split-Document.htm |
| DSK | H: Menus/Document/Page-Setup_Manipulation/Deskewing.htm |
| BCP | H: Menus/Batch/Crop-Setup/Batch-Crop-and-Page-Setup.htm |
| CMB | H: Menus/File/Combine/Combine-PDFs--MT.htm |
| CRF | H: Menus/File/Create/Create-PDF-from-File.htm |
| SCN | H: Menus/File/Create/Create-a-PDF-from-Scanner.htm |
| LAY | H: Menus/File/Create/Creating-a-New-Layered-PDF.htm |
| 3D | H: Menus/File/Open/Create-3D-PDF.htm |
| BLNK | H: Menus/Batch/Link/Batch-Link--T.htm |
| SLIP | H: Menus/Batch/Slip-Sheet/Batch-Slip-Sheet--T.htm |
| BSS | H: Menus/Batch/Sign-Seal/Batch-Sign-and-Seal.htm |
| BST | H: Menus/Batch/Stamp/Batch-Stamp.htm |
| SUM | H: Menus/Batch/Summary/Batch-Summary--MT.htm |
| BPRN | H: Menus/Batch/Print/Batch-Printing-from-Revu--V.htm |
| BHF | H: Menus/Batch/Headers/Batch-Headers-Footers.htm |
| HF | H: Menus/Document/Headers/Headers-and-Footers--M.htm |
| SETP | H: Menus/Window/Panels/Sets/Sets-Tab--V.htm |
| SETW | H: Menus/Window/Panels/Sets/Working-with-Sets--TV.htm |
| STMP | H: Menus/Tools/Stamp/Stamp-Tool--MV.htm |
| WM | support.bluebeam.com/en-us/revu/how-to/tips-and-tricks/revu-21-revu-add-a-watermark-to-your-pdfs.html (search summary) |
| OCR | H: Menus/Document/OCR/OCR.htm |
| SRCH | H: Menus/Window/Panels/Search/Search--MTV.htm |
| FORM | H: Menus/Tools/Form/Form-Menu.htm, Form-Fields.htm, Creating-Forms.htm |
| SIGP | H: Menus/Window/Panels/Signatures/Signatures-Tab--V.htm |
| CERT | H: Menus/Tools/Signature/Certifying-a-Document.htm |
| DID | H: Menus/Tools/Signature/Creating-a-New-Digital-ID--V.htm |
| SEC | H: Menus/Document/Security/Security%20-%20M.htm |
| RED | H: Menus/Edit/PDF-Content/Redaction.htm |
| PRN | H: Menus/File/Printing/Printing--MV.htm |
| FLT | H: Menus/Document/Flatten/Flatten-Markups--MT.htm |
| RFS | H: Menus/Document/Reduce-Repair/Reduce-File-Size--M.htm |
| REV | H: Menus/File/Save/Revisions--V.htm |
| PDFA | H: Menus/Document/PDFA/Viewing-and-Editing-PDF-A.htm |
| EXP | H: Menus/File/Export/Exporting-Documents--T.htm (via the /se/ mirror; same page) |
| PROP | H: Menus/Window/Panels/Properties/Properties--MV.htm |
| ATT | H: Menus/Tools/Attachments/Attachment--MV.htm |
| LNK | H: Menus/Window/Panels/Links/Links-Tab.htm and Working-with-Hyperlink-Places.htm |
| HYP | H: Menus/Tools/Hyperlink/Hyperlink-Tool--MT.htm |
| FA | H: Menus/Window/Panels/File-Access/ (File-Access-Tab--MTV, ...Recent-Files-Mode--TV, ...Explorer-Mode--TV) |
| STU | H: Menus/Window/Panels/Studio/Studio--MTV.htm |

## Opening, tabs and multiple documents

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-001 | Open PDF | Open-file dialog; can select several PDFs at once, each opening in its own tab. | File > Open; File toolbar | Ctrl+O | P0 | have (several files, one tab each) | FILE, KS |
| D-002 | Open images as PDF | TIF, JPG, PNG, GIF and BMP files can be opened and are converted to PDF pages on the way in. | File > Open | Ctrl+O | P2 | missing | FILE |
| D-003 | Drag-and-drop open | Dropping files onto the workspace opens them; dropping onto Thumbnails inserts pages instead (see Insert Pages). | Workspace / Thumbnails | - | P0 | have (workspace: every PDF in its own tab; Thumbnails: insert) | INS, 3D |
| D-004 | Document tabs | Every open file has a tab above the workspace; many documents stay open at once. | Workspace tab bar | - | P0 | have | MV, GEN |
| D-005 | Next / previous document | Cycles through the open document tabs. | Navigation commands | Ctrl+Tab / Ctrl+Shift+Tab | P1 | have | KS |
| D-006 | Close / Close All | Closes the active tab, or every tab. | File > Close / Close All | Ctrl+F4 / Ctrl+Shift+W | P0 | have | FILE, KS |
| D-007 | Save / Save As | Writes the active PDF; Save As asks for a new name, type and folder. | File menu | Ctrl+S / Ctrl+Shift+S | P0 | have | FILE, KS |
| D-008 | Save All | Saves every open file that has unsaved changes. | File > Save All | Shift+F2 | P1 | have | FILE, KS |
| D-009 | Tab name truncation | Preference: long file names in tabs are cut from the start or the end. | Preferences > General | - | P3 | missing | GEN |
| D-010 | Reopen last session | Preference: reopen the files that were open when Revu last closed. | Preferences > General | - | P1 | have (File > Startup and Recovery > Reopen Last Session at Startup; on by default; wave 20) | GEN |
| D-011 | Startup options | Preference: startup mode (markup/view/last used), a PDF to open on start, show recent files on start. | Preferences > General | - | P3 | missing | GEN |
| D-012 | Remember last page | Preference: reopen each PDF at the page and view layout it had when closed. | Preferences > General | - | P1 | have (page, zoom and continuous / side-by-side layout per file, restored on open; option; wave 20) | GEN |
| D-013 | Document recovery | Preference: recover unsaved edits after a crash on the next launch. | Preferences > General | - | P1 | have (unsaved markup state autosaved every 2 min in the background to the app data folder; offered back after a crash, saves to the original; wave 20) | GEN |
| D-014 | Locked-file prompt | When a file is in use elsewhere, offer to open a read-only copy. | Preferences > General | - | P2 | missing | GEN |
| D-015 | Open Recent | Menu list of recently opened PDFs for one-click reopening. | File > Open Recent | - | P0 | have | FILE |
| D-016 | New blank PDF / from template | Create an empty PDF, or one from a saved page template (templates can be managed). | File > New PDF / New PDF from Template | - | P2 | missing | FILE, FTB |
| D-017 | PDF Package | Create an empty PDF Package (portfolio container for several files). | File > Create > PDF Package | - | P3 | missing | FILE |
| D-018 | Email PDF | Opens a new mail message with the saved current PDF attached; email templates can prefill it. | File > Email / Email Templates | Ctrl+E | P2 | missing | FILE, KS |
| D-019 | Save modes and revisions | Preference: keep every saved revision inside the PDF, publish without revisions (default), or publish compressed. | Preferences > General > Document | - | P3 | missing | GEN, REV |
| D-020 | Revert As | Saves an earlier stored revision of the PDF out as a new file (needs Maintain Revisions). | File > Revert As | - | P3 | missing | REV, FILE |
| D-021 | Publish As | Strip revision history on save: flattened (markups burned in), compressed 1.5, or uncompressed. | File > Publish As | Ctrl+Shift+P (Compressed 1.5); Ctrl+Alt+F (Flattened) | P2 | missing | FILE, REV, KS |
| D-022 | Refresh | Re-render the view (F5) or reload the document from disk (Shift+F5). | View / Document | F5 / Shift+F5 | P3 | missing | KS |
| D-023 | Web Tab | Opens a web page in a Revu tab; PDF hyperlinks can open there. | View | Ctrl+T | P3 | missing | KS, HYP |
| D-024 | PDF/A awareness | PDF/A files show an icon on their tab and open locked for editing until unlocked; Verify reports compliance. | Document tab icon; Properties > Standards | - | P3 | missing | PDFA |

## Page navigation and zoom

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-025 | Next / previous page | Step one page forward or back. | Navigation bar; View | Ctrl+Right / Ctrl+Left | P0 | have (also PgUp/PgDn) | NAV, KS |
| D-026 | First / last page | Jump to the first or last page. | Navigation bar | Home / End | P0 | have | NAV, KS |
| D-027 | Page number box | Shows the current page; type a number or label to jump. | Navigation bar | - | P0 | have (number; labels not yet) | NAV |
| D-028 | Previous / next view | Browser-style back and forward through view history (page plus zoom and position). | Navigation bar; View | Alt+Left / Alt+Right | P0 | have | NAV, KS |
| D-029 | Fit Page / Fit Width / Actual Size | Standard zoom presets. | View; Navigation bar | Ctrl+9 / Ctrl+0 / Ctrl+8 | P0 | have | VIEW, KS |
| D-030 | Zoom in / out | Step zoom. | Selection commands | Plus / Minus | P0 | have | KS |
| D-031 | Zoom tool | Drag a rectangle to fill the screen with that area; Ctrl-click zooms out; Shift+Z toggles it temporarily. | Navigation bar | Z / Shift+Z | P1 | have (Z: click in, Ctrl+click / right-click out, drag a rectangle; Shift+Z toggles; wave 20) | NAV, KS |
| D-032 | Mouse wheel behaviour | Per layout mode the wheel either zooms or scrolls; Ctrl swaps zoom/pan; reverse direction and sensitivity are settable. | Preferences > General > Navigation | Ctrl (hold) | P1 | partial (wheel zooms only) | GEN, KS |
| D-033 | Pan with mouse | Middle-button drag pans; middle double-click recentres; Spacebar held pans without dropping the markup in progress. | Mouse | Spacebar / middle button | P0 | partial (middle drag + H tool; no Spacebar, no recentre) | KS |
| D-034 | Pan tool | Dedicated hand tool. | Navigation bar | Shift+V | P0 | have (H) | NAV, KS |
| D-035 | Maximum zoom | Preference capping the zoom percentage. | Preferences > General | - | P3 | missing | GEN |
| D-036 | Horizontal wheel / scrollbars | Tilt-wheel panning; show or hide horizontal and vertical scrollbars, optionally on the left. | Preferences > General > Navigation | - | P3 | missing | GEN |
| D-037 | Lock panning in Fit Width | Constrain panning to vertical while in Fit Width so the sheet does not drift sideways. | Preferences > General > Navigation | - | P3 | missing | GEN |
| D-038 | Page layout modes | Single Page, Continuous (one column), Side-by-Side and Continuous Side-by-Side, with optional cover page shown alone. | View; Navigation bar (One Full Page / Scrolling Pages) | Ctrl+4 / Ctrl+5 / Ctrl+6 / Ctrl+7 | P1 | partial (Single Page, Continuous, Continuous Side-by-Side Ctrl+7; no plain Side-by-Side, no cover page alone) | VIEW, NAV, KS |
| D-039 | Default layout rule | Preference: open every PDF in a chosen layout, or by document type, or by page size (small pages continuous, drawings single). Default zoom per mode too. | Preferences > General > Document | - | P3 | missing | GEN |
| D-040 | Rotate view | Turn the display 90 degrees clockwise or counterclockwise without changing the file. | View > Rotate View | Ctrl+Shift+Plus / Ctrl+Shift+Minus | P1 | have | VIEW, ROT, KS |
| D-041 | Full screen / Presentation | Hide chrome for full-screen viewing; separate presentation mode. Option to go full screen in View Mode. | Window | F11 / Ctrl+Enter | P2 | missing | KS, GEN |
| D-042 | Always on top | Keep the Revu window above other windows. | Window | Ctrl+F12 | P3 | missing | KS |
| D-043 | Hide panels / toolbars | Toggle all side panels, the menu bar, navigation bar and status bar. | Window | Shift+F4 / F9 / F4 / F8 | P2 | partial (dock toggles) | KS, NAV |
| D-044 | Navigation bar page info | Shows the page's print size and its scale ("Scale Not Set" if none); clicking the scale starts calibration. | Navigation bar | - | P0 | partial (scale box on toolbar) | NAV |
| D-045 | Rulers | Top and left rulers in inches, cm, mm, points or picas; show page bounds, selected-object extent and pointer position. | View > Rulers | Ctrl+R | P2 | missing | VIEW, KS |
| D-046 | Full-screen crosshair | Replace the cursor with lines spanning the workspace. | View; Preferences > General | - | P2 | missing | VIEW, GEN |
| D-047 | Dimmer | Fade the page content by a percentage so markups stand out. | View > Dimmer; Navigation bar | Ctrl+F5 | P2 | missing | VIEW, NAV, KS |
| D-048 | Disable line weights | Draw all PDF lines at a screen-optimal thickness instead of their real weights. | View | - | P2 | missing | VIEW |
| D-049 | Dark mode / theme | Dark colour scheme for the workspace; Light/Dark app theme. | View; Navigation bar; Preferences | - | P3 | missing | VIEW, GEN |

## Split views, sync and second monitor (MultiView)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-050 | Split vertical / horizontal | Divide the workspace into independent panes (up to 16), each with its own tabs; same or different files. | View; Navigation bar | Ctrl+2 / Ctrl+H | P1 | missing | MV, VIEW, KS |
| D-051 | Unsplit | Remove the current split. | View; Navigation bar | Ctrl+Shift+2 | P1 | missing | MV, KS |
| D-052 | Toggle split / Switch / Balance | Flip a split between vertical and horizontal; move the active tab to the last-used split; equalise split sizes. | View | Ctrl+I / Ctrl+1 / Shift+F12 | P2 | missing | VIEW, KS |
| D-053 | Synchronize Document | Linked panes follow by page index and pan/zoom together (page 1 with page 1). | View; Status bar Sync | - | P1 | missing | MV, VIEW, GEN |
| D-054 | Synchronize Page | Linked panes pan/zoom together regardless of page number (compare sheet A-101 with A-201). | View; Status bar Sync | - | P1 | missing | MV, VIEW, GEN |
| D-055 | Move tab between splits | Drag a document tab to another pane's tab bar; an arrow marks the drop spot. | Tab bar | - | P2 | missing | MV |
| D-056 | Detach tab (dual screen) | Drag a tab out (or right-click > Detach) to make a floating window for a second monitor; Ctrl-drag leaves a copy in the main window. Floating windows hold tabs, split, sync and stay on top. | Tab bar | - | P1 | partial (tab right-click / Window > Detach: the document in its own window, second screen if any, panels and shortcuts follow it, Reattach or close puts it back; no drag-out, no Ctrl-drag copy, one document per window; wave 20) | MV |

## Thumbnails panel

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-057 | Thumbnail list | Page previews for jumping around a set; keyboard arrows move between them. | Window > Panels > Thumbnails | Alt+T | P0 | have | THM, KS |
| D-058 | Thumbnail size slider | Slider at the panel bottom grows or shrinks thumbnails. | Thumbnails panel | - | P2 | missing | THM |
| D-059 | Label display toggles | Choose to show the page label and/or the page scale under each thumbnail. | Thumbnails > Labels | - | P1 | partial (label always shown, no scale) | THM |
| D-060 | Multi-select pages | Shift/Ctrl select several thumbnails so page commands apply to all of them. | Thumbnails panel | Shift/Ctrl+click | P1 | have | THM |
| D-061 | Reorder by drag | Drag a thumbnail to a new position to reorder pages; optional bookmark reordering to match. | Thumbnails panel; Preferences | - | P1 | have (bookmarks follow their pages) | THM, GEN |
| D-062 | Cut / copy / paste pages | Move or duplicate pages within or between documents via the thumbnail menu. | Thumbnails menu | - | P2 | missing | THM |
| D-063 | Copy page to snapshot | Copy the current page as a snapshot image for pasting. | Thumbnails menu; Edit | Ctrl+Alt+C | P3 | missing | THM, KS |
| D-064 | Set scale from thumbnails | Double-click the scale label, or right-click one or many thumbnails, to open the apply-scale dialog for those pages. | Thumbnails context menu | - | P0 | partial (scale per page via toolbar) | THM |
| D-065 | Page commands from Thumbnails | The panel menu repeats page commands: page setup, stamp, insert blank/insert/extract/replace/delete/rotate, number pages, re-label, export, email, print, repair, flatten, markup summary. | Thumbnails menu / right-click | - | P1 | partial (rotate, insert blank / pages, extract, delete) | THM |
| D-066 | Drop files to insert | Dragging PDFs from Explorer into Thumbnails opens Insert Pages with those files. | Thumbnails panel | - | P2 | have | INS |

## Page labels

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-067 | Read page labels | PDF page labels are shown under thumbnails and used throughout (navigation, naming, matching). | Thumbnails | - | P0 | have | PLB |
| D-068 | Rename label inline | Edit a thumbnail's label in place; Tab / Shift+Tab move to the next / previous label while staying in edit mode. | Thumbnails > Rename Page Label; double-click label | F2 | P1 | have (F2 or double-click a thumbnail; Tab / Shift+Tab move on while editing; one undo step; wave 20) | PLB |
| D-069 | Number Pages | Dialog sets numbering style, prefix and start number for a page range (all, current, selected, custom list like 1-3,5,9). | Document > Number Pages; Thumbnails | - | P2 | missing | PLB, DOC |
| D-070 | Text-only label / clear labels | Style None plus a prefix gives a plain text label; style None with empty prefix removes labels. | Number Pages dialog | - | P2 | missing | PLB |
| D-071 | Labels from bookmarks | Each page that has a bookmark takes that bookmark's title as its label. | Document > Create Page Labels; Thumbnails | - | P1 | have (Document > Page Labels > Create from Bookmarks, with a page range; wave 20) | PLB, DOC |
| D-072 | Labels from page region (AutoMark) | Drag one or more boxes over the title block (sheet number, title); text found there on every page becomes the label. Extra literal text can go before, between or after regions, with a live preview. | Create Page Labels > Page Region | - | P1 | missing | PLB, BMW |

## Bookmarks

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-073 | Bookmarks panel | Tree of the PDF's bookmarks with expand/collapse; clicking runs the bookmark's action; a marker shows bookmarks on the current page. | Window > Panels > Bookmarks | Alt+B | P0 | have | BMT, KS |
| D-074 | Add bookmark | Adds a bookmark to the current view, named after the page label and immediately editable; Add Before / After / Child variants. | Bookmarks toolbar; right-click > Add | Ctrl+B | P1 | partial (Add, Add Child; no Before / After variants) | BMW, KS |
| D-075 | Rename / delete | Rename by double-click, F2 or menu; delete with Delete key or menu. | Bookmarks panel | F2 / Del | P1 | have | BMW |
| D-076 | Reorder, nest and copy | Drag to reorder (multi-select allowed), drag inward to make a child, Ctrl-drag to copy. | Bookmarks panel | - | P1 | partial (drag reorder / nest; no Ctrl-drag copy) | BMW |
| D-077 | Bookmark properties | Title, text colour and bold/italic, for one or many bookmarks at once. | Right-click > Properties | - | P3 | missing | BMW |
| D-078 | Bookmark actions | Six target types: page in an open PDF with zoom (fit page/width/actual/inherit), Place, Space, snapshot rectangle, URL, open a file (relative or full path). "Set to Current View" re-targets quickly. | Right-click > Action | - | P2 | missing | BMW |
| D-079 | Create bookmarks automatically | Generate bookmarks from page labels, or from title-block regions via AutoMark, for a page range. | Bookmarks > Create Bookmarks | - | P1 | partial (from page labels; no AutoMark) | BMT, BMW |
| D-080 | Bookmark structures | Saved bookmark-tree templates (e.g. discipline folders with sheet entries) used to file new bookmarks into a standard hierarchy. | Bookmarks > Structures > Manage Structures | - | P3 | missing | BMS, BMT |
| D-081 | Audit bookmarks | Finds bookmarks pointing to pages no longer in the file and flags them with a broken icon. | Bookmarks > Audit Bookmarks | - | P3 | missing | BMW |
| D-082 | Export bookmarks | Bookmark report for one or many PDFs to PDF or CSV, as tree or flat index, top level or nested, optional hyperlinks, date/time stamp, page size, output folder and suffix. | Bookmarks > Export Bookmarks | - | P2 | missing | BMW |
| D-083 | Save collapse state | Stores the current expand/collapse state as the file's default. | Bookmarks menu | - | P3 | missing | BMT |
| D-084 | Bookmarks from source apps | PDFs made by the Office/CAD plugins get bookmarks from TOCs, sheet tabs, slides or CAD layouts. | (creation side) | - | P3 | missing | BMT |

## Page operations

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-085 | Insert pages from PDF | Pull pages from one or more PDFs (open or on disk, sortable, per-file page ranges) into the active PDF before/after first, last or a given page. | Document > Insert > Pages from Document | Ctrl+Shift+I | P1 | partial (one PDF, page range, before / after a page) | INS, DOC, KS |
| D-086 | Insert options | Carry over bookmarks and attachments, merge document properties, keep layers, use file names as page labels, interleave pages (odd/even rejoin). | Insert Pages dialog | - | P2 | missing | INS |
| D-087 | Insert blank page | Insert N blank pages from a template or custom width/height, portrait/landscape, optional grid style, at a chosen position; can be saved as default. | Document > Insert > Blank Page | Ctrl+Shift+N | P2 | partial (count, standard sizes, orientation, position; no grid / template) | INS, KS |
| D-088 | Insert layered pages | Insert another PDF's pages as new layers on existing pages. | Document > Insert > Layered Pages | - | P3 | missing | DOC |
| D-089 | Insert from scanner/camera | Acquire images and add them as pages (OCR offered). | Document > Insert > From Scanner/Camera | - | P3 | missing | DOC, SCN |
| D-090 | Extract pages | Copy a page range into a new PDF or one file per page; options: delete after extracting, name files by page label, overwrite, open after, relative-path link update. | Document > Extract Pages | Ctrl+Shift+X | P1 | partial (range to one file, delete after, open after) | EXT, KS |
| D-091 | Replace pages | Swap given pages for pages from another PDF; "content only" keeps the old page's markups and links (the slip-sheet case). | Document > Replace Pages | Ctrl+Shift+Y | P1 | have (Document > Replace Pages, Ctrl+Shift+Y: page range, source start page, content only keeps markups and links; undoable; wave 20) | REP, KS |
| D-092 | Delete pages | Remove a page range or the selected thumbnails. | Document > Delete Pages; Thumbnails right-click | Ctrl+Shift+D | P1 | have | DEL, KS |
| D-093 | Rotate pages (dialog) | Permanently rotate pages 90/180 degrees across one or many files, filtered by all/current/even/odd/landscape/portrait. | Document > Rotate Pages; Batch > Rotate Pages | Ctrl+Shift+R | P1 | partial (page range, 90 / 180 / 270; no even / odd / orientation filters) | ROT, KS |
| D-094 | Rotate page quick buttons | One-click CW/CCW rotation of the current page, or all pages if "Rotate all Pages by Default" is on. | Rotation toolbar | Shift+Alt+Plus / Shift+Alt+Minus | P1 | have (selected pages) | ROT, GEN, KS |
| D-095 | Split document | Break PDFs into files by page count, file size (MB) or top-level bookmarks; naming by prefix/suffix with # placeholder or by bookmark name; optional subfolder, link update, drop markup-free layers. | Document > Split Document; Batch > Split | - | P2 | missing | SPL, BAT |
| D-096 | Crop pages | Set crop-box margins (top/bottom/left/right, or proportional); the page shrinks to the crop. | Document > Crop Pages; Batch > Crop & Page Setup | Shift+Alt+O | P2 | missing | BCP, DOC, KS |
| D-097 | Page setup / resize | Change media size (standard or custom, orientation), auto-scale content to it, manual scale, X/Y offset, rotation (or from a drawn line), center, fit-to-media alignment; also adds binding borders. | Document > Page Setup; Batch > Crop & Page Setup | - | P3 | missing | BCP, DOC |
| D-098 | Deskew | Pick two points that should be level; the page is rotated to straighten a skewed scan. | Document > Page Setup > Get Line; Deskew button | Ctrl+Alt+D | P3 | missing | DSK, KS |
| D-099 | Page range picker (shared) | Common page selector used by most page and batch dialogs: all, current, current view, selected, even, odd, landscape, portrait, first, last, custom list "1-3, 5, 9". | Many dialogs | - | P1 | have (app/PageRangePicker: all, current, selected, even, odd, landscape, portrait, first, last, custom; used by Flatten, Replace Pages, page labels; wave 20) | ROT, BCP, BPRN, PLB |
| D-100 | Batch file list (shared) | Common multi-file picker: add files, open files, current Set, folder, folder plus subfolders; reorder by drag; save/load the file list; works on closed files and saves them in place. | All Batch dialogs | - | P2 | missing | ROT, SPL, BCP |
| D-101 | Signed / certified guard | Page edits are blocked on certified or PDF/A files; on signed files the user is warned that signatures will be cleared. | Page dialogs | - | P3 | missing | INS, EXT, SPL, CMB |

## Creating and combining PDFs

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-102 | Combine PDFs | Merge several PDFs (open or on disk) in a sortable order, each with its own page range, into one file. | File > Combine; File toolbar | - | P1 | missing | CMB, FILE |
| D-103 | Combine options | Include bookmarks, include attachments, merge document properties, keep layers, use file names as page labels; signatures are cleared with a warning. | Combine dialog | - | P2 | missing | CMB |
| D-104 | Create PDF from a file | Convert one non-PDF file (Office, CAD, anything printable) to PDF, using a plugin when available. | File > Create > From File | Ctrl+N | P3 | missing | CRF, KS |
| D-105 | Create from multiple files (Stapler) | Batch conversion wizard: one combined output or one PDF per source, output to source folder or a chosen folder, queued jobs. | File > Create > From Multiple Files | - | P3 | missing | CRF |
| D-106 | Explorer right-click combine/convert | Windows shell menu entries to combine or convert selected files. | Windows Explorer | - | P3 | missing | CMB, CRF |
| D-107 | Create from scanner or camera | Scan pages (scan next / finish) or take a picture into a new PDF, with OCR options (language, skew, orientation, vertical text, text in graphics). | File > Create > From Scanner or Camera | - | P3 | missing | SCN |
| D-108 | Layered PDF from PDFs | Build one PDF where each source PDF's content becomes a layer (markups and links are not carried), layer names from file names optional. | File > Create > Layered PDF | - | P3 | missing | LAY |
| D-109 | Authoring plugins (high level) | Separate plugins create PDFs from Microsoft Office, AutoCAD, Revit and SolidWorks (each has its own help file installed). | Host applications | - | P3 | missing | CRF, installed Help folder (.chm names) |
| D-110 | 3D PDF basics | Create 3D PDFs from U3D or IFC (drag-drop or Open), add a 3D box to an existing page, 3D model tree panel, rotate/pan/zoom with mouse or a 3D mouse. | File > Open (3D Files); Edit > PDF Content > Add & Edit 3D Content | Ctrl+Alt+3; Alt+3 (model tree) | P3 | missing | 3D, KS, GEN |

## Batch tools

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-111 | Batch Link: search terms | Finds sheet references in text across many PDFs and turns them into links; terms come from file names, page labels or AutoMark page regions, or are typed / imported from CSV. | Batch > Link | - | P1 | missing | BLNK, BAT |
| D-112 | Batch Link: term filters | Trim generated terms at a filter character, keeping text from the start or from the end. | Batch Link > Settings | - | P2 | missing | BLNK |
| D-113 | Batch Link: destinations | Each term targets a file, a page in a file, a named Place in a file, or a web URL. | Batch Link term table | - | P1 | missing | BLNK |
| D-114 | Batch Link: link options | Relative or full paths, highlight links with a colour, how to handle overlapping links, appearance (outline/fill/highlight style, colour, width), flatten highlight. | Batch Link > Link Options | - | P2 | missing | BLNK |
| D-115 | Batch Link: save and report | Export terms to CSV, save the whole run as XML config or into a Set file, and get a summary (links created/deleted, pages skipped, files not opened). | Batch Link | - | P2 | missing | BLNK |
| D-116 | Batch Slip Sheet: matching | Pair current sheets with revised sheets across many files by file name + page index, by page label, by AutoMark region, or manually; optional wildcard match filter. | Batch > Slip Sheet | - | P1 | missing | SLIP |
| D-117 | Batch Slip Sheet: apply | Either insert revised pages ahead of current ones or replace them; copy markups forward (optionally unflatten first / flatten after); stamp old pages "Superseded". | Slip Sheet dialog | - | P1 | missing | SLIP |
| D-118 | Batch Slip Sheet: leftovers and report | Unmatched new sheets can be extracted to files; a PDF or CSV report lists what was slip-sheeted, with links. Links and bookmarks can be redirected to the new pages. | Slip Sheet dialog; Preferences | - | P2 | missing | SLIP, GEN |
| D-119 | Batch Sign & Seal | Across many files: add a date, place a professional seal image, digitally sign (in matching signature fields or at a manual position set on a preview) or certify. | Batch > Sign & Seal | - | P3 | missing | BSS, BAT |
| D-120 | Batch Apply Stamp | Place one stamp on many files/pages at the same spot: page filter, stamp folder, blend mode, opacity, lock, rotation, scale, anchor grid plus X/Y offset; closed files are saved directly. | Batch > Apply Stamp; Thumbnails | - | P2 | missing | BST |
| D-121 | Batch Flatten / Unflatten | Flatten or unflatten markups on chosen pages of many files (same options as single Flatten). | Batch > Flatten Markups | - | P2 | missing | FLT, BAT |
| D-122 | Batch Summary | Markup summary report built from one or many PDFs (see Markup summary rows below). | Batch > Summary | - | P0 | partial | SUM, BAT |
| D-123 | Batch Print | Print many PDFs with one set of print settings, in list order. | Batch > Print; Print > Add Files | - | P2 | missing | BPRN |
| D-124 | Batch Headers & Footers | Add (not edit) or remove headers/footers on many PDFs. | Batch > Headers & Footers | - | P3 | missing | BHF |
| D-125 | Other batch processes | Batch security, OCR, reduce file size, repair, rotate, crop/page setup, split, compare, overlay, script. | Batch menu | - | P3 | missing | BAT |

## Sets (many PDFs viewed as one)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-126 | Create / open / save Set | A .bex file listing many PDFs (relative paths optional) that open as one virtual drawing set without merging files; one Set open at a time; others can open it read-only. | Sets panel > Sets menu | Alt+2 | P1 | missing | SETP, SETW, KS |
| D-127 | Navigate a Set | Page through all sheets of all files as one sequence; thumbnail or list view. | Sets panel | - | P1 | missing | SETP |
| D-128 | Set sorting | Sort by page label, file name + label, or file name + index, ascending/descending alphabetic or numeric; stack multi-page files; show file names/labels/sheet names. | Set options > Sorting | - | P2 | missing | SETP |
| D-129 | Revision handling | Detect revisions automatically or by wildcard filter; previous revisions hidden, greyed, crossed out or shown; current ones highlighted or stacked; select latest or previous revisions. | Set options; Sets menu | - | P2 | missing | SETP |
| D-130 | Carry markups to new revision | Copy unflattened markups from the old sheet to its new revision (unflatten/flatten options) and stamp old sheets Superseded. | Set options | - | P2 | missing | SETP |
| D-131 | Categories | Group sheets by discipline prefix of file name or sheet-number tag (off/manual/auto) using editable category templates. | Set options > Categories | - | P3 | missing | SETP |
| D-132 | Tags | Per-sheet tags (sheet number, revision, discipline, sheet type, custom), some derived from the sheet number. | Set options > Tags; Edit Tags | - | P2 | missing | SETP, SETW |
| D-133 | Publish a Set | Combine the Set into one PDF, package the files, or export a drawing log. | Sets > Publish | - | P2 | missing | SETP |
| D-134 | Print / search a Set | Print the whole Set; search text across every file in it. | Sets menu | - | P2 | missing | SETP, SRCH |

## Markup summary and print

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-135 | Summary output types | Report of markups as CSV, XML, a PDF (separate or appended to the document with links back), or straight to the printer. | Markups list > Summary; Thumbnails; Batch > Summary | - | P0 | partial (CSV, XML, PDF report; not appended to the document, no printer) | SUM |
| D-136 | Summary columns | Choose and order columns, optionally include empty ones, load saved column configs. | Summary > Columns | - | P1 | missing | SUM |
| D-137 | Summary filter and sort | Filter by any column (defaults to Markups-list filters), multi-level sort ascending/descending. | Summary > Filter and Sort | - | P1 | missing | SUM |
| D-138 | Summary output options | Title (date can be appended), output folder, overwrite, open after, one report per value of the first column. CSV/XML: markups, totals or both, headers, ID/parent columns, units, number formats, replies. | Summary > Output | - | P1 | partial (CSV export, fixed options) | SUM |
| D-139 | Summary PDF layout | Table or flow style, report template with logo, page break per value, Spaces cover sheet, markup thumbnail size, include page content, padding, totals, links to source pages, capture media, status history, page size. | Summary > Output (PDF/Print) | - | P1 | missing | SUM |
| D-140 | Print dialog: printer | Printer choice, printer properties, status, print to .prn file, live preview with margins. | File > Print | Ctrl+P | P1 | missing | PRN, KS |
| D-141 | Print: pages | Page range including current view and "Get Window" to print a dragged region. | Print dialog | - | P1 | missing | PRN |
| D-142 | Print: what to print | Document and markups, document only, or markups only. | Print dialog | - | P1 | missing | PRN |
| D-143 | Print: paper and copies | Paper size (or auto from page size) and orientation, copies, collate, reverse order, auto-rotate. | Print dialog | - | P1 | missing | PRN |
| D-144 | Print: scaling | None, fit to paper, reduce to paper, custom percent, fit to margins, reduce to margins; center or manual X/Y position. | Print dialog | - | P1 | missing | PRN |
| D-145 | Print: pages per sheet | N-up with layout direction and border. | Print dialog | - | P2 | missing | PRN |
| D-146 | Print: emphasis options | Dim page content, dim filtered-out markups, print Spaces, print visible hyperlinks; Advanced dialog and reset to defaults. | Print dialog | - | P2 | missing | PRN |
| D-147 | Print tiling | Not described on the Revu 21 print pages we read. Nearest documented route is custom scale plus a Get Window region; tiled poster printing needs verifying in Advanced Printing. | Print > Advanced (unverified) | - | P3 | missing | PRN |

## Headers, footers, stamps and watermarks

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-148 | Add header/footer | Six text boxes (left/center/right, top and bottom) on chosen pages of one or many files, with font and margin settings and a page-by-page preview. | Document > Headers & Footers > Add | - | P3 | missing | HF |
| D-149 | Header/footer tokens | Insert page number, date, Bates number (digits, prefix, suffix) and file data (name, path, author, ...). | Header/footer dialog | - | P3 | missing | HF |
| D-150 | Fit content inside margins | Shrink the page content so the header/footer does not overlap it. | Header/footer dialog | - | P3 | missing | HF |
| D-151 | Header/footer templates | Save and reuse named header/footer configurations. | Header/footer dialog > Save | - | P3 | missing | HF, BHF |
| D-152 | Edit / update / delete | Edit the master header/footer (re-applied to all pages, updating numbers), or delete all; extra "secondary" ones are static. | Document > Headers & Footers > Edit | - | P3 | missing | HF |
| D-153 | Stamps as document feature | Rubber-stamp library (16+ built-in, custom with logo and dynamic date/time/user), stamp folder selection, default stamp, lock, opacity and PDF blend modes. | Tools > Stamp | - | P2 | missing | STMP, GS |
| D-154 | Watermark | Made by placing a custom stamp (on many pages via Batch Apply Stamp) and flattening it into the page. | Batch > Apply Stamp + Flatten | - | P3 | missing | WM, BST |

## OCR and search

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-155 | OCR | Recognise text in scanned pages so they can be searched and copied; runs on one or many files and page ranges; not on signed files. | Document > OCR; Batch > OCR | Ctrl+Shift+O | P2 | have (Document > OCR, Ctrl+Shift+O, any page range, progress + cancel, one undo step; `markupcraft-cli ocr`; Tesseract, optional at build; not Batch across files yet) | OCR, KS |
| D-156 | OCR options | Languages, document type (CAD drawing vs text document), accuracy vs speed, correct skew, detect orientation, vertical text, text in graphics, skip vector pages, page chunk size, max vector size. | OCR dialog | - | P3 | partial (language, page type drawing / text document, DPI, skip pages with text, vertical text; no skew / orientation / chunk options) | OCR |
| D-157 | Text search | Find text in the current PDF; results list shows snippet and page, click to jump; F3 / Shift+F3 step through results. | Window > Panels > Search | Ctrl+F / Alt+1; F3 / Shift+F3 | P0 | have | SRCH, KS |
| D-158 | Search scope | Current page, current document, all open documents, current Set, recent files, a folder (with subfolders), or file names in the current Studio Project. | Search panel | - | P1 | partial (current page, current document, all open documents) | SRCH |
| D-159 | Search options | Search page text, file names, file properties, form field values and markup text; case sensitive; whole words. | Search panel > Options | - | P1 | partial (page text, case, whole words) | SRCH |
| D-160 | Search selected text | Select text on the page, right-click > Search. | Select Text tool | Shift+T | P2 | missing | SRCH, KS |
| D-161 | Act on search results | For checked results: make hyperlinks, mark for redaction, place Count markups, highlight, underline, squiggly, strikethrough. | Search results > Check Options | - | P1 | partial (Search panel > Results: select all, copy text, Highlight / Underline / Strikethrough / Squiggly every hit or the selected ones; no hyperlinks, redaction or Count; wave 20) | SRCH |
| D-162 | Search and replace | Replace found text in the page content (not in markups), with font fallback or skip when a font is not editable. | Search results > Replace Checked | - | P3 | missing | SRCH |
| D-163 | Visual Search | Box a symbol and find every visually similar instance: sensitivity slider, search rotated copies (45-degree steps), colour filter, limit to selection; results can be turned into Count measurements. | Search panel > Visual | - | P0 | missing | SRCH |

## Forms

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-164 | Fill forms | Type into AcroForm and static XFA fields; field highlight toggle; reset all fields. | Forms panel; Tools > Form | Alt+Q | P2 | missing | FORM, KS |
| D-165 | Create form fields | Place text box, radio button, check box, list box, dropdown, button and signature fields with properties and actions. | Tools > Form | X (signature field) | P3 | missing | FORM, KS |
| D-166 | Auto-create fields | Detect form-like areas in a PDF and turn them into fields. | Tools > Form > Automatically Create Form Fields | - | P3 | missing | FORM |
| D-167 | Form data | Import, export and merge form data; migrate Typewriter text into fields; JavaScript for validation and calculation. | Tools > Form | Ctrl+Shift+F (Editor); Alt+J (console) | P3 | missing | FORM, KS |

## Digital signatures and certificates

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-168 | Signatures panel | Lists signatures and certifications (including invisible ones) with validation status and details; Validate All. | Window > Panels > Signatures | Alt+4 | P2 | missing | SIGP, KS |
| D-169 | Sign document | Place a signature field and sign it with a digital ID, appearance template, reason, location and contact; saved as a new file at signing. | Signatures panel > Sign Document | - | P3 | missing | SIGP, CERT |
| D-170 | Certify document | Certify (as first signer or without signing) and choose what later changes are allowed: none, forms and signatures, or markups plus forms and signatures; certifier can clear it. | Tools > Signatures > Certify Document | - | P3 | missing | CERT |
| D-171 | Digital ID manager | Create self-signed IDs (password-protected file or Windows certificate store), import, view, export public cert, change password, log out, delete. | Tools > Signatures > Digital IDs | - | P3 | missing | DID |

## Security and redaction

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-172 | Open password-protected PDFs | Prompt for the open password; master password lifts permission limits. | On open | - | P1 | missing | SEC |
| D-173 | Security status icon | Navigation-bar icon shows none / open password / limited printing-editing / both; click for details. | Navigation bar; Document > Security | Ctrl+L | P2 | missing | SEC, NAV, KS |
| D-174 | Set passwords and permissions | Require an open password and/or a master password with a permission set for everyone else; choose encryption strength; takes effect on save. | Document > Security > Change Permissions; Batch > Security | - | P3 | missing | SEC |
| D-175 | Security presets | Save named security policies and apply them from the Navigation bar menu. | PDF Security dialog | - | P3 | missing | SEC |
| D-176 | Mark for redaction | Mark areas or text runs for removal. | Edit > PDF Content > Mark for Redaction | Shift+R (area) / Shift+K (text) | P3 | missing | RED, KS |
| D-177 | Apply redactions | Permanently delete marked content on chosen pages, optionally scrubbing metadata; advanced choice of text, images or both. | Edit > PDF Content > Apply Redactions | Shift+A | P3 | missing | RED, KS |
| D-178 | Redaction appearance | Outline colour, fill colour after redaction, overlay text (custom or DOD/FOIA codes), autosize or repeat text; save as a Tool Chest tool. | Properties panel | - | P3 | missing | RED |

## Flatten, file size and archive

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-179 | Flatten markups | Burn markups into page content: pick markup types, all / skip filtered / selected only; optionally keep them recoverable. | Document > Flatten; right-click > Flatten | Ctrl+Shift+M | P2 | missing | FLT, KS |
| D-180 | Flatten extras | Flatten into a named layer with overlay text (font, position), keep chosen properties in a pop-up, turn capture media into an attached summary. | Flatten dialog | - | P3 | missing | FLT |
| D-181 | Unflatten | Recover markups flattened with recovery enabled (whole file or chosen pages); some edits (redaction, reduce size, edit text...) make it impossible. | Document > Unflatten | Ctrl+Shift+U | P2 | missing | FLT, KS |
| D-182 | Reduce file size | Compress images and drop invisible data with a quality slider or saved custom preset; size breakdown chart; prefix/suffix to keep the original; results report. | Document > Reduce File Size; Batch | - | P2 | missing | RFS |
| D-183 | Reduce size custom settings | Per image class (colour, 8-bit, grey, mono): convert type, max DPI, bit depth, JPEG quality; drop ICC profiles, embedded fonts, metadata, private data, thumbnails, unused resources, free xrefs; compress streams; crop to crop box. | Reduce File Size > Edit | - | P3 | missing | RFS |
| D-184 | Repair PDF | Rewrites a PDF to cure some display problems. | Document > Repair PDF | - | P3 | missing | DOC |
| D-185 | Archive as PDF/A | Export a PDF/A-1b file; Verify reports compliance; Unlock to edit an archive. | Document > Archive as PDF/A; File > Export > PDF/A | - | P3 | missing | DOC, PDFA, FILE |
| D-186 | Color processing | Recolour vector and raster content (e.g. make a background set grey). | Document > Color Processing | - | P2 | missing | DOC |

## Export

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-187 | Export to images | Each page saved as TIFF, JPEG, PNG, GIF or BMP, auto-numbered with a configurable suffix. | File > Export; Thumbnails > Export Pages | - | P2 | missing | EXP, FILE |
| D-188 | Export to text / RTF / HTML | Plain-text, rich-text or HTML conversion of the PDF. | File > Export | - | P3 | missing | EXP |
| D-189 | Export to Word / Excel / PowerPoint | Whole-document conversion to Office formats, with text recovery for scans. | File > Export > [format] > Entire Document | - | P3 | missing | EXP |
| D-190 | Export page region to Excel | Drag a box (e.g. an equipment schedule) and save only that text as an Office file. | File > Export > [format] > Page Region | - | P1 | missing | EXP |

## Properties, metadata and attachments

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-191 | Document Properties | Dialog with general file information and a Security tab. | Document > Document Properties; Navigation bar | Ctrl+D | P2 | missing | DOC, NAV, KS |
| D-192 | File properties metadata | With nothing selected the Properties panel shows standard metadata (title, author, subject...) that can be edited. | Properties panel | Alt+P | P3 | missing | PROP, KS |
| D-193 | Custom properties | Add, edit and delete custom name/value metadata. | Properties panel | - | P3 | missing | PROP |
| D-194 | Embedded file attachments | List, open, save out, add and delete files attached to the PDF. | Properties panel > File Attachments | - | P2 | missing | PROP |
| D-195 | Attachment icon markup | Place a clickable paperclip-style icon carrying one file; icon, colour, opacity, extract, pop-up note. | Tools > File Attachment | - | P3 | missing | ATT |
| D-196 | Page tags / standards info | Read-only page tags and a Standards section (PDF/A status, unlock). | Properties panel | - | P3 | missing | PROP, PDFA |

## Links and places

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-197 | Follow PDF links | Clicking an existing link runs its action (jump to sheet, open file, open URL); URL-looking text can be made clickable. | Workspace; Preferences > General | - | P0 | have (page links, web links and files after a confirm) | HYP, GEN |
| D-198 | Hyperlink tool | Draw a rectangle or select text to make a link; view mode highlights all links in blue (Esc exits). | Tools > Hyperlink | - | P1 | missing | HYP |
| D-199 | Link actions | Same six actions as bookmarks: page (with zoom), Place, Space, snapshot rectangle (even in another PDF), URL, open file; relative or full paths. | Action dialog | - | P1 | missing | HYP |
| D-200 | Hyperlinks from URLs | Turn URL text on chosen pages into links automatically. | Tools > Markup > PDF Content > Create Hyperlinks from URLs | - | P3 | missing | HYP |
| D-201 | Links panel | Lists every link (filterable) with Edit Action; multi-select to change several. | Window > Panels > Links | Alt+N | P2 | missing | LNK, KS |
| D-202 | Places | Named anchors (also read from named destinations) that links target; moving a Place keeps links working, renaming breaks them. | Links panel > Places | - | P2 | missing | LNK |

## Recent files and File Access panel

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-203 | File Access: Recents | Auto list of recent files with hover path and preview; click to open, Ctrl-click to open in background; remove or clear. | Window > Panels > File Access | Alt+A | P1 | have (File Access panel, Alt+A: recents with first-page thumbnails and path tooltip, click opens, Ctrl+click opens behind, remove / clear; wave 20) | FA, KS |
| D-204 | Recents sorting | By date, by folder, most accessed, or access history grouped by day. | File Access | - | P2 | missing | FA |
| D-205 | Pinned files | Pin files so they never drop off; sort pinned by date, folder or name. | File Access | - | P1 | have (pin files and folders, a pinned folder lists its PDFs; sort pinned by name, date or folder; wave 20) | FA |
| D-206 | Pin categories | User-named groups of pinned files from different folders; rename, remove, collapse. | File Access | - | P2 | missing | FA |
| D-207 | Recents preferences | On/off, number of items, days kept (default 90), preview on/off, clear. | Preferences > Interface | - | P3 | missing | INTF |
| D-208 | File Access: Explorer | In-app file browser: path box with autocomplete and favourites, drive list, sort by name/type/size/date, file-type filter, back/forward/up, new folder, pin folder. | File Access > Explorer | - | P2 | missing | FA |
| D-209 | Explorer context menu | Open file, open folder, rename, delete from disk, Windows properties. | File Access > Explorer | - | P3 | missing | FA |
| D-210 | Link from file list | Hover a file and drag out a link area that opens it. | File Access | - | P3 | missing | HYP |
| D-211 | DMS / SharePoint | Check-in/out integration with configured document management systems and SharePoint. | Preferences > Interface | Alt+K (Remote Files) | P3 | missing | INTF, KS |

## Studio (summary only)

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-212 | Studio Sessions | Cloud room where invited attendees mark up the same PDFs live or asynchronously. | Studio panel | Alt+C | P2 | missing | STU, GS, KS |
| D-213 | Studio Projects | Cloud document store for any file type with folders, permissions and check-out; PDF files can be pushed into a Session. | Studio panel; File > New Studio Project | - | P2 | missing | STU, FILE |
| D-214 | Studio offline | Work on Studio content without a connection and sync later. | Studio | - | P3 | missing | STU |

## Pointers to related features covered elsewhere

| ID | Feature | What it does (your words, 1-2 sentences) | Where in Revu UI | Shortcut | Priority | MarkupCraft | Source |
|---|---|---|---|---|---|---|---|
| D-215 | Compare / Overlay / Smart Overlay | Revision comparison tools (see part 4). | Document / Batch menus | - | P1 | missing | DOC, BAT |
| D-216 | Stitching | Join several drawings into one continuous view (Max plan). | Document > Stitching | - | P3 | missing | DOC |
| D-217 | Script / Translate Markups | Script Manager automation; machine translation of markup text. | Document menu | - | P3 | missing | DOC |

## Biggest gaps (top 10)

1. **Multiple documents**: tabs, Close/Close All, Ctrl+Tab cycling, Save All, Open Recent and reopen-last-session. Today MarkupCraft holds one file, and estimators routinely work across specs, drawings and addenda at once.
2. **Navigation basics**: page-number box, view history (Alt+Left/Right), continuous layout, rotate view, Zoom tool, Spacebar pan.
3. **Follow PDF links and the Bookmarks panel**: most CD sets ship with bookmarks and sheet links. MarkupCraft can neither show nor follow them.
4. **Text search plus Visual Search to Count**: Ctrl+F on the current document is daily use. Visual Search that drops Count markups is a direct takeoff accelerator.
5. **Split views with sync and a detachable second-monitor window**: plan beside enlarged plan, old revision beside new, specs beside drawings.
6. **Printing**: no print at all today. Needed: markups-only and document-only, page scaling (fit, custom percent), a print region and a page range.
7. **Page operations from Thumbnails**: multi-select, drag reorder, insert, extract, replace (content only, keeping markups), delete, rotate pages.
8. **Page labels from a title-block region (AutoMark)**: label editing plus generation from a region. This one capability also feeds bookmarks, Batch Link, Slip Sheet and Sets matching.
9. **Markup Summary beyond CSV**: column choice, filter and sort, totals, and a PDF/print report with markup thumbnails. This is the estimator's deliverable.
10. **Revision workflow**: Batch Slip Sheet (match, replace, carry markups forward, Superseded stamp), Sets, Combine and Batch Link. These keep a marked-up takeoff alive through addenda.
