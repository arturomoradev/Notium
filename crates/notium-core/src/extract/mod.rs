//! Headless Markdown extractor (`.md`).

pub mod markdown;
pub mod model;

pub use markdown::MarkdownExtractor;
pub use model::{
    code_chunk_kind, scan_tags, scan_wikilinks, Chunk, EditorKind, ExtractError, ExtractedDocument,
    LinkKind, NoteMeta, OutLink,
};

/// A format-agnostic document extractor.
pub trait DocumentExtractor: Send + Sync {
    fn extract(&self, note_id: &str, bytes: &[u8]) -> Result<ExtractedDocument, ExtractError>;
}

/// Extract a document using the extractor matching `editor`.
pub fn extract(
    editor: EditorKind,
    note_id: &str,
    bytes: &[u8],
) -> Result<ExtractedDocument, ExtractError> {
    match editor {
        EditorKind::Markdown => MarkdownExtractor.extract(note_id, bytes),
    }
}

/// Infer the editor kind from a (vault-relative) path.
pub fn editor_from_path(path: &str) -> Option<EditorKind> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Some(EditorKind::Markdown)
    } else {
        None
    }
}
