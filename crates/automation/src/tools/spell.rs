//! Spell check.

use markupcraft_engine::spell::{SpellOptions, check_text, dictionary};
use serde_json::{Value, json};

use super::{Tool, ids_arg, schema};
use crate::bad_args;

pub static CHECK: Tool = Tool {
    name: "spell_check",
    title: "Spell Check",
    description: "Spell check the comment text of markups (ids, default all) against a Hunspell dictionary (default en_US), or a given `text`. Words with digits, one-letter words and (unless ignore_uppercase is false) ALL-CAPS words are not checked. Returns each misspelled word with its markup, page, position and suggestions. Changes nothing.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({
                "ids": ids_arg(),
                "text": { "type": "string", "description": "Check this text instead of markups." },
                "language": { "type": "string", "description": "Dictionary name: en_US (default) or en_GB (British spellings accepted)." },
                "ignore_uppercase": { "type": "boolean", "description": "Skip ALL-CAPS words (default true)." },
                "accept": { "type": "array", "items": { "type": "string" }, "description": "Words to accept (a user dictionary)." },
                "suggestions": { "type": "integer", "minimum": 0, "maximum": 20, "description": "Suggestions per word (default 5)." }
            }),
            &[],
        )
    },
    run: |a, args| {
        let lang = args.opt_str("language")?.unwrap_or("en_US");
        // British English: the American dictionary accepting British spellings.
        let british = matches!(lang, "en_GB" | "en-GB");
        let lang = if british { "en_US" } else { lang };
        let opts = SpellOptions {
            ignore_uppercase: args.bool_or("ignore_uppercase", true)?,
            british,
            accept: args.opt_strings("accept")?.unwrap_or_default(),
            suggestions: args.opt_u64("suggestions")?.unwrap_or(5).min(20) as usize,
        };
        let dict = dictionary(lang)?;
        let found = match args.opt_str("text")? {
            Some(t) => {
                if args.has("ids") {
                    return Err(bad_args("give ids or text, not both"));
                }
                if t.chars().count() > 1_000_000 {
                    return Err(bad_args("text is too long"));
                }
                check_text(&dict, t, &opts)
            }
            None => {
                let (_, s) = a.session_ref(args)?;
                let ids = args.opt_strings("ids")?;
                s.spell_check(&dict, ids.as_deref(), &opts)?
            }
        };
        let list: Vec<Value> = found
            .iter()
            .map(|m| {
                json!({
                    "id": (!m.markup.is_empty()).then_some(&m.markup),
                    "page": m.page.map(|p| p + 1),
                    "word": m.word,
                    "start": m.start,
                    "suggestions": m.suggestions,
                })
            })
            .collect();
        Ok(json!({ "language": lang, "count": list.len(), "misspelled": list }))
    },
};
