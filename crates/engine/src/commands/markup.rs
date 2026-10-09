//! Markup menu commands: groups and locks.

use crate::{Result, Session};

pub fn group(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let id = s.group(&ids)?;
    Ok(format!("grouped as {id}"))
}

pub fn ungroup(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.ungroup(&ids)?;
    Ok(format!("{n} markups ungrouped"))
}

pub fn lock(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.set_locked(&ids, true)?;
    Ok(format!("locked {n} markups"))
}

pub fn unlock(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.set_locked(&ids, false)?;
    Ok(format!("unlocked {n} markups"))
}
