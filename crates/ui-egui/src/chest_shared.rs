//! Tool sets shared from a network folder: a set kept in a `.mctools` file that several people
//! use. It is read-only until checked out; Check Out writes `<file>.lock` naming who has it (a
//! set checked out by someone else cannot be checked out), Check In writes the set back to the
//! shared file and removes the lock, and Refresh reads the shared file again.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::chest::ToolChest;

/// A tool set linked to a shared file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSet {
    pub set: String,
    pub path: PathBuf,
    /// Checked out by this user (the lock file is ours).
    #[serde(default)]
    pub checked_out: bool,
}

fn lock_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

/// Who holds the lock of a shared set's file, if anyone.
pub fn holder(path: &Path) -> Option<String> {
    let b = markupcraft_revu::fsio::read(lock_path(path)).ok()?;
    let t = String::from_utf8_lossy(b.get(..b.len().min(400))?).trim().to_string();
    Some(if t.is_empty() { "someone".into() } else { t })
}

impl ToolChest {
    pub fn shared(&self, set: &str) -> Option<&SharedSet> {
        self.extras.shared.iter().find(|s| s.set == set)
    }

    /// Add the tool set in the shared file `path`: read-only until checked out. Returns its id.
    pub fn add_shared(&mut self, path: &Path) -> Result<String, String> {
        if self.extras.shared.iter().any(|s| s.path == path) {
            return Err(format!("{} is already in the Tool Chest", path.display()));
        }
        let text = markupcraft_revu::fsio::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let id = self.import_set(&text)?;
        self.extras.shared.push(SharedSet {
            set: id.clone(),
            path: path.to_path_buf(),
            checked_out: false,
        });
        self.set_locked(&id, true);
        Ok(id)
    }

    /// Check out a shared set for editing as `user`.
    pub fn check_out(&mut self, set: &str, user: &str) -> Result<(), String> {
        let sh = self.shared(set).cloned().ok_or("not a shared tool set")?;
        if sh.checked_out {
            return Ok(());
        }
        if let Some(who) = holder(&sh.path) {
            return Err(format!("Checked out by {who}: read-only until it is checked in"));
        }
        // the newest shared version first
        self.refresh_shared(set)?;
        let user = if user.trim().is_empty() {
            "a MarkupCraft user"
        } else {
            user.trim()
        };
        crate::chest::write_atomic(&lock_path(&sh.path), user.as_bytes())?;
        if let Some(s) = self.extras.shared.iter_mut().find(|s| s.set == set) {
            s.checked_out = true;
        }
        self.set_locked(set, false);
        Ok(())
    }

    /// Check a shared set back in: write it to the shared file, remove the lock, read-only again.
    pub fn check_in(&mut self, set: &str) -> Result<(), String> {
        let sh = self.shared(set).cloned().ok_or("not a shared tool set")?;
        if !sh.checked_out {
            return Err("the tool set is not checked out".into());
        }
        let text = self.export_set(set)?;
        crate::chest::write_atomic(&sh.path, text.as_bytes())?;
        let _ = std::fs::remove_file(lock_path(&sh.path));
        if let Some(s) = self.extras.shared.iter_mut().find(|s| s.set == set) {
            s.checked_out = false;
        }
        self.set_locked(set, true);
        Ok(())
    }

    /// Read a shared set's file again (its tools replace ours; not while checked out by us).
    pub fn refresh_shared(&mut self, set: &str) -> Result<(), String> {
        let sh = self.shared(set).cloned().ok_or("not a shared tool set")?;
        if sh.checked_out {
            return Ok(());
        }
        let text =
            markupcraft_revu::fsio::read_to_string(&sh.path).map_err(|e| format!("{}: {e}", sh.path.display()))?;
        let new_id = self.import_set(&text)?;
        let pos_new = self.sets.iter().position(|s| s.id == new_id).ok_or("import failed")?;
        let fresh = self.sets.remove(pos_new);
        if let Some(old) = self.sets.iter_mut().find(|s| s.id == set) {
            old.items = fresh.items;
            old.title = fresh.title;
            old.scale = fresh.scale;
        }
        self.save();
        Ok(())
    }

    /// Stop sharing: the set stays as an ordinary (locked) set.
    pub fn unlink_shared(&mut self, set: &str) {
        if let Some(sh) = self.shared(set).cloned()
            && sh.checked_out
        {
            let _ = std::fs::remove_file(lock_path(&sh.path));
        }
        self.extras.shared.retain(|s| s.set != set);
        self.save();
    }
}

/// The shared-set rows of a tool set's right-click menu.
pub fn set_menu(ui: &mut egui::Ui, app: &crate::AppState, set: &str, out: &mut Vec<crate::chest_more::ChestAct>) {
    use crate::chest_more::ChestAct;
    let Some(sh) = app.toolchest.shared(set) else { return };
    ui.separator();
    let who = holder(&sh.path);
    if sh.checked_out {
        if ui.button("Check In").clicked() {
            out.push(ChestAct::CheckIn(set.into()));
            ui.close();
        }
    } else {
        let label = match &who {
            Some(w) => format!("Checked out by {w}"),
            None => "Check Out".into(),
        };
        if ui.add_enabled(who.is_none(), egui::Button::new(label)).clicked() {
            out.push(ChestAct::CheckOut(set.into()));
            ui.close();
        }
        if ui.button("Refresh from Shared Folder").clicked() {
            out.push(ChestAct::RefreshShared(set.into()));
            ui.close();
        }
    }
    ui.label(egui::RichText::new(sh.path.display().to_string()).weak().small());
}

/// Run a shared-set act.
pub fn apply(app: &mut crate::AppState, act: &crate::chest_more::ChestAct) {
    use crate::chest_more::ChestAct;
    let r = match act {
        ChestAct::CheckOut(s) => {
            let user = app.author.clone();
            app.toolchest
                .check_out(s, &user)
                .map(|_| "Checked out: the tool set can be edited".to_string())
        }
        ChestAct::CheckIn(s) => app
            .toolchest
            .check_in(s)
            .map(|_| "Checked in to the shared folder".to_string()),
        ChestAct::RefreshShared(s) => app
            .toolchest
            .refresh_shared(s)
            .map(|_| "Refreshed from the shared folder".to_string()),
        _ => return,
    };
    app.status = match r {
        Ok(m) => m,
        Err(e) => e,
    };
}
