//! `pages <in> <out> <op> ...`: one page operation on a file.
//!
//!   rotate <range> <degrees> | delete <range> | move <range> <beforePage>
//!   | blank <atPage> <count> | insert <atPage> <other.pdf> <range> | extract <range>
//!
//! Ranges are like "1-3, 5, 8-". Markups, page scales and page labels follow their pages.

use std::path::Path;

use markupcraft_automation::parse_range;
use markupcraft_engine::Session;
use markupcraft_engine::pages::ForeignPdf;

type Res = Result<bool, Box<dyn std::error::Error>>;

const USAGE: &str = "pages <in.pdf> <out.pdf> rotate <range> <deg> | delete <range> | move <range> <beforePage> \
                     | blank <atPage> <count> | insert <atPage> <other.pdf> <range> | extract <range>";

fn arg(args: &[String], i: usize) -> Result<&str, String> {
    args.get(i).map(String::as_str).ok_or_else(|| format!("usage: {USAGE}"))
}

fn number(s: &str, what: &str) -> Result<i64, String> {
    s.trim()
        .parse::<i64>()
        .map_err(|_| format!("{what} must be a whole number (got {s:?})"))
}

/// 1-based position → 0-based.
fn position(s: &str, what: &str) -> Result<usize, String> {
    match number(s, what)? {
        p if p >= 1 => Ok(p as usize - 1),
        p => Err(format!("{what} starts at 1 (got {p})")),
    }
}

/// `args` = [in, out, op, ...]
pub fn run(args: &[String]) -> Res {
    let (input, out, op) = (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?);
    let mut s = Session::open(input)?;
    let n = s.page_count();
    let range = |i: usize| -> Result<Vec<usize>, String> {
        let r = parse_range(arg(args, i)?, n).map_err(|e| format!("range: expected {e}"))?;
        if r.is_empty() {
            return Err("range: no pages".into());
        }
        Ok(r)
    };
    match op {
        "rotate" => {
            let deg = number(arg(args, 4)?, "degrees")?;
            s.rotate_pages(&range(3)?, deg)?;
        }
        "delete" => {
            s.delete_pages(&range(3)?)?;
        }
        "move" => {
            let before = position(arg(args, 4)?, "beforePage")?;
            s.move_pages(&range(3)?, before)?;
        }
        "blank" => {
            let at = position(arg(args, 3)?, "atPage")?;
            let count = number(arg(args, 4)?, "count")?;
            let count = usize::try_from(count).map_err(|_| "count must be positive".to_string())?;
            s.insert_blank_pages(at, count, Some((612.0, 792.0)))?;
        }
        "insert" => {
            let at = position(arg(args, 3)?, "atPage")?;
            let other = arg(args, 4)?;
            let m = ForeignPdf::open(other)?.page_count();
            let pages = parse_range(arg(args, 5)?, m).map_err(|e| format!("range: expected {e}"))?;
            s.insert_file_pages(at, Path::new(other), Some(&pages))?;
        }
        "extract" => {
            let pages = range(3)?;
            s.extract_pages(&pages, Path::new(out), false)?;
            let x = Session::open(out)?;
            println!("{out}: {} pages, {} markups", x.page_count(), x.doc().markups.len());
            return Ok(true);
        }
        _ => return Err(format!("unknown page operation {op:?}; usage: {USAGE}").into()),
    }
    s.save_as(out, true)?;
    let back = Session::open(out)?;
    println!(
        "{out}: {} pages, {} markups",
        back.page_count(),
        back.doc().markups.len()
    );
    Ok(true)
}
