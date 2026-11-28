//! Note files: a JSON envelope around a Tiptap document.
//!
//! ```json
//! { "notium_id": "note_…", "created": "…", "updated": "…", "tags": [], "doc": { "type": "doc", "content": [] } }
//! ```
//!
//! The editor only sees `doc`; identity and timestamps live beside it. Files are
//! written pretty-printed so the line-based journal and diffs stay readable.

use serde_json::{json, Map, Value};

fn parse(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str(text) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

fn render(m: Map<String, Value>) -> String {
    let mut s = serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default();
    s.push('\n');
    s
}

fn is_doc(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("doc")
}

/// An empty Tiptap document.
pub fn empty_doc() -> Value {
    json!({ "type": "doc", "content": [] })
}

/// Whether `text` is a note envelope (a JSON object whose `doc` is a Tiptap doc).
pub fn is_note(text: &str) -> bool {
    parse(text).is_some_and(|m| m.get("doc").is_some_and(is_doc))
}

/// A string field of the envelope.
pub fn field(text: &str, key: &str) -> Option<String> {
    parse(text)?.get(key)?.as_str().map(str::to_string)
}

/// The embedded note id, if any.
pub fn notium_id(text: &str) -> Option<String> {
    field(text, "notium_id").filter(|s| !s.trim().is_empty())
}

/// Embedded tags (`"tags": [..]`).
pub fn tags(text: &str) -> Vec<String> {
    parse(text)
        .and_then(|m| m.get("tags").cloned())
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

/// The Tiptap document as compact JSON; an empty doc when there is none.
pub fn doc_json(text: &str) -> String {
    let doc = parse(text)
        .and_then(|mut m| m.remove("doc"))
        .filter(is_doc)
        .unwrap_or_else(empty_doc);
    serde_json::to_string(&doc).unwrap_or_default()
}

/// Set `key` to `val`. Text that is not an envelope is returned unchanged, so a
/// stray file is never overwritten with an empty note.
pub fn upsert_field(text: &str, key: &str, val: &str) -> String {
    let Some(mut m) = parse(text) else {
        return text.to_string();
    };
    m.insert(key.to_string(), Value::String(val.to_string()));
    render(m)
}

/// Give the note `id` when it has none (kept first in the file).
pub fn inject_notium_id(text: &str, id: &str) -> String {
    let Some(m) = parse(text) else {
        return text.to_string();
    };
    if m.get("notium_id")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
    {
        return render(m);
    }
    let mut out = Map::new();
    out.insert("notium_id".into(), Value::String(id.to_string()));
    out.extend(m.into_iter().filter(|(k, _)| k != "notium_id"));
    render(out)
}

/// Replace the document of `previous` (keeping its other fields) with `doc`,
/// which must be a Tiptap doc in JSON. A `previous` that is not an envelope
/// starts a new one.
pub fn with_doc(previous: &str, doc: &str) -> Result<String, String> {
    let doc: Value =
        serde_json::from_str(doc).map_err(|e| format!("invalid document JSON: {e}"))?;
    if !is_doc(&doc) {
        return Err("document is not a Tiptap doc".into());
    }
    let mut m = parse(previous).unwrap_or_default();
    m.insert("doc".into(), doc);
    Ok(render(m))
}

/// A new note file.
pub fn new_note(id: &str, created: &str, doc: Value) -> String {
    let mut m = Map::new();
    m.insert("notium_id".into(), Value::String(id.to_string()));
    m.insert("created".into(), Value::String(created.to_string()));
    m.insert("updated".into(), Value::String(created.to_string()));
    m.insert("doc".into(), doc);
    render(m)
}

/// A doc from plain text: `#`-prefixed lines become headings, other non-empty
/// lines paragraphs. Used for generated notes (conflict copies, tests, demos).
pub fn doc_from_text(text: &str) -> Value {
    let content: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            let rest = &line[hashes..];
            if (1..=6).contains(&hashes) && rest.starts_with(' ') {
                json!({ "type": "heading", "attrs": { "level": hashes },
                        "content": [{ "type": "text", "text": rest.trim() }] })
            } else {
                json!({ "type": "paragraph", "content": [{ "type": "text", "text": line }] })
            }
        })
        .collect();
    json!({ "type": "doc", "content": content })
}

/// A note file from text: an optional `---` block of `key: value` lines becomes
/// envelope fields, the rest goes through [`doc_from_text`]. For tests and
/// generated notes; text that already is an envelope is returned as is.
pub fn note_from_text(text: &str) -> String {
    if parse(text).is_some() {
        return text.to_string();
    }
    let mut m = Map::new();
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest
            .find("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(str::len))
        {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    m.insert(k.trim().into(), Value::String(v.trim().into()));
                }
            }
            body = rest.get(end + 5..).unwrap_or("");
        }
    }
    m.insert("doc".into(), doc_from_text(body));
    render(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_fields_round_trip() {
        let note = new_note("note_a", "2026-01-01", doc_from_text("# T\nhola"));
        assert!(is_note(&note));
        assert_eq!(notium_id(&note).as_deref(), Some("note_a"));
        let note = upsert_field(&note, "updated", "2026-02-02");
        assert_eq!(field(&note, "updated").as_deref(), Some("2026-02-02"));
        assert!(note.starts_with("{\n  \"notium_id\""));
    }

    #[test]
    fn inject_keeps_existing_id_and_puts_new_one_first() {
        let note = with_doc("", r#"{"type":"doc","content":[]}"#).unwrap();
        let injected = inject_notium_id(&note, "note_b");
        assert!(injected.starts_with("{\n  \"notium_id\": \"note_b\""));
        assert_eq!(
            notium_id(&inject_notium_id(&injected, "note_c")).as_deref(),
            Some("note_b")
        );
    }

    #[test]
    fn with_doc_keeps_header_and_rejects_bad_docs() {
        let note = new_note("note_a", "c", empty_doc());
        let next = with_doc(&note, r#"{"type":"doc","content":[{"type":"paragraph"}]}"#).unwrap();
        assert_eq!(notium_id(&next).as_deref(), Some("note_a"));
        assert!(doc_json(&next).contains("paragraph"));
        assert!(with_doc(&note, "not json").is_err());
        assert!(with_doc(&note, r#"{"type":"paragraph"}"#).is_err());
    }

    #[test]
    fn note_from_text_reads_the_header() {
        let note = note_from_text("---\nnotium_id: note_x\nupdated: 5\n---\n# A\n\nSee [[B]].\n");
        assert_eq!(notium_id(&note).as_deref(), Some("note_x"));
        assert_eq!(field(&note, "updated").as_deref(), Some("5"));
        assert!(doc_json(&note).contains("See [[B]]."));
        assert_eq!(note_from_text(&note), note);
    }

    #[test]
    fn non_envelopes_are_left_alone() {
        assert_eq!(upsert_field("hello", "updated", "x"), "hello");
        assert_eq!(inject_notium_id("[1,2]", "id"), "[1,2]");
        assert!(!is_note(r#"{"name":"package"}"#));
        assert_eq!(doc_json("garbage"), r#"{"type":"doc","content":[]}"#);
    }
}
