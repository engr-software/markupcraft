//! File menu commands.

use crate::{Result, Session};

pub fn save(s: &mut Session) -> Result<String> {
    s.save(false)?;
    Ok(format!("saved {}", s.path().display()))
}
