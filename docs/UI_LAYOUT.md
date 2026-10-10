# Default window layout

MarkupCraft's default workspace follows the layout Revu 21 users know, so they find everything
where they expect it. This is a description of positions and grouping (observed black-box);
colours, icons, fonts and wording are our own. No Revu imagery is used or stored here.

```
+---------------------------------------------------------------------------------------------+
| [app menu] File  Edit  View  Document  Batch  Tools  Window  Help              (window btns) |  1 menu bar
+---------------------------------------------------------------------------------------------+
| [doc icon v]  Name: <document title>   Pages: <n>   [doc props btn]                          |  2 document bar
+----+-------------------------+-------------------------------------------------------+----+
|    | Thumbnails v  [icons]   | [tab] [tab*] [tab]                                  v |    |  3 tab strip
| P  |-------------------------+-------------------------------------------------------| T  |
| A  |                         |                                                       | O  |
| N  |  left panel             |                  page canvas                          | O  |
| E  |  (one panel at a time,  |                                                       | L  |
| L  |   chosen from the bar)  |                                                       |    |
|    |                         |                                                       | S  |
| B  |                         |                                                       | T  |
| A  |                         |                                                       | R  |
| R  |                         |                                                       | I  |
|    |                         |                                                       | P  |
+----+-------------------------+-------------------------------------------------------+----+
| [list] [size slider]      | [view layout btns] | [pan][select][text][zoom] |< < [page box] > >| (<)(>) | [theme] size  scale |  4 bottom bar
+---------------------------------------------------------------------------------------------+
```

1. **Menu bar.** An application menu first (MarkupCraft's equivalent of the app menu: profiles,
   preferences, administrator), then File, Edit, View, Document, Batch, Tools, Window, Help.
   Markup and Measure commands live in Tools and in the tool strip, not in top-level menus.
2. **Document bar.** A small document icon with a dropdown, then "Name:" with the document title
   and "Pages:" with the page count, then a button for document properties. No toolbar of icons
   in the top area by default; toolbars are optional (Window > Toolbars).
3. **Tab strip.** Document tabs run across the top of the canvas only (not over the panels), with
   a dropdown at the right end listing open documents.
4. **Left panel bar.** A narrow vertical bar of icons on the far left edge. Each icon opens one
   panel in the left panel area (Thumbnails, Bookmarks, Layers, Tool Chest, Spaces, Properties,
   Measurements, Markups List shortcut, Signatures, Links, Sets, Search, File Access ...).
   Clicking the active icon collapses the panel. The panel shows its title with a dropdown and a
   few panel-specific buttons in its header.
5. **Right tool strip.** A narrow vertical strip on the far right edge with the markup tools,
   grouped and separated: text tools (text box, typewriter, callout, note), highlight and pen,
   stamp, image, snapshot, then lines (line, arrow, arc, polyline, dimension), shapes (rectangle,
   ellipse, polygon, cloud), then measurement tools. A grip at the top. Panels may also open on
   the right side when the user docks them there; the strip stays at the edge.
6. **Bottom bar.** One row under everything:
   - left: a toggle for the Markups List (opens the bottom panel) and the thumbnail size slider;
   - view layout buttons (single page / continuous, split vertical, split horizontal, ...);
   - centre: pan, select, select text and zoom tools, then first / previous page, the page box
     showing "label (n of N)", next / last page, then back / forward view;
   - right: light/dark toggle, page size and the page scale.
7. **Markups List** is a bottom panel that opens from the bottom bar toggle and spans the width
   under the canvas.

Default theme: dark (our own dark palette), with a light theme available.
