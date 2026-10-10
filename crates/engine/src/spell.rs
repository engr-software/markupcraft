//! Spell check of markup text against a Hunspell dictionary (`assets/dictionaries/en_US`).
//!
//! The dictionary is read with a small pure-Rust Hunspell reader: the `.dic` words are expanded
//! with the `.aff` prefix and suffix rules (conditions, stripping, cross products) into the set
//! of accepted forms; `REP` and `TRY` drive suggestions (replacement patterns first, then one
//! edit, then two). Rules for what is checked, for drawings full of codes and abbreviations:
//! words containing a digit are never checked, ALL-CAPS words are skipped unless asked, and a
//! one-letter word is never checked.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{Result, Session, invalid};

/// Largest `.dic` / `.aff` read.
const MAX_FILE: u64 = 64 << 20;
/// Most expanded forms kept.
const MAX_FORMS: usize = 5_000_000;
/// Longest word checked or suggested for.
const MAX_WORD: usize = 64;

#[derive(Debug, Clone)]
enum CondPart {
    Any,
    Set(Vec<char>, bool),
    Char(char),
}

#[derive(Debug, Clone)]
struct Rule {
    strip: String,
    add: String,
    cond: Vec<CondPart>,
}

#[derive(Debug, Clone)]
struct Affix {
    cross: bool,
    rules: Vec<Rule>,
}

fn parse_cond(s: &str) -> Vec<CondPart> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '.' => out.push(CondPart::Any),
            '[' => {
                let neg = it.peek() == Some(&'^');
                if neg {
                    it.next();
                }
                let mut set = Vec::new();
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                    set.push(d);
                }
                out.push(CondPart::Set(set, neg));
            }
            c => out.push(CondPart::Char(c)),
        }
    }
    out
}

fn part_matches(p: &CondPart, c: char) -> bool {
    match p {
        CondPart::Any => true,
        CondPart::Char(x) => *x == c,
        CondPart::Set(s, neg) => s.contains(&c) != *neg,
    }
}

/// Does the word's end (suffix) or start (prefix) meet the condition?
fn cond_ok(cond: &[CondPart], word: &[char], suffix: bool) -> bool {
    if cond.len() == 1 && matches!(cond.first(), Some(CondPart::Any)) {
        return true;
    }
    if cond.len() > word.len() {
        return false;
    }
    let tail = if suffix {
        word.get(word.len() - cond.len()..)
    } else {
        word.get(..cond.len())
    };
    tail.is_some_and(|t| cond.iter().zip(t).all(|(p, c)| part_matches(p, *c)))
}

fn apply_suffix(word: &str, r: &Rule) -> Option<String> {
    let chars: Vec<char> = word.chars().collect();
    if !cond_ok(&r.cond, &chars, true) || !word.ends_with(r.strip.as_str()) {
        return None;
    }
    let base = word.get(..word.len() - r.strip.len())?;
    if base.is_empty() {
        return None;
    }
    Some(format!("{base}{}", r.add))
}

fn apply_prefix(word: &str, r: &Rule) -> Option<String> {
    let chars: Vec<char> = word.chars().collect();
    if !cond_ok(&r.cond, &chars, false) || !word.starts_with(r.strip.as_str()) {
        return None;
    }
    let base = word.get(r.strip.len()..)?;
    if base.is_empty() {
        return None;
    }
    Some(format!("{}{base}", r.add))
}

/// A loaded dictionary: the accepted forms and the suggestion tables.
#[derive(Debug, Default)]
pub struct Dictionary {
    words: HashSet<String>,
    /// Forms never offered as suggestions (`NOSUGGEST`).
    no_suggest: HashSet<String>,
    try_chars: Vec<char>,
    rep: Vec<(String, String)>,
    /// Words the user accepted.
    accepted: Mutex<HashSet<String>>,
}

fn read_capped(path: &Path) -> Result<String> {
    let io = |e| crate::EngineError::Io {
        path: path.display().to_string(),
        source: e,
    };
    let meta = std::fs::metadata(path).map_err(io)?;
    if meta.len() > MAX_FILE {
        return Err(invalid(format!("{} is too large for a dictionary", path.display())));
    }
    let bytes = std::fs::read(path).map_err(io)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl Dictionary {
    /// Read `<dir>/<lang>.aff` and `<dir>/<lang>.dic`.
    pub fn open(dir: &Path, lang: &str) -> Result<Self> {
        if lang.is_empty() || !lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(invalid(format!("{lang:?} is not a dictionary name like en_US")));
        }
        let aff = read_capped(&dir.join(format!("{lang}.aff")))?;
        let dic = read_capped(&dir.join(format!("{lang}.dic")))?;
        Ok(Self::parse(&aff, &dic))
    }

    /// Build from the text of a `.aff` and a `.dic` file.
    pub fn parse(aff: &str, dic: &str) -> Self {
        let mut prefixes: HashMap<char, Affix> = HashMap::new();
        let mut suffixes: HashMap<char, Affix> = HashMap::new();
        let mut d = Dictionary::default();
        let (mut no_suggest_flag, mut only_compound) = (None, None);
        for line in aff.lines() {
            let t: Vec<&str> = line.split_whitespace().collect();
            match t.as_slice() {
                ["TRY", chars, ..] => d.try_chars = chars.chars().collect(),
                ["NOSUGGEST", f, ..] => no_suggest_flag = f.chars().next(),
                ["ONLYINCOMPOUND", f, ..] => only_compound = f.chars().next(),
                ["REP", from, to, ..] => d.rep.push((from.replace('_', " "), to.replace('_', " "))),
                [kind @ ("PFX" | "SFX"), flag, cross, n] if n.parse::<usize>().is_ok() => {
                    let Some(f) = flag.chars().next() else { continue };
                    let table = if *kind == "PFX" { &mut prefixes } else { &mut suffixes };
                    table.insert(
                        f,
                        Affix {
                            cross: *cross == "Y",
                            rules: Vec::new(),
                        },
                    );
                }
                [kind @ ("PFX" | "SFX"), flag, strip, add, cond, ..] => {
                    let Some(f) = flag.chars().next() else { continue };
                    let table = if *kind == "PFX" { &mut prefixes } else { &mut suffixes };
                    if let Some(a) = table.get_mut(&f) {
                        let clean = |s: &str| {
                            if s == "0" {
                                String::new()
                            } else {
                                s.split('/').next().unwrap_or("").to_string()
                            }
                        };
                        a.rules.push(Rule {
                            strip: clean(strip),
                            add: clean(add),
                            cond: parse_cond(cond),
                        });
                    }
                }
                _ => {}
            }
        }
        for (i, line) in dic.lines().enumerate() {
            if i == 0 && line.trim().parse::<usize>().is_ok() {
                continue;
            }
            let line = line.trim();
            if line.is_empty() || d.words.len() >= MAX_FORMS {
                continue;
            }
            let (word, flags) = match line.split_once('/') {
                Some((w, f)) => (w, f.split_whitespace().next().unwrap_or("")),
                None => (line.split_whitespace().next().unwrap_or(""), ""),
            };
            let flags: Vec<char> = flags.chars().collect();
            if word.is_empty() || only_compound.is_some_and(|c| flags.contains(&c)) {
                continue;
            }
            let hidden = no_suggest_flag.is_some_and(|c| flags.contains(&c));
            let mut forms = vec![word.to_string()];
            let mut suffixed = Vec::new();
            for f in &flags {
                if let Some(a) = suffixes.get(f) {
                    for r in &a.rules {
                        if let Some(w) = apply_suffix(word, r) {
                            forms.push(w.clone());
                            if a.cross {
                                suffixed.push(w);
                            }
                        }
                    }
                }
            }
            for f in &flags {
                if let Some(a) = prefixes.get(f) {
                    for r in &a.rules {
                        if let Some(w) = apply_prefix(word, r) {
                            forms.push(w);
                        }
                        if a.cross {
                            for s in &suffixed {
                                if let Some(w) = apply_prefix(s, r) {
                                    forms.push(w);
                                }
                            }
                        }
                    }
                }
            }
            for w in forms {
                if hidden {
                    d.no_suggest.insert(w.clone());
                }
                d.words.insert(w);
            }
        }
        d
    }

    /// How many forms the dictionary accepts.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Accept `word` from now on (a user dictionary word or "Ignore All").
    pub fn accept(&self, word: &str) {
        let mut a = self.accepted.lock().unwrap_or_else(|e| e.into_inner());
        a.insert(word.to_string());
    }

    fn known(&self, w: &str) -> bool {
        self.words.contains(w) || self.accepted.lock().map(|a| a.contains(w)).unwrap_or(false)
    }

    /// Is this word spelled correctly? (Capitalised and all-caps forms of a lower-case word
    /// are accepted; a typographic apostrophe counts as `'`.)
    pub fn check(&self, word: &str) -> bool {
        let w = word.replace('\u{2019}', "'");
        if w.is_empty() || w.chars().count() > MAX_WORD || self.known(&w) {
            return true;
        }
        let lower = w.to_lowercase();
        let mut cs = w.chars();
        let first_upper = cs.next().is_some_and(char::is_uppercase);
        let rest_lower = cs.clone().all(|c| !c.is_uppercase());
        let all_upper = w.chars().all(|c| !c.is_lowercase());
        if first_upper && (rest_lower || all_upper) && self.known(&lower) {
            return true;
        }
        if all_upper {
            let title: String = lower
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i == 0 {
                        c.to_uppercase().next().unwrap_or(c)
                    } else {
                        c
                    }
                })
                .collect();
            return self.known(&title);
        }
        false
    }

    fn good(&self, w: &str) -> bool {
        !w.is_empty() && self.words.contains(w) && !self.no_suggest.contains(w)
    }

    /// Up to `max` suggestions for a misspelled word, best first, in the word's capitalisation.
    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let w = word.replace('\u{2019}', "'");
        if w.is_empty() || w.chars().count() > MAX_WORD || max == 0 {
            return Vec::new();
        }
        let first_upper = w.chars().next().is_some_and(char::is_uppercase);
        let all_upper = w.chars().all(|c| !c.is_lowercase()) && w.chars().count() > 1;
        let lower = w.to_lowercase();
        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut push = |s: String, out: &mut Vec<String>| {
            if seen.insert(s.clone()) {
                out.push(s);
            }
        };
        // Replacement patterns (common misspellings).
        for (from, to) in &self.rep {
            let mut start = 0;
            while let Some(i) = lower.get(start..).and_then(|s| s.find(from.as_str())) {
                let at = start + i;
                let cand = format!("{}{}{}", &lower[..at], to, &lower[at + from.len()..]);
                if cand.split(' ').all(|p| self.good(p)) {
                    push(cand, &mut out);
                }
                start = at + from.len().max(1);
                if start > lower.len() {
                    break;
                }
            }
        }
        let alphabet: Vec<char> = if self.try_chars.is_empty() {
            ('a'..='z').collect()
        } else {
            self.try_chars.iter().filter(|c| !c.is_uppercase()).copied().collect()
        };
        let one = edits(&lower, &alphabet);
        for c in &one {
            if self.good(c) {
                push(c.clone(), &mut out);
            }
        }
        // Two words run together.
        let chars: Vec<char> = lower.chars().collect();
        for i in 2..chars.len().saturating_sub(1) {
            let (a, b): (String, String) = (chars[..i].iter().collect(), chars[i..].iter().collect());
            if self.good(&a) && self.good(&b) && b.chars().count() > 1 {
                push(format!("{a} {b}"), &mut out);
            }
        }
        if out.len() < max {
            let mut two: Vec<String> = Vec::new();
            for c in &one {
                for d in edits(c, &alphabet) {
                    if self.good(&d) && !one.contains(&d) {
                        two.push(d);
                    }
                }
                if two.len() > 200 {
                    break;
                }
            }
            two.sort_by_key(|s| (s.chars().count().abs_diff(chars.len()), s.clone()));
            for d in two {
                push(d, &mut out);
            }
        }
        out.truncate(max);
        out.into_iter()
            .map(|s| {
                if all_upper {
                    s.to_uppercase()
                } else if first_upper {
                    let mut c = s.chars();
                    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or(s)
                } else {
                    s
                }
            })
            .collect()
    }
}

/// Every string one edit away: deletion, transposition, replacement, insertion.
fn edits(w: &str, alphabet: &[char]) -> Vec<String> {
    let c: Vec<char> = w.chars().collect();
    let n = c.len();
    let mut out = Vec::new();
    let s = |v: &[char]| v.iter().collect::<String>();
    for i in 0..n {
        let mut v = c.clone();
        v.remove(i);
        out.push(s(&v));
    }
    for i in 0..n.saturating_sub(1) {
        let mut v = c.clone();
        v.swap(i, i + 1);
        out.push(s(&v));
    }
    for i in 0..n {
        for &a in alphabet {
            if c.get(i) != Some(&a) {
                let mut v = c.clone();
                v[i] = a;
                out.push(s(&v));
            }
        }
    }
    for i in 0..=n {
        for &a in alphabet {
            let mut v = c.clone();
            v.insert(i, a);
            out.push(s(&v));
        }
    }
    out
}

/// A word of a text: its char offset, length (chars) and text.
#[derive(Debug, Clone, PartialEq)]
pub struct WordSpan {
    pub start: usize,
    pub len: usize,
    pub text: String,
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

/// Words: runs of letters and digits, with inner apostrophes (don't, O'Neil).
pub fn split_words(text: &str) -> Vec<WordSpan> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !(is_letter(c[i]) || c[i].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let s = i;
        while i < c.len() {
            let ch = c[i];
            let inner_apostrophe =
                (ch == '\'' || ch == '\u{2019}') && i > s && c.get(i + 1).is_some_and(|n| is_letter(*n));
            if is_letter(ch) || ch.is_ascii_digit() || inner_apostrophe {
                i += 1;
            } else {
                break;
            }
        }
        out.push(WordSpan {
            start: s,
            len: i - s,
            text: c[s..i].iter().collect(),
        });
    }
    out
}

/// Should this word be checked at all?
pub fn should_check(word: &str, ignore_uppercase: bool) -> bool {
    if word.chars().count() < 2 || word.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    word.chars().any(char::is_lowercase) || !ignore_uppercase
}

/// The folders searched for `<lang>.dic`: `$MARKUPCRAFT_DICT_DIR`, `dictionaries/` beside the
/// executable (and the macOS / Linux package locations), then the source tree's
/// `assets/dictionaries/`.
pub fn dictionary_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("MARKUPCRAFT_DICT_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        dirs.push(exe.join("dictionaries"));
        dirs.push(exe.join("../Resources/dictionaries"));
        dirs.push(exe.join("../share/markupcraft/dictionaries"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/dictionaries"));
    dirs
}

static LOADED: Mutex<Vec<(String, Arc<Dictionary>)>> = Mutex::new(Vec::new());

/// The dictionary for `lang` (loaded once per process).
pub fn dictionary(lang: &str) -> Result<Arc<Dictionary>> {
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, d)) = loaded.iter().find(|(l, _)| l == lang) {
        return Ok(d.clone());
    }
    let dir = dictionary_dirs()
        .into_iter()
        .find(|d| d.join(format!("{lang}.dic")).is_file() && d.join(format!("{lang}.aff")).is_file())
        .ok_or_else(|| {
            invalid(format!(
                "no {lang} dictionary found (put {lang}.aff and {lang}.dic in a folder named by MARKUPCRAFT_DICT_DIR)"
            ))
        })?;
    let d = Arc::new(Dictionary::open(&dir, lang)?);
    loaded.push((lang.to_string(), d.clone()));
    Ok(d)
}

/// A misspelled word.
#[derive(Debug, Clone, PartialEq)]
pub struct Misspelling {
    /// The markup it is in ("" for free text).
    pub markup: String,
    pub page: Option<usize>,
    pub word: String,
    /// Char offset in the text.
    pub start: usize,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpellOptions {
    pub ignore_uppercase: bool,
    /// English (United Kingdom): British spellings (colour, organise, centre...) are accepted.
    pub british: bool,
    /// Words to accept.
    pub accept: Vec<String>,
    pub suggestions: usize,
}

impl Default for SpellOptions {
    fn default() -> Self {
        Self {
            ignore_uppercase: true,
            british: false,
            accept: Vec::new(),
            suggestions: 5,
        }
    }
}

/// British spellings and their American forms (the dictionary is American English).
const BRITISH: &[(&str, &str)] = &[
    ("isation", "ization"),
    ("ising", "izing"),
    ("ised", "ized"),
    ("ise", "ize"),
    ("ysing", "yzing"),
    ("ysed", "yzed"),
    ("yse", "yze"),
    ("our", "or"),
    ("tre", "ter"),
    ("ogue", "og"),
    ("ence", "ense"),
    ("lled", "led"),
    ("lling", "ling"),
    ("ller", "ler"),
    ("mme", "m"),
    ("ae", "e"),
    ("oe", "e"),
];

/// Is `word` a British spelling of a word the dictionary knows?
pub fn british_form(dict: &Dictionary, word: &str) -> bool {
    if word.chars().count() > MAX_WORD {
        return false;
    }
    for (gb, us) in BRITISH {
        let mut start = 0;
        while let Some(i) = word.get(start..).and_then(|s| s.find(gb)) {
            let at = start + i;
            let mut c = String::with_capacity(word.len());
            c.push_str(word.get(..at).unwrap_or_default());
            c.push_str(us);
            c.push_str(word.get(at + gb.len()..).unwrap_or_default());
            if dict.check(&c) {
                return true;
            }
            start = at + gb.len();
        }
    }
    false
}

/// The misspelled words of `text`.
pub fn check_text(dict: &Dictionary, text: &str, opts: &SpellOptions) -> Vec<Misspelling> {
    let accept: HashSet<&str> = opts.accept.iter().map(String::as_str).collect();
    split_words(text)
        .into_iter()
        .filter(|w| should_check(&w.text, opts.ignore_uppercase) && !accept.contains(w.text.as_str()))
        .filter(|w| !dict.check(&w.text) && !(opts.british && british_form(dict, &w.text)))
        .map(|w| Misspelling {
            markup: String::new(),
            page: None,
            suggestions: dict.suggest(&w.text, opts.suggestions),
            word: w.text,
            start: w.start,
        })
        .collect()
}

impl Session {
    /// Spell check the comment text of markups (`ids`, or all).
    pub fn spell_check(
        &self,
        dict: &Dictionary,
        ids: Option<&[String]>,
        opts: &SpellOptions,
    ) -> Result<Vec<Misspelling>> {
        if let Some(ids) = ids {
            for id in ids {
                self.markup(id)?;
            }
        }
        let mut out = Vec::new();
        for m in &self.doc.markups {
            if ids.is_some_and(|ids| !ids.contains(&m.id)) || m.contents.trim().is_empty() {
                continue;
            }
            for mut x in check_text(dict, &m.contents, opts) {
                x.markup = m.id.clone();
                x.page = Some(m.page);
                out.push(x);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz'\nREP 1\nREP f ph\nNOSUGGEST !\n\
        SFX S Y 2\nSFX S y ies [^aeiou]y\nSFX S 0 s [^sxzhy]\n\
        SFX D Y 2\nSFX D 0 d e\nSFX D 0 ed [^ey]\n\
        PFX U Y 1\nPFX U 0 un .\n";
    const DIC: &str = "5\nduct/SD\nphone/S\ncity/S\nlock/UD\ndamn/!\n";

    #[test]
    fn affix_expansion_and_checks() {
        let d = Dictionary::parse(AFF, DIC);
        for w in [
            "duct", "ducts", "ducted", "cities", "unlock", "unlocked", "locked", "phones", "damn",
        ] {
            assert!(d.check(w), "{w}");
        }
        for w in ["citys", "ductd", "unduct", "phon"] {
            assert!(!d.check(w), "{w}");
        }
        assert!(d.check("Duct") && d.check("DUCTS"));
        assert!(!d.check("dUct"));
        assert_eq!(d.suggest("fone", 3)[0], "phone", "REP pattern first");
        assert_eq!(d.suggest("Dcut", 3)[0], "Duct", "a transposition, in the word's case");
        assert!(
            d.suggest("damm", 5).iter().all(|s| s != "damn"),
            "NOSUGGEST words are not offered"
        );
        d.accept("VAV");
        assert!(d.check("VAV"));
    }

    #[test]
    fn words_and_rules() {
        let w: Vec<String> = split_words("Don't use 2x4s; O'Neil's AHU-1")
            .into_iter()
            .map(|w| w.text)
            .collect();
        assert_eq!(w, ["Don't", "use", "2x4s", "O'Neil's", "AHU", "1"]);
        assert!(!should_check("2x4s", true) && !should_check("AHU", true) && !should_check("a", true));
        assert!(should_check("AHU", false) && should_check("Duct", true));
    }

    #[test]
    fn the_bundled_dictionary_checks_english() {
        let d = dictionary("en_US").unwrap();
        assert!(d.len() > 100_000, "{}", d.len());
        let opts = SpellOptions::default();
        let bad = check_text(
            &d,
            "Instal the suply duct befor the ceiling is closed. VAV boxes per M-101.",
            &opts,
        );
        let words: Vec<&str> = bad.iter().map(|m| m.word.as_str()).collect();
        assert_eq!(words, ["Instal", "suply", "befor"]);
        assert!(
            bad[1].suggestions.contains(&"supply".to_string()),
            "{:?}",
            bad[1].suggestions
        );
        assert!(
            bad[2].suggestions.contains(&"before".to_string()),
            "{:?}",
            bad[2].suggestions
        );
        assert!(check_text(&d, "Insulated ducts; the contract's responsibilities", &opts).is_empty());
        assert!(dictionary("xx_XX").is_err());
        assert!(dictionary("../etc").is_err());
    }
}
