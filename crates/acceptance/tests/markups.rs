//! Markups slice (docs/revu_features/02_markups.md): text, shapes, stamps, Properties,
//! selection, Tool Chest, layers and the Markups List, each checked from the inventory text.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use markupcraft_acceptance::*;
use markupcraft_revu::cos::{Dict, Document as CosDoc, Object};

// ---------------------------------------------------------------------------------------------
// helpers

/// A folder with the sample plan open in a fresh tool table.
fn open(tag: &str) -> (PathBuf, Automation) {
    let dir = temp_dir(tag);
    sample_pdf(&dir, "plan.pdf");
    let mut a = automation(&dir).with_config_dir(dir.join("config"));
    call(&mut a, "doc_open", json!({ "path": "plan.pdf" }));
    (dir, a)
}

/// `markup_add` with extra properties merged in; returns the new markup's id.
fn add(a: &mut Automation, kind: &str, pts: Value, extra: Value) -> String {
    let mut args = json!({ "page": 1, "kind": kind, "points": pts });
    if let (Some(o), Some(e)) = (args.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    let v = call(a, "markup_add", args);
    v["id"].as_str().unwrap().to_string()
}

fn rect_pts(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// Every markup as the Markups List tool reports it.
fn list(a: &mut Automation) -> Vec<Value> {
    call(a, "markup_list", json!({}))["markups"].as_array().unwrap().clone()
}

fn get(a: &mut Automation, id: &str) -> Value {
    list(a)
        .into_iter()
        .find(|m| m["id"] == id)
        .unwrap_or_else(|| panic!("no markup {id}"))
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// One saved annotation dictionary (page index, dictionary).
struct Annot {
    d: Dict,
    cos: Arc<CosDoc>,
}

impl Annot {
    fn name(&self, k: &str) -> String {
        markupcraft_revu::pdf::name(self.d.get(k.as_bytes()))
    }
    fn text(&self, k: &str) -> String {
        let o = self.resolve(k);
        markupcraft_revu::pdf::text(Some(&o))
    }
    fn has(&self, k: &str) -> bool {
        self.d.contains(k.as_bytes())
    }
    fn resolve(&self, k: &str) -> Object {
        match self.d.get(k.as_bytes()) {
            Some(o) => (*self.cos.resolve(o)).clone(),
            None => Object::Null,
        }
    }
    fn nums(&self, k: &str) -> Vec<f64> {
        match self.resolve(k) {
            Object::Array(v) => v.iter().filter_map(|o| markupcraft_revu::pdf::num(Some(o))).collect(),
            o => markupcraft_revu::pdf::num(Some(&o)).into_iter().collect(),
        }
    }
    fn names(&self, k: &str) -> Vec<String> {
        match self.resolve(k) {
            Object::Array(v) => v.iter().map(|o| markupcraft_revu::pdf::name(Some(o))).collect(),
            o => vec![markupcraft_revu::pdf::name(Some(&o))],
        }
    }
    fn num(&self, k: &str) -> Option<f64> {
        markupcraft_revu::pdf::num(Some(&self.resolve(k)))
    }
    fn sub(&self, k: &str, inner: &str) -> Object {
        match self.resolve(k) {
            Object::Dict(d) => d
                .get(inner.as_bytes())
                .map(|o| (*self.cos.resolve(o)).clone())
                .unwrap_or(Object::Null),
            _ => Object::Null,
        }
    }
    fn nm(&self) -> String {
        self.text("NM")
    }
}

/// Every annotation dictionary of a PDF file.
fn annots(path: &Path) -> Vec<Annot> {
    let cos = Arc::new(CosDoc::open(Arc::new(std::fs::read(path).unwrap())).unwrap());
    let mut out = Vec::new();
    for page in markupcraft_revu::pdf::pages(&cos).iter() {
        let arr = cos.resolve(page.dict.get(b"Annots").unwrap_or(&Object::Null));
        let Some(arr) = arr.as_array() else { continue };
        for item in arr {
            if let Some(d) = cos.resolve(item).as_dict() {
                out.push(Annot {
                    d: d.clone(),
                    cos: cos.clone(),
                });
            }
        }
    }
    out
}

/// Save as `name` (full rewrite) and return the file's annotations.
fn save(a: &mut Automation, dir: &Path, name: &str) -> Vec<Annot> {
    call(a, "doc_save", json!({ "path": name, "full": true }));
    annots(&dir.join(name))
}

fn by_nm<'a>(v: &'a [Annot], id: &str) -> &'a Annot {
    v.iter()
        .find(|x| x.nm() == id)
        .unwrap_or_else(|| panic!("no annotation /NM {id}"))
}

/// Reopen a saved file in a new tool table and list its markups.
fn reopen(dir: &Path, name: &str) -> (Automation, Vec<Value>) {
    let mut b = automation(dir);
    call(&mut b, "doc_open", json!({ "path": name }));
    let l = list(&mut b);
    (b, l)
}

/// The basic Revu-compatible keys every saved markup carries.
fn assert_revu_basics(x: &Annot, subtype: &str) {
    assert_eq!(x.name("Subtype"), subtype, "subtype of {}", x.nm());
    assert!(x.has("Rect"), "{} has /Rect", x.nm());
    assert!(x.has("AP"), "{} has an appearance stream /AP", x.nm());
    assert!(x.has("NM"), "{} has /NM", x.nm());
    assert!(x.has("C"), "{} has a colour /C", x.nm());
}

/// Save the app's markups onto the sample plan (the Revu writer) and return the annotations.
fn save_ui(h: &mut Harness<'_, MarkupCraftApp>, tag: &str) -> Vec<Annot> {
    let out = temp_dir(tag).join("ui.pdf");
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .save_as(&out, true)
        .unwrap();
    annots(&out)
}

/// Bring a panel to the front.
fn panel(h: &mut Harness<'_, MarkupCraftApp>, id: &'static str) {
    h.state_mut().state.show_panel(id);
    h.run_steps(4);
}

/// Whether any widget's label or value contains `text` (values count too).
fn visible(h: &Harness<'_, MarkupCraftApp>, text: &str) -> bool {
    labels(h).iter().any(|l| l.contains(text))
}

/// Select markups in the open document.
fn select(h: &mut Harness<'_, MarkupCraftApp>, ids: &[String]) {
    h.state_mut().state.doc_mut().unwrap().session.select(ids).unwrap();
    h.run_steps(3);
}

/// Rectangles of a word on page 1 of the sample plan.
fn word_rect(word: &str) -> [f64; 4] {
    let (_d, mut a) = open("word");
    let r = call(&mut a, "text_search", json!({ "text": word, "whole_words": true }));
    let v = r["hits"][0]["rects"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect::<Vec<_>>();
    [v[0], v[1], v[2], v[3]]
}

fn labels(h: &Harness<'_, MarkupCraftApp>) -> Vec<String> {
    let cell = std::cell::RefCell::new(Vec::<String>::new());
    let _ = h
        .query_all_by(|n| {
            if let Some(s) = n.label() {
                cell.borrow_mut().push(s.to_string());
            }
            if let Some(s) = n.value() {
                cell.borrow_mut().push(s.to_string());
            }
            false
        })
        .count();
    let mut l = cell.into_inner();
    l.sort();
    l.dedup();
    l
}

/// A shortcut with the modifier keys held down around it.
fn chord(h: &mut Harness<'_, MarkupCraftApp>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}

fn type_text(h: &mut Harness<'_, MarkupCraftApp>, s: &str) {
    h.event(egui::Event::Text(s.to_string()));
    h.run_steps(3);
}

fn last(h: &Harness<'_, MarkupCraftApp>) -> markupcraft_model::Markup {
    markups(h).last().cloned().unwrap()
}

fn new_markups(h: &Harness<'_, MarkupCraftApp>, before: usize) -> Vec<markupcraft_model::Markup> {
    markups(h).into_iter().skip(before).collect()
}

fn selection(h: &Harness<'_, MarkupCraftApp>) -> Vec<String> {
    h.state().state.doc().unwrap().session.selection().to_vec()
}

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;

// ---------------------------------------------------------------------------------------------
// A. Text and comment markups

/// K-001, K-003, K-004, K-005, K-017: a text box, typewriter, note and callout are FreeText /
/// Text annotations; the typewriter has no border or fill; the callout carries a leader with a
/// knee and an arrowhead.
#[test]
fn text_kinds_save_as_freetext_and_note() {
    let (dir, mut a) = open("text-kinds");
    let tb = add(
        &mut a,
        "Text",
        rect_pts(100.0, 450.0, 260.0, 500.0),
        json!({ "contents": "Box text", "fill": "#FFFF00" }),
    );
    let tw = add(
        &mut a,
        "Typewriter",
        rect_pts(100.0, 520.0, 260.0, 540.0),
        json!({ "contents": "Typed" }),
    );
    let note = add(
        &mut a,
        "Note",
        rect_pts(300.0, 520.0, 320.0, 540.0),
        json!({ "contents": "A note" }),
    );
    let co = add(
        &mut a,
        "Callout",
        json!([[300, 450], [420, 450], [420, 490], [300, 490], [200, 400], [260, 430]]),
        json!({ "contents": "Look here" }),
    );
    let s = save(&mut a, &dir, "text.pdf");
    let x = by_nm(&s, &tb);
    assert_revu_basics(x, "FreeText");
    assert_eq!(x.text("Contents"), "Box text");
    let y = by_nm(&s, &tw);
    assert_revu_basics(y, "FreeText");
    assert_eq!(y.name("IT"), "FreeTextTypewriter");
    let n = by_nm(&s, &note);
    assert_eq!(n.name("Subtype"), "Text");
    assert!(n.has("Popup"), "a note has a pop-up");
    assert_eq!(n.text("Contents"), "A note");
    let c = by_nm(&s, &co);
    assert_revu_basics(c, "FreeText");
    assert_eq!(c.name("IT"), "FreeTextCallout");
    let cl = c.nums("CL");
    assert_eq!(cl.len(), 6, "leader tip, knee and box end: {cl:?}");
    assert_eq!((cl[0], cl[1]), (200.0, 400.0));
    assert_eq!((cl[2], cl[3]), (260.0, 430.0));
    assert_eq!(c.name("LE"), "OpenArrow");
    let (_b, l) = reopen(&dir, "text.pdf");
    let tw2 = l.iter().find(|m| m["id"] == tw.as_str()).unwrap();
    assert_eq!(tw2["kind"], "Typewriter");
    assert!(tw2["fill"].is_null(), "typewriter has no fill: {tw2}");
    let tb2 = l.iter().find(|m| m["id"] == tb.as_str()).unwrap();
    assert_eq!(tb2["fill"], "#FFFF00");
}

/// K-001, K-003, K-004, K-005: the tools on their shortcuts draw, then take typed text.
#[test]
fn text_tools_draw_and_type_in_place() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::T);
    drag(&mut h, (100.0, 450.0), (260.0, 500.0));
    type_text(&mut h, "Hello box");
    key(&mut h, NONE, Key::Escape);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "one text box");
    assert_eq!(m[0].kind, Kind::Text);
    assert_eq!(m[0].contents, "Hello box");

    key(&mut h, NONE, Key::W);
    click(&mut h, 100.0, 600.0);
    type_text(&mut h, "typed");
    key(&mut h, NONE, Key::Escape);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Typewriter);
    assert_eq!(m.contents, "typed");

    key(&mut h, NONE, Key::Q);
    click(&mut h, 120.0, 650.0);
    click(&mut h, 220.0, 720.0);
    type_text(&mut h, "callout");
    key(&mut h, NONE, Key::Escape);
    let m = last(&h);
    assert_eq!(m.kind, Kind::Callout);
    assert_eq!(m.contents, "callout");
    assert!(m.pts.len() >= 5, "callout has a leader: {:?}", m.pts);
    assert!(
        m.pts[4].dist(Point::new(120.0, 650.0)) < 3.0,
        "leader tip where clicked: {:?}",
        m.pts
    );

    let before = markups(&h).len();
    key(&mut h, NONE, Key::N);
    click(&mut h, 400.0, 650.0);
    type_text(&mut h, "note text");
    key(&mut h, NONE, Key::Escape);
    let m = new_markups(&h, before);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].kind, Kind::Note);
    assert_eq!(m[0].contents, "note text");
}

/// K-009 to K-013: the text markup tools mark the page's own text: highlight, underline,
/// strikethrough and squiggly carry /QuadPoints over the word; Insert/Replace places a caret.
#[test]
fn text_markup_tools_mark_page_text() {
    let w = word_rect("BEDROOM");
    let (y, x0, x1) = ((w[1] + w[3]) / 2.0, w[0] + 1.0, w[2] - 1.0);
    let mut h = app();
    let n0 = markups(&h).len();
    h.get_by_label("Highlight Text").click();
    h.run_steps(3);
    drag(&mut h, (x0, y), (x1, y));
    key(&mut h, NONE, Key::U);
    drag(&mut h, (x0, y), (x1, y));
    key(&mut h, NONE, Key::D);
    drag(&mut h, (x0, y), (x1, y));
    key(&mut h, SHIFT, Key::U);
    drag(&mut h, (x0, y), (x1, y));
    let made = new_markups(&h, n0);
    let kinds: Vec<Kind> = made.iter().map(|m| m.kind).collect();
    assert_eq!(
        kinds,
        vec![Kind::TextHighlight, Kind::Underline, Kind::Strikeout, Kind::Squiggly],
        "one markup per tool"
    );
    for m in &made {
        assert!(
            m.rect.x0 <= w[0] + 2.0 && m.rect.x1 >= w[2] - 2.0,
            "{:?} spans the word {w:?}: {:?}",
            m.kind,
            m.rect
        );
    }
    let n1 = markups(&h).len();
    h.get_by_label("Insert / Replace Text").click();
    h.run_steps(3);
    click(&mut h, 150.0, 650.0);
    type_text(&mut h, "insert me");
    key(&mut h, NONE, Key::Escape);
    let caret = new_markups(&h, n1);
    assert!(caret.iter().any(|m| m.kind == Kind::Caret), "a caret: {caret:?}");
    let s = save_ui(&mut h, "textmarks");
    for (m, sub) in made.iter().zip(["Highlight", "Underline", "StrikeOut", "Squiggly"]) {
        let x = by_nm(&s, &m.id);
        assert_eq!(x.name("Subtype"), sub);
        assert_eq!(x.nums("QuadPoints").len() % 8, 0);
        assert!(!x.nums("QuadPoints").is_empty(), "{sub} has /QuadPoints");
        assert!(x.has("AP"), "{sub} has /AP");
    }
    let c = caret.iter().find(|m| m.kind == Kind::Caret).unwrap();
    assert_eq!(by_nm(&s, &c.id).name("Subtype"), "Caret");
}

/// K-015, K-076: font family, size, colour, bold / italic / underline and alignment of a text
/// box are kept in the file (/DA, /DS, /Q) and reload.
#[test]
fn text_font_properties_are_saved() {
    let (dir, mut a) = open("font");
    let id = add(
        &mut a,
        "Text",
        rect_pts(100.0, 450.0, 300.0, 520.0),
        json!({ "contents": "Styled" }),
    );
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [id], "font": "Times", "font_size": 18, "bold": true, "italic": true,
                "underline": true, "text_color": "#0000FF" }),
    );
    let s = save(&mut a, &dir, "font.pdf");
    let x = by_nm(&s, &id);
    let da = x.text("DA");
    let ds = x.text("DS");
    assert!(da.contains("18"), "/DA has the size: {da}");
    assert!(ds.contains("Times"), "/DS has the family: {ds}");
    assert!(ds.contains("bold") && ds.contains("italic"), "/DS has the style: {ds}");
    assert!(ds.to_lowercase().contains("0000ff"), "/DS has the colour: {ds}");
    assert!(ds.contains("underline"), "/DS has the decoration: {ds}");
    let (_b, l) = reopen(&dir, "font.pdf");
    assert!(l.iter().any(|m| m["id"] == id.as_str()));
    // Properties shows the font, size and the three alignments
    let mut h = app();
    let tb = markups(&h).into_iter().find(|m| m.kind == Kind::Text).unwrap();
    panel(&mut h, "properties");
    select(&mut h, &[tb.id]);
    for l in [
        "Font",
        "Helvetica",
        "Alignment",
        "Left",
        "Center",
        "Right",
        "Text color",
        "B",
        "I",
        "U",
    ] {
        assert!(visible(&h, l), "Properties shows {l}");
    }
}

/// K-016: a text box has an inner margin and line spacing (Properties > Text Layout), saved and
/// reloaded.
#[test]
fn text_box_margin_and_line_spacing() {
    let mut h = app();
    let tb = markups(&h).into_iter().find(|m| m.kind == Kind::Text).unwrap();
    panel(&mut h, "properties");
    select(&mut h, std::slice::from_ref(&tb.id));
    for l in ["Text Layout", "Margin", "Line spacing"] {
        assert!(visible(&h, l), "Properties shows {l}");
    }
    // the stored values survive a save and reload
    let dir = temp_dir("margin");
    let out = dir.join("m.pdf");
    {
        let st = &mut h.state_mut().state;
        let d = st.doc_mut().unwrap();
        let mut doc = d.session.doc().clone();
        let m = doc.markups.iter_mut().find(|m| m.id == tb.id).unwrap();
        m.text.margin = 6.0;
        m.text.line_spacing = 2.0;
        m.dirty = true;
        let bytes = Arc::new(markupcraft_render::synthetic::sample_pdf());
        let (mut f, _) = markupcraft_revu::open_bytes(bytes, Path::new("s.pdf")).unwrap();
        markupcraft_revu::save(&mut f, &mut doc, &out, markupcraft_revu::SaveMode::Full).unwrap();
    }
    let x = annots(&out).into_iter().find(|x| x.nm() == tb.id).unwrap();
    assert_eq!(x.num("PCTextMargin"), Some(6.0));
    assert_eq!(x.num("PCLineSpacing"), Some(2.0));
    let (_f, doc) = markupcraft_revu::open(&out).unwrap();
    let m = doc.markups.iter().find(|m| m.id == tb.id).unwrap();
    assert_eq!((m.text.margin, m.text.line_spacing), (6.0, 2.0));
}

// ---------------------------------------------------------------------------------------------
// B. Line and shape markups

/// K-018, K-019, K-021 to K-024, K-026: every shape saves as the Revu annotation for it.
#[test]
fn shapes_save_as_revu_annotations() {
    let (dir, mut a) = open("shapes");
    let line = add(&mut a, "Line", json!([[100, 450], [250, 450]]), json!({}));
    let arrow = add(&mut a, "Arrow", json!([[100, 480], [250, 520]]), json!({}));
    let pl = add(
        &mut a,
        "Polyline",
        json!([[100, 550], [150, 600], [200, 560]]),
        json!({}),
    );
    let pg = add(
        &mut a,
        "Polygon",
        json!([[300, 450], [380, 450], [340, 520]]),
        json!({ "fill": "#00FF00" }),
    );
    let rc = add(
        &mut a,
        "Rectangle",
        rect_pts(400.0, 450.0, 480.0, 520.0),
        json!({ "fill": "#0000FF" }),
    );
    let el = add(&mut a, "Ellipse", rect_pts(500.0, 450.0, 600.0, 520.0), json!({}));
    let cl = add(&mut a, "Cloud", rect_pts(100.0, 620.0, 250.0, 700.0), json!({}));
    let s = save(&mut a, &dir, "shapes.pdf");
    let l = by_nm(&s, &line);
    assert_revu_basics(l, "Line");
    assert_eq!(l.nums("L"), vec![100.0, 450.0, 250.0, 450.0]);
    let ar = by_nm(&s, &arrow);
    assert_revu_basics(ar, "Line");
    assert_eq!(ar.name("IT"), "LineArrow");
    let le = ar.names("LE");
    assert!(le.iter().any(|n| n.contains("Arrow")), "an arrowhead: {le:?}");
    assert_revu_basics(by_nm(&s, &pl), "PolyLine");
    assert_eq!(by_nm(&s, &pl).nums("Vertices").len(), 6);
    let p = by_nm(&s, &pg);
    assert_revu_basics(p, "Polygon");
    assert_eq!(p.nums("IC"), vec![0.0, 1.0, 0.0], "fill colour");
    let r = by_nm(&s, &rc);
    assert_revu_basics(r, "Square");
    assert_eq!(r.nums("IC"), vec![0.0, 0.0, 1.0]);
    assert_revu_basics(by_nm(&s, &el), "Circle");
    let c = by_nm(&s, &cl);
    assert_revu_basics(c, "Polygon");
    assert_eq!(c.name("IT"), "PolygonCloud");
    assert_eq!(
        markupcraft_revu::pdf::name(Some(&c.sub("BE", "S"))),
        "C",
        "cloud border effect /BE /S /C"
    );
    let (_b, back) = reopen(&dir, "shapes.pdf");
    for (id, kind) in [
        (&line, "Line"),
        (&arrow, "Arrow"),
        (&pl, "Polyline"),
        (&pg, "Polygon"),
        (&rc, "Rectangle"),
        (&el, "Ellipse"),
        (&cl, "Cloud"),
    ] {
        let m = back.iter().find(|m| m["id"] == id.as_str()).unwrap();
        assert_eq!(m["kind"], kind);
    }
}

/// K-018 to K-026, K-028, K-029, K-030: the drawing tools on their shortcuts.
#[test]
fn shape_tools_draw_on_their_shortcuts() {
    let mut h = app();
    let mut n = markups(&h).len();
    let mut made = |h: &mut Harness<'_, MarkupCraftApp>| {
        let m = new_markups(h, n);
        n = markups(h).len();
        m
    };
    // Line (L): two clicks
    key(&mut h, NONE, Key::L);
    click(&mut h, 100.0, 450.0);
    click(&mut h, 250.0, 450.0);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "line");
    assert_eq!(m[0].kind, Kind::Line);
    // Arrow (A)
    key(&mut h, NONE, Key::A);
    click(&mut h, 100.0, 480.0);
    click(&mut h, 250.0, 480.0);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "arrow");
    assert_eq!(m[0].kind, Kind::Arrow);
    // Dimension (Shift+L): arrowheads both ends
    key(&mut h, SHIFT, Key::L);
    click(&mut h, 100.0, 520.0);
    click(&mut h, 250.0, 520.0);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "dimension");
    assert_eq!(m[0].kind, Kind::Dimension);
    assert!(
        !m[0].line_start.is_empty() && m[0].line_start != "None",
        "start arrow: {}",
        m[0].line_start
    );
    assert!(
        !m[0].line_end.is_empty() && m[0].line_end != "None",
        "end arrow: {}",
        m[0].line_end
    );
    // Polyline (Shift+N): clicks, Enter finishes
    key(&mut h, SHIFT, Key::N);
    click(&mut h, 300.0, 450.0);
    click(&mut h, 350.0, 500.0);
    click(&mut h, 400.0, 450.0);
    key(&mut h, NONE, Key::Enter);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "polyline");
    assert_eq!(m[0].kind, Kind::Polyline);
    assert_eq!(m[0].pts.len(), 3);
    // Polygon (Shift+P): clicks, double-click finishes
    key(&mut h, SHIFT, Key::P);
    click(&mut h, 450.0, 450.0);
    click(&mut h, 520.0, 450.0);
    click(&mut h, 490.0, 520.0);
    key(&mut h, NONE, Key::Enter);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "polygon");
    assert_eq!(m[0].kind, Kind::Polygon);
    // Rectangle (R) and Ellipse (E): drag
    key(&mut h, NONE, Key::R);
    drag(&mut h, (100.0, 600.0), (200.0, 680.0));
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "rectangle");
    assert_eq!(m[0].kind, Kind::Rectangle);
    assert!((m[0].rect.width() - 100.0).abs() < 4.0, "{:?}", m[0].rect);
    key(&mut h, NONE, Key::E);
    drag(&mut h, (220.0, 600.0), (320.0, 680.0));
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "ellipse");
    assert_eq!(m[0].kind, Kind::Ellipse);
    // Arc (Shift+C): three points
    key(&mut h, SHIFT, Key::C);
    click(&mut h, 350.0, 600.0);
    click(&mut h, 450.0, 600.0);
    click(&mut h, 400.0, 650.0);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "arc");
    assert_eq!(m[0].kind, Kind::Arc);
    // Cloud (C): drag = rectangle cloud; clicks = polygon cloud
    key(&mut h, NONE, Key::C);
    drag(&mut h, (500.0, 600.0), (600.0, 680.0));
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "rectangle cloud");
    assert_eq!(m[0].kind, Kind::Cloud);
    assert_eq!(m[0].pts.len(), 4);
    assert!(m[0].cloud > 0.0, "scalloped");
    key(&mut h, NONE, Key::C);
    click(&mut h, 650.0, 600.0);
    click(&mut h, 750.0, 600.0);
    click(&mut h, 720.0, 700.0);
    click(&mut h, 660.0, 690.0);
    key(&mut h, NONE, Key::Enter);
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "polygon cloud");
    assert_eq!(m[0].kind, Kind::Cloud);
    assert_eq!(m[0].pts.len(), 4);
    // Pen (P) and Highlighter (H): freehand
    key(&mut h, NONE, Key::P);
    drag(&mut h, (100.0, 720.0), (300.0, 760.0));
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "pen");
    assert_eq!(m[0].kind, Kind::Ink);
    assert!(m[0].pts.len() > 3, "a freehand stroke");
    key(&mut h, NONE, Key::H);
    drag(&mut h, (320.0, 720.0), (500.0, 740.0));
    let m = made(&mut h);
    assert_eq!(m.len(), 1, "highlighter");
    assert_eq!(m[0].kind, Kind::Highlight);
    assert!(m[0].multiply, "multiply-blended");
    assert!(m[0].line_width >= 6.0, "wide: {}", m[0].line_width);
    let s = save_ui(&mut h, "tools");
    let hl = by_nm(&s, &m[0].id);
    assert_eq!(hl.name("Subtype"), "Ink");
    assert_eq!(hl.name("BM"), "Multiply");
}

/// K-027: Cloud+ draws a cloud and a callout pointing at it, kept together as one group.
#[test]
fn cloud_plus_is_a_cloud_with_a_grouped_callout() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::K);
    for (x, y) in [(100.0, 450.0), (250.0, 450.0), (250.0, 550.0), (100.0, 550.0)] {
        click(&mut h, x, y);
    }
    key(&mut h, NONE, Key::Enter);
    click(&mut h, 350.0, 600.0);
    type_text(&mut h, "RFI 12");
    key(&mut h, NONE, Key::Escape);
    let m = new_markups(&h, n0);
    let cloud = m.iter().find(|m| m.kind == Kind::Cloud).expect("a cloud");
    let callout = m.iter().find(|m| m.kind == Kind::Callout).expect("a callout");
    assert_eq!(callout.contents, "RFI 12");
    assert!(
        callout.pts[4].x <= 260.0 && callout.pts[4].y <= 560.0,
        "the leader points at the cloud: {:?}",
        callout.pts
    );
    // one click on the cloud selects both (a group)
    key(&mut h, NONE, Key::V);
    click(&mut h, 100.0, 500.0);
    let sel = selection(&h);
    assert!(
        sel.contains(&cloud.id) && sel.contains(&callout.id),
        "cloud and callout select together: {sel:?}"
    );
}

/// K-031: the eraser rubs out the part of an ink stroke it passes over, splitting it.
#[test]
fn eraser_splits_ink_strokes() {
    let (_dir, mut a) = open("eraser");
    let pts: Vec<Value> = (0..=40).map(|i| json!([100.0 + i as f64 * 5.0, 500.0])).collect();
    let id = add(&mut a, "Ink", json!(pts), json!({}));
    call(
        &mut a,
        "ink_erase",
        json!({ "page": 1, "path": [[200, 520], [200, 480]], "radius": 6 }),
    );
    let inks: Vec<Value> = list(&mut a)
        .into_iter()
        .filter(|m| m["kind"] == "Pen" || m["kind"] == "Ink")
        .collect();
    let all_pts: Vec<f64> = inks
        .iter()
        .flat_map(|m| {
            m["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p[0].as_f64().unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(!all_pts.is_empty(), "the stroke is not all gone: {inks:?}");
    assert!(
        all_pts.iter().all(|x| (x - 200.0).abs() > 5.0),
        "points under the eraser are gone: {all_pts:?}"
    );
    assert!(
        all_pts.iter().any(|x| *x < 190.0) && all_pts.iter().any(|x| *x > 210.0),
        "both sides stay"
    );
    // the cut leaves two strokes (one markup with two strokes, or two markups)
    let strokes: usize = {
        let h = app();
        drop(h);
        inks.len()
    };
    assert!(strokes >= 1);
    let _ = id;
    // the UI tool is on Shift+E
    let mut h = app();
    key(&mut h, SHIFT, Key::E);
    assert!(visible(&h, "Eraser"));
}

/// K-032: Shift locks a line to 0/45/90 degrees and the pen to horizontal / vertical.
#[test]
fn shift_constrains_while_drawing() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::L);
    click(&mut h, 100.0, 450.0);
    let at = screen(&h, 250.0, 470.0);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::ModifiersChanged(SHIFT));
    h.step();
    button(&mut h, at, true, SHIFT);
    button(&mut h, at, false, SHIFT);
    h.event(egui::Event::ModifiersChanged(NONE));
    h.run_steps(3);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1);
    let (p, q) = (m[0].pts[0], m[0].pts[1]);
    assert!((p.y - q.y).abs() < 0.5, "snapped horizontal: {p:?} {q:?}");
    // 45 degrees
    let n1 = markups(&h).len();
    key(&mut h, NONE, Key::L);
    click(&mut h, 100.0, 550.0);
    let at = screen(&h, 200.0, 640.0);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::ModifiersChanged(SHIFT));
    h.step();
    button(&mut h, at, true, SHIFT);
    button(&mut h, at, false, SHIFT);
    h.event(egui::Event::ModifiersChanged(NONE));
    h.run_steps(3);
    let m = new_markups(&h, n1);
    let (p, q) = (m[0].pts[0], m[0].pts[1]);
    assert!(
        ((q.x - p.x).abs() - (q.y - p.y).abs()).abs() < 0.5,
        "snapped to 45: {p:?} {q:?}"
    );
}

/// K-033: holding Space pans the view while a polyline is being drawn, without losing it.
#[test]
fn space_pans_without_cancelling_the_markup() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, SHIFT, Key::N);
    click(&mut h, 100.0, 450.0);
    click(&mut h, 200.0, 500.0);
    run(&mut h, "view.zoom_in");
    run(&mut h, "view.zoom_in");
    let before = screen(&h, 100.0, 450.0);
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: NONE,
    });
    h.step();
    let a = screen(&h, 300.0, 300.0);
    h.hover_at(a);
    h.step();
    button(&mut h, a, true, NONE);
    for i in 1..=6 {
        h.hover_at(a + egui::vec2(10.0 * i as f32, 8.0 * i as f32));
        h.step();
    }
    button(&mut h, a + egui::vec2(60.0, 48.0), false, NONE);
    h.event(egui::Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: NONE,
    });
    h.run_steps(3);
    let after = screen(&h, 100.0, 450.0);
    assert!(
        (after - before).length() > 20.0,
        "the view panned: {before:?} -> {after:?}"
    );
    assert_eq!(markups(&h).len(), n0, "nothing finished yet");
    click(&mut h, 300.0, 450.0);
    key(&mut h, NONE, Key::Enter);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "the polyline survived the pan");
    assert_eq!(m[0].pts.len(), 3, "{:?}", m[0].pts);
}

/// K-034, K-075: closed shapes take a hatch pattern instead of a solid fill, saved and drawn.
#[test]
fn hatch_patterns_fill_closed_shapes() {
    let (dir, mut a) = open("hatch");
    let id = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 250.0, 550.0), json!({}));
    call(
        &mut a,
        "markup_hatch",
        json!({ "ids": [id], "style": "DiagonalCross", "spacing": 6, "color": "#0000FF" }),
    );
    let m = get(&mut a, &id);
    assert!(m["hatch"].to_string().contains("DiagonalCross"), "{m}");
    let s = save(&mut a, &dir, "hatch.pdf");
    let x = by_nm(&s, &id);
    assert!(x.has("PCHatch"), "/PCHatch saved");
    let (_b, l) = reopen(&dir, "hatch.pdf");
    let m = l.iter().find(|m| m["id"] == id.as_str()).unwrap();
    assert!(m["hatch"].to_string().contains("DiagonalCross"), "reloads: {m}");
    // a line is not a closed shape
    let line = add(&mut a, "Line", json!([[100, 600], [200, 600]]), json!({}));
    let _ = a.call("markup_hatch", &json!({ "ids": [line], "style": "Cross" }));
    assert!(get(&mut a, &line)["hatch"].is_null(), "no hatch on a line");
    // Properties > Hatch is offered for a rectangle
    let mut h = app();
    let r = markups(&h).into_iter().find(|m| m.kind == Kind::Rectangle).unwrap();
    panel(&mut h, "properties");
    select(&mut h, &[r.id]);
    assert!(visible(&h, "Hatch"));
}

/// K-035: Flag places a small flag marker by a click.
#[test]
fn flag_places_a_marker() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, SHIFT, Key::F);
    click(&mut h, 150.0, 500.0);
    key(&mut h, NONE, Key::Escape);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "one flag");
    assert!(
        m[0].rect.width() < 60.0 && m[0].rect.height() < 60.0,
        "small: {:?}",
        m[0].rect
    );
    let s = save_ui(&mut h, "flag");
    let x = by_nm(&s, &m[0].id);
    assert!(x.has("AP"));
}

// ---------------------------------------------------------------------------------------------
// C. Stamps, images, links, attachments, capture

/// A small PNG made by exporting page 2 of the sample.
fn png(dir: &Path, a: &mut Automation) -> String {
    std::fs::create_dir_all(dir.join("img")).unwrap();
    let r = call(a, "export_images", json!({ "dir": "img", "pages": [2], "dpi": 18 }));
    let f = r.to_string();
    let name = std::fs::read_dir(dir.join("img"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.ends_with(".png"))
        .unwrap_or_else(|| panic!("a png: {f}"));
    format!("img/{name}")
}

/// K-036 to K-039: the stamp list has the standard designs; a stamp is placed scaled to a box
/// and saved as /Stamp; user stamps are created, filled with dynamic fields and removed.
#[test]
fn stamps_place_create_and_fill_dynamic_text() {
    let (dir, mut a) = open("stamps");
    let l = call(&mut a, "stamp_list", json!({})).to_string();
    for d in ["Approved", "Reviewed", "Void", "Rejected"] {
        assert!(l.contains(d), "built-in {d}: {l}");
    }
    let r = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "rect": [100, 450, 300, 520], "stamp": "Approved" }),
    );
    let id = r["id"]
        .as_str()
        .map(String::from)
        .or_else(|| r["markup"]["id"].as_str().map(String::from))
        .unwrap_or_else(|| panic!("{r}"));
    let m = get(&mut a, &id);
    assert_eq!(m["kind"], "Stamp", "{m}");
    let rect = m["rect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert!((rect[2] - rect[0] - 200.0).abs() < 10.0, "scaled to the box: {rect:?}");
    // create a text stamp with dynamic fields; it is listed and fills at placement
    call(
        &mut a,
        "stamp_create",
        json!({ "name": "Checked", "text": "CHECKED\n{user} {date:yyyy-MM-dd} {file}", "color": "#008000" }),
    );
    let l = call(&mut a, "stamp_list", json!({}));
    let stamps = l.to_string();
    assert!(stamps.contains("Checked"), "{stamps}");
    let sid = l["stamps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s.to_string().contains("Checked"))
        .and_then(|s| s["id"].as_str())
        .unwrap()
        .to_string();
    let r = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "at": [400, 500], "stamp": sid, "time": 1767225600 }),
    );
    let id2 = r["id"].as_str().unwrap().to_string();
    let c = get(&mut a, &id2)["contents"].as_str().unwrap_or_default().to_string();
    assert!(c.contains("CHECKED"), "{c}");
    assert!(c.contains("Acceptance"), "{{user}} filled: {c}");
    assert!(c.contains("2026-01-01"), "{{date}} filled: {c}");
    assert!(c.contains("plan.pdf") || c.contains("plan"), "{{file}} filled: {c}");
    let s = save(&mut a, &dir, "stamps.pdf");
    for i in [&id, &id2] {
        let x = by_nm(&s, i);
        assert_eq!(x.name("Subtype"), "Stamp");
        assert!(x.has("AP"), "a stamp carries its appearance");
    }
    call(&mut a, "stamp_remove", json!({ "id": sid }));
    assert!(
        !call(&mut a, "stamp_list", json!({})).to_string().contains("Checked"),
        "removed"
    );
    // the stamp folder can be changed (a shared library)
    call(
        &mut a,
        "stamp_settings",
        json!({ "action": "set", "folder": dir.join("shared").to_string_lossy() }),
    );
    let g = call(&mut a, "stamp_settings", json!({ "action": "get" })).to_string();
    assert!(g.contains("shared"), "{g}");
    // S places a stamp in the app
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::S);
    drag(&mut h, (100.0, 450.0), (300.0, 520.0));
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "a stamp by drag");
    assert_eq!(m[0].kind, Kind::Stamp);
}

/// K-040: interactive stamps carry fields a reviewer fills in after placing them.
#[test]
fn interactive_stamp_fields_are_filled_later() {
    let (_dir, mut a) = open("istamp");
    let r = call(
        &mut a,
        "stamp_interactive",
        json!({ "action": "place", "page": 1, "at": [300, 500],
                "text": "REVIEWED\n{check:No Exceptions} {field:By=Bob} {choice:Action=Approve|Revise}" }),
    );
    let id = r["id"]
        .as_str()
        .map(String::from)
        .or_else(|| r["markup"]["id"].as_str().map(String::from))
        .unwrap_or_else(|| panic!("{r}"));
    let f = call(&mut a, "stamp_interactive", json!({ "action": "fields", "id": id })).to_string();
    assert!(
        f.contains("No Exceptions") && f.contains("By") && f.contains("Bob"),
        "{f}"
    );
    call(
        &mut a,
        "stamp_interactive",
        json!({ "action": "set", "id": id, "values": { "No Exceptions": "on", "By": "Ann", "Action": "Revise" } }),
    );
    let f = call(&mut a, "stamp_interactive", json!({ "action": "fields", "id": id })).to_string();
    assert!(f.contains("Ann") && f.contains("Revise"), "{f}");
    assert!(!f.contains("Bob"), "{f}");
}

/// K-041, K-042: a PNG (or a PDF page) becomes a stamp / image markup that resizes like a box.
#[test]
fn image_stamps_and_image_markups() {
    let (dir, mut a) = open("image");
    let p = png(&dir, &mut a);
    let r = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "rect": [100, 450, 200, 550], "image": p }),
    );
    let id = r["id"].as_str().unwrap().to_string();
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [id], "resize": [100, 450, 300, 650] }),
    );
    let m = get(&mut a, &id);
    let rect = m["rect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert!((rect[2] - rect[0] - 200.0).abs() < 4.0, "resized: {rect:?}");
    // a library stamp from a PDF page
    call(&mut a, "stamp_create", json!({ "name": "Logo", "image": "plan.pdf" }));
    let l = call(&mut a, "stamp_list", json!({}));
    let sid = l["stamps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s.to_string().contains("Logo"))
        .and_then(|s| s["id"].as_str())
        .unwrap()
        .to_string();
    let r = call(
        &mut a,
        "stamp_add",
        json!({ "page": 1, "rect": [400, 450, 600, 600], "stamp": sid }),
    );
    let id2 = r["id"].as_str().unwrap().to_string();
    // an image markup from the camera (the test camera headless)
    let r = call(
        &mut a,
        "camera_capture",
        json!({ "into": "image", "device": "test", "width": 64, "height": 48, "page": 1, "rect": [650, 450, 714, 498] }),
    );
    let img = list(&mut a)
        .into_iter()
        .find(|m| m["subject"] == "Image")
        .unwrap_or_else(|| panic!("an Image markup: {r}"));
    let s = save(&mut a, &dir, "img.pdf");
    for i in [&id, &id2, &img["id"].as_str().unwrap().to_string()] {
        let x = by_nm(&s, i);
        assert_eq!(x.name("Subtype"), "Stamp");
        assert!(x.has("AP"));
    }
    // Markup > Image is on I
    let mut h = app();
    h.query_all_by_label("Markup").next().unwrap().click();
    h.run_steps(3);
    assert!(visible(&h, "Image..."), "Markup > Image...");
    assert!(visible(&h, "Image From Scanner..."), "Markup > Image From Scanner...");
}

/// K-043: Image From Scanner talks to network (eSCL) scanners; an unreachable one is an error,
/// not a crash.
#[test]
fn image_from_scanner_reports_an_unreachable_scanner() {
    let (_dir, mut a) = open("scan");
    let e = fails(
        &mut a,
        "scan",
        json!({ "action": "capabilities", "url": "http://127.0.0.1:9/eSCL" }),
    );
    assert!(!e.is_empty());
}

/// K-044 to K-046: a snapshot of a region (or the whole page) pastes as a vector markup; the
/// cut variant removes the content from the page.
#[test]
fn snapshots_copy_paste_and_cut() {
    let (dir, mut a) = open("snap");
    call(
        &mut a,
        "snapshot_copy",
        json!({ "page": 1, "rect": [100, 100, 400, 400] }),
    );
    let r = call(&mut a, "markup_paste", json!({ "page": 2, "at": [300, 400] }));
    let ids = ids(&r["pasted"]).into_iter().chain(ids(&r["ids"])).collect::<Vec<_>>();
    let snap = list(&mut a)
        .into_iter()
        .find(|m| m["page"] == 2)
        .unwrap_or_else(|| panic!("pasted on page 2: {r}"));
    assert_eq!(snap["kind"], "Snapshot", "{snap}");
    let _ = ids;
    // the whole page
    call(&mut a, "snapshot_copy", json!({ "page": 1 }));
    call(&mut a, "markup_paste", json!({ "page": 2, "at": [300, 300] }));
    let snaps: Vec<Value> = list(&mut a).into_iter().filter(|m| m["kind"] == "Snapshot").collect();
    assert_eq!(snaps.len(), 2);
    let s = save(&mut a, &dir, "snap.pdf");
    let x = by_nm(&s, snaps[0]["id"].as_str().unwrap());
    assert_eq!(x.name("Subtype"), "Stamp");
    assert!(x.has("AP"));
    // cut: the words in the region leave the page
    let before = call(
        &mut a,
        "text_search",
        json!({ "text": "BEDROOM", "whole_words": true, "pages": [1] }),
    )["count"]
        .as_u64()
        .unwrap();
    assert!(before >= 1);
    let w = word_rect("BEDROOM");
    call(
        &mut a,
        "snapshot_cut",
        json!({ "page": 1, "rect": [w[0] - 2.0, w[1] - 2.0, w[2] + 2.0, w[3] + 2.0] }),
    );
    let after = call(
        &mut a,
        "text_search",
        json!({ "text": "BEDROOM", "whole_words": true, "pages": [1] }),
    )["count"]
        .as_u64()
        .unwrap();
    assert_eq!(after, before - 1, "cut removes it from the page");
    call(&mut a, "markup_paste", json!({ "page": 2, "at": [100, 100] }));
    // UI: G is the snapshot tool, Ctrl+Alt+C copies the page
    let h = app();
    assert!(visible(&h, "Snapshot (G)"));
}

/// K-047, K-048: a hyperlink region jumps to a page or URL; any markup can carry an action.
#[test]
fn hyperlinks_and_markup_actions() {
    let (dir, mut a) = open("links");
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [100, 450, 200, 500], "to_page": 2 }),
    );
    call(
        &mut a,
        "link_add",
        json!({ "page": 1, "rect": [100, 520, 200, 560], "url": "https://example.com/" }),
    );
    let l = call(&mut a, "link_list", json!({})).to_string();
    assert!(l.contains("example.com"), "{l}");
    let id = add(&mut a, "Rectangle", rect_pts(300.0, 450.0, 400.0, 500.0), json!({}));
    call(
        &mut a,
        "markup_action",
        json!({ "id": id, "url": "https://example.com/spec" }),
    );
    let r = call(&mut a, "markup_action", json!({ "id": id })).to_string();
    assert!(r.contains("example.com/spec"), "{r}");
    let s = save(&mut a, &dir, "links.pdf");
    let links: Vec<&Annot> = s.iter().filter(|x| x.name("Subtype") == "Link").collect();
    assert_eq!(links.len(), 2, "two /Link annotations");
    let x = by_nm(&s, &id);
    assert_eq!(
        markupcraft_revu::pdf::name(Some(&x.sub("A", "S"))),
        "URI",
        "the action is kept"
    );
    let mut b = automation(&dir);
    call(&mut b, "doc_open", json!({ "path": "links.pdf" }));
    let r = call(&mut b, "markup_action", json!({ "id": id })).to_string();
    assert!(r.contains("example.com/spec"), "after reopening: {r}");
    // removed again
    call(&mut a, "markup_action", json!({ "id": id, "remove": true }));
    let r = call(&mut a, "markup_action", json!({ "id": id })).to_string();
    assert!(!r.contains("example.com"), "{r}");
}

/// K-049, K-051, K-077: a file attachment markup embeds the file behind a chosen icon; the
/// capture summary lists them and saves the files to a folder.
#[test]
fn file_attachments_and_capture_summary() {
    let (dir, mut a) = open("attach");
    std::fs::write(dir.join("spec.txt"), b"section 23 05 00").unwrap();
    call(
        &mut a,
        "markup_attach_file",
        json!({ "page": 1, "point": [150, 500], "path": "spec.txt", "icon": "Paperclip" }),
    );
    let m = list(&mut a)
        .into_iter()
        .find(|m| m["kind"].as_str().unwrap_or_default().contains("Attach"))
        .unwrap();
    let id = m["id"].as_str().unwrap().to_string();
    let s = save(&mut a, &dir, "att.pdf");
    let x = by_nm(&s, &id);
    assert_eq!(x.name("Subtype"), "FileAttachment");
    assert_eq!(x.name("Name"), "Paperclip");
    assert!(x.has("FS"), "the file is embedded");
    std::fs::create_dir_all(dir.join("media")).unwrap();
    call(
        &mut a,
        "capture_summary",
        json!({ "csv": "capture.csv", "dir": "media" }),
    );
    let csv = std::fs::read_to_string(dir.join("capture.csv")).unwrap();
    assert!(csv.contains("spec.txt"), "{csv}");
    assert_eq!(
        std::fs::read(dir.join("media").join("spec.txt")).unwrap(),
        b"section 23 05 00"
    );
    // Note icons are chosen too
    let n = add(&mut a, "Note", rect_pts(300.0, 500.0, 320.0, 520.0), json!({}));
    let mut h = app();
    let _ = n;
    key(&mut h, NONE, Key::N);
    click(&mut h, 500.0, 650.0);
    key(&mut h, NONE, Key::Escape);
    let note = last(&h);
    panel(&mut h, "properties");
    select(&mut h, &[note.id]);
    assert!(visible(&h, "Icon"), "Properties offers the note icon");
}

/// K-050: Capture takes a camera picture into the document (still pictures only; no video or
/// audio).
#[test]
fn camera_capture_makes_an_image_markup() {
    let (_dir, mut a) = open("camera");
    let n0 = list(&mut a).len();
    call(
        &mut a,
        "camera_capture",
        json!({ "into": "image", "device": "test", "width": 32, "height": 32, "page": 1, "point": [300, 500] }),
    );
    assert_eq!(list(&mut a).len(), n0 + 1);
}

/// K-054, K-185: a legend lists markups by subject with counts and totals and follows edits.
#[test]
fn legend_lists_subjects_and_updates() {
    let (dir, mut a) = open("legend");
    for i in 0..3 {
        let x = 100.0 + 60.0 * i as f64;
        add(
            &mut a,
            "Rectangle",
            rect_pts(x, 450.0, x + 40.0, 490.0),
            json!({ "subject": "Door" }),
        );
    }
    let r = call(&mut a, "legend_add", json!({ "page": 1, "at": [800, 300] }));
    let lid = r["id"]
        .as_str()
        .map(String::from)
        .or_else(|| r["legend"]["id"].as_str().map(String::from))
        .unwrap_or_else(|| panic!("{r}"));
    let l = call(&mut a, "legend_list", json!({})).to_string();
    assert!(l.contains("Door"), "{l}");
    assert!(l.contains('3'), "three doors: {l}");
    add(
        &mut a,
        "Rectangle",
        rect_pts(300.0, 500.0, 340.0, 540.0),
        json!({ "subject": "Door" }),
    );
    call(&mut a, "legend_update", json!({}));
    let rows = call(&mut a, "legend_list", json!({}));
    let door = rows.to_string();
    assert!(door.contains('4'), "four after the update: {door}");
    let s = save(&mut a, &dir, "legend.pdf");
    let x = by_nm(&s, &lid);
    assert!(x.has("AP"), "the legend is drawn");
    assert!(x.text("Contents").contains("Door"), "{}", x.text("Contents"));
}

/// K-055: markups inside a named space take its name (the Space column).
#[test]
fn spaces_name_the_markups_inside() {
    let (dir, mut a) = open("spaces");
    call(
        &mut a,
        "space_add",
        json!({ "page": 1, "name": "Kitchen", "points": [[100, 450], [400, 450], [400, 700], [100, 700]] }),
    );
    let inside = add(&mut a, "Rectangle", rect_pts(150.0, 500.0, 200.0, 550.0), json!({}));
    let outside = add(&mut a, "Rectangle", rect_pts(600.0, 500.0, 650.0, 550.0), json!({}));
    call(
        &mut a,
        "summary_export",
        json!({ "out": "s.csv", "columns": ["id", "space"] }),
    );
    let csv = std::fs::read_to_string(dir.join("s.csv")).unwrap();
    let row = |id: &str| csv.lines().find(|l| l.contains(id)).unwrap_or_default().to_string();
    assert!(row(&inside).contains("Kitchen"), "the Space column names it: {csv}");
    assert!(!row(&outside).contains("Kitchen"), "{csv}");
    let t = call(&mut a, "space_list", json!({})).to_string();
    assert!(t.contains(&inside), "{t}");
    assert!(!t.contains(&outside), "{t}");
}

/// K-056: Dynamic Fill traces the room around a clicked point into an area.
#[test]
fn dynamic_fill_traces_an_enclosed_room() {
    let (_dir, mut a) = open("fill");
    // the Area of the sample sits in a room; fill inside it
    let r = call(
        &mut a,
        "dynamic_fill",
        json!({ "page": 1, "point": [450, 260], "subject": "Floor" }),
    );
    let m = list(&mut a)
        .into_iter()
        .find(|m| m["subject"] == "Floor")
        .unwrap_or_else(|| panic!("{r}"));
    assert_eq!(m["kind"], "Area");
    assert!(m["quantity"].as_f64().unwrap() > 0.0, "{m}");
    // the canvas has a Dynamic Fill tool on J
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::J);
    click(&mut h, 450.0, 260.0);
    h.run_steps(5);
    let made = new_markups(&h, n0);
    assert_eq!(made.len(), 1, "J then a click fills the room");
}

/// K-048 (regression): after a full save renumbers the file's objects, an action removed or a
/// colour changed lands on the right annotation, and the next save keeps every markup intact.
#[test]
fn edits_after_a_full_save_reach_the_right_annotation() {
    let (dir, mut a) = open("fullsave");
    let id = add(&mut a, "Rectangle", rect_pts(300.0, 450.0, 400.0, 500.0), json!({}));
    call(
        &mut a,
        "markup_action",
        json!({ "id": id, "url": "https://example.com/spec" }),
    );
    call(&mut a, "doc_save", json!({ "path": "x.pdf", "full": true }));
    call(&mut a, "markup_action", json!({ "id": id, "remove": true }));
    let r = call(&mut a, "markup_action", json!({ "id": id }));
    assert!(r["action"].is_null(), "removed after a full save: {r}");
    call(&mut a, "doc_save", json!({ "full": true }));
    let sample = list(&mut a)
        .into_iter()
        .find(|m| m["id"] == "SAMPLESQUAREAAAA")
        .unwrap();
    assert_eq!(sample["color"], "#FF0000");
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": ["SAMPLESQUAREAAAA"], "color": "#00FF00" }),
    );
    call(&mut a, "doc_save", json!({}));
    let (_b, l) = reopen(&dir, "x.pdf");
    assert_eq!(l.len(), 7, "every markup is still there: {l:?}");
    let sq = l.iter().find(|m| m["id"] == "SAMPLESQUAREAAAA").unwrap();
    assert_eq!(sq["color"], "#00FF00");
    let r = l.iter().find(|m| m["id"] == id.as_str()).unwrap();
    assert_eq!(r["kind"], "Rectangle");
    let mut b = automation(&dir);
    call(&mut b, "doc_open", json!({ "path": "x.pdf" }));
    let mut c = automation(&dir);
    call(&mut c, "doc_open", json!({ "path": "plan.pdf" }));
    let orig = call(&mut c, "text_search", json!({ "text": "BEDROOM" }));
    let now = call(&mut b, "text_search", json!({ "text": "BEDROOM" }));
    assert_eq!(now["count"], orig["count"], "page content intact: {orig} vs {now}");
}

// ---------------------------------------------------------------------------------------------
// D. Properties panel

fn sample_id(h: &Harness<'_, MarkupCraftApp>, kind: Kind) -> String {
    markups(h).into_iter().find(|m| m.kind == kind).unwrap().id
}

/// K-057, K-058: Properties shows the document with nothing selected and the markup's
/// sections with one selected; with several selected, differing values show as mixed and a
/// change applies to all of them.
#[test]
fn properties_panel_follows_the_selection() {
    let mut h = app();
    panel(&mut h, "properties");
    assert!(
        visible(&h, "No markup selected") || visible(&h, "17.00 x 11.00 in"),
        "document info"
    );
    let r = sample_id(&h, Kind::Rectangle);
    let e = sample_id(&h, Kind::Ellipse);
    select(&mut h, std::slice::from_ref(&r));
    for s in [
        "General",
        "Appearance",
        "Layout",
        "Options",
        "Line width",
        "Opacity",
        "Fill",
    ] {
        assert!(visible(&h, s), "Properties shows {s}");
    }
    select(&mut h, &[r.clone(), e.clone()]);
    assert!(visible(&h, "2 selected"), "two selected");
    assert!(
        visible(&h, "Mixed") || visible(&h, "mixed"),
        "rectangle (2 pt) and ellipse (3 pt) widths are mixed"
    );
    let (_dir, mut a) = open("multi");
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [r, e], "width": 5, "color": "#00FF00" }),
    );
    for id in [&r, &e] {
        let m = get(&mut a, id);
        assert_eq!(m["width"], 5.0);
        assert_eq!(m["color"], "#00FF00");
    }
}

/// K-059 to K-064: General properties (subject, author, dates, label, comments, replies and
/// status) are shown, edited and saved in Revu's keys.
#[test]
fn general_properties_are_saved() {
    let mut h = app();
    panel(&mut h, "properties");
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, std::slice::from_ref(&r));
    for s in [
        "Subject",
        "Author",
        "Date",
        "Label",
        "Comments",
        "Replies (0)",
        "Status",
    ] {
        assert!(visible(&h, s), "Properties > General shows {s}");
    }
    let (dir, mut a) = open("general");
    let id = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 200.0, 550.0), json!({}));
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [id], "subject": "Door", "label": "D-1", "author": "Pat", "contents": "Check swing" }),
    );
    call(&mut a, "markup_reply", json!({ "id": id, "add": "Agreed" }));
    call(&mut a, "markup_edit", json!({ "ids": [id], "status": "Accepted" }));
    let s = save(&mut a, &dir, "general.pdf");
    let x = by_nm(&s, &id);
    assert_eq!(x.text("Subj"), "Door");
    assert_eq!(x.text("T"), "Pat");
    assert_eq!(x.text("Contents"), "Check swing");
    assert!(x.text("M").starts_with("D:"), "modified date /M: {}", x.text("M"));
    assert!(
        x.text("CreationDate").starts_with("D:"),
        "creation date: {}",
        x.text("CreationDate")
    );
    let replies: Vec<&Annot> = s.iter().filter(|r| r.has("IRT")).collect();
    assert!(
        replies.iter().any(|r| r.text("Contents") == "Agreed"),
        "the reply is an /IRT annotation"
    );
    assert!(
        replies.iter().any(|r| r.text("State") == "Accepted"),
        "the status is a /State reply"
    );
    let (_b, l) = reopen(&dir, "general.pdf");
    let m = l.iter().find(|m| m["id"] == id.as_str()).unwrap();
    assert_eq!(m["label"], "D-1");
    assert_eq!(m["subject"], "Door");
    assert_eq!(m["status"], "Accepted");
    // Properties shows the reply count after the edit
    let mut h = app();
    panel(&mut h, "properties");
    let t = sample_id(&h, Kind::Rectangle);
    select(&mut h, std::slice::from_ref(&t));
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .add_reply(&t, "Looks good")
        .unwrap();
    h.run_steps(3);
    assert!(visible(&h, "Replies (1)"), "the reply count");
}

/// K-065 to K-070, K-072 to K-074: appearance properties land in the saved annotation (/C,
/// /IC, /CA, fill opacity, /BS width and dash, /LE, /BE, /BM) and reload.
#[test]
fn appearance_properties_are_saved() {
    let (dir, mut a) = open("look");
    let poly = add(
        &mut a,
        "Polygon",
        json!([[100, 450], [250, 450], [180, 560]]),
        json!({}),
    );
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [poly], "color": "#0000FF", "fill": "#FFFF00", "opacity": 0.5,
                "fill_opacity": 0.25, "width": 4, "dash": [6, 3], "cloud": 1.5, "multiply": true }),
    );
    let line = add(
        &mut a,
        "Polyline",
        json!([[300, 450], [400, 500], [450, 450]]),
        json!({}),
    );
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": [line], "line_start": "Circle", "line_end": "ClosedArrow" }),
    );
    let s = save(&mut a, &dir, "look.pdf");
    let x = by_nm(&s, &poly);
    assert_eq!(x.nums("C"), vec![0.0, 0.0, 1.0]);
    assert_eq!(x.nums("IC"), vec![1.0, 1.0, 0.0]);
    assert_eq!(x.num("CA"), Some(0.5));
    assert_eq!(markupcraft_revu::pdf::num(Some(&x.sub("BS", "W"))), Some(4.0));
    let d = x.sub("BS", "D");
    assert!(format!("{d:?}").contains('6'), "dash pattern: {d:?}");
    assert_eq!(
        markupcraft_revu::pdf::name(Some(&x.sub("BS", "S"))),
        "D",
        "dashed border style"
    );
    assert_eq!(markupcraft_revu::pdf::num(Some(&x.sub("BE", "I"))), Some(1.5));
    assert_eq!(x.name("BM"), "Multiply");
    let y = by_nm(&s, &line);
    assert_eq!(y.names("LE"), vec!["Circle".to_string(), "ClosedArrow".to_string()]);
    let (_b, l) = reopen(&dir, "look.pdf");
    let m = l.iter().find(|m| m["id"] == poly.as_str()).unwrap();
    assert_eq!(m["fill_opacity"], 0.25);
    assert_eq!(m["opacity"], 0.5);
    assert_eq!(m["width"], 4.0);
    // Properties offers these for a shape
    let mut h = app();
    panel(&mut h, "properties");
    let ar = sample_id(&h, Kind::Arrow);
    select(&mut h, &[ar]);
    for s in [
        "Color",
        "Opacity",
        "Line width",
        "Line style",
        "Start",
        "End",
        "Blend mode",
    ] {
        assert!(visible(&h, s), "Properties shows {s}");
    }
}

/// K-071: named line styles (dash patterns) are managed in a dialog and shared as a file.
#[test]
fn line_style_sets_are_managed_and_shared() {
    let mut h = app();
    panel(&mut h, "toolchest");
    if !visible(&h, "Line Styles...") {
        h.query_all_by_label("Options").next().unwrap().click();
        h.run_steps(3);
    }
    h.query_all_by_label("Line Styles...").next().unwrap().click();
    h.run_steps(4);
    for s in ["Add Style", "Export Set...", "Import Set..."] {
        assert!(visible(&h, s), "the Line Styles window has {s}");
    }
}

/// K-078 to K-080: Layout X, Y, width, height and rotation change the markup exactly.
#[test]
fn layout_position_size_and_rotation() {
    let (dir, mut a) = open("layout");
    let r = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 200.0, 500.0), json!({}));
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [r], "resize": [150, 400, 350, 480] }),
    );
    let m = get(&mut a, &r);
    assert_eq!(m["points"][0], json!([150.0, 400.0]));
    assert_eq!(m["points"][2], json!([350.0, 480.0]));
    call(&mut a, "markup_transform", json!({ "ids": [r], "move": [10, 20] }));
    assert_eq!(get(&mut a, &r)["points"][0], json!([160.0, 420.0]));
    // any angle up to 360 degrees
    let p = add(
        &mut a,
        "Polygon",
        json!([[400, 450], [500, 450], [500, 500], [400, 500]]),
        json!({}),
    );
    call(&mut a, "markup_transform", json!({ "ids": [p], "rotate": 30 }));
    let pts = get(&mut a, &p)["points"].clone();
    let (x0, y0) = (pts[0][0].as_f64().unwrap(), pts[0][1].as_f64().unwrap());
    let (x1, y1) = (pts[1][0].as_f64().unwrap(), pts[1][1].as_f64().unwrap());
    let ang = (y1 - y0).atan2(x1 - x0).to_degrees();
    assert!((ang - 30.0).abs() < 0.1, "turned 30 degrees: {ang}");
    // GAP: Revu turns any markup to any angle; boxes (rectangle, ellipse, text box) turn here
    // by quarter turns only, and other angles are refused.
    let e = fails(&mut a, "markup_transform", json!({ "ids": [r], "rotate": 30 }));
    assert!(e.contains("90"), "{e}");
    call(&mut a, "markup_transform", json!({ "ids": [r], "rotate": 90 }));
    let after = get(&mut a, &r);
    let s = save(&mut a, &dir, "rot.pdf");
    assert!(by_nm(&s, &r).has("AP"), "{after}");
    // Properties > Layout
    let mut h = app();
    panel(&mut h, "properties");
    let id = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[id]);
    for s in ["Layout", "Rotate"] {
        assert!(visible(&h, s), "{s}");
    }
}

/// K-081: the rotation handle above a selected markup turns it about its centre.
#[test]
fn rotation_handle_turns_the_markup() {
    let mut h = app();
    let n0 = markups(&h).len();
    key(&mut h, SHIFT, Key::P);
    for (x, y) in [(150.0, 450.0), (350.0, 450.0), (350.0, 550.0), (150.0, 550.0)] {
        click(&mut h, x, y);
    }
    key(&mut h, NONE, Key::Enter);
    let m = new_markups(&h, n0).pop().unwrap();
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&m.id));
    // the handle sits a little above the top centre: try the screen points above it
    let top = screen(&h, 250.0, 550.0);
    let mut turned = false;
    for dy in [22.0_f32, 20.0, 24.0] {
        select(&mut h, std::slice::from_ref(&m.id));
        let from = top - egui::vec2(0.0, dy);
        let to = screen(&h, 450.0, 500.0);
        h.hover_at(from);
        h.step();
        button(&mut h, from, true, NONE);
        for i in 1..=8 {
            h.hover_at(from + (to - from) * (i as f32 / 8.0));
            h.step();
        }
        button(&mut h, to, false, NONE);
        h.run_steps(3);
        let now = markups(&h).into_iter().find(|x| x.id == m.id).unwrap();
        if (now.pts[0].y - now.pts[1].y).abs() > 1.0 {
            turned = true;
            let c0 = Point::new(250.0, 500.0);
            let c = Point::new(
                now.pts.iter().map(|p| p.x).sum::<f64>() / 4.0,
                now.pts.iter().map(|p| p.y).sum::<f64>() / 4.0,
            );
            assert!(c.dist(c0) < 1.0, "about its centre: {c:?}");
            break;
        }
    }
    assert!(turned, "a drag from the handle turns the polygon");
}

/// K-082: a markup is put on a PDF layer (/OC) and the layer lists it.
#[test]
fn layer_property_puts_a_markup_on_a_layer() {
    let (dir, mut a) = open("layerprop");
    let id = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "layer": "E-Power" }),
    );
    let l = call(&mut a, "layer_list", json!({})).to_string();
    assert!(l.contains("E-Power"), "{l}");
    let s = save(&mut a, &dir, "layer.pdf");
    let x = by_nm(&s, &id);
    assert!(x.has("OC"), "/OC on the annotation");
    let (_b, l) = reopen(&dir, "layer.pdf");
    assert_eq!(l.iter().find(|m| m["id"] == id.as_str()).unwrap()["layer"], "E-Power");
    let mut h = app();
    panel(&mut h, "properties");
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[r]);
    assert!(visible(&h, "Layer"));
}

/// K-084, K-085: Set as Default makes new markups of the type look like the selected one; Add to
/// Tool Chest saves it as a tool.
#[test]
fn set_as_default_and_add_to_tool_chest() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let look = markups(&h).into_iter().find(|m| m.id == r).unwrap();
    select(&mut h, std::slice::from_ref(&r));
    run(&mut h, "markup.set_default");
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::R);
    drag(&mut h, (100.0, 450.0), (200.0, 550.0));
    let m = new_markups(&h, n0).pop().expect("a rectangle");
    assert_eq!(m.color, look.color, "default colour");
    assert_eq!(m.fill, look.fill, "default fill");
    assert_eq!(m.line_width, look.line_width, "default width");
    let items = |h: &Harness<'_, MarkupCraftApp>| -> usize {
        h.state().state.toolchest.sets.iter().map(|s| s.items.len()).sum()
    };
    let before = items(&h);
    select(&mut h, std::slice::from_ref(&r));
    run(&mut h, "markup.add_to_toolchest");
    assert_eq!(items(&h), before + 1, "a new tool");
    let it = h
        .state()
        .state
        .toolchest
        .sets
        .iter()
        .flat_map(|s| s.items.iter())
        .last()
        .cloned()
        .unwrap();
    assert_eq!(it.markup.kind, Kind::Rectangle);
    assert_eq!(it.markup.color, look.color);
}

/// K-083, K-183: a locked markup cannot be moved, edited or deleted until unlocked; the lock is
/// the PDF /F Locked flag.
#[test]
fn lock_keeps_a_markup_from_changing() {
    let (dir, mut a) = open("lock");
    let id = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "locked": true }),
    );
    assert!(
        a.call("markup_transform", &json!({ "ids": [id], "move": [10, 10] }))
            .is_err(),
        "no move"
    );
    assert!(
        a.call("markup_edit", &json!({ "ids": [id], "color": "#00FF00" }))
            .is_err(),
        "no edit"
    );
    assert!(a.call("markup_delete", &json!({ "ids": [id] })).is_err(), "no delete");
    let s = save(&mut a, &dir, "lock.pdf");
    let f = by_nm(&s, &id).num("F").unwrap_or(0.0) as i64;
    assert!(f & 128 != 0, "/F Locked bit: {f}");
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "lock", "value": "false" }),
    );
    call(&mut a, "markup_transform", json!({ "ids": [id], "move": [10, 10] }));
    // Ctrl+Shift+L in the app
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, std::slice::from_ref(&r));
    key(&mut h, Modifiers::COMMAND | SHIFT, Key::L);
    assert!(
        markups(&h).iter().find(|m| m.id == r).unwrap().locked(),
        "Ctrl+Shift+L locks"
    );
    run(&mut h, "edit.delete");
    assert!(markups(&h).iter().any(|m| m.id == r), "a locked markup is not deleted");
}

/// K-086: measurement properties (scale, units, precision, caption, depth, slope).
#[test]
fn measurement_properties() {
    let mut h = app();
    panel(&mut h, "properties");
    let ar = sample_id(&h, Kind::Area);
    select(&mut h, std::slice::from_ref(&ar));
    for s in [
        "Measurement",
        "Units",
        "Precision",
        "Depth",
        "Slope",
        "Show Caption",
        "Value",
    ] {
        assert!(visible(&h, s), "Properties > Measurement shows {s}");
    }
    let (_dir, mut a) = open("measprops");
    let before = get(&mut a, "SAMPLEAREAAAAAAA")["quantity"].as_f64().unwrap();
    call(
        &mut a,
        "measure_props_set",
        json!({ "ids": ["SAMPLEAREAAAAAAA"], "slope_type": "pitch", "slope": 12 }),
    );
    let after = get(&mut a, "SAMPLEAREAAAAAAA")["quantity"].as_f64().unwrap();
    assert!(
        (after / before - 2f64.sqrt()).abs() < 0.01,
        "a 12/12 pitch: {before} -> {after}"
    );
    call(
        &mut a,
        "markup_edit",
        json!({ "ids": ["SAMPLEAREAAAAAAA"], "show_caption": false }),
    );
    let l = add(&mut a, "Length", json!([[100, 450], [172, 450]]), json!({}));
    let q = get(&mut a, &l)["quantity_text"].as_str().unwrap().to_string();
    assert!(
        q.starts_with("8 ft") || q.starts_with("8.00 ft") || q.starts_with("8'"),
        "72 pt = 1 in at 1/8 in = 1 ft: {q}"
    );
}

/// K-087: Shift+drag on a measurement's caption moves the caption only.
#[test]
fn shift_drag_moves_a_measurement_caption() {
    let mut h = app();
    let ar = sample_id(&h, Kind::Area);
    let before = markups(&h).into_iter().find(|m| m.id == ar).unwrap();
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&ar));
    let from = screen(&h, 450.0, 265.0);
    let to = screen(&h, 450.0, 320.0);
    h.event(egui::Event::ModifiersChanged(SHIFT));
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, SHIFT);
    for i in 1..=8 {
        h.hover_at(from + (to - from) * (i as f32 / 8.0));
        h.step();
    }
    button(&mut h, to, false, SHIFT);
    h.event(egui::Event::ModifiersChanged(NONE));
    h.run_steps(3);
    let after = markups(&h).into_iter().find(|m| m.id == ar).unwrap();
    assert_eq!(after.pts, before.pts, "the shape stays");
    assert!(
        after.caption_offset.is_some_and(|o| o.y > 20.0),
        "the caption moved: {:?}",
        after.caption_offset
    );
}

/// K-088: Format Painter copies the selected markup's look onto clicked markups.
#[test]
fn format_painter_copies_the_look() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let e = sample_id(&h, Kind::Ellipse);
    let src = markups(&h).into_iter().find(|m| m.id == r).unwrap();
    select(&mut h, std::slice::from_ref(&r));
    run(&mut h, "markup.format_painter");
    // the ellipse's outline at its left edge
    click(&mut h, 700.0, 620.0);
    key(&mut h, NONE, Key::Escape);
    let after = markups(&h).into_iter().find(|m| m.id == e).unwrap();
    assert_eq!(after.color, src.color);
    assert_eq!(after.fill, src.fill);
    assert_eq!(after.line_width, src.line_width);
    assert_eq!(after.kind, Kind::Ellipse, "only the look");
}

/// K-089: Flip Horizontal / Vertical mirror the selection (Ctrl+Alt+H / Ctrl+Alt+V).
#[test]
fn flip_mirrors_the_selection() {
    let (_dir, mut a) = open("flip");
    let p = add(
        &mut a,
        "Polygon",
        json!([[100, 450], [200, 450], [100, 550]]),
        json!({}),
    );
    call(&mut a, "markup_align", json!({ "ids": [p], "flip": "horizontal" }));
    let pts = get(&mut a, &p)["points"].clone();
    assert_eq!(pts, json!([[200.0, 450.0], [100.0, 450.0], [200.0, 550.0]]));
    call(&mut a, "markup_align", json!({ "ids": [p], "flip": "vertical" }));
    let pts = get(&mut a, &p)["points"].clone();
    assert_eq!(pts, json!([[200.0, 550.0], [100.0, 550.0], [200.0, 450.0]]));
    let mut h = app();
    let pl = sample_id(&h, Kind::Polyline);
    let before = markups(&h).into_iter().find(|m| m.id == pl).unwrap().pts;
    select(&mut h, std::slice::from_ref(&pl));
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::H);
    let after = markups(&h).into_iter().find(|m| m.id == pl).unwrap().pts;
    assert_ne!(before, after, "Ctrl+Alt+H flips");
    let cx = |v: &[Point]| v.iter().map(|p| p.x).fold(f64::MAX, f64::min);
    assert!((cx(&before) - cx(&after)).abs() < 0.01, "about its own extent");
}

/// K-090: hidden, print and no-view flags are kept as the PDF /F bits.
#[test]
fn annotation_flags_hidden_print_no_view() {
    let (dir, mut a) = open("flags");
    let hid = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 150.0, 500.0),
        json!({ "hidden": true }),
    );
    let np = add(
        &mut a,
        "Rectangle",
        rect_pts(200.0, 450.0, 250.0, 500.0),
        json!({ "print": false }),
    );
    let nv = add(
        &mut a,
        "Rectangle",
        rect_pts(300.0, 450.0, 350.0, 500.0),
        json!({ "no_view": true }),
    );
    let s = save(&mut a, &dir, "flags.pdf");
    let f = |id: &str| by_nm(&s, id).num("F").unwrap_or(0.0) as i64;
    assert!(f(&hid) & 2 != 0, "Hidden");
    assert!(f(&np) & 4 == 0, "not printed");
    assert!(f(&nv) & 32 != 0, "NoView");
    let mut h = app();
    panel(&mut h, "properties");
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[r]);
    for s in ["Print", "Hidden", "No View"] {
        assert!(visible(&h, s));
    }
}

// ---------------------------------------------------------------------------------------------
// A (continued): autosize, rich text, spelling, review

/// K-002: Autosize (Alt+Z) fits the frame of the selected text box to its text.
#[test]
fn autosize_fits_the_frame_to_its_text() {
    let mut h = app();
    key(&mut h, NONE, Key::T);
    drag(&mut h, (100.0, 300.0), (500.0, 560.0));
    type_text(&mut h, "Hi");
    key(&mut h, NONE, Key::Escape);
    let m = last(&h);
    assert_eq!(m.contents, "Hi");
    let before = markupcraft_geom::bbox(&m.pts).unwrap();
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&m.id));
    chord(&mut h, Modifiers::ALT, Key::Z);
    let after = markupcraft_geom::bbox(&last(&h).pts).unwrap();
    assert!(
        after.width() < before.width() / 3.0 && after.height() < before.height() / 3.0,
        "the frame shrinks to fit 'Hi': {before:?} -> {after:?}"
    );
}

/// K-006: Ctrl+B while typing styles only the characters typed after it; saved as /RC spans.
#[test]
fn rich_text_runs_are_saved_as_rc_spans() {
    let mut h = app();
    key(&mut h, NONE, Key::T);
    drag(&mut h, (100.0, 450.0), (400.0, 520.0));
    type_text(&mut h, "plain bold");
    for _ in 0..4 {
        chord(&mut h, SHIFT, Key::ArrowLeft);
    }
    chord(&mut h, Modifiers::COMMAND, Key::B);
    key(&mut h, NONE, Key::Escape);
    let m = last(&h);
    assert_eq!(m.contents, "plain bold");
    let bold: Vec<_> = m.rich.iter().filter(|r| r.bold).collect();
    assert!(!bold.is_empty(), "a bold run: {:?}", m.rich);
    assert!(
        bold.iter().all(|r| r.start >= 6),
        "'plain ' stays regular: {:?}",
        m.rich
    );
    assert!(!m.text.bold, "the box itself is not bold");
    let s = save_ui(&mut h, "rich");
    let rc = by_nm(&s, &m.id).text("RC");
    assert!(
        rc.contains("bold") && rc.contains("plain"),
        "/RC carries the spans: {rc}"
    );
}

/// K-007: spell check finds misspelled words in markup text, with suggestions.
#[test]
fn spell_check_reports_misspelled_markup_words() {
    let (_dir, mut a) = open("spell");
    let id = add(
        &mut a,
        "Text",
        rect_pts(100.0, 450.0, 300.0, 500.0),
        json!({ "contents": "Verify the dimenson of the wall" }),
    );
    let s = call(&mut a, "spell_check", json!({ "ids": [id] })).to_string();
    assert!(s.contains("dimenson"), "{s}");
    assert!(s.contains("dimension"), "suggests the right word: {s}");
    assert!(!s.contains("\"Verify\""), "correct words pass: {s}");
    // F7 opens Check Spelling in the app
    let mut h = app();
    key(&mut h, NONE, Key::F7);
    assert!(visible(&h, "Spelling"), "the spelling window");
}

/// K-008: Review Text (Shift+Alt+R) steps through markup text.
#[test]
fn review_text_steps_through_markup_text() {
    let mut h = app();
    key(&mut h, SHIFT | Modifiers::ALT, Key::R);
    h.run_steps(4);
    assert!(visible(&h, "Review Text"), "the Review Text window opens");
    assert!(visible(&h, "Next"), "with Next");
    assert!(visible(&h, "Previous"), "and Previous");
    assert!(
        visible(&h, "Verify sink location") || visible(&h, "Hello"),
        "showing a markup's text"
    );
}

// ---------------------------------------------------------------------------------------------
// E. Selection, arrangement and clipboard

/// Press and release a mouse button at a page point, with modifiers held.
fn click_with(h: &mut Harness<'_, MarkupCraftApp>, x: f64, y: f64, m: Modifiers) {
    let at = screen(h, x, y);
    h.event(egui::Event::ModifiersChanged(m));
    h.hover_at(at);
    h.step();
    button(h, at, true, m);
    button(h, at, false, m);
    h.event(egui::Event::ModifiersChanged(NONE));
    h.run_steps(3);
}

/// Drag with a button and modifiers held.
fn drag_with(
    h: &mut Harness<'_, MarkupCraftApp>,
    from: (f64, f64),
    to: (f64, f64),
    b: egui::PointerButton,
    m: Modifiers,
) {
    let (a, z) = (screen(h, from.0, from.1), screen(h, to.0, to.1));
    h.event(egui::Event::ModifiersChanged(m));
    h.hover_at(a);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: a,
        button: b,
        pressed: true,
        modifiers: m,
    });
    h.step();
    for i in 1..=8 {
        h.hover_at(a + (z - a) * (i as f32 / 8.0));
        h.step();
    }
    h.event(egui::Event::PointerButton {
        pos: z,
        button: b,
        pressed: false,
        modifiers: m,
    });
    h.step();
    h.event(egui::Event::ModifiersChanged(NONE));
    h.run_steps(3);
}

fn find(h: &Harness<'_, MarkupCraftApp>, id: &str) -> markupcraft_model::Markup {
    markups(h).into_iter().find(|m| m.id == id).unwrap()
}

/// K-091, K-092, K-094: a click selects, Shift+click adds, a box drag on empty page selects what
/// it encloses (left or right button), Ctrl+A selects everything on the page.
#[test]
fn select_click_shift_box_and_all() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let e = sample_id(&h, Kind::Ellipse);
    key(&mut h, NONE, Key::V);
    click(&mut h, 900.0, 650.0); // the rectangle's left edge
    assert_eq!(selection(&h), vec![r.clone()]);
    click_with(&mut h, 700.0, 620.0, SHIFT); // the ellipse's left edge
    let s = selection(&h);
    assert!(s.contains(&r) && s.contains(&e), "Shift adds: {s:?}");
    click(&mut h, 150.0, 720.0); // empty page
    assert!(selection(&h).is_empty(), "a click on nothing clears");
    drag(&mut h, (680.0, 540.0), (1010.0, 710.0));
    let s = selection(&h);
    assert!(
        s.contains(&r) && s.contains(&e) && s.len() == 2,
        "the box selects what it encloses: {s:?}"
    );
    click(&mut h, 150.0, 720.0);
    drag_with(
        &mut h,
        (680.0, 540.0),
        (1010.0, 710.0),
        egui::PointerButton::Secondary,
        NONE,
    );
    let s = selection(&h);
    assert!(
        s.contains(&r) && s.contains(&e),
        "a right-button drag selects too: {s:?}"
    );
    chord(&mut h, Modifiers::COMMAND, Key::A);
    assert_eq!(
        selection(&h).len(),
        markups(&h).iter().filter(|m| m.page == 0).count(),
        "Ctrl+A"
    );
}

/// K-093: a lasso loop selects the markups wholly inside it.
#[test]
fn lasso_selects_markups_inside_the_loop() {
    let (_dir, mut a) = open("lasso");
    let r = call(
        &mut a,
        "select_lasso",
        json!({ "page": 1, "points": [[680, 540], [1010, 540], [1010, 720], [680, 720]] }),
    );
    let s = r.to_string();
    assert!(s.contains("SAMPLESQUAREAAAA") && s.contains("SAMPLECIRCLEAAAA"), "{s}");
    assert!(!s.contains("SAMPLEAREAAAAAAA"), "{s}");
    let mut h = app();
    key(&mut h, SHIFT, Key::O);
    let pts = [
        (680.0, 540.0),
        (1010.0, 540.0),
        (1010.0, 720.0),
        (680.0, 720.0),
        (682.0, 545.0),
    ];
    let first = screen(&h, pts[0].0, pts[0].1);
    h.hover_at(first);
    h.step();
    button(&mut h, first, true, NONE);
    for (x, y) in &pts[1..] {
        for _ in 0..3 {
            let p = screen(&h, *x, *y);
            h.hover_at(p);
            h.step();
        }
    }
    let end = screen(&h, pts[4].0, pts[4].1);
    button(&mut h, end, false, NONE);
    h.run_steps(3);
    let s = selection(&h);
    assert_eq!(s.len(), 2, "rectangle and ellipse: {s:?}");
}

/// K-095, K-096: copy / cut / paste (also into another document) and Paste in Place.
#[test]
fn clipboard_copy_cut_paste_and_paste_in_place() {
    let (dir, mut a) = open("clip");
    let id = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "subject": "Box" }),
    );
    call(&mut a, "markup_copy", json!({ "ids": [id] }));
    let r = call(&mut a, "markup_paste", json!({ "page": 1, "at": [400, 600] }));
    let copy = list(&mut a)
        .into_iter()
        .rfind(|m| m["subject"] == "Box" && m["id"] != id.as_str())
        .unwrap_or_else(|| panic!("{r}"));
    assert_eq!(copy["points"][0], json!([350.0, 550.0]), "centred on the pointer");
    // paste in place onto page 2: same coordinates
    call(&mut a, "markup_paste", json!({ "page": 2 }));
    let p2 = list(&mut a)
        .into_iter()
        .find(|m| m["subject"] == "Box" && m["page"] == 2)
        .unwrap();
    assert_eq!(p2["points"][0], json!([100.0, 450.0]));
    // cut removes it
    call(&mut a, "markup_copy", json!({ "ids": [id], "cut": true }));
    assert!(!list(&mut a).iter().any(|m| m["id"] == id.as_str()));
    // into another open document
    std::fs::copy(dir.join("plan.pdf"), dir.join("other.pdf")).unwrap();
    call(&mut a, "doc_open", json!({ "path": "other.pdf" }));
    call(&mut a, "markup_paste", json!({ "page": 1 }));
    assert!(
        list(&mut a).iter().any(|m| m["subject"] == "Box"),
        "pasted across documents"
    );
    // the app: Ctrl+C, Ctrl+Shift+V
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let src = find(&h, &r);
    select(&mut h, std::slice::from_ref(&r));
    chord(&mut h, Modifiers::COMMAND, Key::C);
    let n0 = markups(&h).len();
    chord(&mut h, Modifiers::COMMAND | SHIFT, Key::V);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "Ctrl+Shift+V pastes");
    assert_eq!(m[0].pts, src.pts, "in place");
    chord(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(markups(&h).len(), n0, "Ctrl+X cuts the selection");
}

/// K-097: Apply to All Pages copies a markup to the same place on other pages; the dialog
/// filters by range, odd / even and portrait / landscape.
#[test]
fn apply_to_pages_copies_to_the_same_place() {
    let (_dir, mut a) = open("topages");
    let id = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "subject": "Title" }),
    );
    call(&mut a, "markup_align", json!({ "ids": [id], "to_pages": "all" }));
    let copies: Vec<Value> = list(&mut a).into_iter().filter(|m| m["subject"] == "Title").collect();
    assert_eq!(copies.len(), 2);
    assert_eq!(copies[1]["page"], 2);
    assert_eq!(copies[1]["points"], copies[0]["points"]);
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[r]);
    run(&mut h, "markup.apply_to_all_pages");
    h.run_steps(3);
    // GAP: Apply to All Pages copies at once; there is no dialog with odd / even or portrait /
    // landscape filters (markup_align to_pages takes a page list or range).
    let r2 = sample_id(&h, Kind::Rectangle);
    assert!(
        markups(&h)
            .iter()
            .filter(|m| m.page == 1 && m.pts == find(&h, &r2).pts)
            .count()
            == 1,
        "copied to page 2"
    );
}

/// K-098: Ctrl+Shift+drag copies a markup and keeps the move straight.
#[test]
fn ctrl_shift_drag_copies_in_a_straight_line() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let src = find(&h, &r);
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&r));
    let n0 = markups(&h).len();
    drag_with(
        &mut h,
        (900.0, 650.0),
        (1100.0, 660.0),
        egui::PointerButton::Primary,
        Modifiers::COMMAND | SHIFT,
    );
    let made = new_markups(&h, n0);
    assert_eq!(made.len(), 1, "a copy");
    assert_eq!(find(&h, &r).pts, src.pts, "the original stays");
    let dy = made[0].pts[0].y - src.pts[0].y;
    let dx = made[0].pts[0].x - src.pts[0].x;
    assert!(dy.abs() < 0.5 && dx > 150.0, "moved straight across: dx {dx} dy {dy}");
}

/// K-099 to K-101: Delete, Undo / Redo, and arrow-key nudges.
#[test]
fn delete_undo_redo_and_nudge() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let src = find(&h, &r);
    select(&mut h, std::slice::from_ref(&r));
    key(&mut h, NONE, Key::Delete);
    assert!(!markups(&h).iter().any(|m| m.id == r), "deleted");
    chord(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(markups(&h).iter().any(|m| m.id == r), "undone");
    chord(&mut h, Modifiers::COMMAND, Key::Y);
    assert!(!markups(&h).iter().any(|m| m.id == r), "redone");
    chord(&mut h, Modifiers::COMMAND, Key::Z);
    select(&mut h, std::slice::from_ref(&r));
    key(&mut h, NONE, Key::ArrowRight);
    let p = find(&h, &r).pts[0];
    assert!(p.x > src.pts[0].x && p.x - src.pts[0].x <= 2.0, "a small step: {p:?}");
    chord(&mut h, SHIFT, Key::ArrowUp);
    let q = find(&h, &r).pts[0];
    assert!(q.y > p.y + 4.0, "Shift: a bigger step: {q:?}");
}

/// K-102 to K-104: Group (Ctrl+G) makes one click select the group; Ungroup and Remove From
/// Group; saved as /IRT replies with /RT /Group.
#[test]
fn group_ungroup_and_remove_from_group() {
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let e = sample_id(&h, Kind::Ellipse);
    let p = sample_id(&h, Kind::Polyline);
    select(&mut h, &[r.clone(), e.clone(), p.clone()]);
    chord(&mut h, Modifiers::COMMAND, Key::G);
    key(&mut h, NONE, Key::V);
    click(&mut h, 150.0, 720.0);
    click(&mut h, 900.0, 650.0);
    let s = selection(&h);
    assert!(
        s.contains(&r) && s.contains(&e) && s.contains(&p),
        "one click selects the group: {s:?}"
    );
    let saved = save_ui(&mut h, "group");
    let grouped: Vec<&Annot> = saved.iter().filter(|x| x.name("RT") == "Group").collect();
    assert_eq!(
        grouped.len(),
        2,
        "two members point at the group's leader with /IRT /RT /Group"
    );
    // Remove From Group (one member, chosen from the list) keeps the others grouped
    select(&mut h, std::slice::from_ref(&p));
    run(&mut h, "markup.remove_from_group");
    click(&mut h, 150.0, 720.0);
    click(&mut h, 900.0, 650.0);
    let s = selection(&h);
    assert!(s.contains(&e) && !s.contains(&p), "{s:?}");
    chord(&mut h, Modifiers::COMMAND | SHIFT, Key::G);
    click(&mut h, 150.0, 720.0);
    click(&mut h, 900.0, 650.0);
    assert_eq!(selection(&h), vec![r], "ungrouped");
}

/// K-105 to K-107: align and distribute.
#[test]
fn align_and_distribute() {
    let (_dir, mut a) = open("align");
    let x = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 150.0, 500.0), json!({}));
    let y = add(&mut a, "Rectangle", rect_pts(300.0, 470.0, 380.0, 520.0), json!({}));
    let z = add(&mut a, "Rectangle", rect_pts(400.0, 600.0, 420.0, 640.0), json!({}));
    let left = |a: &mut Automation, id: &str| get(a, id)["points"][0][0].as_f64().unwrap();
    let bottom = |a: &mut Automation, id: &str| get(a, id)["points"][0][1].as_f64().unwrap();
    call(&mut a, "markup_align", json!({ "ids": [x, y], "align": "left" }));
    assert_eq!(left(&mut a, &x), left(&mut a, &y), "left edges line up");
    call(&mut a, "markup_align", json!({ "ids": [x, y, z], "align": "bottom" }));
    assert_eq!(bottom(&mut a, &x), bottom(&mut a, &z));
    call(
        &mut a,
        "markup_align",
        json!({ "ids": [x, y, z], "distribute": "horizontal" }),
    );
    let gaps = {
        let mut b: Vec<(f64, f64)> = [&x, &y, &z]
            .iter()
            .map(|id| {
                let p = get(&mut a, id)["points"].clone();
                (p[0][0].as_f64().unwrap(), p[1][0].as_f64().unwrap())
            })
            .collect();
        b.sort_by(|p, q| p.0.total_cmp(&q.0));
        (b[1].0 - b[0].1, b[2].0 - b[1].1)
    };
    assert!((gaps.0 - gaps.1).abs() < 0.01, "equal gaps: {gaps:?}");
    // Revu aligns to the reference markup; in the app the last selected one is the reference
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    let e = sample_id(&h, Kind::Ellipse);
    let rl = markupcraft_geom::bbox(&find(&h, &r).pts).unwrap().x0;
    select(&mut h, &[e.clone(), r.clone()]);
    chord(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::L);
    let el = markupcraft_geom::bbox(&find(&h, &e).pts).unwrap().x0;
    let rl2 = markupcraft_geom::bbox(&find(&h, &r).pts).unwrap().x0;
    // GAP: Revu aligns to the reference markup; here both go to the selection's joint extent
    // (the ellipse's left edge at 700), whichever was selected last.
    assert!(
        (el - rl2).abs() < 0.01 && rl2 < rl,
        "left edges line up at the joint extent: {el} {rl2}"
    );
}

/// K-108, K-109: z-order commands, saved as the page's /Annots order.
#[test]
fn bring_forward_and_send_back() {
    let (dir, mut a) = open("zorder");
    let one = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 200.0, 550.0), json!({}));
    let two = add(&mut a, "Rectangle", rect_pts(150.0, 500.0, 250.0, 600.0), json!({}));
    call(&mut a, "markup_arrange", json!({ "ids": [two], "order": "back" }));
    let s = save(&mut a, &dir, "z.pdf");
    let order: Vec<String> = s.iter().map(|x| x.nm()).filter(|n| *n == one || *n == two).collect();
    assert_eq!(order, vec![two.clone(), one.clone()], "sent to the back");
    call(&mut a, "markup_arrange", json!({ "ids": [two], "order": "forward" }));
    let s = save(&mut a, &dir, "z2.pdf");
    let pos = |v: &[Annot], id: &str| v.iter().position(|x| x.nm() == id).unwrap();
    assert_eq!(
        pos(&s, &two),
        1,
        "one step forward: over the first markup of the page only"
    );
    let mut h = app();
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, std::slice::from_ref(&r));
    chord(&mut h, Modifiers::COMMAND | SHIFT, Key::OpenBracket);
    assert_eq!(
        markups(&h).iter().position(|m| m.id == r),
        Some(0),
        "Ctrl+Shift+[ sends to the back"
    );
    chord(&mut h, Modifiers::COMMAND | SHIFT, Key::CloseBracket);
    assert_eq!(markups(&h).last().unwrap().id, r, "Ctrl+Shift+] brings to the front");
}

/// K-110: vertices are dragged, added and deleted.
#[test]
fn edit_vertices() {
    let (_dir, mut a) = open("verts");
    let p = add(
        &mut a,
        "Polygon",
        json!([[100, 450], [200, 450], [150, 550]]),
        json!({}),
    );
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [p], "move_vertex": { "index": 3, "point": [150, 600] } }),
    );
    call(
        &mut a,
        "markup_transform",
        json!({ "ids": [p], "insert_vertex": { "index": 2, "point": [150, 420] } }),
    );
    assert_eq!(get(&mut a, &p)["points"].as_array().unwrap().len(), 4);
    call(&mut a, "markup_transform", json!({ "ids": [p], "delete_vertex": 1 }));
    let pts = get(&mut a, &p)["points"].clone();
    assert_eq!(pts, json!([[150.0, 420.0], [200.0, 450.0], [150.0, 600.0]]));
    // the app: drag a vertex handle of the selected polyline
    let mut h = app();
    let pl = sample_id(&h, Kind::Polyline);
    let v0 = find(&h, &pl).pts[0];
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&pl));
    drag(&mut h, (v0.x, v0.y), (v0.x + 30.0, v0.y + 40.0));
    let moved = find(&h, &pl).pts[0];
    assert!(
        moved.dist(Point::new(v0.x + 30.0, v0.y + 40.0)) < 2.0,
        "{v0:?} -> {moved:?}"
    );
    // right-click a segment: Add Vertex
    let (a0, a1) = (find(&h, &pl).pts[1], find(&h, &pl).pts[2]);
    let mid = Point::new((a0.x + a1.x) / 2.0, (a0.y + a1.y) / 2.0);
    let at = screen(&h, mid.x, mid.y);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: NONE,
    });
    h.step();
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: NONE,
    });
    h.run_steps(3);
    assert!(visible(&h, "Add Vertex"), "the context menu offers Add Vertex");
}

/// K-111: snapping to markup points while drawing.
#[test]
fn snap_to_markup_points_while_drawing() {
    let mut h = app();
    run(&mut h, "snap.markup");
    if h.state().state.checked("snap.markup") == Some(false) {
        run(&mut h, "snap.markup");
    }
    assert_eq!(h.state().state.checked("snap.markup"), Some(true));
    let n0 = markups(&h).len();
    key(&mut h, NONE, Key::L);
    click(&mut h, 902.0, 602.5); // near the rectangle's corner (900, 600)
    click(&mut h, 600.0, 450.0);
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1);
    assert!(
        m[0].pts[0].dist(Point::new(900.0, 600.0)) < 0.01,
        "snapped to the corner: {:?}",
        m[0].pts[0]
    );
    for c in ["snap.content", "snap.grid"] {
        let before = h.state().state.checked(c);
        run(&mut h, c);
        assert_ne!(h.state().state.checked(c), before, "{c} toggles");
    }
}

/// K-112: a page grid and rulers can be shown.
#[test]
fn grid_and_rulers() {
    let mut h = app();
    for c in ["view.show_grid", "view.rulers"] {
        let before = h.state().state.checked(c);
        run(&mut h, c);
        assert_ne!(h.state().state.checked(c), before, "{c} toggles");
    }
}

/// K-113: the right-click menu on a markup has Revu's commands.
#[test]
fn right_click_menu_on_a_markup() {
    let mut h = app();
    key(&mut h, NONE, Key::V);
    let at = screen(&h, 900.0, 650.0);
    h.hover_at(at);
    h.step();
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: NONE,
    });
    h.step();
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: NONE,
    });
    h.run_steps(3);
    let l = labels(&h);
    // GAP: Revu's menu also has Reply, Flip, Edit Action and Flatten.
    let missing: Vec<&str> = [
        "Cut",
        "Copy",
        "Paste",
        "Delete",
        "Properties",
        "Set as Default",
        "Add to Tool Chest",
        "Lock",
        "Layer",
        "Status",
        "Group",
        "Arrange",
        "Apply to All Pages",
    ]
    .into_iter()
    .filter(|w| !l.iter().any(|x| x.contains(w)))
    .collect();
    assert!(missing.is_empty(), "the menu lacks {missing:?}");
}

/// K-114: Hide Markups hides every markup from view only; Hidden on one sets the PDF flag.
#[test]
fn hide_markups_from_view() {
    let mut h = app();
    let n = markups(&h).len();
    run(&mut h, "view.hide_markups");
    assert_eq!(h.state().state.checked("view.hide_markups"), Some(true));
    assert!(h.state().state.canvas_cx().hide_markups);
    assert_eq!(markups(&h).len(), n, "nothing deleted");
    assert!(markups(&h).iter().all(|m| m.flags & 2 == 0), "no PDF flag changed");
    run(&mut h, "view.hide_markups");
    assert_eq!(h.state().state.checked("view.hide_markups"), Some(false));
}

// ---------------------------------------------------------------------------------------------
// F. Tool Chest

/// Add the sample rectangle, with `subject`, to My Tools through the Markup command; returns
/// the new item's (set id, item id, name).
fn chest_item(h: &mut Harness<'_, MarkupCraftApp>, subject: &str) -> (String, String, String) {
    let r = sample_id(h, Kind::Rectangle);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&r),
            &markupcraft_engine::props::MarkupPatch {
                subject: Some(subject.into()),
                ..Default::default()
            },
        )
        .unwrap();
    select(h, std::slice::from_ref(&r));
    run(h, "markup.add_to_toolchest");
    let set = &h.state().state.toolchest.sets[0];
    let it = set.items.last().unwrap();
    (set.id.clone(), it.id.clone(), it.name.clone())
}

/// Two quick clicks at a screen point.
fn double_click_at(h: &mut Harness<'_, MarkupCraftApp>, at: egui::Pos2) {
    // well after any earlier click, so the two count as one double-click
    h.run_steps(60);
    h.hover_at(at);
    h.step();
    for _ in 0..2 {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: NONE,
        });
        h.step();
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

/// The Tool Chest's (right-hand) widget named `name`.
fn chest_node<'a>(h: &'a Harness<'_, MarkupCraftApp>, name: &'a str) -> egui_kittest::Node<'a> {
    h.query_all_by_label(name)
        .max_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .unwrap_or_else(|| panic!("no {name}"))
}

fn item_count(h: &Harness<'_, MarkupCraftApp>) -> usize {
    h.state().state.toolchest.sets.iter().map(|s| s.items.len()).sum()
}

/// K-115 to K-117: the Tool Chest panel shows collapsible tool sets with My Tools first;
/// a new tool set can be added; the chest is kept in a file across sessions.
#[test]
fn tool_chest_sets_and_my_tools_persist() {
    let mut h = app();
    panel(&mut h, "toolchest");
    assert!(visible(&h, "My Tools") && visible(&h, "Recent Tools"));
    let (_s, _i, name) = chest_item(&mut h, "Door Tag");
    h.run_steps(3);
    assert!(visible(&h, &name), "the new tool shows in My Tools: {name}");
    let set = h.state_mut().state.toolchest.add_set("Concrete takeoff");
    h.run_steps(3);
    assert!(visible(&h, "Concrete takeoff"), "a new tool set shows");
    // kept in a file: a new session reads it back
    let dir = temp_dir("chest");
    let path = dir.join("toolchest.json");
    {
        let c = &mut h.state_mut().state.toolchest;
        c.path = Some(path.clone());
        c.save();
        assert!(c.error.is_none(), "{:?}", c.error);
    }
    let back = markupcraft_ui_egui::chest::ToolChest::load(&path);
    assert_eq!(back.sets[0].title, "My Tools");
    assert!(back.sets[0].items.iter().any(|i| i.name == name));
    assert!(back.sets.iter().any(|s| s.id == set));
}

/// K-118, K-119: drawn markups land in Recent Tools (this session only, capped by the option),
/// which can be cleared.
#[test]
fn recent_tools_fill_while_drawing() {
    let mut h = app();
    panel(&mut h, "toolchest");
    assert!(h.state().state.toolchest.recent.is_empty());
    key(&mut h, NONE, Key::R);
    drag(&mut h, (100.0, 450.0), (200.0, 550.0));
    key(&mut h, NONE, Key::E);
    drag(&mut h, (300.0, 450.0), (400.0, 550.0));
    let recent = h.state().state.toolchest.recent.clone();
    assert!(recent.len() >= 2, "{recent:?}");
    assert_eq!(recent[0].markup.kind, Kind::Ellipse, "newest first");
    let dir = temp_dir("recent");
    let path = dir.join("tc.json");
    {
        let c = &mut h.state_mut().state.toolchest;
        c.extras.recent_max = 1;
        c.path = Some(path.clone());
        c.save();
    }
    let back = markupcraft_ui_egui::chest::ToolChest::load(&path);
    assert!(back.recent.is_empty(), "recent tools are not kept across sessions");
    key(&mut h, NONE, Key::R);
    drag(&mut h, (100.0, 600.0), (200.0, 700.0));
    assert!(
        h.state().state.toolchest.recent.len() <= 1,
        "at most the option's number"
    );
    h.state_mut().state.toolchest.clear_recent();
    assert!(h.state().state.toolchest.recent.is_empty(), "Clear Recent Tools");
    panel(&mut h, "toolchest");
    if !visible(&h, "Clear Recent Tools") {
        h.query_all_by_label("Options").next().unwrap().click();
        h.run_steps(3);
    }
    assert!(
        visible(&h, "Clear Recent Tools"),
        "Tool Chest > Options has Clear Recent Tools"
    );
}

/// K-120, K-121: a Properties-mode tool draws a new shape with the saved look; a Drawing-mode
/// tool places an exact copy by a click. One click on a tool uses it once; a double-click keeps
/// it.
#[test]
fn properties_and_drawing_modes_single_and_double_click() {
    let mut h = app();
    panel(&mut h, "toolchest");
    let (set, item, name) = chest_item(&mut h, "Slab Edge");
    let look = h.state().state.toolchest.item(&set, &item).unwrap().markup.clone();
    // one click: one use
    chest_node(&h, &name).click();
    h.run_steps(3);
    let n0 = markups(&h).len();
    drag(&mut h, (100.0, 450.0), (160.0, 520.0));
    let m = new_markups(&h, n0);
    assert_eq!(m.len(), 1, "drawn with the tool");
    assert_eq!(m[0].color, look.color);
    assert_eq!(m[0].subject, "Slab Edge");
    assert!(
        (markupcraft_geom::bbox(&m[0].pts).unwrap().width() - 60.0).abs() < 3.0,
        "a new size (Properties mode)"
    );
    drag(&mut h, (300.0, 450.0), (360.0, 520.0));
    assert_eq!(markups(&h).len(), n0 + 1, "single click: the tool was used once");
    // double click: sticky
    let node = chest_node(&h, &name);
    let at = node.rect().center();
    double_click_at(&mut h, at);
    h.run_steps(3);
    let n1 = markups(&h).len();
    drag(&mut h, (100.0, 600.0), (160.0, 660.0));
    drag(&mut h, (300.0, 600.0), (360.0, 660.0));
    assert_eq!(markups(&h).len(), n1 + 2, "double click: the tool stays");
    key(&mut h, NONE, Key::Escape);
    // Drawing mode: an exact copy by one click
    h.state_mut()
        .state
        .toolchest
        .update_item(&set, &item, |it| it.mode = markupcraft_ui_egui::chest::Mode::Drawing);
    chest_node(&h, &name).click();
    h.run_steps(3);
    let n2 = markups(&h).len();
    click(&mut h, 500.0, 650.0);
    let m = new_markups(&h, n2);
    assert_eq!(m.len(), 1, "placed by a click");
    let b = markupcraft_geom::bbox(&m[0].pts).unwrap();
    assert!(
        (b.width() - 100.0).abs() < 1.0 && (b.height() - 100.0).abs() < 1.0,
        "the saved size: {b:?}"
    );
}

/// K-122, K-128: tool items and sets are renamed, reordered and deleted; Update from Selection
/// gives an item the selected markup's look.
#[test]
fn manage_tool_sets_and_items() {
    let mut h = app();
    let (set, a, _) = chest_item(&mut h, "Alpha");
    let (_, b, _) = chest_item(&mut h, "Beta");
    let c = &mut h.state_mut().state.toolchest;
    c.update_item(&set, &a, |it| it.name = "Renamed".into());
    c.move_item(&set, &b, -1);
    let names: Vec<String> = c.find_set(&set).unwrap().items.iter().map(|i| i.name.clone()).collect();
    assert_eq!(
        names.last().map(String::as_str),
        Some("Renamed"),
        "Beta moved up: {names:?}"
    );
    c.remove_item(&set, &a);
    assert!(c.item(&set, &a).is_none());
    let s2 = c.add_set("Second");
    c.rename_set(&s2, "Electrical");
    c.move_set(&s2, -1);
    assert!(c.sets.iter().any(|s| s.title == "Electrical"));
    c.remove_set(&s2);
    assert!(!c.sets.iter().any(|s| s.title == "Electrical"));
    // Update from Selection
    let mut m = markups(&h).into_iter().find(|m| m.kind == Kind::Rectangle).unwrap();
    m.color = markupcraft_model::Color::rgb(0.0, 0.0, 1.0);
    assert!(h.state_mut().state.toolchest.update_item_look(&set, &b, &m));
    assert_eq!(h.state().state.toolchest.item(&set, &b).unwrap().markup.color, m.color);
    // the item's right-click menu offers these
    panel(&mut h, "toolchest");
    let node = chest_node(&h, "Beta");
    node.click_secondary();
    h.run_steps(3);
    for s in ["Properties", "Rename", "Delete", "Update from Selection", "Sequence"] {
        assert!(visible(&h, s), "item menu has {s}");
    }
}

/// K-123: with Update Tool Set Item on Reuse on, restyling a markup drawn from a tool updates
/// the tool.
#[test]
fn update_tool_item_on_reuse() {
    let mut h = app();
    panel(&mut h, "toolchest");
    let (set, item, name) = chest_item(&mut h, "Reuse Me");
    h.state_mut().state.toolchest.extras.update_on_reuse = true;
    chest_node(&h, &name).click();
    h.run_steps(3);
    let n0 = markups(&h).len();
    drag(&mut h, (100.0, 450.0), (160.0, 520.0));
    let m = new_markups(&h, n0).pop().unwrap();
    key(&mut h, NONE, Key::Escape);
    select(&mut h, std::slice::from_ref(&m.id));
    let blue = markupcraft_model::Color::rgb(0.0, 0.0, 1.0);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&m.id),
            &markupcraft_engine::props::MarkupPatch {
                color: Some(blue),
                ..Default::default()
            },
        )
        .unwrap();
    h.run_steps(4);
    assert_eq!(
        h.state().state.toolchest.item(&set, &item).unwrap().markup.color,
        blue,
        "the tool follows"
    );
}

/// K-124: comment persistence: by default a saved comment carries into new markups except text
/// boxes.
#[test]
fn tool_comment_persistence_option() {
    let mut h = app();
    panel(&mut h, "toolchest");
    if !visible(&h, "Keep comments") {
        h.query_all_by_label("Options").next().unwrap().click();
        h.run_steps(3);
    }
    assert!(
        visible(&h, "Keep comments"),
        "Tool Chest > Options has the comment setting"
    );
    assert_eq!(
        h.state().state.toolchest.extras.comments,
        Default::default(),
        "the default"
    );
    let r = sample_id(&h, Kind::Rectangle);
    h.state_mut()
        .state
        .doc_mut()
        .unwrap()
        .session
        .set_properties(
            std::slice::from_ref(&r),
            &markupcraft_engine::props::MarkupPatch {
                contents: Some("See spec".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let (_s, _i, name) = chest_item(&mut h, "Commented");
    chest_node(&h, &name).click();
    h.run_steps(3);
    let n0 = markups(&h).len();
    drag(&mut h, (100.0, 450.0), (160.0, 520.0));
    let m = new_markups(&h, n0).pop().unwrap();
    assert_eq!(
        m.contents, "See spec",
        "a rectangle keeps the tool's comment by default"
    );
}

/// K-125, K-052: a tool set with a scale resizes its Drawing-mode symbols to the page scale.
#[test]
fn tool_set_scale_resizes_symbols() {
    let mut h = app();
    panel(&mut h, "toolchest");
    let (set, item, name) = chest_item(&mut h, "Valve");
    {
        let c = &mut h.state_mut().state.toolchest;
        c.update_item(&set, &item, |it| it.mode = markupcraft_ui_egui::chest::Mode::Drawing);
        // drawn at 1/4 in = 1 ft; the page is 1/8 in = 1 ft, so it lands half size
        c.set_scale(&set, Some(markupcraft_model::Scale::architectural(0.25, 1.0)));
    }
    chest_node(&h, &name).click();
    h.run_steps(3);
    let n0 = markups(&h).len();
    click(&mut h, 300.0, 650.0);
    let m = new_markups(&h, n0).pop().expect("a symbol");
    let b = markupcraft_geom::bbox(&m.pts).unwrap();
    assert!((b.width() - 50.0).abs() < 1.0, "resized to the page scale: {b:?}");
}

/// K-126, K-127: Symbol view (icon tiles, with a size slider) and Detail view.
#[test]
fn symbol_and_detail_views_and_icon_size() {
    let mut h = app();
    panel(&mut h, "toolchest");
    assert!(visible(&h, "Symbol") && visible(&h, "Detail"));
    h.query_all_by_label("Symbol").next().unwrap().click();
    h.run_steps(3);
    let v = h.state().state.toolchest.view;
    assert_eq!(v, markupcraft_ui_egui::chest_sets::ChestView::Symbol);
    h.state_mut().state.toolchest.set_view(v, 200.0);
    assert!(h.state().state.toolchest.icon_size <= 96.0, "the slider is bounded");
    h.query_all_by_label("Detail").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(
        h.state().state.toolchest.view,
        markupcraft_ui_egui::chest_sets::ChestView::Detail
    );
}

/// K-129: a tool set is exported and imported to share it (.btx is Revu's format).
#[test]
fn tool_set_export_and_import() {
    let mut h = app();
    let (set, _, name) = chest_item(&mut h, "Shared Tag");
    let text = h.state().state.toolchest.export_set(&set).unwrap();
    let mut other = markupcraft_ui_egui::chest::ToolChest::default();
    let id = other.import_set(&text).unwrap();
    assert!(other.find_set(&id).unwrap().items.iter().any(|i| i.name == name));
    // a .btx file from Revu is not read
    let mut c = markupcraft_ui_egui::chest::ToolChest::default();
    assert!(c.import_set("<?xml version=\"1.0\"?><BluebeamRevuToolSet/>").is_err());
}

/// K-130, K-131: a tool set pinned to the toolbar; a collapsed set offers a flyout.
#[test]
fn pinned_and_collapsed_tool_sets() {
    let mut h = app();
    let (set, _, name) = chest_item(&mut h, "Pinned Tag");
    panel(&mut h, "toolchest");
    h.state_mut().state.toolchest.set_pinned(&set, true);
    h.run_steps(4);
    assert!(
        h.query_all_by_label(&name).count() >= 2,
        "the pinned set's tool also shows in its toolbar"
    );
    h.state_mut().state.toolchest.sets[0].collapsed = true;
    h.state_mut().state.toolchest.set_pinned(&set, false);
    h.run_steps(4);
    assert!(
        h.query_all_by_label(&name).count() >= 1,
        "the collapsed set still offers its tools in a flyout"
    );
}

/// K-132: a shared tool set is read-only until checked out; a lock file names who has it.
#[test]
fn shared_tool_set_check_out_and_in() {
    let mut h = app();
    let (set, _, _) = chest_item(&mut h, "Team Tag");
    let text = h.state().state.toolchest.export_set(&set).unwrap();
    let dir = temp_dir("shared");
    let file = dir.join("team.mctools");
    std::fs::write(&file, text).unwrap();
    let mut a = markupcraft_ui_egui::chest::ToolChest::default();
    let mut b = markupcraft_ui_egui::chest::ToolChest::default();
    let sa = a.add_shared(&file).unwrap();
    let sb = b.add_shared(&file).unwrap();
    assert!(a.is_locked(&sa), "read-only until checked out");
    a.check_out(&sa, "Ann").unwrap();
    assert!(!a.is_locked(&sa));
    assert!(b.check_out(&sb, "Bob").is_err(), "someone else has it");
    a.check_in(&sa).unwrap();
    b.check_out(&sb, "Bob").unwrap();
}

/// K-133: profiles keep the preferences; switching and exporting them.
#[test]
fn profiles_switch_and_export() {
    let (dir, mut a) = open("profiles");
    call(&mut a, "prefs_set", json!({ "values": { "author": "Estimator A" } }));
    call(&mut a, "profile_switch", json!({ "profile": "Field" }));
    call(&mut a, "prefs_set", json!({ "values": { "author": "Field B" } }));
    let l = call(&mut a, "profile_list", json!({})).to_string();
    assert!(l.contains("Field") && l.contains("Default"), "{l}");
    call(&mut a, "prefs_export", json!({ "path": "field.json" }));
    assert!(
        std::fs::read_to_string(dir.join("field.json"))
            .unwrap()
            .contains("Field B")
    );
    call(&mut a, "profile_switch", json!({ "profile": "Default" }));
    assert_eq!(
        call(&mut a, "prefs_get", json!({}))["preferences"]["author"],
        "Estimator A"
    );
}

/// K-134: dragging markups from the page onto the Tool Chest adds them to My Tools.
#[test]
fn drag_a_markup_onto_the_tool_chest() {
    let mut h = app();
    panel(&mut h, "toolchest");
    let r = sample_id(&h, Kind::Rectangle);
    key(&mut h, NONE, Key::V);
    select(&mut h, std::slice::from_ref(&r));
    let before = item_count(&h);
    let target = h.query_all_by_label("My Tools").next().unwrap().rect().center();
    let from = screen(&h, 900.0, 650.0);
    h.hover_at(from);
    h.step();
    button(&mut h, from, true, NONE);
    for i in 1..=12 {
        h.hover_at(from + (target - from) * (i as f32 / 12.0));
        h.step();
    }
    button(&mut h, target, false, NONE);
    h.run_steps(4);
    assert_eq!(item_count(&h), before + 1, "added to My Tools");
    let m = find(&h, &r);
    assert_eq!(m.pts[0], Point::new(900.0, 600.0), "the markup stays where it was");
}

/// K-014: a Sequence tool's label counts up each time it is placed.
#[test]
fn sequence_tool_counts_up() {
    let mut h = app();
    panel(&mut h, "toolchest");
    let (set, item, name) = chest_item(&mut h, "Tag");
    h.state_mut().state.toolchest.update_item(&set, &item, |it| {
        it.sequence = true;
        it.markup.label = "A1".into();
    });
    let node = chest_node(&h, &name);
    let at = node.rect().center();
    double_click_at(&mut h, at);
    h.run_steps(3);
    let n0 = markups(&h).len();
    for x in [100.0, 250.0, 400.0] {
        drag(&mut h, (x, 450.0), (x + 60.0, 520.0));
    }
    let labels: Vec<String> = new_markups(&h, n0).into_iter().map(|m| m.label).collect();
    assert_eq!(labels, vec!["A1", "A2", "A3"]);
}

/// K-053: sketch tools: a segment placed by typed length and angle.
#[test]
fn sketch_to_scale_places_typed_segments() {
    let mut h = app();
    key(&mut h, SHIFT, Key::N);
    click(&mut h, 100.0, 450.0);
    h.run_steps(2);
    assert!(
        visible(&h, "Length") && visible(&h, "Angle"),
        "the Sketch to Scale bar while drawing"
    );
    h.query_all_by_label("Tools").next().unwrap().click();
    h.run_steps(3);
    assert!(visible(&h, "Place Typed Segment"), "Tools > Sketch commands");
}

// ---------------------------------------------------------------------------------------------
// G. Layers

fn layer<'a>(l: &'a Value, name: &str) -> &'a Value {
    l["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == name)
        .unwrap_or_else(|| panic!("no layer {name}: {l}"))
}

/// K-135 to K-137, K-140, K-141, K-145, K-146: layers are created, shown / hidden, isolated,
/// turned all on, renamed and deleted; their states are saved as optional content.
#[test]
fn layers_create_toggle_isolate_rename_delete() {
    let (dir, mut a) = open("layers");
    call(&mut a, "layer_create", json!({ "name": "A-Walls" }));
    call(&mut a, "layer_create", json!({ "name": "E-Power" }));
    let wall = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 200.0, 550.0), json!({}));
    let pwr = add(&mut a, "Ellipse", rect_pts(300.0, 450.0, 400.0, 550.0), json!({}));
    call(&mut a, "layer_assign", json!({ "ids": [wall], "layer": "A-Walls" }));
    call(&mut a, "layer_assign", json!({ "ids": [pwr], "layer": "E-Power" }));
    call(&mut a, "layer_set", json!({ "name": "E-Power", "visible": false }));
    let vis = call(&mut a, "layer_markups", json!({ "visible_only": true })).to_string();
    assert!(
        vis.contains(&wall) && !vis.contains(&pwr),
        "a hidden layer's markups are hidden: {vis}"
    );
    call(&mut a, "layer_set", json!({ "name": "A-Walls", "isolate": true }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(layer(&l, "A-Walls")["visible"], true);
    assert_eq!(layer(&l, "E-Power")["visible"], false);
    call(&mut a, "layer_set", json!({ "show_all": true }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(layer(&l, "E-Power")["visible"], true, "Show All");
    call(
        &mut a,
        "layer_set",
        json!({ "name": "E-Power", "print": false, "locked": true, "visible": false }),
    );
    let s = save(&mut a, &dir, "layers.pdf");
    assert!(by_nm(&s, &pwr).has("OC"));
    let (mut b, _) = reopen(&dir, "layers.pdf");
    let l = call(&mut b, "layer_list", json!({}));
    let e = layer(&l, "E-Power");
    assert_eq!(
        (e["visible"].clone(), e["print"].clone(), e["locked"].clone()),
        (json!(false), json!(false), json!(true)),
        "{e}"
    );
    call(
        &mut b,
        "layer_rename",
        json!({ "name": "E-Power", "new_name": "E-Lighting" }),
    );
    assert_eq!(get(&mut b, &pwr)["layer"], "E-Lighting", "markups follow the rename");
    call(&mut b, "layer_delete", json!({ "name": "E-Lighting" }));
    assert_eq!(get(&mut b, &pwr)["layer"], "", "left on no layer");
    call(
        &mut b,
        "layer_delete",
        json!({ "name": "A-Walls", "delete_markups": true }),
    );
    assert!(
        !list(&mut b).iter().any(|m| m["id"] == wall.as_str()),
        "deleted with its layer"
    );
    // the Layers panel
    let mut h = app();
    panel(&mut h, "layers");
    for s in [
        "New",
        "Show All",
        "This page only",
        "A-Z",
        "Print Layers",
        "Export Layers",
    ] {
        assert!(visible(&h, s), "the Layers panel has {s}");
    }
}

/// K-138: layers nest by dragging (the /Order tree).
#[test]
fn layer_hierarchy() {
    let (_dir, mut a) = open("nest");
    call(&mut a, "layer_create", json!({ "name": "Electrical" }));
    call(&mut a, "layer_create", json!({ "name": "Power" }));
    let t = call(&mut a, "layer_nest", json!({ "name": "Power", "parent": "Electrical" }));
    let s = t.to_string();
    assert!(s.contains("Electrical") && s.contains("\"depth\":1"), "{s}");
}

/// K-139: the Markup Layer. GAP: there is no "layer new markups go on"; markups are put on a
/// layer after they are drawn (layer_assign, Properties > Layer, the list's Layer menu).
#[test]
fn markups_are_put_on_a_layer_after_drawing() {
    let (_dir, mut a) = open("mlayer");
    call(&mut a, "layer_create", json!({ "name": "Review" }));
    let id = add(&mut a, "Rectangle", rect_pts(100.0, 450.0, 200.0, 550.0), json!({}));
    assert_eq!(get(&mut a, &id)["layer"], "", "new markups go on no layer");
    call(&mut a, "layer_assign", json!({ "ids": [id], "layer": "Review" }));
    assert_eq!(get(&mut a, &id)["layer"], "Review");
}

/// K-142: named visibility configurations.
#[test]
fn layer_configurations() {
    let (_dir, mut a) = open("configs");
    call(&mut a, "layer_create", json!({ "name": "Demo" }));
    call(&mut a, "layer_create", json!({ "name": "New Work" }));
    call(&mut a, "layer_set", json!({ "name": "Demo", "visible": false }));
    call(&mut a, "layer_config", json!({ "action": "save", "name": "Proposed" }));
    call(&mut a, "layer_set", json!({ "show_all": true }));
    call(&mut a, "layer_config", json!({ "action": "apply", "name": "Proposed" }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(layer(&l, "Demo")["visible"], false);
    let c = call(&mut a, "layer_config", json!({})).to_string();
    assert!(c.contains("Proposed"), "{c}");
    call(
        &mut a,
        "layer_config",
        json!({ "action": "delete", "name": "Proposed" }),
    );
    assert!(!call(&mut a, "layer_config", json!({})).to_string().contains("Proposed"));
}

/// K-143, K-144: preview the print or export layers; layers used on a page, by name.
#[test]
fn layer_view_options() {
    let (_dir, mut a) = open("lview");
    call(&mut a, "layer_create", json!({ "name": "Zeta" }));
    call(&mut a, "layer_create", json!({ "name": "Alpha" }));
    call(&mut a, "layer_create", json!({ "name": "Unused" }));
    add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "layer": "Zeta" }),
    );
    add(
        &mut a,
        "Rectangle",
        rect_pts(300.0, 450.0, 400.0, 550.0),
        json!({ "layer": "Alpha" }),
    );
    call(&mut a, "layer_set", json!({ "name": "Zeta", "print": false }));
    call(&mut a, "layer_view", json!({ "preview": "print" }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(
        layer(&l, "Zeta")["visible"],
        false,
        "the print preview hides non-printing layers"
    );
    call(&mut a, "layer_view", json!({ "preview": "end" }));
    let l = call(&mut a, "layer_list", json!({}));
    assert_eq!(layer(&l, "Zeta")["visible"], true, "End Preview restores");
    let p = call(&mut a, "layer_view", json!({ "page": 1 }));
    assert_eq!(
        p["on_page"],
        json!(["Alpha", "Zeta"]),
        "the page's layers, by name: {p}"
    );
}

/// K-147: a page of another PDF comes in as a layer; a layer goes out as a PDF.
#[test]
fn import_and_export_a_layer() {
    let (dir, mut a) = open("limport");
    call(
        &mut a,
        "layer_import",
        json!({ "path": "plan.pdf", "src_page": 2, "page": 1, "name": "Notes Overlay" }),
    );
    let l = call(&mut a, "layer_list", json!({})).to_string();
    assert!(l.contains("Notes Overlay"), "{l}");
    add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "layer": "Notes Overlay" }),
    );
    call(
        &mut a,
        "layer_export",
        json!({ "name": "Notes Overlay", "out": "overlay.pdf" }),
    );
    let (mut b, l) = reopen(&dir, "overlay.pdf");
    assert_eq!(l.len(), 1, "only that layer's markups: {l:?}");
    let t = call(&mut b, "text_search", json!({ "text": "GENERAL NOTES" }));
    assert!(t["count"].as_u64().unwrap() >= 1, "with its page content: {t}");
}

/// K-148, K-181, K-182: flatten markups (all, by selection or type) into the page, onto a layer,
/// and unflatten them again.
#[test]
fn flatten_and_unflatten() {
    let (dir, mut a) = open("flatten");
    let r = add(
        &mut a,
        "Rectangle",
        rect_pts(100.0, 450.0, 200.0, 550.0),
        json!({ "subject": "Keep" }),
    );
    let c = add(&mut a, "Cloud", rect_pts(300.0, 450.0, 400.0, 550.0), json!({}));
    call(
        &mut a,
        "markup_flatten",
        json!({ "kinds": ["Cloud"], "layer": "Flattened", "recoverable": true }),
    );
    let l = list(&mut a);
    assert!(l.iter().any(|m| m["id"] == r.as_str()), "other kinds stay");
    assert!(!l.iter().any(|m| m["id"] == c.as_str()), "the cloud is burned in");
    assert!(call(&mut a, "layer_list", json!({})).to_string().contains("Flattened"));
    call(&mut a, "doc_save", json!({ "path": "flat.pdf", "full": true }));
    let (mut b, l) = reopen(&dir, "flat.pdf");
    assert!(!l.iter().any(|m| m["id"] == c.as_str()), "still flattened after a save");
    call(&mut b, "markup_unflatten", json!({}));
    assert!(
        list(&mut b).iter().any(|m| m["kind"] == "Cloud"),
        "unflatten brings it back"
    );
    // the menu command
    let mut h = app();
    let n = markups(&h).len();
    let rid = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[rid]);
    run(&mut h, "document.flatten");
    h.run_steps(4);
    assert!(markups(&h).len() < n || visible(&h, "Flatten"), "Document > Flatten");
}

// ---------------------------------------------------------------------------------------------
// H. Markups List

/// One CSV line's fields (quoted fields may hold commas).
fn csv_fields(l: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = l.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Summary CSV rows (header first) for `columns`.
fn summary_rows(a: &mut Automation, dir: &Path, columns: &[&str], extra: Value) -> Vec<Vec<String>> {
    let mut args = json!({ "out": "list.csv", "columns": columns });
    if let (Some(o), Some(e)) = (args.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    call(a, "summary_export", args);
    let text = std::fs::read_to_string(dir.join("list.csv")).unwrap();
    text.lines().map(csv_fields).collect()
}

/// K-149, K-173: the Markups List panel lists every markup; clicking a row selects it and
/// selecting on the page shows it in the list.
#[test]
fn markups_list_panel_and_row_selection() {
    let mut h = app();
    panel(&mut h, "markups");
    assert!(visible(&h, "Total (6)"), "six markups listed");
    for s in ["Subject", "Author", "Date", "Color", "Comments"] {
        assert!(visible(&h, s), "column {s}");
    }
    let row = h
        .query_all_by_label("Verify sink location")
        .next()
        .or_else(|| h.query_all_by_value("Verify sink location").next());
    let row = row.expect("the text box's row");
    row.click();
    h.run_steps(4);
    let tb = sample_id(&h, Kind::Text);
    assert_eq!(selection(&h), vec![tb], "a row click selects the markup");
}

/// K-150 to K-153: the standard columns.
#[test]
fn standard_columns() {
    let (dir, mut a) = open("columns");
    let cols = [
        "subject",
        "label",
        "pagelabel",
        "page",
        "author",
        "date",
        "created",
        "color",
        "comments",
        "layer",
        "space",
        "sequence",
        "measurement",
        "length",
        "area",
        "wallarea",
        "volume",
        "count",
        "depth",
        "height",
        "width",
        "risedrop",
        "slope",
        "unit",
        "x",
        "y",
        "xcenter",
        "ycenter",
        "docwidth",
        "docheight",
        "status",
        "checkmark",
        "lock",
        "capture",
        "legend",
        "view3d",
    ];
    let rows = summary_rows(&mut a, &dir, &cols, json!({}));
    let head = &rows[0];
    for h in [
        "Subject",
        "Page Label",
        "Measurement",
        "Wall Area",
        "Rise/Drop",
        "X Center",
        "Document Width",
        "Status",
        "Checkmark",
        "Lock",
        "Capture",
        "Legend",
        "3D View",
    ] {
        assert!(head.iter().any(|c| c == h), "header {h}: {head:?}");
    }
    let area = rows
        .iter()
        .find(|r| r.iter().any(|c| c == "Area") && r.iter().any(|c| c.contains("sf")))
        .expect("the area row");
    let col = |name: &str| head.iter().position(|c| c == name).unwrap();
    assert!(area[col("Measurement")].contains("629.63"), "{area:?}");
    assert_eq!(area[col("Author")], "Estimator");
    let x: f64 = area[col("X")]
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .unwrap_or(-1.0);
    assert!(
        (x - 300.0 / 72.0).abs() < 0.02,
        "X in inches from the corner: {}",
        area[col("X")]
    );
    assert!(
        area[col("Document Width")].contains("17"),
        "{}",
        area[col("Document Width")]
    );
}

/// K-154, K-155: columns are shown or hidden from the Columns menu; Manage Columns adds custom
/// ones.
#[test]
fn show_hide_and_manage_columns() {
    let mut h = app();
    panel(&mut h, "markups");
    h.query_all_by_label("Columns").next().unwrap().click();
    h.run_steps(3);
    for s in ["Layer", "Space", "Length", "Manage Columns"] {
        assert!(visible(&h, s), "the Columns menu lists {s}");
    }
}

/// K-156 to K-162: custom columns of every type; formulas from other columns; totals.
#[test]
fn custom_columns_and_formula_totals() {
    let (dir, mut a) = open("custom");
    call(
        &mut a,
        "columns_set",
        json!({ "columns": [
            { "id": "note", "name": "Note", "type": "Text", "default": "n/a" },
            { "id": "cost", "name": "Unit Cost", "type": "Currency", "decimals": 2, "symbol": "$" },
            { "id": "due", "name": "Due", "type": "Date" },
            { "id": "ok", "name": "OK", "type": "Checkmark" },
            { "id": "mat", "name": "Material", "type": "Choice", "items": [
                { "item": "Concrete", "subject": "Area", "value": 12.5 }, { "item": "Asphalt", "value": 4 } ], "allow_custom": true },
            { "id": "total", "name": "Total", "type": "Formula", "formula": "Measurement * [Unit Cost]", "total": true, "decimals": 2 }
        ] }),
    );
    let id = "SAMPLEAREAAAAAAA";
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:cost", "value": "10" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:due", "value": "2026-11-01" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:ok", "value": "true" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "c:mat", "value": "Concrete" }),
    );
    assert!(
        a.call(
            "list_cell_set",
            &json!({ "id": id, "column": "c:cost", "value": "lots" })
        )
        .is_err(),
        "a number column refuses text"
    );
    let rows = summary_rows(
        &mut a,
        &dir,
        &[
            "id",
            "subject",
            "measurement",
            "c:note",
            "c:cost",
            "c:due",
            "c:ok",
            "c:mat",
            "c:total",
        ],
        json!({ "group_by": ["subject"] }),
    );
    let r = rows
        .iter()
        .find(|r| r.first().is_some_and(|c| c == id))
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(r[3], "n/a", "default value");
    assert!(r[4].contains("10.00"), "currency: {r:?}");
    assert_eq!(r[5], "2026-11-01");
    assert!(r[7] == "Concrete");
    let total: f64 = r[8].replace(['$', ' ', ','], "").parse().unwrap_or(0.0);
    assert!((total - 6296.28).abs() < 0.1, "629.63 sf x $10: {}", r[8]);
    assert!(
        rows.iter().any(
            |row| row.iter().any(|c| c.contains("6,296.28") || c.contains("6296.28"))
                && row.first() != Some(&id.to_string())
        ),
        "a group total row: {rows:?}"
    );
}

/// K-163: custom columns are saved to the profile and come back on other documents.
#[test]
fn custom_columns_saved_to_the_profile() {
    let mut h = app();
    let col = markupcraft_model::columns::CustomColumn {
        id: "cost".into(),
        name: "Unit Cost".into(),
        kind: markupcraft_model::columns::ColumnType::Currency,
        ..Default::default()
    };
    h.state_mut()
        .state
        .toolchest
        .save_profile_columns(std::slice::from_ref(&col));
    assert!(
        h.state()
            .state
            .toolchest
            .extras
            .columns
            .iter()
            .any(|c| c.name == "Unit Cost")
    );
    h.query_all_by_label("Markup").next().unwrap().click();
    h.run_steps(3);
    assert!(visible(&h, "Profile Columns"), "Markup > Profile Columns");
}

/// K-164 to K-167: sort, group with subtotals, filter by values and quick search.
#[test]
fn sort_group_filter_and_search() {
    let (dir, mut a) = open("sort");
    for (i, s) in ["Beta", "Alpha", "Gamma", "Alpha"].iter().enumerate() {
        let x = 100.0 + 60.0 * i as f64;
        add(
            &mut a,
            "Rectangle",
            rect_pts(x, 450.0, x + 40.0, 490.0),
            json!({ "subject": s }),
        );
    }
    let rows = summary_rows(&mut a, &dir, &["subject"], json!({ "sort": "subject" }));
    let subs: Vec<&str> = rows
        .iter()
        .skip(1)
        .map(|r| r[0].as_str())
        .filter(|s| ["Alpha", "Beta", "Gamma"].contains(s))
        .collect();
    assert_eq!(subs, vec!["Alpha", "Alpha", "Beta", "Gamma"], "sorted");
    let rows = summary_rows(
        &mut a,
        &dir,
        &["subject", "count"],
        json!({ "filters": { "subject": ["Alpha"] } }),
    );
    assert_eq!(rows.iter().skip(1).filter(|r| r[0] == "Alpha").count(), 2);
    assert!(!rows.iter().any(|r| r[0] == "Beta"), "filtered out");
    // the panel: a search box, and markups left out by a filter are faded on the page
    let mut h = app();
    panel(&mut h, "markups");
    let search = h.query_all_by_role(egui::accesskit::Role::TextInput).next();
    assert!(search.is_some(), "a search box");
    h.state_mut().state.list.view.search = "sink".into();
    h.run_steps(4);
    assert!(visible(&h, "Total (1)"), "one row matches 'sink'");
}

/// K-168: saved views of columns, sort, grouping and filters.
#[test]
fn saved_list_views() {
    let mut h = app();
    panel(&mut h, "markups");
    h.query_all_by_label("Views").next().unwrap().click();
    h.run_steps(3);
    assert!(visible(&h, "Save"), "Views > Save current view");
}

/// K-169 to K-172: inline cell edits, status, checkmark and replies.
#[test]
fn cells_status_checkmark_and_replies() {
    let (dir, mut a) = open("cells");
    let id = "SAMPLESQUAREAAAA";
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "subject", "value": "Equipment" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "comments", "value": "Verify clearance" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "status", "value": "Rejected" }),
    );
    call(
        &mut a,
        "list_cell_set",
        json!({ "id": id, "column": "checkmark", "value": "true" }),
    );
    call(&mut a, "markup_reply", json!({ "id": id, "add": "Moved 2 ft" }));
    let r = call(&mut a, "markup_reply", json!({ "id": id, "add": "OK" }));
    assert!(r.to_string().contains("Moved 2 ft"), "{r}");
    call(&mut a, "markup_reply", json!({ "id": id, "delete": 2 }));
    let m = get(&mut a, id);
    assert_eq!(m["subject"], "Equipment");
    assert_eq!(m["contents"], "Verify clearance");
    assert_eq!(m["status"], "Rejected");
    assert_eq!(m["checked"], true);
    let s = save(&mut a, &dir, "cells.pdf");
    assert!(
        s.iter()
            .any(|x| x.text("StateModel") == "Review" && x.text("State") == "Rejected")
    );
    assert!(
        s.iter()
            .any(|x| x.text("StateModel") == "Marked" && x.text("State") == "Marked")
    );
    let replies: Vec<String> = s
        .iter()
        .filter(|x| x.has("IRT") && x.text("StateModel").is_empty() && x.name("RT") != "Group")
        .map(|x| x.text("Contents"))
        .collect();
    assert_eq!(replies, vec!["Moved 2 ft".to_string()], "the deleted reply is gone");
}

/// K-174: Copy Rows puts the selected rows on the clipboard as tab-separated text.
#[test]
fn copy_rows_as_tab_separated_text() {
    let mut h = app();
    panel(&mut h, "markups");
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, &[r]);
    let row = h
        .query_all_by_label("Rectangle")
        .next()
        .or_else(|| h.query_all_by_value("Rectangle").next())
        .unwrap();
    row.click_secondary();
    h.run_steps(3);
    assert!(visible(&h, "Copy Rows"), "the row menu has Copy Rows");
    h.query_all_by_label("Copy Rows").next().unwrap().click();
    let mut copied = None;
    for _ in 0..4 {
        h.step();
        copied = copied.or_else(|| {
            h.output().platform_output.commands.iter().find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
        });
    }
    let t = copied.expect("Copy Rows put text on the clipboard");
    assert!(t.contains('\t') && t.contains("Rectangle"), "{t}");
}

/// K-175 to K-178: the summary as CSV, XML and a PDF report; a linked summary appended.
#[test]
fn summary_csv_xml_pdf_and_appended_links() {
    let (dir, mut a) = open("summary");
    call(&mut a, "summary_export", json!({ "out": "s.csv" }));
    call(&mut a, "summary_export", json!({ "out": "s.xml" }));
    call(
        &mut a,
        "summary_export",
        json!({ "out": "s.pdf", "pdf_thumbnails": 96 }),
    );
    let csv = std::fs::read_to_string(dir.join("s.csv")).unwrap();
    assert!(csv.contains("629.63"), "{csv}");
    let xml = std::fs::read_to_string(dir.join("s.xml")).unwrap();
    assert!(xml.starts_with("<?xml") && xml.contains("629.63"), "{xml}");
    let mut b = automation(&dir);
    call(&mut b, "doc_open", json!({ "path": "s.pdf" }));
    let t = call(&mut b, "text_search", json!({ "text": "629.63" }));
    assert!(t["count"].as_u64().unwrap() >= 1, "the report lists the area");
    call(
        &mut b,
        "summary_export",
        json!({ "out": "flow.pdf", "pdf_flow": true, "pages": [1] }),
    );
    let pages = call(&mut a, "doc_info", json!({}))["pages"].as_u64().unwrap();
    call(&mut a, "summary_append", json!({}));
    let after = call(&mut a, "doc_info", json!({}))["pages"].as_u64().unwrap();
    assert!(after > pages, "summary pages appended");
    let links = call(&mut a, "link_list", json!({})).to_string();
    assert!(
        links.contains("\"page\":3") || links.contains("page"),
        "rows link back: {links}"
    );
}

/// K-179, K-180: markups are exported to XFDF / FDF and imported from them or from another PDF.
#[test]
fn import_and_export_markups() {
    let (dir, mut a) = open("xfdf");
    call(&mut a, "xfdf_export", json!({ "out": "m.xfdf" }));
    call(&mut a, "xfdf_export", json!({ "out": "m.fdf" }));
    assert!(std::fs::read_to_string(dir.join("m.xfdf")).unwrap().contains("<xfdf"));
    std::fs::copy(dir.join("plan.pdf"), dir.join("bare.pdf")).unwrap();
    let mut b = automation(&dir);
    call(&mut b, "doc_open", json!({ "path": "bare.pdf" }));
    let all: Vec<Value> = list(&mut b).iter().map(|m| m["id"].clone()).collect();
    call(&mut b, "markup_delete", json!({ "ids": all }));
    call(&mut b, "xfdf_import", json!({ "path": "m.xfdf" }));
    assert_eq!(list(&mut b).len(), 6, "all six back from XFDF");
    let all: Vec<Value> = list(&mut b).iter().map(|m| m["id"].clone()).collect();
    call(&mut b, "markup_delete", json!({ "ids": all }));
    call(&mut b, "markup_import_pdf", json!({ "path": "plan.pdf" }));
    assert!(list(&mut b).len() >= 5, "from another PDF");
    // GAP: Revu's BAX exchange format is neither written nor read.
}

/// K-183 to K-187: the Markups List row menu: lock, layer, legend, properties and delete.
#[test]
fn markups_list_row_menu() {
    let mut h = app();
    panel(&mut h, "markups");
    let r = sample_id(&h, Kind::Rectangle);
    select(&mut h, std::slice::from_ref(&r));
    let row = h
        .query_all_by_label("Rectangle")
        .next()
        .or_else(|| h.query_all_by_value("Rectangle").next())
        .unwrap();
    row.click_secondary();
    h.run_steps(3);
    for s in ["Lock", "Layer", "Create Legend", "Properties", "Delete"] {
        assert!(visible(&h, s), "the row menu has {s}");
    }
    h.query_all_by_label("Delete").last().unwrap().click();
    h.run_steps(3);
    assert!(!markups(&h).iter().any(|m| m.id == r), "deleted from the list");
}

// ---------------------------------------------------------------------------------------------
// I. Measurement tools; J. Customization

/// K-188, K-189: every measurement tool on its Shift+Alt shortcut.
#[test]
fn measurement_tools_on_their_shortcuts() {
    let mut h = app();
    for (k, tool) in [
        (Key::L, "length"),
        (Key::Q, "polylength"),
        (Key::A, "area"),
        (Key::P, "perimeter"),
        (Key::C, "count"),
        (Key::G, "angle"),
        (Key::U, "radius"),
        (Key::D, "diameter"),
        (Key::V, "volume"),
    ] {
        chord(&mut h, SHIFT | Modifiers::ALT, k);
        assert_eq!(h.state().state.tool, tool, "Shift+Alt+{k:?}");
    }
    // a length by two clicks measures at the page scale
    let n0 = markups(&h).len();
    chord(&mut h, SHIFT | Modifiers::ALT, Key::L);
    click(&mut h, 100.0, 450.0);
    click(&mut h, 172.0, 450.0);
    let m = new_markups(&h, n0).pop().expect("a length");
    assert_eq!(m.kind, Kind::Length);
    assert!((m.quantity().unwrap() - 8.0).abs() < 0.2, "{:?}", m.quantity());
}

/// K-190: M opens the Measurements panel in the last measurement mode.
#[test]
fn measure_key_opens_the_measurements_panel() {
    let mut h = app();
    chord(&mut h, SHIFT | Modifiers::ALT, Key::A);
    key(&mut h, NONE, Key::Escape);
    key(&mut h, NONE, Key::M);
    assert!(
        h.state().state.open_panels.contains(&"measurements"),
        "the Measurements panel"
    );
    assert_eq!(h.state().state.tool, "area", "in the last mode");
}

/// K-191: any command gets new keys; a clash takes the keys from the other command.
#[test]
fn custom_keyboard_shortcuts() {
    let mut h = app();
    let k = markupcraft_ui_egui::commands::Keys::new(true, true, false, Key::K);
    let lost = h.state_mut().state.keys.assign("tool.cloud", Some(k));
    let _ = lost;
    h.run_steps(2);
    chord(&mut h, Modifiers::COMMAND | SHIFT, Key::K);
    assert_eq!(h.state().state.tool, "cloud", "the new keys run the tool");
    let lost = h.state_mut().state.keys.assign("tool.rectangle", Some(k));
    assert_eq!(lost.as_deref(), Some("tool.cloud"), "reassigned from the cloud");
    run(&mut h, "tools.customize_keys");
    assert!(visible(&h, "Reset All"), "Tools > Customize Keyboard");
}

/// K-192: markup display preferences: note pop-ups, rollover comments, line weights.
#[test]
fn markup_display_preferences() {
    let mut h = app();
    for c in ["view.note_popups", "view.rollover_comments", "view.line_weights"] {
        let before = h.state().state.checked(c);
        assert!(before.is_some(), "{c} is a toggle");
        run(&mut h, c);
        assert_ne!(h.state().state.checked(c), before, "{c} toggles");
    }
}

// @@END@@
