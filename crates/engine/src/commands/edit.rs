//! Edit menu commands.

use crate::{Result, Session};

fn sel(s: &Session) -> Vec<String> {
    s.selection().to_vec()
}

pub fn undo(s: &mut Session) -> Result<String> {
    s.undo().map(|l| format!("undid {l}"))
}

pub fn redo(s: &mut Session) -> Result<String> {
    s.redo().map(|l| format!("redid {l}"))
}

pub fn delete(s: &mut Session) -> Result<String> {
    let n = s.delete_markups(&sel(s), true)?;
    Ok(format!("deleted {n} markups"))
}

pub fn duplicate(s: &mut Session) -> Result<String> {
    let ids = s.duplicate_markups(&sel(s), 10.0, -10.0)?;
    Ok(format!("duplicated {} markups", ids.len()))
}

pub fn copy(s: &mut Session) -> Result<String> {
    let n = s.copy_markups(&sel(s))?;
    Ok(format!("copied {n} markups"))
}

pub fn cut(s: &mut Session) -> Result<String> {
    let n = s.cut_markups(&sel(s))?;
    Ok(format!("cut {n} markups"))
}

pub fn paste_in_place(s: &mut Session) -> Result<String> {
    let ids = s.paste(None, None)?;
    Ok(format!("pasted {} markups", ids.len()))
}

pub fn select_all(s: &mut Session) -> Result<String> {
    s.select_all(None);
    Ok(format!("selected {} markups", s.selection().len()))
}

pub fn select_none(s: &mut Session) -> Result<String> {
    s.clear_selection();
    Ok("selection cleared".into())
}
