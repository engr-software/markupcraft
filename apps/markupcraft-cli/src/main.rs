//! markupcraft-cli: the same engine as the app, for scripts, agents and the scorecard.
//!
//!   markupcraft-cli list <pdf> [--csv out.csv]       markups and totals
//!   markupcraft-cli check <pdf>                      our quantities vs the label Revu saved
//!   markupcraft-cli resave <in.pdf> <out.pdf>        rewrite every measurement, reload, compare
//!   markupcraft-cli markupcheck <in.pdf> <out.pdf>   rewrite every other markup, reload, compare
//!   markupcraft-cli demo <in.pdf> <out.pdf>          add one of each measurement (to check in Revu)
//!   markupcraft-cli pages <in> <out> <op> ...        one page operation (see pages.rs)
//!   markupcraft-cli tools [--json]                   the automation tool table
//!   markupcraft-cli run --script steps.json [--root DIR]   run tool steps [{"tool", "params"}]
//!   markupcraft-cli run <tool> key=value ... [--root DIR]  run one tool
//!   markupcraft-cli mcp [--root DIR]                 MCP server on stdio (opt-in, no network)

mod automate;
mod pages;
mod scorecard;

use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  markupcraft-cli list <pdf> [--csv out.csv]\n  markupcraft-cli check <pdf>\n  \
         markupcraft-cli resave <in.pdf> <out.pdf>\n  markupcraft-cli markupcheck <in.pdf> <out.pdf>\n  \
         markupcraft-cli demo <in.pdf> <out.pdf>\n  \
         markupcraft-cli pages <in.pdf> <out.pdf> rotate|delete|move|blank|insert|extract ...\n  \
         markupcraft-cli tools [--json]\n  \
         markupcraft-cli run --script steps.json [--root DIR] [--author NAME]\n  \
         markupcraft-cli run <tool> key=value ... [--root DIR]\n  \
         markupcraft-cli mcp [--root DIR] [--author NAME]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(cmd) = args.get(1) else {
        return usage();
    };
    let rest = args.get(2..).unwrap_or_default();
    let a1 = args.get(2);
    let a2 = args.get(3);
    let result = match (cmd.as_str(), a1, a2) {
        ("tools", _, _) => automate::list(rest),
        ("run", _, _) => automate::run(rest),
        ("mcp", _, _) => automate::serve_mcp(rest),
        ("pages", _, _) => pages::run(rest),
        ("list", Some(a1), _) => {
            let csv = args.iter().position(|a| a == "--csv").and_then(|i| args.get(i + 1));
            scorecard::list(a1, csv.map(String::as_str))
        }
        ("check", Some(a1), _) => scorecard::check(a1),
        ("resave", Some(a1), Some(out)) => scorecard::resave(a1, out),
        ("markupcheck", Some(a1), Some(out)) => scorecard::markupcheck(a1, out),
        ("demo", Some(a1), Some(out)) => scorecard::demo(a1, out),
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
