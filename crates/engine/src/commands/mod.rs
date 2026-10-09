//! The command table: commands that act on the session's selection, with stable ids for menus,
//! shortcuts, the `command_run` tool and the app's control channel.
//!
//! To add one: write `pub fn foo(s: &mut Session) -> Result<String>` in `commands/<area>.rs`
//! and add one line to [`COMMANDS`]. Commands with arguments are `Session` methods; their
//! tools call those directly.

mod arrange;
mod edit;
mod file;
mod markup;

use crate::{Result, Session, invalid};

/// Runs a command; returns a short report for the user or agent.
pub type Run = fn(&mut Session) -> Result<String>;

pub struct CommandDef {
    /// `area.name`, e.g. `edit.delete`
    pub id: &'static str,
    pub label: &'static str,
    /// Acts on the selection (fails with nothing selected).
    pub needs_selection: bool,
    pub run: Run,
}

const fn c(id: &'static str, label: &'static str, needs_selection: bool, run: Run) -> CommandDef {
    CommandDef {
        id,
        label,
        needs_selection,
        run,
    }
}

/// Every command, one line each.
pub static COMMANDS: &[CommandDef] = &[
    c("edit.undo", "Undo", false, edit::undo),
    c("edit.redo", "Redo", false, edit::redo),
    c("edit.delete", "Delete", true, edit::delete),
    c("edit.duplicate", "Duplicate", true, edit::duplicate),
    c("edit.copy", "Copy", true, edit::copy),
    c("edit.cut", "Cut", true, edit::cut),
    c("edit.paste_in_place", "Paste in Place", false, edit::paste_in_place),
    c("edit.select_all", "Select All", false, edit::select_all),
    c("edit.select_none", "Deselect All", false, edit::select_none),
    c(
        "arrange.bring_to_front",
        "Bring to Front",
        true,
        arrange::bring_to_front,
    ),
    c("arrange.send_to_back", "Send to Back", true, arrange::send_to_back),
    c("arrange.bring_forward", "Bring Forward", true, arrange::bring_forward),
    c("arrange.send_backward", "Send Backward", true, arrange::send_backward),
    c("arrange.align_left", "Align Left", true, arrange::align_left),
    c("arrange.align_center", "Align Center", true, arrange::align_center),
    c("arrange.align_right", "Align Right", true, arrange::align_right),
    c("arrange.align_top", "Align Top", true, arrange::align_top),
    c("arrange.align_middle", "Align Middle", true, arrange::align_middle),
    c("arrange.align_bottom", "Align Bottom", true, arrange::align_bottom),
    c(
        "arrange.distribute_horizontal",
        "Distribute Horizontally",
        true,
        arrange::distribute_horizontal,
    ),
    c(
        "arrange.distribute_vertical",
        "Distribute Vertically",
        true,
        arrange::distribute_vertical,
    ),
    c(
        "arrange.flip_horizontal",
        "Flip Horizontal",
        true,
        arrange::flip_horizontal,
    ),
    c("arrange.flip_vertical", "Flip Vertical", true, arrange::flip_vertical),
    c(
        "markup.apply_to_all_pages",
        "Apply to All Pages",
        true,
        arrange::apply_to_all_pages,
    ),
    c(
        "markup.remove_from_group",
        "Remove From Group",
        true,
        arrange::remove_from_group,
    ),
    c("markup.group", "Group", true, markup::group),
    c("markup.ungroup", "Ungroup", true, markup::ungroup),
    c("markup.lock", "Lock", true, markup::lock),
    c("markup.unlock", "Unlock", true, markup::unlock),
    c("file.save", "Save", false, file::save),
];

pub fn find(id: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Run command `id` on `s`.
pub fn run(s: &mut Session, id: &str) -> Result<String> {
    let cmd = find(id).ok_or_else(|| {
        let ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        invalid(format!("unknown command {id:?} (commands: {})", ids.join(", ")))
    })?;
    if cmd.needs_selection && s.selection().is_empty() {
        return Err(invalid(format!("{}: select markups first", cmd.label)));
    }
    (cmd.run)(s)
}
