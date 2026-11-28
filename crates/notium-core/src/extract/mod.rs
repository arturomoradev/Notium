//! Headless extractor for note files (Tiptap JSON, `.json`).

pub mod model;
pub mod tiptap;

pub use model::{
    code_chunk_kind, scan_tags, scan_wikilinks, Chunk, EditorKind, ExtractError, ExtractedDocument,
    LinkKind, NoteMeta, OutLink,
};
pub use tiptap::TiptapExtractor;

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
        EditorKind::Tiptap => TiptapExtractor.extract(note_id, bytes),
    }
}

/// Infer the editor kind from a (vault-relative) path.
pub fn editor_from_path(path: &str) -> Option<EditorKind> {
    path.to_ascii_lowercase()
        .ends_with(".json")
        .then_some(EditorKind::Tiptap)
}
