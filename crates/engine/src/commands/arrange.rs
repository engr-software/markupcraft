//! Arrange (z-order) commands.

use crate::{Arrange, Result, Session};

fn go(s: &mut Session, how: Arrange) -> Result<String> {
    let ids = s.selection().to_vec();
    Ok(if s.arrange(&ids, how)? {
        "arranged".into()
    } else {
        "already in place".into()
    })
}

pub fn bring_to_front(s: &mut Session) -> Result<String> {
    go(s, Arrange::BringToFront)
}

pub fn send_to_back(s: &mut Session) -> Result<String> {
    go(s, Arrange::SendToBack)
}

pub fn bring_forward(s: &mut Session) -> Result<String> {
    go(s, Arrange::BringForward)
}

pub fn send_backward(s: &mut Session) -> Result<String> {
    go(s, Arrange::SendBackward)
}

fn align(s: &mut Session, how: crate::align::Align) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.align_markups(&ids, how)?;
    Ok(format!("aligned ({n} moved)"))
}

pub fn align_left(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Left)
}

pub fn align_center(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Center)
}

pub fn align_right(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Right)
}

pub fn align_top(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Top)
}

pub fn align_middle(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Middle)
}

pub fn align_bottom(s: &mut Session) -> Result<String> {
    align(s, crate::align::Align::Bottom)
}

pub fn distribute_horizontal(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.distribute_markups(&ids, true)?;
    Ok(format!("distributed ({n} moved)"))
}

pub fn distribute_vertical(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.distribute_markups(&ids, false)?;
    Ok(format!("distributed ({n} moved)"))
}

pub fn flip_horizontal(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.flip_markups(&ids, true)?;
    Ok(format!("flipped {n}"))
}

pub fn flip_vertical(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.flip_markups(&ids, false)?;
    Ok(format!("flipped {n}"))
}

pub fn apply_to_all_pages(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let v = s.copy_to_pages(&ids, &[])?;
    Ok(format!("copied to the other pages ({} new)", v.len()))
}

pub fn remove_from_group(s: &mut Session) -> Result<String> {
    let ids = s.selection().to_vec();
    let n = s.remove_from_group(&ids)?;
    Ok(format!("{n} removed from their group"))
}
