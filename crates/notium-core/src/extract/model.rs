//! Unified extraction model for Markdown notes.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Which editor/format a note belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorKind {
    Markdown,
}

impl EditorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EditorKind::Markdown => "markdown",
        }
    }
}

impl fmt::Display for EditorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EditorKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "markdown" | "md" => Ok(EditorKind::Markdown),
            other => Err(format!("unknown editor kind: {other}")),
        }
    }
}

/// The kind of an outgoing link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    /// `[[Note]]` in Markdown.
    Wikilink,
    /// `![[Note]]` / `![[img.png]]` in Markdown.
    Embed,
    /// `[text](path)` or `![](path)` in Markdown.
    MarkdownLink,
}

impl LinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::Wikilink => "wikilink",
            LinkKind::Embed => "embed",
            LinkKind::MarkdownLink => "markdownlink",
        }
    }
}

impl fmt::Display for LinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single outgoing link as written in the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutLink {
    /// Display text exactly as written (target for wikilinks, text for md links).
    pub text: String,
    pub kind: LinkKind,
    /// Raw target: wikilink target, URL/path, or explicit note id.
    pub target: String,
    /// Whether the target is an attachment (image/embed of a file).
    pub is_attachment: bool,
}

/// A structural chunk for search and the future RAG pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub chunk_id: String,
    /// heading | paragraph | list | code | quote | table | math | diagram | callout | columns | block
    pub kind: String,
    /// Heading lineage as `H1 > H2` (empty at root).
    pub heading_path: String,
    pub body: String,
}

/// Embedded metadata common to both formats (ADR-03).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMeta {
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// Embedded tags (from frontmatter / envelope). Inline tags are merged at extraction time.
    pub tags: Vec<String>,
}

/// The canonical output of a headless extraction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedDocument {
    pub note_id: String,
    /// Best-effort title (first heading). The authoritative title is the filename.
    pub title: String,
    pub editor: EditorKind,
    pub meta: NoteMeta,
    /// Plain text for FTS5 (includes code block text).
    pub plain_text: String,
    /// Union of embedded + inline tags (deduped, sorted).
    pub tags: Vec<String>,
    pub out_links: Vec<OutLink>,
    /// Vault-relative attachment paths, deduped and sorted.
    pub attachment_refs: Vec<String>,
    pub chunks: Vec<Chunk>,
}

impl ExtractedDocument {
    pub fn new(note_id: &str, editor: EditorKind, meta: NoteMeta) -> Self {
        ExtractedDocument {
            note_id: note_id.to_string(),
            title: String::new(),
            editor,
            meta,
            plain_text: String::new(),
            tags: Vec::new(),
            out_links: Vec::new(),
            attachment_refs: Vec::new(),
            chunks: Vec::new(),
        }
    }

    /// Finalize: merge embedded + inline tags, dedupe/sort tags, links, and attachments.
    pub fn finalize(&mut self) {
        let mut tags: Vec<String> = self.meta.tags.clone();
        tags.extend(self.tags.clone());
        tags.sort();
        tags.dedup();
        self.tags = tags;

        self.out_links.sort_by(|a, b| {
            (
                a.text.as_str(),
                a.kind.as_str(),
                a.target.as_str(),
                a.is_attachment,
            )
                .cmp(&(
                    b.text.as_str(),
                    b.kind.as_str(),
                    b.target.as_str(),
                    b.is_attachment,
                ))
        });
        self.out_links.dedup();

        self.attachment_refs.sort();
        self.attachment_refs.dedup();
    }
}

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("invalid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("unsupported or unparseable document: {0}")]
    Unsupported(String),
}

/// Classify a code-fence info string / language into a chunk kind.
pub fn code_chunk_kind(info: &str) -> &'static str {
    let info = info.trim().to_ascii_lowercase();
    if matches!(info.as_str(), "math" | "latex" | "tex" | "katex") {
        "math"
    } else if matches!(info.as_str(), "mermaid" | "nomnoml" | "plantuml" | "d2") {
        "diagram"
    } else {
        "code"
    }
}

/// Scan a body of text for inline `#tags`.
///
/// Ignores `#` inside `[[wikilinks]]` (such as `[[#section]]` or `[[Note#section]]`)
/// and markdown link destinations `[text](#anchor)`.
/// A tag starts at a `#` preceded by start-of-text, whitespace, or `(` / `{`,
/// followed by a letter and continuing over alphanumeric, `_`, `-`, or `/` chars.
pub fn scan_tags(text: &str) -> Vec<String> {
    let cleaned = strip_links_for_tag_scan(text);
    let chars: Vec<(usize, char)> = cleaned.char_indices().collect();
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < chars.len() {
        let (_, ch) = chars[idx];
        if ch == '#' {
            let boundary = idx == 0 || {
                let prev = chars[idx - 1].1;
                prev.is_whitespace() || matches!(prev, '(' | '{' | ';' | ',')
            };
            if boundary && idx + 1 < chars.len() && chars[idx + 1].1.is_alphabetic() {
                let start_byte = chars[idx + 1].0;
                let mut j = idx + 1;
                while j < chars.len() {
                    let c = chars[j].1;
                    if c.is_alphanumeric() || matches!(c, '_' | '-' | '/') {
                        j += 1;
                    } else {
                        break;
                    }
                }
                let end_byte = if j < chars.len() {
                    chars[j].0
                } else {
                    cleaned.len()
                };
                let tag = cleaned[start_byte..end_byte].trim_end_matches(&['/', '-'][..]);
                if !tag.is_empty() {
                    out.push(tag.to_string());
                    idx = j;
                    continue;
                }
            }
        }
        idx += 1;
    }
    out
}

fn strip_links_for_tag_scan(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(close) = text[i + 2..].find("]]") {
                out.push(' ');
                i += 2 + close + 2;
                continue;
            }
        }
        if i + 1 < bytes.len() && bytes[i] == b']' && bytes[i + 1] == b'(' {
            if let Some(close) = text[i + 2..].find(')') {
                out.push(']');
                out.push(' ');
                i += 2 + close + 1;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Detect wikilink-style embeds in a body of text.
///
/// Returns `(is_embed, target, alias)` tuples. `[[A]]` → wikilink; `![[A]]` → embed.
pub fn scan_wikilinks(text: &str) -> Vec<(bool, String, Option<String>)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'[' && b[i + 1] == b'[' {
            let is_embed = i > 0 && b[i - 1] == b'!';
            if let Some(close_rel) = text[i + 2..].find("]]") {
                let inner = &text[i + 2..i + 2 + close_rel];
                let (target, alias) = split_target_alias(inner);
                out.push((is_embed, target, alias));
                i = i + 2 + close_rel + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Split a wikilink interior on `|` into `(target, alias)`.
fn split_target_alias(inner: &str) -> (String, Option<String>) {
    match inner.split_once('|') {
        Some((target, alias)) => (target.trim().to_string(), Some(alias.trim().to_string())),
        None => (inner.trim().to_string(), None),
    }
}

/// Whether a target looks like an attachment (has a file extension in a known set).
pub fn looks_like_attachment(target: &str) -> bool {
    const EXTS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico", "pdf", "mp4", "mp3", "wav",
        "ogg", "webm", "zip", "tar", "gz", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt",
        "csv", "json", "md",
    ];
    let lower = target.to_ascii_lowercase();
    if let Some((_, ext)) = lower.rsplit_once('.') {
        EXTS.contains(&ext)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_tags_boundaries() {
        assert_eq!(scan_tags("#idea y #viaje"), vec!["idea", "viaje"]);
        assert_eq!(scan_tags("(#paren)"), vec!["paren"]);
        assert_eq!(scan_tags("no#fragment"), Vec::<String>::new());
        assert_eq!(scan_tags("https://x.com/a#frag"), Vec::<String>::new());
        assert_eq!(
            scan_tags("Ir a [[#hola]] o [[Otra Nota#seccion]] o [link](#ancla) con #tag_real"),
            vec!["tag_real"]
        );
    }

    #[test]
    fn scan_wikilinks_variants() {
        assert_eq!(
            scan_wikilinks("see [[A]] and [[B|alias]] and ![[C.png]]"),
            vec![
                (false, "A".to_string(), None),
                (false, "B".to_string(), Some("alias".to_string())),
                (true, "C.png".to_string(), None),
            ]
        );
    }
}
