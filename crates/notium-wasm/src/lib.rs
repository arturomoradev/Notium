//! WebAssembly bindings for the web build (src/web/core.ts wraps them).
//!
//! Only notium-core's pure modules are exposed, so the browser derives titles,
//! tags, links, hashes and merges exactly like the native app. Structured
//! results cross the boundary as JSON strings to keep the bindings dependency-free.

use notium_core::extract::{self, EditorKind, OutLink};
use notium_core::{envelope, hash, links, paths, sync::merge};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// The parts of `ExtractedDocument` the web client uses.
#[derive(Serialize)]
struct Extracted<'a> {
    title: &'a str,
    tags: &'a [String],
    plain_text: &'a str,
    out_links: &'a [OutLink],
    attachment_refs: &'a [String],
}

/// Extract a note file: JSON `{ title, tags, plain_text, out_links, attachment_refs }`.
#[wasm_bindgen]
pub fn extract_note(note_id: &str, text: &str) -> Result<String, JsError> {
    let doc = extract::extract(EditorKind::Tiptap, note_id, text.as_bytes())
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(serde_json::to_string(&Extracted {
        title: &doc.title,
        tags: &doc.tags,
        plain_text: &doc.plain_text,
        out_links: &doc.out_links,
        attachment_refs: &doc.attachment_refs,
    })?)
}

/// BLAKE3 hex hash, as used for attachment blobs.
#[wasm_bindgen]
pub fn content_hash(bytes: &[u8]) -> String {
    hash::content_hash(bytes)
}

/// The Tiptap doc of a note file, as JSON (an empty doc when there is none).
#[wasm_bindgen]
pub fn doc_json(text: &str) -> String {
    envelope::doc_json(text)
}

/// The note file `previous` with its doc replaced by `doc` (Tiptap JSON).
#[wasm_bindgen]
pub fn with_doc(previous: &str, doc: &str) -> Result<String, JsError> {
    envelope::with_doc(previous, doc).map_err(|e| JsError::new(&e))
}

/// A new, empty note file.
#[wasm_bindgen]
pub fn new_note(id: &str, created: &str) -> String {
    envelope::new_note(id, created, envelope::empty_doc())
}

#[wasm_bindgen]
pub fn inject_notium_id(text: &str, id: &str) -> String {
    envelope::inject_notium_id(text, id)
}

#[wasm_bindgen]
pub fn upsert_field(text: &str, key: &str, val: &str) -> String {
    envelope::upsert_field(text, key, val)
}

#[wasm_bindgen]
pub fn sanitize_title(title: &str) -> String {
    paths::sanitize_title(title)
}

/// Clean a folder path relative to `notes/tiptap`; rejects `.`/`..` segments.
#[wasm_bindgen]
pub fn sanitize_folder_path(folder: &str) -> Result<String, JsError> {
    paths::sanitize_folder_path(folder).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn title_from_path(rel_path: &str) -> String {
    paths::title_from_path(rel_path)
}

#[wasm_bindgen]
pub fn is_safe_note_rel_path(path: &str) -> bool {
    paths::is_safe_note_rel_path(path)
}

#[wasm_bindgen]
pub fn normalize_title(s: &str) -> String {
    links::normalize_title(s)
}

/// JSON `[content, count]`.
#[wasm_bindgen]
pub fn rewrite_note_links(
    content: &str,
    old_title: &str,
    new_title: &str,
) -> Result<String, JsError> {
    Ok(serde_json::to_string(&links::rewrite_note_links(
        content, old_title, new_title,
    ))?)
}

/// JSON `[merged, conflict]`.
#[wasm_bindgen]
pub fn merge_md(base: &str, mine: &str, theirs: &str) -> Result<String, JsError> {
    Ok(serde_json::to_string(&merge::merge_md(base, mine, theirs))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_doc_keeps_prefix_and_body() {
        let doc = "---\nnotium_id: n1\n---\n\n# Hola\n";
        let (prefix, body): (String, String) =
            serde_json::from_str(&split_doc(doc).unwrap()).unwrap();
        assert_eq!(prefix + &body, doc);
        assert_eq!(body, "\n# Hola\n");
    }

    #[test]
    fn extract_reports_title_and_links() {
        let json = extract_markdown("n1", "# Título\n\nVer [[Otra nota]] #tag\n").unwrap();
        assert!(json.contains("\"title\":\"Título\""));
        assert!(json.contains("\"target\":\"Otra nota\""));
        assert!(json.contains("\"tag\""));
    }
}
