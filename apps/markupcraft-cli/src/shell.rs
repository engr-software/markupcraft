//! `markupcraft-cli shell combine|convert <files...>`: what the file manager entries written by
//! the `shell_integration` tool run on the selected files.
//!
//!   shell combine a.pdf b.png ...   one PDF beside the first file: "<first> combined.pdf"
//!   shell convert a.png notes.txt   each file that is not a PDF to "<name>.pdf" beside it

use std::path::PathBuf;

use markupcraft_engine::shell_integration::{shell_combine, shell_convert};

type Res = Result<bool, Box<dyn std::error::Error>>;

pub fn run(args: &[String]) -> Res {
    let (Some(verb), Some(files)) = (args.first(), args.get(1..)) else {
        return Err("shell combine|convert <files...>".into());
    };
    let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    match verb.as_str() {
        "combine" => {
            let out = shell_combine(&files)?;
            println!("{}", out.display());
        }
        "convert" => {
            for f in shell_convert(&files)? {
                println!("{}", f.display());
            }
        }
        v => return Err(format!("shell: unknown verb {v:?} (combine or convert)").into()),
    }
    Ok(true)
}
