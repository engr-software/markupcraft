//! `markupdemo <in> <out>`: one of every markup tool's kind on page 1, to check in Revu or any
//! other viewer.

use markupcraft_model::{Color, Kind, Markup, Point, Rect, SnapshotSource};
use markupcraft_revu::kinds::draw::find_stamp;
use markupcraft_revu::kinds::text::autosize_text_box;
use markupcraft_revu::kinds::textmarkup::default_color;
use markupcraft_revu::{SaveMode, open, save};

type Res = Result<bool, Box<dyn std::error::Error>>;

/// A box `w` x `h` hanging down from its top-left corner (`x`, `y`).
fn bx(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    Rect::new(x, y - h, x + w, y).corners().to_vec()
}

/// One quad (a run of text `w` x `h`, top-left at (`x`, `y`)) in `/QuadPoints` order.
fn quad(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x, y - h),
        Point::new(x + w, y - h),
    ]
}

pub fn markupdemo(input: &str, out: &str) -> Res {
    let (mut f, mut doc) = open(input)?;
    let Some(pg) = doc.pages.first() else {
        return Ok(false);
    };
    let (x0, y0) = (pg.crop.x0 + 40.0, pg.crop.y1 - 40.0);
    let base = |k: Kind, subj: &str, pts: Vec<Point>| {
        let mut m = Markup::new(k, 0, pts);
        m.subject = subj.into();
        m.author = "MarkupCraft".into();
        m.color = Color::rgb(0.85, 0.0, 0.0);
        m.text.color = m.color;
        m.line_width = 2.0;
        m
    };
    let p = Point::new;
    let mut ms: Vec<Markup> = Vec::new();

    let mut m = base(Kind::Text, "Text Box", bx(x0, y0, 180.0, 60.0));
    m.contents = "Text box: verify duct size\rand clearance above ceiling".into();
    m.fill = Some(Color::WHITE);
    autosize_text_box(&mut m);
    ms.push(m);

    let mut m = base(Kind::Callout, "Callout", bx(x0 + 260.0, y0, 150.0, 40.0));
    m.contents = "Callout: relocate VAV-3".into();
    m.line_width = 1.0;
    m.fill = Some(Color::WHITE);
    m.text.bold = true;
    m.line_end = "OpenArrow".into();
    m.pts.push(p(x0 + 200.0, y0 - 80.0)); // tip
    m.pts.push(p(x0 + 245.0, y0 - 20.0)); // knee
    autosize_text_box(&mut m);
    ms.push(m);

    let mut m = base(Kind::Typewriter, "Typewriter", bx(x0 + 470.0, y0, 100.0, 20.0));
    m.contents = "Typewriter text, Times 14 pt".into();
    m.line_width = 0.0;
    m.text.font = "Times".into();
    m.text.size = 14.0;
    m.text.color = Color::rgb(0.0, 0.0, 0.8);
    m.color = m.text.color;
    autosize_text_box(&mut m);
    ms.push(m);

    ms.push(base(
        Kind::Line,
        "Line",
        vec![p(x0, y0 - 110.0), p(x0 + 150.0, y0 - 110.0)],
    ));

    let mut m = base(Kind::Arrow, "Arrow", vec![p(x0, y0 - 150.0), p(x0 + 150.0, y0 - 150.0)]);
    m.line_end = "ClosedArrow".into();
    m.line_width = 1.5;
    ms.push(m);

    let mut m = base(
        Kind::Polyline,
        "PolyLine",
        vec![
            p(x0 + 200.0, y0 - 110.0),
            p(x0 + 260.0, y0 - 170.0),
            p(x0 + 320.0, y0 - 110.0),
            p(x0 + 380.0, y0 - 170.0),
        ],
    );
    m.dash = vec![6.0, 3.0];
    ms.push(m);

    let mut m = base(Kind::Rectangle, "Rectangle", bx(x0, y0 - 190.0, 140.0, 90.0));
    m.fill = Some(Color::rgb(1.0, 1.0, 0.6));
    m.fill_opacity = 0.5;
    ms.push(m);

    let mut m = base(Kind::Ellipse, "Ellipse", bx(x0 + 180.0, y0 - 190.0, 140.0, 90.0));
    m.color = Color::rgb(0.0, 0.4, 0.9);
    ms.push(m);

    let mut m = base(
        Kind::Polygon,
        "Polygon",
        vec![
            p(x0 + 360.0, y0 - 280.0),
            p(x0 + 500.0, y0 - 280.0),
            p(x0 + 470.0, y0 - 190.0),
            p(x0 + 400.0, y0 - 210.0),
        ],
    );
    m.fill = Some(Color::rgb(0.0, 1.0, 1.0));
    m.fill_opacity = 0.4;
    ms.push(m);

    let mut m = base(Kind::Cloud, "Cloud", bx(x0, y0 - 310.0, 200.0, 90.0));
    m.cloud = 2.0;
    m.color = Color::rgb(0.0, 0.0, 1.0);
    ms.push(m);

    // Cloud+: a cloud and a callout pointing at it.
    let mut c = base(Kind::Cloud, "Cloud+", bx(x0 + 260.0, y0 - 310.0, 160.0, 80.0));
    c.cloud = 2.0;
    ms.push(c);
    let mut m = base(Kind::Callout, "Cloud+", bx(x0 + 470.0, y0 - 290.0, 140.0, 30.0));
    m.contents = "Cloud+: REV 2".into();
    m.line_width = 1.0;
    m.fill = Some(Color::WHITE);
    m.pts.push(p(x0 + 422.0, y0 - 350.0));
    m.pts.push(p(x0 + 450.0, y0 - 305.0));
    autosize_text_box(&mut m);
    ms.push(m);

    let mut m = base(Kind::Ink, "Pen", Vec::new());
    for i in 0..=40 {
        let t = f64::from(i);
        m.pts.push(p(x0 + t * 4.0, y0 - 460.0 + 15.0 * (t / 4.0).sin()));
    }
    m.strokes = vec![m.pts.len()];
    for i in 0..=10 {
        m.pts.push(p(x0 + f64::from(i) * 16.0, y0 - 490.0));
    }
    ms.push(m);

    let mut m = base(
        Kind::Highlight,
        "Highlight",
        vec![p(x0 + 220.0, y0 - 470.0), p(x0 + 420.0, y0 - 470.0)],
    );
    m.color = Color::rgb(1.0, 0.9, 0.0);
    m.line_width = 12.0;
    m.multiply = true;
    ms.push(m);

    if let Some(d) = find_stamp("Approved") {
        let mut m = base(Kind::Stamp, "Stamp", bx(x0 + 470.0, y0 - 430.0, 170.0, 60.0));
        m.stamp = d.id.into();
        m.contents = format!("{}\rMarkupCraft  2026-10-09", d.text);
        m.color = d.color;
        m.text.color = d.color;
        m.text.bold = true;
        m.line_width = 2.5;
        ms.push(m);
    }

    // Text markups over (pretend) runs of text, a caret and a note.
    let y = y0 - 530.0;
    for (i, (k, subj)) in [
        (Kind::TextHighlight, "Text Highlight"),
        (Kind::Underline, "Underline"),
        (Kind::Strikeout, "Strikethrough"),
        (Kind::Squiggly, "Squiggly"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut m = base(k, subj, quad(x0 + 130.0 * i as f64, y, 110.0, 12.0));
        m.color = default_color(k);
        m.line_width = 1.0;
        m.multiply = k == Kind::TextHighlight;
        ms.push(m);
    }
    let mut m = base(Kind::Caret, "Inserted Text", bx(x0 + 530.0, y, 10.0, 13.0));
    m.contents = "insert: per spec".into();
    m.line_width = 1.0;
    ms.push(m);
    let mut m = base(Kind::Note, "Note", bx(x0 + 560.0, y + 6.0, 24.0, 24.0));
    m.icon = "Comment".into();
    m.contents = "Note: confirm with the engineer".into();
    m.color = default_color(Kind::Note);
    m.line_width = 1.0;
    ms.push(m);

    // Snapshot: the top-left corner of the page, half size, at the bottom.
    let crop = pg.crop;
    let region = Rect::new(crop.x0, crop.y1 - 150.0, crop.x0 + 200.0, crop.y1);
    let mut m = base(Kind::Snapshot, "Snapshot", bx(x0, y0 - 560.0, 100.0, 75.0));
    m.snapshot = Some(SnapshotSource {
        annot: None,
        page: Some(0),
        region,
    });
    m.line_width = 0.0;
    ms.push(m);

    doc.markups.extend(ms);
    save(&mut f, &mut doc, out, SaveMode::Full)?;
    let (_g, back) = open(out)?;
    let mut n = 0;
    for m in back.markups.iter().filter(|m| m.author == "MarkupCraft") {
        n += 1;
        let text: String = m.contents.chars().take(40).collect();
        println!("{:<15} {:<15} {}", m.kind.name(), m.subject, text.replace('\r', " / "));
    }
    println!("{n} markups added -> {out}");
    Ok(true)
}
