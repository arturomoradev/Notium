//! Surgical YAML-frontmatter utilities for `.md` notes.
//!
//! The design rule (ADR-03/04): we never re-serialize the frontmatter. Identity
//! injection is a single appended line, preserving user comments and formatting
//! byte-for-byte. Parsing is minimal and tolerant of arbitrary user keys.

/// Detect the dominant newline style of a file.
pub fn newline_of(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Split a Markdown document into `(frontmatter_inner, body)`.
///
/// `frontmatter_inner` is `None` when there is no valid opening `---` line.
/// A valid block starts with `---` + newline and ends with a line equal to
/// `---` or `...`. The returned `body` begins after the closing delimiter line.
pub fn split(text: &str) -> (Option<String>, &str) {
    let nl = newline_of(text);
    if !text.starts_with("---") {
        return (None, text);
    }
    let after_open = match text.strip_prefix("---").and_then(|s| s.strip_prefix(nl)) {
        Some(s) => s,
        None => return (None, text),
    };
    let mut idx = 0;
    while idx < after_open.len() {
        let nl_pos = match after_open[idx..].find('\n') {
            Some(p) => idx + p,
            None => return (None, text),
        };
        let line = &after_open[idx..nl_pos];
        let trimmed = line.trim_end_matches('\r');
        if trimmed == "---" || trimmed == "..." {
            let inner = after_open[..idx].to_string();
            let body = &after_open[nl_pos + 1..];
            return (Some(inner), body);
        }
        idx = nl_pos + 1;
    }
    (None, text)
}

/// Whether the document has a frontmatter block.
pub fn has_frontmatter(text: &str) -> bool {
    split(text).0.is_some()
}

/// Read a single-line scalar field `key: value` from frontmatter (value unquoted).
pub fn field(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix(':') {
                let val = rest.trim();
                if !val.is_empty() {
                    return Some(val.trim_matches(|c| c == '\'' || c == '"').to_string());
                }
            }
        }
    }
    None
}

/// Extract the embedded `notium_id`, if present.
pub fn notium_id(fm: &str) -> Option<String> {
    field(fm, "notium_id")
}

/// Parse `tags` from frontmatter, supporting `tags: [a, b]`, `tags: a, b`, and
/// a YAML block list `tags:\n  - a\n  - b`.
pub fn parse_tags(fm: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_block_list = false;
    for line in fm.lines() {
        let line = line.trim_end();
        if in_block_list {
            let t = line.trim_start();
            if let Some(item) = t.strip_prefix('-') {
                let item = item
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string();
                if !item.is_empty() {
                    out.push(item);
                }
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_block_list = false;
            } else {
                continue;
            }
        }
        if let Some(rest) = line.strip_prefix("tags:") {
            let val = rest.trim();
            if val.is_empty() {
                in_block_list = true;
            } else if let Some(bracketed) = val.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            {
                for item in bracketed.split(',') {
                    let item = item
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"')
                        .to_string();
                    if !item.is_empty() {
                        out.push(item);
                    }
                }
            } else {
                for item in val.split(',') {
                    let item = item
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"')
                        .to_string();
                    if !item.is_empty() {
                        out.push(item);
                    }
                }
            }
        }
    }
    out
}

/// Inject a `notium_id` line into the frontmatter, preserving all other bytes.
///
/// - If `notium_id` is already present, returns the original text unchanged.
/// - If a frontmatter block exists, inserts a single line right after the opening `---`.
/// - If no block exists, prepends a minimal `---`/`---` block.
pub fn inject_notium_id(text: &str, id: &str) -> String {
    let nl = newline_of(text);
    let line = format!("notium_id: {id}");
    match split(text).0 {
        Some(fm) => {
            if notium_id(&fm).is_some() {
                return text.to_string();
            }
            let insert_at = 3 + nl.len(); // right after "---" + newline
            let mut s = String::with_capacity(text.len() + line.len() + nl.len());
            s.push_str(&text[..insert_at]);
            s.push_str(&line);
            s.push_str(nl);
            s.push_str(&text[insert_at..]);
            s
        }
        None => format!("---{nl}{line}{nl}---{nl}{text}"),
    }
}

/// Surgically set or update a scalar `key: value` field in the frontmatter block,
/// preserving all comments and unrelated keys byte-for-byte.
pub fn upsert_field(text: &str, key: &str, val: &str) -> String {
    let nl = newline_of(text);
    let new_line = format!("{key}: {val}");
    match split(text) {
        (Some(fm), body) => {
            let mut new_fm = String::with_capacity(fm.len() + new_line.len() + nl.len());
            let mut replaced = false;
            let mut idx = 0;
            while idx < fm.len() {
                let nl_pos = fm[idx..]
                    .find('\n')
                    .map(|p| idx + p + 1)
                    .unwrap_or(fm.len());
                let raw_line = &fm[idx..nl_pos];
                let trimmed = raw_line.trim_end_matches(&['\r', '\n'][..]);
                let is_match = !replaced
                    && trimmed
                        .strip_prefix(key)
                        .map(|r| r.trim_start().starts_with(':'))
                        .unwrap_or(false);
                if is_match {
                    let line_nl = if raw_line.ends_with("\r\n") {
                        "\r\n"
                    } else if raw_line.ends_with('\n') {
                        "\n"
                    } else {
                        nl
                    };
                    new_fm.push_str(&new_line);
                    new_fm.push_str(line_nl);
                    replaced = true;
                } else {
                    new_fm.push_str(raw_line);
                }
                idx = nl_pos;
            }
            if !replaced {
                if !new_fm.is_empty() && !new_fm.ends_with('\n') {
                    new_fm.push_str(nl);
                }
                new_fm.push_str(&new_line);
                new_fm.push_str(nl);
            }
            let close_start = 3 + nl.len() + fm.len();
            let body_start = text.len() - body.len();
            let closing_delim = &text[close_start..body_start];
            format!("---{nl}{new_fm}{closing_delim}{body}")
        }
        (None, _) => format!("---{nl}{new_line}{nl}---{nl}{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_detects_and_returns_inner() {
        let text = "---\ntitle: A\n# a comment\ntags: [x, y]\n---\n\n# Body\n";
        let (fm, body) = split(text);
        let fm = fm.unwrap();
        assert_eq!(fm, "title: A\n# a comment\ntags: [x, y]\n");
        assert_eq!(body, "\n# Body\n");
    }

    #[test]
    fn split_none_without_fm() {
        let text = "# Just a note\n";
        assert!(split(text).0.is_none());
    }

    #[test]
    fn inject_preserves_comments_and_unknown_keys() {
        let text = "---\ntitle: Idea\nsome_user_key: keep me\n# comment\ntags: [a]\n---\nbody";
        let out = inject_notium_id(text, "note_123");
        assert!(out.contains("notium_id: note_123"));
        assert!(out.contains("some_user_key: keep me"));
        assert!(out.contains("# comment"));
        assert!(out.ends_with("body"));
        // idempotent
        let again = inject_notium_id(&out, "note_123");
        assert_eq!(again, out);
    }

    #[test]
    fn inject_creates_block_when_missing() {
        let text = "# No frontmatter\nbody";
        let out = inject_notium_id(text, "note_9");
        assert!(out.starts_with("---\nnotium_id: note_9\n---\n"));
        assert!(out.ends_with("# No frontmatter\nbody"));
    }

    #[test]
    fn parse_tags_forms() {
        assert_eq!(parse_tags("tags: [a, b]"), vec!["a", "b"]);
        assert_eq!(parse_tags("tags: a, b"), vec!["a", "b"]);
        assert_eq!(parse_tags("tags:\n  - a\n  - b"), vec!["a", "b"]);
    }

    #[test]
    fn upsert_field_updates_in_place_or_appends_preserving_comments() {
        let text = "---\nnotium_id: note_1\n# comment\ncreated: 100\n---\n\n# Body\n";
        let with_updated = upsert_field(text, "updated", "200");
        assert!(with_updated.contains("# comment"));
        assert_eq!(
            field(&split(&with_updated).0.unwrap(), "updated").as_deref(),
            Some("200")
        );

        let updated_again = upsert_field(&with_updated, "updated", "300");
        assert!(updated_again.contains("# comment"));
        assert!(!updated_again.contains("updated: 200"));
        assert_eq!(
            field(&split(&updated_again).0.unwrap(), "updated").as_deref(),
            Some("300")
        );
        assert_eq!(split(&updated_again).1, "\n# Body\n");
    }
}
