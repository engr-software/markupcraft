# 05 - Revu 21 default keyboard shortcuts

Key bindings (facts about which key runs which command), taken from Bluebeam's public keyboard
shortcut documentation (the help PDF that ships with Revu 21). Cross-checked against Bluebeam's public page "Keyboard shortcuts and mouse navigation"
(https://support.bluebeam.com/revu/resources/keyboard-shortcuts.html, which says it covers Revu 21 and Revu 20).
Clean-room: only these two public documents were used.

177 shortcuts in 12 categories. No key is bound to two commands.

## Extraction notes and source disagreements

- Page 1 is two columns. Plain text extraction puts "Line ... Ungroup" after the FORMS heading, but the block coordinates
  (pymupdf) show they are the top of column 2 and continue the alphabetical MARKUP list. FORMS has only 2 entries.
- **Camera**: the PDF prints `Ctrl +Alt + l` (lowercase L). Ctrl+Alt+L is already Align Left, and the web summary gives
  Ctrl + Alt + I. Recorded as **Ctrl + Alt + I**.
- **Bring Forward / Bring to Front**: PDF says `Ctrl + ]` / `Ctrl + Shift + ]`; the web page reads `Ctrl + J` / `Ctrl + Shift + J`.
  Ctrl+J pairs poorly with Send Backward `Ctrl + [`, so the PDF is preferred. Verify in-app if it matters.
- **Save All**: PDF says `Shift + F2`; the web page says `Ctrl + F2`, which the PDF gives to Export Markups. PDF preferred.
- **Spell Check (F7)** and **Export Markups (Ctrl + F2)** appear only in the PDF. **Offset (O)** (Edit) appears only on the web
  page; added below as a web-only row.
- The web page calls "Remove From Group" "Remove Selected from Group" and "Hide Panels" "Hide Panel".
- "Plus" / "Minus" mean the bare keys (main row or numeric keypad), no modifier.
- Revu has **no default shortcut for Calibrate** (the scale is set from the Measurements panel). "Measure Tool" (M) is the
  generic measurement tool.

## 1. Full default map

| Category | Command | Revu shortcut | MarkupCraft today | Conflict? |
|---|---|---|---|---|
| Markup | Align Bottom | Ctrl + Alt + B |  |  |
| Markup | Align Center | Ctrl + Alt + E |  |  |
| Markup | Align Left | Ctrl + Alt + L |  |  |
| Markup | Align Middle | Ctrl + Alt + M |  |  |
| Markup | Align Right | Ctrl + Alt + R |  |  |
| Markup | Align Top | Ctrl + Alt + T |  |  |
| Markup | Arc | Shift + C |  |  |
| Markup | Arrow | A | A = Area | YES: MarkupCraft A is Area |
| Markup | Autosize Text Box | Alt + Z |  |  |
| Markup | Bring Forward | Ctrl + ] |  |  |
| Markup | Bring to Front | Ctrl + Shift + ] |  |  |
| Markup | Callout | Q |  |  |
| Markup | Camera | Ctrl + Alt + I |  |  |
| Markup | Spell Check | F7 |  |  |
| Markup | Cloud | C | C = Count | YES: MarkupCraft C is Count |
| Markup | Cloud+ | K | K = Calibrate | YES: MarkupCraft K is Calibrate |
| Markup | Dimension | Shift + L |  |  |
| Markup | Edit Action | Ctrl + Shift + E |  |  |
| Markup | Ellipse | E |  |  |
| Markup | Eraser | Shift + E |  |  |
| Markup | Export Markups | Ctrl + F2 |  |  |
| Markup | File Attachment | F |  |  |
| Markup | Flag | Shift + F |  |  |
| Markup | Flip Horizontal | Ctrl + Alt + H |  |  |
| Markup | Flip Vertical | Ctrl + Alt + V |  |  |
| Markup | Group | Ctrl + G |  |  |
| Markup | Highlight | H | H = Pan | YES: MarkupCraft H is Pan |
| Markup | Hyperlink | Shift + H |  |  |
| Markup | Image | I |  |  |
| Markup | Image From Scanner | Shift + I |  |  |
| Markup | Import | Ctrl + F3 |  |  |
| Measure | Angle | Shift + Alt + G |  |  |
| Measure | Area | Shift + Alt + A | A | YES: Revu uses Shift+Alt+A |
| Measure | Count | Shift + Alt + C | C | YES: Revu uses Shift+Alt+C |
| Measure | Diameter | Shift + Alt + D |  |  |
| Measure | Dynamic Fill | J |  |  |
| Measure | Length | Shift + Alt + L | L | YES: Revu uses Shift+Alt+L |
| Measure | Measure Tool | M |  |  |
| Measure | Perimeter | Shift + Alt + P | R | YES: Revu uses Shift+Alt+P |
| Measure | Polylength | Shift + Alt + Q | P | YES: Revu uses Shift+Alt+Q |
| Measure | Radius | Shift + Alt + U |  |  |
| Measure | Volume | Shift + Alt + V |  |  |
| Forms | Add Signature Field | X |  |  |
| Forms | Editor | Ctrl + Shift + F |  |  |
| Markup | Line | L | L = Length | YES: MarkupCraft L is Length |
| Markup | Lock | Ctrl + Shift + L |  |  |
| Markup | Note | N |  |  |
| Markup | Pen | P | P = Polylength | YES: MarkupCraft P is Polylength |
| Markup | Polygon | Shift + P |  |  |
| Markup | Polyline | Shift + N |  |  |
| Markup | Rectangle | R | R = Perimeter | YES: MarkupCraft R is Perimeter |
| Markup | Remove From Group | Ctrl + Shift + Alt + G |  |  |
| Markup | Review Text | Shift + Alt + R |  |  |
| Markup | Send Backward | Ctrl + [ |  |  |
| Markup | Send to Back | Ctrl + Shift + [ |  |  |
| Markup | Stamp | S |  |  |
| Markup | Text Box | T |  |  |
| Markup | Typewriter | W |  |  |
| Markup | Ungroup | Ctrl + Shift + G |  |  |
| File | Close | Ctrl + F4 |  |  |
| File | Create PDF | Ctrl + N |  |  |
| File | Open | Ctrl + O | Ctrl+O | No (match) |
| File | Print | Ctrl + P |  |  |
| File | Publish as Compressed 1.5 | Ctrl + Shift + P |  |  |
| File | Save | Ctrl + S | Ctrl+S | No (match) |
| File | Save All | Shift + F2 |  |  |
| File | Save As | Ctrl + Shift + S | Ctrl+Shift+S | No (match) |
| Edit | Copy | Ctrl + C |  |  |
| Edit | Copy Page to Snapshot | Ctrl + Alt + C |  |  |
| Edit | Cut | Ctrl + X |  |  |
| Edit | Delete | Del | Delete | No (match) |
| Edit | Format Painter | Ctrl + Shift + C |  |  |
| Edit | Paste | Ctrl + V |  |  |
| Edit | Paste in Place | Ctrl + Shift + V |  |  |
| Edit | Redo | Ctrl + Y | Ctrl+Y | No (match) |
| Edit | Select All | Ctrl + A |  |  |
| Edit | Select All Text | Ctrl + Shift + A |  |  |
| Edit | Snapshot | G |  |  |
| Edit | Undo | Ctrl + Z | Ctrl+Z | No (match) |
| View | Actual Size | Ctrl + 8 | Ctrl+1 | YES: Revu Actual Size is Ctrl+8 |
| View | Balance | Shift + F12 |  |  |
| View | Continuous Mode | Ctrl + 5 |  |  |
| View | Continuous Side by Side Mode | Ctrl + 7 |  |  |
| View | Dimmer | Ctrl + F5 |  |  |
| View | Fit Page | Ctrl + 9 | Ctrl+0 | YES: Revu Fit Page is Ctrl+9; Ctrl+0 is Fit Width |
| View | Fit Width | Ctrl + 0 | Ctrl+2 / Ctrl+0 = Fit Page | YES: Revu Fit Width is Ctrl+0 |
| View | Next Page | Ctrl + Right | PageDown | Missing: add Ctrl+Right |
| View | Next View | Alt + Right |  |  |
| View | Previous Page | Ctrl + Left | PageUp | Missing: add Ctrl+Left |
| View | Previous View | Alt + Left |  |  |
| View | Refresh | F5 |  |  |
| View | Remote Files | Alt + K |  |  |
| View | Rotate View Clockwise | Ctrl + Shift + Plus |  |  |
| View | Rotate View Counterclockwise | Ctrl + Shift + Minus |  |  |
| View | Rulers | Ctrl + R |  |  |
| View | Show Grid | Shift + F9 |  |  |
| View | Side by Side | Ctrl + 6 |  |  |
| View | Single Page Mode | Ctrl + 4 |  |  |
| View | Snap to Content | Ctrl + Shift + F8 |  |  |
| View | Snap to Grid | Ctrl + Shift + F9 |  |  |
| View | Snap to Markup | Ctrl + Shift + F7 |  |  |
| View | Split Horizontal | Ctrl + H |  |  |
| View | Split Vertical | Ctrl + 2 | Ctrl+2 = Fit Width | YES: MarkupCraft Ctrl+2 is Fit Width |
| View | Switch | Ctrl + 1 | Ctrl+1 = Actual Size | YES: MarkupCraft Ctrl+1 is Actual Size |
| View | Toggle Split | Ctrl + I |  |  |
| View | Unsplit | Ctrl + Shift + 2 |  |  |
| View | Web Tab | Ctrl + T |  |  |
| Selection | Lasso | Shift + O |  |  |
| Selection | Pan | Shift + V | H = Pan | YES: Revu Pan is Shift+V; H is Highlight |
| Selection | Select | V | V Select | No (match) |
| Selection | Select Text | Shift + T |  |  |
| Selection | Toggle Zoom Tool | Shift + Z |  |  |
| Selection | Zoom In | Plus | Ctrl+= | Partial: Revu is plain Plus |
| Selection | Zoom Out | Minus | Ctrl+- | Partial: Revu is plain Minus |
| Selection | Zoom Tool | Z |  |  |
| Search | Next Result | F3 |  |  |
| Search | Previous Result | Shift + F3 |  |  |
| Search | Search | Ctrl + F |  |  |
| Document | Add & Edit 3D Content | Ctrl + Alt + 3 |  |  |
| Document | Add Bookmark | Ctrl + B |  |  |
| Document | Apply Redactions | Shift + A |  |  |
| Document | Crop Pages | Shift + Alt + O |  |  |
| Document | Delete Pages | Ctrl + Shift + D |  |  |
| Document | Deskew | Ctrl + Alt + D |  |  |
| Document | Document Properties | Ctrl + D |  |  |
| Document | Email | Ctrl + E |  |  |
| Document | Extract Pages | Ctrl + Shift + X |  |  |
| Document | Flatten | Ctrl + Shift + M |  |  |
| Document | Flattened | Ctrl + Alt + F |  |  |
| Document | Insert Blank Page | Ctrl + Shift + N |  |  |
| Document | Insert Pages | Ctrl + Shift + I |  |  |
| Document | Mark for Redaction | Shift + R |  |  |
| Document | Mark Text for Redaction | Shift + K |  |  |
| Document | OCR | Ctrl + Shift + O |  |  |
| Document | Refresh Document | Shift + F5 |  |  |
| Document | Replace Pages | Ctrl + Shift + Y |  |  |
| Document | Rotate Clockwise | Shift + Alt + Plus |  |  |
| Document | Rotate Counterclockwise | Shift + Alt + Minus |  |  |
| Document | Rotate Pages | Ctrl + Shift + R |  |  |
| Document | Security | Ctrl + L |  |  |
| Document | Snapshot Content | Shift + G |  |  |
| Document | Squiggly | Shift + U |  |  |
| Document | Strikethrough | D |  |  |
| Document | Underline | U |  |  |
| Document | Unflatten | Ctrl + Shift + U |  |  |
| Window | 3D Model Tree | Alt + 3 |  |  |
| Window | Always on Top | Ctrl + F12 |  |  |
| Window | Bookmarks | Alt + B |  |  |
| Window | Close All | Ctrl + Shift + W |  |  |
| Window | File Access | Alt + A |  |  |
| Window | Forms | Alt + Q |  |  |
| Window | Full Screen | F11 |  |  |
| Window | Hide Panels | Shift + F4 |  |  |
| Window | JavaScript Console | Alt + J |  |  |
| Window | Layers | Alt + Y |  |  |
| Window | Links | Alt + N |  |  |
| Window | Markups | Alt + L |  |  |
| Window | Measurements | Alt + U |  |  |
| Window | Menu Bar | F9 |  |  |
| Window | Navigation Bar | F4 |  |  |
| Window | Preferences | Ctrl + K |  |  |
| Window | Presentation | Ctrl + Enter |  |  |
| Window | Properties | Alt + P |  |  |
| Window | Search | Alt + 1 |  |  |
| Window | Sets | Alt + 2 |  |  |
| Window | Show Context Menu | Shift + F10 |  |  |
| Window | Signatures | Alt + 4 |  |  |
| Window | Spaces | Alt + S |  |  |
| Window | Status Bar | F8 |  |  |
| Window | Studio | Alt + C |  |  |
| Window | Thumbnails | Alt + T |  |  |
| Window | Tool Chest | Alt + X |  |  |
| Help | Help | F1 |  |  |
| Navigation | First Page | Home |  |  |
| Navigation | Last Page | End |  |  |
| Navigation | Next Document | Ctrl + Tab |  |  |
| Navigation | Previous Document | Ctrl + Shift + Tab |  |  || Edit (web only) | Offset | O |  |  |

### Mouse and modifier behavior (PDF pages 5-7, not key bindings but part of "the hands")

| Input | Revu behavior |
|---|---|
| Middle button click + drag | Pan |
| Middle button double-click | Re-center view |
| Left click | Tool operation |
| Left click + drag (no tool) | Multi-select (marquee) |
| Shift + left click + drag | Multi-select |
| Spacebar held + click/drag | Pan without leaving the current tool or markup |
| Right click | Context menu |
| Ctrl held + mouse wheel | Toggles the wheel between zoom and pan |
| Shift while rotating a markup | Free rotation in 1 degree steps (default snaps to 15 degrees) |
| Shift + click a measurement caption | Move the caption alone |
| Ctrl + Shift + click-drag a markup | Copy it and move the copy in a straight line |
| Shift while drawing Line/Arrow/Polyline/Polygon/Measurement | Constrain to horizontal, vertical or 45 degrees |
| Shift while drawing Pen/Highlight | Constrain to horizontal or vertical |
| 3D: wheel scroll / middle drag / middle double-click / left drag | Zoom / pan model / return to view / rotate model |

## 2. MarkupCraft rebinding

Current bindings are in `src/app/MainWindow.cpp` (menus at lines ~80-107, tool table at ~127-130) and `src/app/PageView.cpp`
(keyPressEvent, ~555-568).

| MarkupCraft command | MarkupCraft today | Revu default for same command | What Revu does with MarkupCraft's key | Verdict |
|---|---|---|---|---|
| Select | V | V | Select | Match |
| Pan | H | Shift + V (also Space-hold, middle drag) | H = Highlight | Conflict |
| Calibrate | K | none | K = Cloud+ | Conflict |
| Length | L | Shift + Alt + L | L = Line | Conflict |
| Polylength | P | Shift + Alt + Q | P = Pen | Conflict |
| Area | A | Shift + Alt + A | A = Arrow | Conflict |
| Perimeter | R | Shift + Alt + P | R = Rectangle | Conflict |
| Count | C | Shift + Alt + C | C = Cloud | Conflict |
| Open | Ctrl + O | Ctrl + O | same | Match |
| Save | Ctrl + S | Ctrl + S | same | Match |
| Save As | Ctrl + Shift + S | Ctrl + Shift + S | same | Match |
| Undo | Ctrl + Z | Ctrl + Z | same | Match |
| Redo | Ctrl + Y | Ctrl + Y | same | Match |
| Delete markup | Delete | Del | same | Match |
| Fit Page | Ctrl + 0 | Ctrl + 9 | Ctrl+0 = Fit Width | Conflict |
| Fit Width | Ctrl + 2 | Ctrl + 0 | Ctrl+2 = Split Vertical | Conflict |
| Actual Size | Ctrl + 1 | Ctrl + 8 | Ctrl+1 = Switch (split pane) | Conflict |
| Zoom In | Ctrl + = (QKeySequence::ZoomIn) | Plus (no modifier) | Ctrl+= unbound in Revu | Missing Revu key |
| Zoom Out | Ctrl + - (QKeySequence::ZoomOut) | Minus (no modifier) | Ctrl+- unbound; Ctrl+Shift+Minus = Rotate View CCW | Missing Revu key |
| Next Page | PageDown | Ctrl + Right | PageDown not in Revu's map | Missing Revu key |
| Previous Page | PageUp | Ctrl + Left | PageUp not in Revu's map | Missing Revu key |

Ten of MarkupCraft's 21 bindings conflict, four lack the Revu key, and seven match. The worst case is the measurement tools: a Revu user
who presses L, A, R, C or P expects to start drawing a markup, and in MarkupCraft they start a measurement instead.

### Changes to make (exact)

1. Pan: H -> **Shift + V**. Also make Space-hold pan temporarily without dropping the active tool, and middle-drag pan.
2. Length: L -> **Shift + Alt + L**.
3. Polylength: P -> **Shift + Alt + Q**.
4. Area: A -> **Shift + Alt + A**.
5. Perimeter: R -> **Shift + Alt + P**.
6. Count: C -> **Shift + Alt + C**.
7. Calibrate: drop K, since K is Cloud+ in Revu. Revu has no default key for it, so leave it unbound (menu/toolbar only) and
   make it user-assignable. Do not use M; M is Revu's Measure Tool.
8. Fit Page: Ctrl + 0 -> **Ctrl + 9**.
9. Fit Width: Ctrl + 2 -> **Ctrl + 0**.
10. Actual Size: Ctrl + 1 -> **Ctrl + 8**.
11. Zoom In / Zoom Out: add bare **Plus** and **Minus**, main row and keypad (Qt::Key_Plus, Qt::Key_Equal for the
    unshifted key, Qt::Key_Minus). Keep Ctrl+= / Ctrl+- as harmless aliases, since Revu does not bind them.
12. Next / Previous Page: add **Ctrl + Right** / **Ctrl + Left** as the primary keys. Keep PageDown/PageUp as aliases (not in
    Revu's map). Also add **Home** = First Page and **End** = Last Page.
13. Leave Ctrl + 1 and Ctrl + 2 unbound until split view exists. Then they become Switch and Split Vertical.
14. Leave H, K, L, P, A, R, C unbound (not reassigned) until the matching markup tools exist (Highlight, Cloud+, Line, Pen,
    Arrow, Rectangle, Cloud). A Revu user pressing them should get nothing rather than a different tool.
15. Unchanged: V, Ctrl+O, Ctrl+S, Ctrl+Shift+S, Ctrl+Z, Ctrl+Y, Delete. PageView's Esc / Enter / Backspace while drafting
    are not in Revu's printed map and can stay.
16. Cheap next additions with the Revu keys: Select All Ctrl+A, Copy/Cut/Paste Ctrl+C/X/V, Paste in Place Ctrl+Shift+V,
    Print Ctrl+P, Close Ctrl+F4, Search Ctrl+F (F3 / Shift+F3), Markups list Alt+L, Measurements panel Alt+U,
    Thumbnails Alt+T, Full Screen F11, Next/Previous Document Ctrl+Tab / Ctrl+Shift+Tab, Zoom Tool Z, Lasso Shift+O,
    Measure Tool M.

Revu 21 lets users rebind every key (Revu > Keyboard Shortcuts). MarkupCraft should keep the bindings in one table, data rather
than code, so a rebinding UI can be added later.
