//! Extractor for note files (Tiptap JSON inside the `envelope`).
//!
//! Extracts plain text, `#tags`, `[[wikilinks]]`, `![[embeds]]`, link-mark
//! destinations, attachment references and one structural chunk per top-level block.

use serde_json::Value;

use super::model::{
    code_chunk_kind, looks_like_attachment, scan_tags, scan_wikilinks, Chunk, EditorKind,
    ExtractError, ExtractedDocument, LinkKind, NoteMeta, OutLink,
};
use crate::hash;

/// Headless extractor for `.json` notes.
pub struct TiptapExtractor;

impl super::DocumentExtractor for TiptapExtractor {
    fn extract(&self, note_id: &str, bytes: &[u8]) -> Result<ExtractedDocument, ExtractError> {
        let text = std::str::from_utf8(bytes)?;
        let file: Value = serde_json::from_str(text)
            .map_err(|e| ExtractError::Unsupported(format!("invalid JSON: {e}")))?;
        let doc = file
            .get("doc")
            .filter(|d| d.get("type").and_then(Value::as_str) == Some("doc"))
            .ok_or_else(|| ExtractError::Unsupported("not a Notium note (no `doc`)".into()))?;

        let str_field = |k: &str| file.get(k).and_then(Value::as_str).map(str::to_string);
        let meta = NoteMeta {
            created_at: str_field("created"),
            updated_at: str_field("updated"),
            tags: file
                .get("tags")
                .and_then(|t| serde_json::from_value(t.clone()).ok())
                .unwrap_or_default(),
        };

        let mut out = ExtractedDocument::new(note_id, EditorKind::Tiptap, meta);
        let mut st = State {
            note_id,
            heading_path: Vec::new(),
            plain_parts: Vec::new(),
            doc: &mut out,
        };
        for block in children(doc) {
            st.block(block);
        }
        let plain = st.plain_parts.join("\n");
        out.plain_text = plain;
        out.finalize();
        Ok(out)
    }
}

fn children(node: &Value) -> &[Value] {
    node.get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn node_type(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

fn attr<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    node.get("attrs")?.get(key)
}

struct State<'a> {
    note_id: &'a str,
    heading_path: Vec<String>,
    plain_parts: Vec<String>,
    doc: &'a mut ExtractedDocument,
}

impl State<'_> {
    fn block(&mut self, node: &Value) {
        let kind = match node_type(node) {
            "heading" => "heading",
            "codeBlock" => {
                code_chunk_kind(attr(node, "language").and_then(Value::as_str).unwrap_or(""))
            }
            "bulletList" | "orderedList" | "taskList" => "list",
            "blockquote" => "quote",
            "table" => "table",
            "paragraph" => "paragraph",
            "blockMath" => "math",
            "callout" => "callout",
            "horizontalRule" => return,
            _ => "block",
        };
        let in_code = node_type(node) == "codeBlock";
        let (mut text, mut scan) = (String::new(), String::new());
        self.collect(node, in_code, &mut text, &mut scan);

        for tag in scan_tags(&scan) {
            self.doc.tags.push(tag);
        }
        for (is_embed, target, alias) in scan_wikilinks(&scan) {
            self.wikilink(is_embed, &target, alias.as_deref());
        }

        let body = text.trim().to_string();
        if body.is_empty() {
            return;
        }
        if kind == "heading" {
            let level = attr(node, "level").and_then(Value::as_u64).unwrap_or(1) as usize;
            self.heading_path.truncate(level.saturating_sub(1));
            self.heading_path.push(body.clone());
            if self.doc.title.is_empty() {
                self.doc.title = body.clone();
            }
        }
        let hp = self.heading_path.join(" > ");
        let chunk_id = hash::chunk_id(self.note_id, kind, &hp, &body);
        self.plain_parts.push(body.clone());
        self.doc.chunks.push(Chunk {
            chunk_id,
            kind: kind.to_string(),
            heading_path: hp,
            body,
        });
    }

    /// Text of `node` into `text`; the part outside code into `scan` (tags, wikilinks).
    fn collect(&mut self, node: &Value, in_code: bool, text: &mut String, scan: &mut String) {
        match node_type(node) {
            "text" => {
                let t = node.get("text").and_then(Value::as_str).unwrap_or("");
                text.push_str(t);
                let marks = node.get("marks").and_then(Value::as_array);
                let code_mark = marks.is_some_and(|ms| ms.iter().any(|m| node_type(m) == "code"));
                if !in_code && !code_mark {
                    scan.push_str(t);
                }
                for m in marks.into_iter().flatten() {
                    if node_type(m) == "link" {
                        if let Some(href) = attr(m, "href").and_then(Value::as_str) {
                            self.link(href);
                        }
                    }
                }
            }
            "wikiLink" => {
                let target = attr(node, "target").and_then(Value::as_str).unwrap_or("");
                let embed = attr(node, "embed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let raw = format!("{}[[{target}]]", if embed { "!" } else { "" });
                text.push_str(&raw);
                scan.push_str(&raw);
            }
            // LaTeX is searchable text but never holds tags or links.
            "inlineMath" | "blockMath" => {
                text.push_str(attr(node, "latex").and_then(Value::as_str).unwrap_or(""));
            }
            "hardBreak" => {
                text.push('\n');
                scan.push('\n');
            }
            _ => {
                // A callout's title is part of its text (searchable, may hold tags/links).
                if let Some(title) = attr(node, "title").and_then(Value::as_str) {
                    if node_type(node) == "callout" && !title.is_empty() {
                        text.push_str(title);
                        text.push('\n');
                        scan.push_str(title);
                        scan.push('\n');
                    }
                }
                let kids = children(node);
                let inline = kids.first().is_some_and(|k| {
                    matches!(
                        node_type(k),
                        "text" | "wikiLink" | "hardBreak" | "inlineMath"
                    )
                });
                for (i, kid) in kids.iter().enumerate() {
                    if i > 0 && !inline {
                        text.push('\n');
                        scan.push('\n');
                    }
                    self.collect(kid, in_code, text, scan);
                }
            }
        }
    }

    fn wikilink(&mut self, is_embed: bool, target: &str, alias: Option<&str>) {
        if is_embed {
            let is_att = looks_like_attachment(target);
            self.doc.out_links.push(OutLink {
                text: target.to_string(),
                kind: LinkKind::Embed,
                target: target.to_string(),
                is_attachment: is_att,
            });
            if is_att {
                self.doc.attachment_refs.push(target.to_string());
            }
        } else {
            self.doc.out_links.push(OutLink {
                text: alias.unwrap_or(target).to_string(),
                kind: LinkKind::Wikilink,
                target: target.to_string(),
                is_attachment: false,
            });
        }
    }

    /// A link mark: external URLs are ignored; vault paths become links/attachments.
    fn link(&mut self, href: &str) {
        if href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("mailto:")
        {
            return;
        }
        let is_att = href.starts_with("attachments/") || looks_like_attachment(href);
        self.doc.out_links.push(OutLink {
            text: href.to_string(),
            kind: LinkKind::MarkdownLink,
            target: href.to_string(),
            is_attachment: is_att,
        });
        if is_att {
            self.doc.attachment_refs.push(href.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::extract::{extract, EditorKind};

    #[test]
    fn math_is_searchable_but_never_tags() {
        let file = r##"{"doc":{"type":"doc","content":[
            {"type":"paragraph","content":[{"type":"text","text":"Euler "},
              {"type":"inlineMath","attrs":{"latex":"e^{i\\pi}+1=0 #notag"}}]},
            {"type":"blockMath","attrs":{"latex":"\\sum_i i"}}]}}"##;
        let doc = extract(EditorKind::Tiptap, "n", file.as_bytes()).unwrap();
        assert!(doc.plain_text.contains(r"Euler e^{i\pi}+1=0"));
        assert!(doc.tags.is_empty());
        let kinds: Vec<&str> = doc.chunks.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["paragraph", "math"]);
    }

    #[test]
    fn callout_title_and_body_are_indexed() {
        let file = r##"{"doc":{"type":"doc","content":[
            {"type":"callout","attrs":{"type":"tip","title":"Ver [[Plan]]","fold":""},
             "content":[{"type":"paragraph","content":[{"type":"text","text":"con #idea"}]}]}]}}"##;
        let doc = extract(EditorKind::Tiptap, "n", file.as_bytes()).unwrap();
        assert_eq!(doc.chunks[0].kind, "callout");
        assert_eq!(doc.chunks[0].body, "Ver [[Plan]]\ncon #idea");
        assert_eq!(doc.tags, ["idea"]);
        assert_eq!(doc.out_links[0].target, "Plan");
    }
}
