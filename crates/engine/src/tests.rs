use std::path::PathBuf;

use markupcraft_measure::units::LengthUnit;
use markupcraft_revu::cos::{Dict, Object, PdfString, SaveOptions, write_full};

use super::*;
use crate::labels::LabelSpec;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("markupcraft-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn letter(n: usize) -> Vec<(f64, f64)> {
    vec![(612.0, 792.0); n]
}

fn session(n: usize, name: &str) -> Session {
    Session::new_blank(tmp(name), &letter(n)).unwrap()
}

fn area(page: usize, x: f64) -> Markup {
    Markup::new(
        Kind::Area,
        page,
        Rect::new(x, 100.0, x + 90.0, 190.0).corners().to_vec(),
    )
}

fn line(page: usize) -> Markup {
    Markup::new(Kind::Line, page, vec![Point::new(10.0, 10.0), Point::new(110.0, 10.0)])
}

#[test]
fn add_undo_redo_and_dirty() {
    let mut s = session(1, "a.pdf");
    s.save(false).unwrap();
    assert!(!s.is_dirty());
    let id = s.add_markup(area(0, 100.0)).unwrap();
    assert!(s.is_dirty());
    assert_eq!(s.selection(), std::slice::from_ref(&id));
    assert_eq!(s.markup(&id).unwrap().subject, "Area Measurement");
    assert_eq!(s.undo().unwrap(), "Add Area");
    assert!(s.doc().markups.is_empty());
    assert!(!s.is_dirty(), "undo back to the saved state is clean");
    assert!(s.selection().is_empty());
    s.redo().unwrap();
    assert_eq!(s.doc().markups.len(), 1);
    assert!(s.undo().is_ok() && s.undo().is_err());
}

#[test]
fn refused_kinds_and_points() {
    let mut s = session(1, "refuse.pdf");
    let e = s
        .add_markup(Markup::new(Kind::Other, 0, vec![Point::new(1.0, 1.0)]))
        .unwrap_err();
    assert!(e.to_string().contains("creatable kinds"), "{e}");
    let e = s
        .add_markup(Markup::new(Kind::Line, 0, vec![Point::new(1.0, 1.0)]))
        .unwrap_err();
    assert!(e.to_string().contains("exactly 2"), "{e}");
    let e = s
        .add_markup(Markup::new(Kind::Line, 3, vec![Point::default(); 2]))
        .unwrap_err();
    assert!(matches!(e, EngineError::NoPage { page: 4, count: 1 }));
    let bad = Markup::new(Kind::Line, 0, vec![Point::new(f64::NAN, 0.0), Point::default()]);
    assert!(s.add_markup(bad).is_err());
    assert!(!s.can_undo());
}

#[test]
fn save_reopen_round_trip() {
    let path = tmp("round.pdf");
    let mut s = Session::new_blank(&path, &letter(2)).unwrap();
    let mut m = area(1, 50.0);
    m.scale = Some(Scale::architectural(0.125, 1.0));
    m.label = "Room 1".into();
    let id = s.add_markup(m).unwrap();
    s.save(false).unwrap();
    let back = Session::open(&path).unwrap();
    let m = back.markup(&id).unwrap();
    assert_eq!(
        (m.page, m.label.as_str(), m.quantity_text().as_str()),
        (1, "Room 1", "100 sf")
    );
}

#[test]
fn failed_edit_changes_nothing() {
    let mut s = session(1, "atomic.pdf");
    let a = s.add_markup(line(0)).unwrap();
    let b = s.add_markup(line(0)).unwrap();
    s.set_locked(std::slice::from_ref(&b), true).unwrap();
    let depth = s.undo_depth();
    let e = s.move_markups(&[a.clone(), b.clone()], 5.0, 0.0).unwrap_err();
    assert!(matches!(e, EngineError::Locked(_)));
    assert_eq!(
        s.markup(&a).unwrap().pts[0],
        Point::new(10.0, 10.0),
        "a did not move either"
    );
    assert_eq!(s.undo_depth(), depth);
    // An unlock with other changes in one patch works.
    let patch = MarkupPatch {
        locked: Some(false),
        color: props::parse_color("#00FF00"),
        ..Default::default()
    };
    s.set_properties(std::slice::from_ref(&b), &patch).unwrap();
    assert!(!s.markup(&b).unwrap().locked());
    assert_eq!(s.markup(&b).unwrap().color.hex(), "#00FF00");
}

#[test]
fn properties_validate() {
    let mut s = session(1, "props.pdf");
    let a = s.add_markup(line(0)).unwrap();
    let bad = MarkupPatch {
        opacity: Some(2.0),
        ..Default::default()
    };
    assert!(s.set_properties(std::slice::from_ref(&a), &bad).is_err());
    let p = MarkupPatch {
        fill: Some(Some(Color::rgb(0.0, 0.0, 1.0))),
        line_width: Some(3.0),
        dash: Some(vec![4.0, 2.0]),
        subject: Some("Wall".into()),
        status: Some("Accepted".into()),
        layer: Some("Demo".into()),
        ..Default::default()
    };
    s.set_properties(std::slice::from_ref(&a), &p).unwrap();
    let m = s.markup(&a).unwrap();
    assert_eq!(
        (m.line_width, m.dash.len(), m.subject.as_str(), m.status.as_str()),
        (3.0, 2, "Wall", "Accepted")
    );
}

#[test]
fn geometry_edits() {
    let mut s = session(1, "geom.pdf");
    let a = s.add_markup(area(0, 100.0)).unwrap();
    let ids = [a.clone()];
    s.move_markups(&ids, 10.0, 5.0).unwrap();
    assert_eq!(s.markup(&a).unwrap().pts[0], Point::new(110.0, 105.0));
    s.rotate_markups(&ids, 90.0, None).unwrap();
    s.resize_markup(&a, Rect::new(0.0, 0.0, 180.0, 90.0)).unwrap();
    let b = markupcraft_geom::bbox(&s.markup(&a).unwrap().pts).unwrap();
    assert!((b.width() - 180.0).abs() < 1e-9 && (b.height() - 90.0).abs() < 1e-9);
    s.insert_vertex(&a, 1, Point::new(90.0, -10.0)).unwrap();
    assert_eq!(s.markup(&a).unwrap().pts.len(), 5);
    s.move_vertex(&a, 1, Point::new(90.0, -20.0)).unwrap();
    s.delete_vertex(&a, 1).unwrap();
    assert_eq!(s.markup(&a).unwrap().pts.len(), 4);
    let l = s.add_markup(line(0)).unwrap();
    assert!(s.delete_vertex(&l, 0).is_err(), "a line keeps 2 points");
    assert!(s.set_points(&l, vec![Point::default()]).is_err());
}

#[test]
fn arrange_and_save_order() {
    let path = tmp("order.pdf");
    let mut s = Session::new_blank(&path, &letter(1)).unwrap();
    let a = s.add_markup(line(0)).unwrap();
    let b = s.add_markup(line(0)).unwrap();
    let c = s.add_markup(line(0)).unwrap();
    s.save(false).unwrap();
    assert!(s.arrange(std::slice::from_ref(&a), Arrange::BringToFront).unwrap());
    assert!(!s.arrange(std::slice::from_ref(&a), Arrange::BringToFront).unwrap());
    s.arrange(std::slice::from_ref(&c), Arrange::SendBackward).unwrap();
    let order: Vec<&str> = s.doc().markups.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(order, [c.as_str(), b.as_str(), a.as_str()]);
    s.save(false).unwrap();
    let back = Session::open(&path).unwrap();
    let order: Vec<&str> = back.doc().markups.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(order, [c.as_str(), b.as_str(), a.as_str()]);
}

#[test]
fn copy_paste_across_pages_and_groups() {
    let mut s = session(2, "clip.pdf");
    let a = s.add_markup(area(0, 100.0)).unwrap();
    let b = s.add_markup(line(0)).unwrap();
    let g = s.group(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(g, a);
    assert_eq!(s.markup(&b).unwrap().group, a);
    s.copy_markups(&[a.clone(), b.clone()]).unwrap();
    let new = s.paste(Some(1), None).unwrap();
    assert_eq!(new.len(), 2);
    let c = s.markup(&new[0]).unwrap();
    assert_eq!((c.page, c.pts[0]), (1, Point::new(100.0, 100.0)));
    assert!(c.group.is_empty(), "copies leave the group");
    let at = s.paste(Some(1), Some(Point::new(300.0, 300.0))).unwrap();
    let all: Vec<Point> = at.iter().flat_map(|id| s.markup(id).unwrap().pts.clone()).collect();
    let bb = markupcraft_geom::bbox(&all).unwrap();
    assert!(((bb.x0 + bb.x1) / 2.0 - 300.0).abs() < 1e-9);
    assert_eq!(s.ungroup(std::slice::from_ref(&b)).unwrap(), 2);
    let dup = s.duplicate_markups(std::slice::from_ref(&a), 5.0, 5.0).unwrap();
    assert_eq!(s.markup(&dup[0]).unwrap().pts[0], Point::new(105.0, 105.0));
    assert_eq!(s.cut_markups(std::slice::from_ref(&a)).unwrap(), 1);
    assert!(s.markup(&a).is_err());
}

#[test]
fn calibrate_measure_and_viewports_survive_save() {
    let path = tmp("scale.pdf");
    let mut s = Session::new_blank(&path, &letter(1)).unwrap();
    // 72 points = 10 feet.
    let sc = s
        .calibrate(
            &[0],
            Point::new(0.0, 0.0),
            Point::new(72.0, 0.0),
            10.0,
            LengthUnit::Foot,
            false,
        )
        .unwrap();
    assert!(sc.valid());
    let m = s
        .measure(0, Kind::Length, &[Point::new(0.0, 0.0), Point::new(144.0, 0.0)], None)
        .unwrap();
    assert!((m.value - 20.0).abs() < 1e-9, "{m:?}");
    assert_eq!(m.text, "20'-0\"");
    let vp = s
        .add_viewport(
            0,
            Rect::new(300.0, 300.0, 500.0, 500.0),
            "Detail",
            &Scale::architectural(1.0, 1.0),
        )
        .unwrap();
    let inside = s
        .measure(
            0,
            Kind::Length,
            &[Point::new(310.0, 310.0), Point::new(382.0, 310.0)],
            None,
        )
        .unwrap();
    assert!((inside.value - 1.0).abs() < 1e-9, "{inside:?}");
    let id = s
        .add_markup(Markup::new(
            Kind::Length,
            0,
            vec![Point::new(0.0, 0.0), Point::new(36.0, 0.0)],
        ))
        .unwrap();
    assert!(
        (s.markup(&id).unwrap().quantity().unwrap() - 5.0).abs() < 1e-9,
        "takes the page scale"
    );
    s.save(false).unwrap();
    let back = Session::open(&path).unwrap();
    let vps = &back.doc().pages[0].viewports;
    assert_eq!(vps.len(), 2);
    assert_eq!((vps[0].name.as_str(), vps[0].id.as_str()), ("Detail", vp.as_str()));
    let again = back
        .measure(0, Kind::Length, &[Point::new(0.0, 0.0), Point::new(144.0, 0.0)], None)
        .unwrap();
    assert!((again.value - 20.0).abs() < 1e-6);
    // Metric calibration and measuring without a scale.
    let metric = scales::calibrated_scale(100.0, 2.0, LengthUnit::Meter).unwrap();
    let mut fresh = session(1, "noscale.pdf");
    assert!(
        fresh
            .measure(0, Kind::Area, &Rect::new(0.0, 0.0, 1.0, 1.0).corners(), None)
            .is_err()
    );
    fresh.set_page_scale(&[0], &metric, false).unwrap();
    let a = fresh
        .measure(0, Kind::Area, &Rect::new(0.0, 0.0, 100.0, 50.0).corners(), None)
        .unwrap();
    assert!((a.value - 2.0).abs() < 1e-9 && a.unit == "sq m", "{a:?}");
    fresh.delete_viewport(0, 0).unwrap();
    assert!(fresh.doc().pages[0].scale.is_none());
}

/// A file whose pages carry labels "i", "ii", then "1", "2" ... and a markup per page.
fn labelled(path: &PathBuf, n: usize) -> Vec<String> {
    let mut s = Session::new_blank(path, &letter(n)).unwrap();
    let ids: Vec<String> = (0..n)
        .map(|p| s.add_markup(area(p, 100.0 + p as f64)).unwrap())
        .collect();
    s.save(true).unwrap();
    let bytes = std::fs::read(path).unwrap();
    let mut cos = markupcraft_revu::cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    let mut roman = Dict::new();
    roman.set(b"S".to_vec(), Object::name("r"));
    let mut dec = Dict::new();
    dec.set(b"S".to_vec(), Object::name("D"));
    dec.set(b"P".to_vec(), Object::String(PdfString::text("A-")));
    let mut tree = Dict::new();
    tree.set(
        b"Nums".to_vec(),
        Object::Array(vec![
            Object::Int(0),
            Object::Dict(roman),
            Object::Int(2),
            Object::Dict(dec),
        ]),
    );
    let root = cos.root().unwrap();
    cos.update_dict(root, |d| d.set(b"PageLabels".to_vec(), Object::Dict(tree)))
        .unwrap();
    std::fs::write(path, write_full(&cos, &SaveOptions::default()).unwrap()).unwrap();
    ids
}

#[test]
fn page_ops_keep_markups_and_labels() {
    let path = tmp("pages.pdf");
    let ids = labelled(&path, 5);
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.page_labels(), ["i", "ii", "A-1", "A-2", "A-3"]);
    // An unsaved markup on page 4 must follow it too.
    let extra = s.add_markup(line(3)).unwrap();

    s.rotate_pages(&[0, 2], 90).unwrap();
    assert_eq!(
        (
            s.doc().pages[0].rotate,
            s.doc().pages[1].rotate,
            s.doc().pages[2].rotate
        ),
        (90, 0, 90)
    );
    assert!(s.rotate_pages(&[0], 45).is_err());

    // Move pages 4-5 to the front.
    s.move_pages(&[3, 4], 0).unwrap();
    assert_eq!(s.page_labels(), ["A-2", "A-3", "i", "ii", "A-1"]);
    assert_eq!(s.markup(&ids[3]).unwrap().page, 0);
    assert_eq!(s.markup(&extra).unwrap().page, 0);
    assert_eq!(s.markup(&ids[0]).unwrap().page, 2);

    s.delete_pages(&[3]).unwrap(); // "ii"
    assert_eq!(s.page_count(), 4);
    assert!(s.markup(&ids[1]).is_err(), "its markup went with the page");
    assert!(s.delete_pages(&[0, 1, 2, 3]).is_err());

    s.insert_blank_pages(1, 2, Some((1224.0, 792.0))).unwrap();
    assert_eq!(s.page_count(), 6);
    assert_eq!(s.doc().pages[1].media.width(), 1224.0);

    // Undo the whole sequence back to the start, then redo one step.
    for _ in 0..4 {
        s.undo().unwrap();
    }
    assert_eq!(s.page_count(), 5);
    assert_eq!(s.markup(&extra).unwrap().page, 3);
    s.redo().unwrap();
    assert_eq!(s.doc().pages[2].rotate, 90);

    // Save and reopen: everything is in the file.
    let out = tmp("pages-out.pdf");
    s.move_pages(&[4], 0).unwrap();
    s.save_as(&out, false).unwrap();
    let back = Session::open(&out).unwrap();
    assert_eq!(back.page_labels(), ["A-3", "i", "ii", "A-1", "A-2"]);
    assert_eq!(back.markup(&ids[4]).unwrap().page, 0);
    assert_eq!(back.markup(&extra).unwrap().page, 4);
    assert_eq!(back.doc().pages[1].rotate, 90);
}

#[test]
fn insert_from_file_and_extract() {
    let src = tmp("src.pdf");
    let ids = labelled(&src, 3);
    let mut s = Session::open(&src).unwrap();
    // Insert the file into itself: the copies' ids are renamed, every page brings its markup.
    s.insert_file_pages(1, &src, Some(&[0, 2])).unwrap();
    assert_eq!(s.page_count(), 5);
    assert_eq!(s.doc().markups.len(), 5);
    let unique: std::collections::HashSet<&str> = s.doc().markups.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(unique.len(), 5);
    assert_eq!(s.doc().markups_on(1).count(), 1);
    assert_eq!(s.page_labels(), ["i", "i", "A-1", "ii", "A-1"]);
    let e = s.insert_file_pages(0, &src, Some(&[7])).unwrap_err();
    assert!(e.to_string().contains("page 8 is not in"), "{e}");

    let out = tmp("extract.pdf");
    let n = s.extract_pages(&[3, 4], &out, true).unwrap();
    assert_eq!(n, 2);
    assert_eq!(s.page_count(), 3);
    let x = Session::open(&out).unwrap();
    assert_eq!(x.page_count(), 2);
    assert_eq!(x.page_labels(), ["ii", "A-1"]);
    assert!(x.markup(&ids[1]).is_ok());
    assert!(s.extract_pages(&[0], &src, false).is_err(), "not onto the open file");
}

#[test]
fn undo_history_is_capped() {
    let mut s = session(1, "cap.pdf").with_undo_limits(UndoLimits {
        max_steps: 3,
        max_bytes: usize::MAX,
    });
    for _ in 0..5 {
        s.add_markup(line(0)).unwrap();
    }
    assert_eq!(s.undo_depth(), 3);
    let mut t = session(1, "capb.pdf").with_undo_limits(UndoLimits {
        max_steps: 100,
        max_bytes: 1,
    });
    for _ in 0..5 {
        t.add_markup(line(0)).unwrap();
    }
    assert_eq!(t.undo_depth(), 1);
}

#[test]
fn command_table() {
    let mut s = session(1, "cmd.pdf");
    assert!(commands::run(&mut s, "edit.delete").is_err(), "needs a selection");
    assert!(
        commands::run(&mut s, "nope")
            .unwrap_err()
            .to_string()
            .contains("edit.undo")
    );
    let a = s.add_markup(line(0)).unwrap();
    s.add_markup(line(0)).unwrap();
    commands::run(&mut s, "edit.select_all").unwrap();
    commands::run(&mut s, "markup.group").unwrap();
    commands::run(&mut s, "edit.copy").unwrap();
    commands::run(&mut s, "edit.paste_in_place").unwrap();
    assert_eq!(s.doc().markups.len(), 4);
    s.select(std::slice::from_ref(&a)).unwrap();
    commands::run(&mut s, "arrange.bring_to_front").unwrap();
    assert_eq!(s.doc().markups.last().unwrap().id, a);
    commands::run(&mut s, "markup.lock").unwrap();
    commands::run(&mut s, "edit.delete").unwrap();
    assert!(s.markup(&a).is_ok(), "locked markups stay");
    commands::run(&mut s, "edit.undo").unwrap();
    let ids: std::collections::HashSet<&str> = commands::COMMANDS.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), commands::COMMANDS.len(), "ids are unique");
}

#[test]
fn labels_merge_runs() {
    let mut cos = markupcraft_revu::cos::Document::new_empty();
    let specs = [
        LabelSpec::decimal(1),
        LabelSpec::decimal(2),
        LabelSpec::decimal(7),
        LabelSpec::decimal(8),
    ];
    labels::write(&mut cos, &specs);
    let root = cos.root().unwrap();
    let cat = cos.get(root);
    let nums = cat
        .as_dict()
        .unwrap()
        .get(b"PageLabels")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Nums")
        .unwrap()
        .clone();
    assert_eq!(nums.as_array().unwrap().len(), 4, "two runs");
    let back = labels::read(&cos, 4).unwrap();
    assert_eq!(
        back.iter().map(|l| l.as_ref().unwrap().text()).collect::<Vec<_>>(),
        ["1", "2", "7", "8"]
    );
}

#[test]
fn csv_export() {
    let mut s = session(1, "csv.pdf");
    let mut m = area(0, 0.0);
    m.scale = Some(Scale::architectural(0.125, 1.0));
    m.subject = "=cmd".into();
    s.add_markup(m).unwrap();
    let csv = export::markups_csv(s.doc(), None);
    let row = csv.lines().nth(1).unwrap();
    assert!(row.starts_with("1,\"\",\""), "{row}");
    assert!(
        row.contains("\"'=cmd\"") && row.contains("\"100 sf\"") && row.contains(",100,"),
        "{row}"
    );
}
