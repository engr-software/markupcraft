//! markupcraft-cli: the same engine as the app, for scripts, agents and the scorecard.
//!
//!   markupcraft-cli list <pdf> [--csv out.csv]       markups and totals
//!   markupcraft-cli summary <pdf> [--csv out.csv] [--totals out.csv] [--xml out.xml]
//!                                                    totals per group and grand total per unit
//!     view options for list / summary: --group a[,b] --sort col [--desc] --filter col=value
//!     --search text --columns a,b,... --page N --measurements
//!   markupcraft-cli check <pdf>                      our quantities vs the label Revu saved
//!   markupcraft-cli resave <in.pdf> <out.pdf>        rewrite every measurement, reload, compare
//!   markupcraft-cli markupcheck <in.pdf> <out.pdf>   rewrite every other markup, reload, compare
//!   markupcraft-cli demo <in.pdf> <out.pdf>          add one of each measurement (to check in Revu)

mod listing;
mod scorecard;

use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  markupcraft-cli list <pdf> [--csv out.csv] [view options]\n  \
         markupcraft-cli summary <pdf> [--csv out.csv] [--totals out.csv] [--xml out.xml] [view options]\n  \
         view options: --group a[,b] --sort col [--desc] --filter col=value --search text --columns a,b \
         --page N --measurements\n  markupcraft-cli check <pdf>\n  \
         markupcraft-cli resave <in.pdf> <out.pdf>\n  markupcraft-cli markupcheck <in.pdf> <out.pdf>\n  \
         markupcraft-cli demo <in.pdf> <out.pdf>"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some(cmd), Some(a1)) = (args.get(1), args.get(2)) else {
        return usage();
    };
    let a2 = args.get(3);
    let result = match (cmd.as_str(), a2) {
        ("list", _) => listing::list(a1, &args),
        ("summary", _) => listing::summary(a1, &args),
        ("check", _) => scorecard::check(a1),
        ("resave", Some(out)) => scorecard::resave(a1, out),
        ("markupcheck", Some(out)) => scorecard::markupcheck(a1, out),
        ("demo", Some(out)) => scorecard::demo(a1, out),
        _ => return usage(),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("markupcraft-cli {cmd}: {e}");
            ExitCode::from(1)
        }
    }
}
