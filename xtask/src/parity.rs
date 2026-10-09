//! `cargo xtask parity`: validate `parity/revu-features.toml` and report progress.
//!
//! Placeholder until the parity checker lands: it reports whether the table exists and passes.
//! The real module keeps this entry point, `run(args)`, so `main.rs` and `cargo xtask ci` need no
//! change when it is replaced.

use crate::gates::root;

pub fn run(_args: &[String]) -> anyhow::Result<()> {
    let table = root().join("parity").join("revu-features.toml");
    if table.is_file() {
        println!("parity: {} present; validation not implemented yet", table.display());
    } else {
        println!("parity: SKIPPED, parity/revu-features.toml does not exist yet");
    }
    Ok(())
}
