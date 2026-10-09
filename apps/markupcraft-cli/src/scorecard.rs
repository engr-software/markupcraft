//! The compatibility scorecard: Revu-made files read, computed and rewritten the way Revu does.

use std::collections::BTreeMap;

use markupcraft_measure::parse_label;
use markupcraft_model::{Color, Kind, Markup, Point, Scale};
use markupcraft_revu::{SaveMode, open, save};

type Res = Result<bool, Box<dyn std::error::Error>>;

/// Compare our geometry-derived quantity with the label Revu stored in `/Contents`.
pub fn check(path: &str) -> Res {
    let (_f, doc) = open(path)?;
    let (mut total, mut text_same, mut value_same, mut no_scale, mut shown) = (0, 0, 0, 0, 0);
    for m in doc
        .markups
        .iter()
        .filter(|m| m.kind.is_measurement() && m.kind != Kind::Count)
    {
        total += 1;
        let Some(q) = m.quantity() else {
            no_scale += 1;
            continue;
        };
        let ours = m.quantity_text();
        let same = ours == m.contents;
        let theirs = parse_label(&m.contents);
        // Agreement within the label's own display precision.
        let tol = if m.kind == Kind::Area {
            0.0051
        } else {
            1.0 / 96.0 + 1e-6
        };
        let close = theirs.is_some_and(|t| (t - q).abs() <= tol);
        text_same += usize::from(same);
        value_same += usize::from(close);
        if (!same || !close) && shown < 15 {
            shown += 1;
            println!(
                "  p{:<3} {:<10} ours {:<16} revu {:<16} {}",
                m.page + 1,
                m.kind.name(),
                ours,
                m.contents,
                if close { "(value agrees)" } else { "VALUE DIFFERS" }
            );
        }
    }
    let scaled = total - no_scale;
    println!(
        "\n{path}\nmeasurements {total}  no scale {no_scale}\nvalue agrees {value_same}/{scaled}  label text identical {text_same}/{scaled}"
    );
    Ok(value_same == scaled)
}

pub fn resave(input: &str, out: &str) -> Res {
    let (mut f, mut doc) = open(input)?;
    let mut before: BTreeMap<String, f64> = BTreeMap::new();
    for m in &mut doc.markups {
        if m.kind.is_measurement()
            && let Some(q) = m.quantity()
        {
            m.dirty = true;
            before.insert(m.id.clone(), q);
        }
    }
    save(&mut f, &mut doc, out, SaveMode::Full)?;
    let (_g, back) = open(out)?;
    let (mut n, mut same) = (0, 0);
    for m in &back.markups {
        let Some(q0) = before.get(&m.id) else { continue };
        n += 1;
        if m.quantity().is_some_and(|q| (q - q0).abs() < 1e-3) {
            same += 1;
        }
    }
    println!(
        "resaved {} measurements -> {out}\nreloaded and matched {n}/{} (same quantity {same}/{n})",
        before.len(),
        before.len()
    );
    Ok(same == before.len())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 2e-3
}

fn same_color(a: &Color, b: &Color) -> bool {
    near(a.r, b.r) && near(a.g, b.g) && near(a.b, b.b)
}

/// Differences between a markup before saving and after reloading; empty = identical.
fn diff(a: &Markup, b: &Markup) -> String {
    let mut d: Vec<String> = Vec::new();
    if a.kind != b.kind {
        d.push(format!("kind {}->{}", a.kind.name(), b.kind.name()));
    }
    if a.pts.len() != b.pts.len() {
        d.push(format!("point count {}->{}", a.pts.len(), b.pts.len()));
    } else if let Some((i, (p, q))) = a
        .pts
        .iter()
        .zip(&b.pts)
        .enumerate()
        .find(|(_, (p, q))| !near(p.x, q.x) || !near(p.y, q.y))
    {
        d.push(format!("pt{i} ({:.3},{:.3})->({:.3},{:.3})", p.x, p.y, q.x, q.y));
    }
    if a.strokes != b.strokes {
        d.push("ink strokes".into());
    }
    if a.contents != b.contents {
        d.push(format!("text [{}]->[{}]", a.contents, b.contents));
    }
    if a.subject != b.subject {
        d.push("subject".into());
    }
    if !same_color(&a.color, &b.color) {
        d.push("color".into());
    }
    match (&a.fill, &b.fill) {
        (Some(x), Some(y)) if !same_color(x, y) => d.push("fill".into()),
        (Some(_), None) | (None, Some(_)) => d.push("fill".into()),
        _ => {}
    }
    if !near(a.opacity, b.opacity) {
        d.push("opacity".into());
    }
    if !near(a.line_width, b.line_width) {
        d.push("line width".into());
    }
    if a.dash.len() != b.dash.len() {
        d.push("dash".into());
    }
    if a.line_start != b.line_start || a.line_end != b.line_end {
        d.push(format!(
            "line ends {}/{}->{}/{}",
            a.line_start, a.line_end, b.line_start, b.line_end
        ));
    }
    if !near(a.cloud, b.cloud) {
        d.push("cloud".into());
    }
    if a.stamp != b.stamp {
        d.push("stamp".into());
    }
    if a.multiply != b.multiply {
        d.push("blend".into());
    }
    if a.kind.is_text() || !a.stamp.is_empty() {
        let (s, t) = (&a.text, &b.text);
        if s.font != t.font
            || !near(s.size, t.size)
            || s.bold != t.bold
            || s.italic != t.italic
            || s.underline != t.underline
            || s.align != t.align
            || !same_color(&s.color, &t.color)
        {
            d.push("font".into());
        }
    }
    d.join(", ")
}

pub fn markupcheck(input: &str, out: &str) -> Res {
    let (mut f, mut doc) = open(input)?;
    let mut before: BTreeMap<String, Markup> = BTreeMap::new();
    let mut per_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &mut doc.markups {
        if m.kind.is_measurement() || m.id.is_empty() {
            continue;
        }
        m.dirty = true;
        before.insert(m.id.clone(), m.clone());
        *per_kind.entry(m.kind.name()).or_default() += 1;
    }
    save(&mut f, &mut doc, out, SaveMode::Full)?;
    let (_g, back) = open(out)?;
    let (mut n, mut same, mut shown) = (0, 0, 0);
    let mut same_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &back.markups {
        let Some(b) = before.get(&m.id) else { continue };
        n += 1;
        let d = diff(b, m);
        if d.is_empty() {
            same += 1;
            *same_kind.entry(m.kind.name()).or_default() += 1;
        } else if shown < 20 {
            shown += 1;
            println!("  p{:<3} {:<10} {}: {d}", m.page + 1, b.kind.name(), m.id);
        }
    }
    for (k, c) in &per_kind {
        println!(
            "  {k:<14} {:>3}/{c:<3} identical",
            same_kind.get(k).copied().unwrap_or(0)
        );
    }
    println!(
        "resaved {} non-measurement markups -> {out}\nreloaded {n}/{}, identical {same}/{}",
        before.len(),
        before.len(),
        before.len()
    );
    Ok(same == before.len())
}

/// Add one of each measurement to page 1 at 1/8" = 1'-0" and save, for checking in Revu.
pub fn demo(input: &str, out: &str) -> Res {
    let (mut f, mut doc) = open(input)?;
    let Some(pg) = doc.pages.first_mut() else {
        return Ok(false);
    };
    let sc = Scale::architectural(0.125, 1.0);
    pg.scale = Some(sc.clone());
    pg.scale_changed = true;
    let (x0, y0) = (pg.crop.x0 + 200.0, pg.crop.y0 + 200.0);
    let mut add = |k: Kind, subj: &str, pts: Vec<Point>, c: Color| {
        let mut m = Markup::new(k, 0, pts);
        m.subject = subj.into();
        m.author = "MarkupCraft".into();
        m.label = format!("MarkupCraft {}", k.name());
        m.color = c;
        m.line_width = 2.0;
        if k != Kind::Count {
            m.scale = Some(sc.clone());
        }
        if k == Kind::Area {
            m.fill = Some(Color::rgb(1.0, 0.3, 0.3));
            m.fill_opacity = 0.3;
        }
        doc.markups.push(m);
    };
    let p = Point::new;
    add(
        Kind::Area,
        "Area Measurement",
        vec![
            p(x0, y0),
            p(x0 + 360.0, y0),
            p(x0 + 360.0, y0 + 180.0),
            p(x0, y0 + 180.0),
        ],
        Color::rgb(0.85, 0.0, 0.0),
    );
    add(
        Kind::Length,
        "Length Measurement",
        vec![p(x0, y0 - 60.0), p(x0 + 270.0, y0 - 60.0)],
        Color::rgb(0.0, 0.5, 0.0),
    );
    add(
        Kind::Polylength,
        "Polylength Measurement",
        vec![p(x0 + 450.0, y0), p(x0 + 450.0, y0 + 180.0), p(x0 + 630.0, y0 + 180.0)],
        Color::rgb(0.0, 0.0, 0.85),
    );
    add(
        Kind::Perimeter,
        "Perimeter Measurement",
        vec![
            p(x0 + 700.0, y0),
            p(x0 + 790.0, y0),
            p(x0 + 790.0, y0 + 90.0),
            p(x0 + 700.0, y0 + 90.0),
        ],
        Color::rgb(0.6, 0.0, 0.6),
    );
    add(
        Kind::Count,
        "Count Measurement",
        vec![
            p(x0 + 100.0, y0 + 260.0),
            p(x0 + 150.0, y0 + 260.0),
            p(x0 + 200.0, y0 + 260.0),
        ],
        Color::rgb(0.0, 0.4, 0.9),
    );
    save(&mut f, &mut doc, out, SaveMode::Full)?;
    let (_g, back) = open(out)?;
    for m in back.markups.iter().filter(|m| m.author == "MarkupCraft") {
        println!("{:<10} {:<24} {}", m.kind.name(), m.subject, m.contents);
    }
    Ok(true)
}
