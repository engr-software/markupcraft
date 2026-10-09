//! Every non-measurement markup kind written, read back, and re-saved as if edited (the
//! `markupcheck` loop), on small PDFs built here. No fixture files.

use std::path::Path;
use std::sync::Arc;

use markupcraft_geom::shapes::LINE_ENDINGS;
use markupcraft_geom::{Point, Rect};
use markupcraft_model::{Color, Document, Kind, Markup, SnapshotSource};
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, SaveOptions, Stream, write_full};
use markupcraft_revu::kinds::text::autosize_text_box;
use markupcraft_revu::{PdfFile, open_bytes, write};

fn obj(entries: &[(&str, Object)]) -> Dict {
    let mut d = Dict::new();
    for (k, v) in entries {
        d.set(k.as_bytes().to_vec(), v.clone());
    }
    d
}

fn nums(v: &[f64]) -> Object {
    Object::Array(v.iter().map(|x| Object::Real(*x)).collect())
}

/// A one-page letter PDF with a blue box in its content and `extra` annotations.
fn blank_pdf(rotate: i64, extra: Vec<Dict>) -> Vec<u8> {
    let mut cos = CosDoc::new_empty();
    let root = cos.root().unwrap();
    let pages = cos.get(root).as_dict().unwrap().reference(b"Pages").unwrap();
    let content = cos.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"0 0 1 rg 100 100 200 100 re f\n",
    )));
    let mut page = obj(&[
        ("Type", Object::name("Page")),
        ("Parent", Object::Ref(pages)),
        ("MediaBox", nums(&[0.0, 0.0, 612.0, 792.0])),
        ("Contents", Object::Ref(content)),
        ("Resources", Object::Dict(Dict::new())),
        ("Rotate", Object::Int(rotate)),
    ]);
    if !extra.is_empty() {
        let refs = extra
            .into_iter()
            .map(|d| Object::Ref(cos.add(Object::Dict(d))))
            .collect();
        page.set(b"Annots".to_vec(), Object::Array(refs));
    }
    let p = cos.add(Object::Dict(page));
    cos.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(p)]));
        d.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    write_full(&cos, &SaveOptions::default()).unwrap()
}

fn open(bytes: Vec<u8>) -> (PdfFile, Document) {
    open_bytes(Arc::new(bytes), Path::new("test.pdf")).unwrap()
}

fn save(f: &mut PdfFile, doc: &mut Document) -> Vec<u8> {
    write::apply(&mut f.cos, doc);
    write_full(&f.cos, &SaveOptions::default()).unwrap()
}

/// Add `ms` to a blank page, save, reload; then mark every markup edited, save and reload
/// again. Returns (first reload, second reload, bytes of the second save).
fn round_trip(ms: Vec<Markup>) -> (Document, Document, Vec<u8>) {
    let (mut f, mut doc) = open(blank_pdf(0, Vec::new()));
    doc.markups.extend(ms);
    let bytes = save(&mut f, &mut doc);
    let (mut g, mut first) = open(bytes);
    let kept = first.clone();
    for m in &mut first.markups {
        m.dirty = true;
    }
    let bytes = save(&mut g, &mut first);
    let (_h, second) = open(bytes.clone());
    (kept, second, bytes)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 2e-3
}

fn same_color(a: &Color, b: &Color) -> bool {
    near(a.r, b.r) && near(a.g, b.g) && near(a.b, b.b)
}

/// The fields `markupcheck` compares, plus the note and stamp ones.
fn assert_same(a: &Markup, b: &Markup) {
    let what = a.kind.name();
    assert_eq!(a.kind, b.kind, "{what}: kind");
    assert_eq!(a.pts.len(), b.pts.len(), "{what}: point count");
    for (p, q) in a.pts.iter().zip(&b.pts) {
        assert!(near(p.x, q.x) && near(p.y, q.y), "{what}: {p:?} -> {q:?}");
    }
    assert_eq!(a.strokes, b.strokes, "{what}: strokes");
    assert_eq!(a.contents, b.contents, "{what}: contents");
    assert!(
        same_color(&a.color, &b.color),
        "{what}: color {:?} -> {:?}",
        a.color,
        b.color
    );
    match (&a.fill, &b.fill) {
        (Some(x), Some(y)) => assert!(same_color(x, y), "{what}: fill"),
        (None, None) => {}
        _ => panic!("{what}: fill {:?} -> {:?}", a.fill, b.fill),
    }
    assert!(near(a.opacity, b.opacity), "{what}: opacity");
    assert!(near(a.line_width, b.line_width), "{what}: line width");
    assert_eq!(a.dash.len(), b.dash.len(), "{what}: dash");
    assert_eq!(
        (&a.line_start, &a.line_end),
        (&b.line_start, &b.line_end),
        "{what}: line ends"
    );
    assert!(near(a.cloud, b.cloud), "{what}: cloud");
    assert_eq!(a.stamp, b.stamp, "{what}: stamp");
    assert_eq!(a.multiply, b.multiply, "{what}: blend");
    if a.kind.is_text() || !a.stamp.is_empty() {
        let (s, t) = (&a.text, &b.text);
        assert_eq!(
            (&s.font, s.bold, s.italic, s.underline, s.align),
            (&t.font, t.bold, t.italic, t.underline, t.align),
            "{what}: font"
        );
        assert!(
            near(s.size, t.size) && same_color(&s.color, &t.color),
            "{what}: font size / colour"
        );
    }
    assert_eq!(a.icon, b.icon, "{what}: icon");
    assert_eq!(a.popup_open, b.popup_open, "{what}: popup open");
}

/// The annotation dictionary of `m` in a saved file.
fn annot_dict(bytes: &[u8], m: &Markup) -> (CosDoc, Dict) {
    let cos = CosDoc::open(Arc::new(bytes.to_vec())).unwrap();
    let d = cos.get(ObjRef::new(m.obj.0, m.obj.1)).as_dict().unwrap().clone();
    (cos, d)
}

/// The decoded `/AP /N` content of an annotation.
fn appearance(cos: &CosDoc, a: &Dict) -> String {
    let ap = cos.resolve(a.get(b"AP").unwrap());
    let n = cos.resolve(ap.as_dict().unwrap().get(b"N").unwrap());
    let Object::Stream(st) = n.as_ref() else {
        panic!("no /AP /N stream")
    };
    String::from_utf8_lossy(&st.decoded().unwrap()).into_owned()
}

fn markup(kind: Kind, pts: Vec<Point>) -> Markup {
    let mut m = Markup::new(kind, 0, pts);
    m.subject = kind.name().into();
    m.author = "Test".into();
    m.line_width = 2.0;
    m
}

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
    Rect::new(x0, y0, x1, y1).corners().to_vec()
}

/// One markup through two saves: the first reload equals what was written, and the second
/// (as if edited) equals the first.
fn check_one(m: Markup) -> (Markup, Vec<u8>) {
    let (first, second, bytes) = round_trip(vec![m.clone()]);
    let a = first.markups.first().expect("markup reloaded");
    let b = second
        .markups
        .iter()
        .find(|x| x.id == a.id)
        .expect("markup reloaded twice");
    assert_same(&m, a);
    assert_same(a, b);
    (b.clone(), bytes)
}

#[test]
fn rectangle_round_trip() {
    let mut m = markup(Kind::Rectangle, bx(100.0, 100.0, 240.0, 190.0));
    m.fill = Some(Color::rgb(1.0, 1.0, 0.6));
    m.fill_opacity = 0.5;
    let (back, bytes) = check_one(m);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"Subtype"), Some(&b"Square"[..]));
    assert!(a.contains(b"RD"));
    // /Rect is the box grown by half the line width
    let r = back.rect;
    assert!(near(r.x0, 99.0) && near(r.y1, 191.0));

    let mut c = markup(Kind::Rectangle, bx(300.0, 300.0, 400.0, 360.0));
    c.cloud = 2.0;
    let (back, bytes) = check_one(c);
    let (cos, a) = annot_dict(&bytes, &back);
    assert!(a.contains(b"BE"));
    assert!(appearance(&cos, &a).contains(" c "));
}

#[test]
fn ellipse_round_trip() {
    let mut m = markup(Kind::Ellipse, bx(100.0, 100.0, 240.0, 190.0));
    m.color = Color::rgb(0.0, 0.4, 0.9);
    let (back, bytes) = check_one(m);
    let (cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"Subtype"), Some(&b"Circle"[..]));
    assert_eq!(appearance(&cos, &a).matches(" c ").count(), 4);
}

#[test]
fn polygon_and_cloud_round_trip() {
    let pts = vec![
        Point::new(360.0, 300.0),
        Point::new(500.0, 300.0),
        Point::new(470.0, 390.0),
        Point::new(400.0, 370.0),
    ];
    let mut p = markup(Kind::Polygon, pts.clone());
    p.fill = Some(Color::rgb(0.0, 1.0, 1.0));
    p.fill_opacity = 0.4;
    check_one(p);
    let mut c = markup(Kind::Cloud, pts);
    c.cloud = 2.0;
    let (back, bytes) = check_one(c);
    let (cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"IT"), Some(&b"PolygonCloud"[..]));
    let be = a.get(b"BE").and_then(Object::as_dict).unwrap();
    assert_eq!(be.name(b"S"), Some(&b"C"[..]));
    assert!(appearance(&cos, &a).matches(" c ").count() > 10);
    // the scallops are inside /Rect
    assert!(back.rect.x0 < 360.0 - 5.0);
}

#[test]
fn polyline_round_trip() {
    let mut m = markup(
        Kind::Polyline,
        vec![
            Point::new(200.0, 110.0),
            Point::new(260.0, 170.0),
            Point::new(320.0, 110.0),
        ],
    );
    m.dash = vec![6.0, 3.0];
    m.line_end = "OpenArrow".into();
    check_one(m);
}

#[test]
fn line_with_every_ending_round_trip() {
    let ms: Vec<Markup> = LINE_ENDINGS
        .iter()
        .enumerate()
        .map(|(i, le)| {
            let y = 100.0 + 30.0 * i as f64;
            let mut m = markup(Kind::Line, vec![Point::new(100.0, y), Point::new(250.0, y)]);
            m.line_start = (*le).into();
            m.line_end = LINE_ENDINGS[(i + 3) % LINE_ENDINGS.len()].into();
            m
        })
        .collect();
    let (first, second, bytes) = round_trip(ms.clone());
    assert_eq!(first.markups.len(), ms.len());
    for (m, a) in ms.iter().zip(&first.markups) {
        assert_same(m, a);
        let b = second.markups.iter().find(|x| x.id == a.id).unwrap();
        assert_same(a, b);
        let (cos, d) = annot_dict(&bytes, b);
        let ap = appearance(&cos, &d);
        if m.line_start == "ClosedArrow" {
            assert!(ap.contains("b Q"), "a closed arrow is filled");
        }
        // the endings are inside /Rect
        assert!(b.rect.x0 <= 100.0 - 1.0 && b.rect.x1 >= 251.0);
    }
}

#[test]
fn arrow_round_trip() {
    let mut m = markup(Kind::Arrow, vec![Point::new(100.0, 150.0), Point::new(250.0, 150.0)]);
    m.line_end = "ClosedArrow".into();
    m.line_width = 1.5;
    // Revu writes an arrow's /IC (its head fill) as the stroke colour
    m.fill = Some(m.color);
    let (back, bytes) = check_one(m);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"IT"), Some(&b"LineArrow"[..]));
    assert!(a.contains(b"IC"));
}

#[test]
fn text_box_round_trip() {
    let mut m = markup(Kind::Text, bx(40.0, 700.0, 220.0, 760.0));
    m.contents = "Text box: verify duct size\rand clearance above ceiling".into();
    m.fill = Some(Color::WHITE);
    m.color = Color::rgb(0.0, 0.0, 1.0); // border differs from the text colour
    m.text.color = Color::rgb(0.85, 0.0, 0.0);
    m.text.align = 1;
    m.text.underline = true;
    autosize_text_box(&mut m);
    let (back, bytes) = check_one(m);
    let (cos, a) = annot_dict(&bytes, &back);
    // FreeText's /C is the fill
    assert_eq!(a.get(b"C").and_then(Object::as_array).map(Vec::len), Some(3));
    assert!(!a.contains(b"IC"));
    let ap = appearance(&cos, &a);
    assert!(ap.contains("(Text box: verify duct size) Tj"), "{ap}");
    assert!(ap.contains("/Helv 12.000 Tf"));

    // no fill: /C []
    let mut n = markup(Kind::Text, bx(40.0, 600.0, 220.0, 640.0));
    n.contents = "plain".into();
    n.text.font = "Courier".into();
    n.text.italic = true;
    n.color = n.text.color;
    check_one(n);
}

#[test]
fn callout_round_trip() {
    let mut m = markup(Kind::Callout, bx(300.0, 700.0, 450.0, 740.0));
    m.contents = "Callout: relocate VAV-3".into();
    m.line_width = 1.0;
    m.fill = Some(Color::WHITE);
    m.text.bold = true;
    m.line_end = "OpenArrow".into();
    m.color = m.text.color;
    m.pts.push(Point::new(240.0, 660.0)); // tip
    m.pts.push(Point::new(285.0, 720.0)); // knee
    autosize_text_box(&mut m);
    let (back, bytes) = check_one(m);
    assert_eq!(back.pts.len(), 6);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.get(b"CL").and_then(Object::as_array).map(Vec::len), Some(6));
    assert_eq!(a.name(b"LE"), Some(&b"OpenArrow"[..]));
    assert!(a.contains(b"RD"));
    assert!(back.rect.x0 < 240.0);
}

#[test]
fn typewriter_round_trip() {
    let mut m = markup(Kind::Typewriter, bx(470.0, 740.0, 570.0, 760.0));
    m.contents = "Typewriter text, Times 14 pt".into();
    m.line_width = 0.0;
    m.text.font = "Times".into();
    m.text.size = 14.0;
    m.text.color = Color::rgb(0.0, 0.0, 0.8);
    m.color = m.text.color;
    autosize_text_box(&mut m);
    let (back, bytes) = check_one(m);
    let (cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"IT"), Some(&b"FreeTextTypewriter"[..]));
    assert!(appearance(&cos, &a).contains("/TiRo 14.000 Tf"));
}

#[test]
fn ink_and_highlight_round_trip() {
    let mut m = markup(Kind::Ink, Vec::new());
    for i in 0..=40 {
        m.pts
            .push(Point::new(40.0 + i as f64 * 4.0, 300.0 + 15.0 * (i as f64 / 4.0).sin()));
    }
    m.strokes = vec![m.pts.len()];
    for i in 0..=10 {
        m.pts.push(Point::new(40.0 + i as f64 * 16.0, 270.0));
    }
    m.strokes.push(m.pts.len()); // a lone point stroke
    m.pts.push(Point::new(10.0, 10.0));
    let (back, bytes) = check_one(m);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.get(b"InkList").and_then(Object::as_array).map(Vec::len), Some(3));

    let mut h = markup(
        Kind::Highlight,
        vec![Point::new(260.0, 290.0), Point::new(460.0, 290.0)],
    );
    h.color = Color::rgb(1.0, 0.9, 0.0);
    h.line_width = 12.0;
    h.multiply = true;
    let (back, bytes) = check_one(h);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"BM"), Some(&b"Multiply"[..]));
    assert_eq!(a.name(b"Subtype"), Some(&b"Ink"[..]));
}

#[test]
fn stamp_round_trip() {
    use markupcraft_revu::kinds::draw::find_stamp;
    let d = find_stamp("Approved").unwrap();
    let mut m = markup(Kind::Stamp, bx(400.0, 400.0, 570.0, 460.0));
    m.stamp = d.id.into();
    m.contents = format!("{}\rMarkupCraft  2026-10-07", d.text);
    m.color = d.color;
    m.text.color = d.color;
    m.text.bold = true;
    m.line_width = 2.5;
    let (back, bytes) = check_one(m);
    let (cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"Name"), Some(&b"Approved"[..]));
    let ap = appearance(&cos, &a);
    assert!(ap.contains("(APPROVED) Tj") && ap.contains("/HeBo"), "{ap}");
}

#[test]
fn text_markups_round_trip() {
    let quads = vec![
        Point::new(100.0, 512.0),
        Point::new(166.0, 512.0),
        Point::new(100.0, 500.0),
        Point::new(166.0, 500.0),
        Point::new(100.0, 498.0),
        Point::new(136.0, 498.0),
        Point::new(100.0, 486.0),
        Point::new(136.0, 486.0),
    ];
    for (k, sub) in [
        (Kind::TextHighlight, "Highlight"),
        (Kind::Underline, "Underline"),
        (Kind::Strikeout, "StrikeOut"),
        (Kind::Squiggly, "Squiggly"),
    ] {
        let mut m = markup(k, quads.clone());
        m.color = markupcraft_revu::kinds::textmarkup::default_color(k);
        m.multiply = k == Kind::TextHighlight;
        let (back, bytes) = check_one(m);
        let (cos, a) = annot_dict(&bytes, &back);
        assert_eq!(a.name(b"Subtype"), Some(sub.as_bytes()));
        assert_eq!(a.get(b"QuadPoints").and_then(Object::as_array).map(Vec::len), Some(16));
        assert!(!appearance(&cos, &a).is_empty());
        assert!(back.rect.x0 < 100.0 && back.rect.y1 > 512.0);
    }
}

#[test]
fn caret_round_trip() {
    let m = markup(Kind::Caret, bx(150.0, 495.0, 160.0, 508.0));
    let (back, bytes) = check_one(m);
    let (_cos, a) = annot_dict(&bytes, &back);
    assert_eq!(a.name(b"Sy"), Some(&b"None"[..]));
    assert!(a.contains(b"RD"));
}

#[test]
fn note_round_trip_with_popup() {
    let mut m = markup(Kind::Note, bx(500.0, 600.0, 524.0, 624.0));
    m.icon = "Key".into();
    m.contents = "Check this".into();
    m.popup_open = true;
    m.color = markupcraft_revu::kinds::textmarkup::default_color(Kind::Note);
    let (back, bytes) = check_one(m);
    let popup = back.popup.expect("popup rect read back");
    assert!(near(popup.x0, 530.0) && near(popup.y1, 624.0));
    let (cos, a) = annot_dict(&bytes, &back);
    let pr = a.reference(b"Popup").unwrap();
    let pd = cos.get(pr);
    let pd = pd.as_dict().unwrap();
    assert_eq!(pd.reference(b"Parent"), Some(ObjRef::new(back.obj.0, back.obj.1)));
    // exactly one popup in the page's /Annots after two saves
    let page = cos.get(a.reference(b"P").unwrap());
    let annots = cos.resolve(page.as_dict().unwrap().get(b"Annots").unwrap());
    let popups = annots
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| cos.resolve(o).as_dict().and_then(|d| d.name(b"Subtype")) == Some(b"Popup"))
        .count();
    assert_eq!(popups, 1);
}

#[test]
fn snapshot_capture_and_copy() {
    let (mut f, mut doc) = open(blank_pdf(90, Vec::new()));
    let mut s = markup(Kind::Snapshot, bx(300.0, 500.0, 400.0, 550.0));
    s.snapshot = Some(SnapshotSource {
        annot: None,
        page: Some(0),
        region: Rect::new(100.0, 100.0, 300.0, 200.0),
    });
    doc.markups.push(s);
    let bytes = save(&mut f, &mut doc);
    let (mut g, mut back) = open(bytes);
    let snap = back.markups.first().cloned().unwrap();
    assert_eq!(snap.kind, Kind::Snapshot);
    {
        let a = g.cos.get(ObjRef::new(snap.obj.0, snap.obj.1));
        let a = a.as_dict().unwrap();
        assert_eq!(a.name(b"IT"), Some(&b"StampSnapshot"[..]));
        let ap = appearance(&g.cos, a);
        assert!(ap.contains("200 100 re f"), "the page content is captured: {ap}");
        assert!(near(snap.rect.x0, 300.0) && near(snap.rect.y1, 550.0));
    }
    // A copy of it elsewhere draws the original's appearance.
    let mut copy = snap.clone();
    copy.obj = (0, 0);
    copy.id = String::new();
    copy.annot_index = None;
    copy.dirty = true;
    copy.pts = bx(50.0, 50.0, 100.0, 75.0);
    back.markups.push(copy);
    let bytes = save(&mut g, &mut back);
    let (h, again) = open(bytes);
    assert_eq!(again.markups.len(), 2);
    let c = again.markups.get(1).unwrap();
    assert_eq!(c.kind, Kind::Snapshot);
    assert!(near(c.rect.x0, 50.0) && near(c.rect.y1, 75.0));
    let a = h.cos.get(ObjRef::new(c.obj.0, c.obj.1));
    assert!(appearance(&h.cos, a.as_dict().unwrap()).contains("/S0 Do"));
}

/// Annotations MarkupCraft does not draw (other apps' stamps, unknown subtypes) keep their
/// `/AP` and `/Rect` when re-saved.
#[test]
fn foreign_annotations_are_preserved() {
    let stamp = obj(&[
        ("Type", Object::name("Annot")),
        ("Subtype", Object::name("Stamp")),
        (
            "NM",
            Object::String(markupcraft_revu::cos::PdfString::text("FOREIGNSTAMP")),
        ),
        ("Rect", nums(&[10.0, 10.0, 60.0, 40.0])),
        ("Name", Object::name("Approved")),
    ]);
    let sound = obj(&[
        ("Type", Object::name("Annot")),
        ("Subtype", Object::name("Sound")),
        (
            "NM",
            Object::String(markupcraft_revu::cos::PdfString::text("SOUNDANNOT")),
        ),
        ("Rect", nums(&[70.0, 10.0, 90.0, 30.0])),
    ]);
    let (mut f, mut doc) = open(blank_pdf(0, vec![stamp, sound]));
    assert_eq!(doc.markups.len(), 2);
    let before = doc.markups.clone();
    for m in &mut doc.markups {
        m.dirty = true;
    }
    let bytes = save(&mut f, &mut doc);
    let (g, back) = open(bytes);
    for (a, b) in before.iter().zip(&back.markups) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.rect, b.rect);
        assert_eq!(a.subtype, b.subtype);
        let d = g.cos.get(ObjRef::new(b.obj.0, b.obj.1));
        assert!(!d.as_dict().unwrap().contains(b"AP"), "no appearance was made up");
    }
    assert_eq!(back.markups.first().map(|m| m.kind), Some(Kind::Stamp));
    assert_eq!(back.markups.get(1).map(|m| m.kind), Some(Kind::Other));
}
