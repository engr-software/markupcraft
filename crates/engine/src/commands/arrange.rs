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
