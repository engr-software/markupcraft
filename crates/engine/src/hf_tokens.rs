//! File-data tokens for headers and footers: `<<File Name>>`, `<<File Path>>`, `<<Author>>`,
//! `<<Title>>`, `<<Subject>>`, `<<Keywords>>`, `<<Creator>>` and `<<Producer>>` are replaced
//! with the document's file name, full path and information entries when a header or footer is
//! drawn. Page, date and Bates tokens are left for the page-mark editor.

use crate::Session;

/// The file-data tokens a header or footer can hold, with what each shows.
pub const FILE_TOKENS: &[(&str, &str)] = &[
    ("<<File Name>>", "The file name"),
    ("<<File Path>>", "The file's full path"),
    ("<<Author>>", "Author (document properties)"),
    ("<<Title>>", "Title (document properties)"),
    ("<<Subject>>", "Subject (document properties)"),
    ("<<Keywords>>", "Keywords (document properties)"),
    ("<<Creator>>", "Creator (document properties)"),
    ("<<Producer>>", "Producer (document properties)"),
];

/// Every token a header or footer can hold (page, date, Bates and file data), with a label:
/// what a token picker offers.
pub fn all_tokens() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("<<1>>", "Page number"),
        ("<<n>>", "Page count"),
        ("<<Page 1 of n>>", "Page 1 of n"),
        ("<<m/d/yyyy>>", "Date (m/d/yyyy)"),
        ("<<yyyy-mm-dd>>", "Date (yyyy-mm-dd)"),
        ("<<Bates Number#6#1##>>", "Bates number (digits, start, prefix, suffix)"),
    ];
    v.extend_from_slice(FILE_TOKENS);
    v
}

/// A value safe to put where a token was: no `<` or `>` (the page-mark editor would read a
/// `<<...>>` in a file name as another token), no line breaks, at most 500 characters.
fn clean(v: &str) -> String {
    v.chars()
        .map(|c| match c {
            '<' => '(',
            '>' => ')',
            '\r' | '\n' | '\t' => ' ',
            c => c,
        })
        .take(500)
        .collect()
}

/// Replace `token` (ASCII case-insensitive) in `text` with `value`.
fn replace_token(text: &str, token: &str, value: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let t = token.to_ascii_lowercase();
    if t.is_empty() || !lower.contains(&t) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(off) = lower.get(i..).and_then(|s| s.find(&t)) {
        let start = i.saturating_add(off);
        out.push_str(text.get(i..start).unwrap_or_default());
        out.push_str(value);
        i = start.saturating_add(t.len());
    }
    out.push_str(text.get(i..).unwrap_or_default());
    out
}

/// Replace the file-data tokens in `text` from `values` (token, value).
pub fn expand(text: &str, values: &[(&str, String)]) -> String {
    let mut s = text.to_string();
    for (tok, v) in values {
        s = replace_token(&s, tok, &clean(v));
    }
    s
}

impl Session {
    /// The values of the file-data tokens for this document (token, value).
    pub fn file_token_values(&self) -> Vec<(&'static str, String)> {
        let props = self.doc_properties();
        let info = |k: &str| {
            props
                .standard
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let path = self.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        vec![
            ("<<File Name>>", name),
            ("<<File Path>>", path.display().to_string()),
            ("<<Author>>", info("Author")),
            ("<<Title>>", info("Title")),
            ("<<Subject>>", info("Subject")),
            ("<<Keywords>>", info("Keywords")),
            ("<<Creator>>", info("Creator")),
            ("<<Producer>>", info("Producer")),
        ]
    }

    /// `text` with its file-data tokens replaced.
    pub fn expand_file_tokens(&self, text: &str) -> String {
        if !text.contains("<<") {
            return text.to_string();
        }
        expand(text, &self.file_token_values())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_replaced_case_insensitively_and_cleaned() {
        let v = vec![
            ("<<File Name>>", "a<<1>>.pdf".to_string()),
            ("<<Author>>", "Pat".to_string()),
        ];
        assert_eq!(
            expand("<<file name>> by <<Author>> <<1>>", &v),
            "a((1)).pdf by Pat <<1>>"
        );
        assert_eq!(expand("none", &v), "none");
    }
}
