//! Markdown (`.md`) extractor via `pulldown-cmark`.
//!
//! Extracts plain text, `#tags`, `[[wikilinks]]`, `![[embeds]]`, markdown link/
//! image destinations, attachment references, and deterministic structural chunks.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::model::{
    code_chunk_kind, looks_like_attachment, scan_tags, scan_wikilinks, Chunk, EditorKind,
    ExtractError, ExtractedDocument, LinkKind, NoteMeta, OutLink,
};
use crate::{frontmatter, hash};

/// Headless Markdown extractor.
pub struct MarkdownExtractor;

impl super::DocumentExtractor for MarkdownExtractor {
    fn extract(&self, note_id: &str, bytes: &[u8]) -> Result<ExtractedDocument, ExtractError> {
        let text = std::str::from_utf8(bytes)?;
        let (fm, body) = frontmatter::split(text);
        let meta = build_meta(fm.as_deref());
        let mut doc = ExtractedDocument::new(note_id, EditorKind::Markdown, meta);

        let opts = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES;
        let parser = Parser::new_ext(body, opts);

        let mut st = State::new(note_id);
        for event in parser {
            st.feed(event);
        }
        st.flush_root();

        doc.title = st.title.clone();
        doc.plain_text = st.plain_parts.join("\n");
        doc.tags = st.tags.clone();
        doc.out_links = st.links.clone();
        doc.attachment_refs = st.attachments.clone();
        doc.chunks = st.chunks.clone();
        doc.finalize();
        Ok(doc)
    }
}

fn build_meta(fm: Option<&str>) -> NoteMeta {
    match fm {
        Some(f) => NoteMeta {
            created_at: frontmatter::field(f, "created"),
            updated_at: frontmatter::field(f, "updated"),
            tags: frontmatter::parse_tags(f),
        },
        None => NoteMeta::default(),
    }
}

fn heading_level(l: HeadingLevel) -> u32 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn code_info(kind: &CodeBlockKind) -> String {
    match kind {
        CodeBlockKind::Fenced(info) => info.to_string(),
        CodeBlockKind::Indented => String::new(),
    }
}

struct Frame {
    kind: &'static str,
    text: String,
    /// Scannable text (excludes inline code and code-block content) for tag/link detection.
    scan_text: String,
    level: Option<u32>,
    code_info: Option<String>,
}

struct State<'a> {
    note_id: &'a str,
    heading_path: Vec<String>,
    stack: Vec<Frame>,
    root: String,
    root_scan: String,
    title: String,
    plain_parts: Vec<String>,
    tags: Vec<String>,
    links: Vec<OutLink>,
    attachments: Vec<String>,
    chunks: Vec<Chunk>,
}

impl<'a> State<'a> {
    fn new(note_id: &'a str) -> Self {
        State {
            note_id,
            heading_path: Vec::new(),
            stack: Vec::new(),
            root: String::new(),
            root_scan: String::new(),
            title: String::new(),
            plain_parts: Vec::new(),
            tags: Vec::new(),
            links: Vec::new(),
            attachments: Vec::new(),
            chunks: Vec::new(),
        }
    }

    fn push(&mut self, kind: &'static str, level: Option<u32>, code_info: Option<String>) {
        self.stack.push(Frame {
            kind,
            text: String::new(),
            scan_text: String::new(),
            level,
            code_info,
        });
    }

    fn append_text(&mut self, s: &str) {
        if let Some(top) = self.stack.last_mut() {
            top.text.push_str(s);
        } else {
            self.root.push_str(s);
        }
    }

    fn in_code(&self) -> bool {
        self.stack.last().map(|f| f.kind == "code").unwrap_or(false)
    }

    fn feed(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.on_start(tag),
            Event::End(end) => self.on_end(end),
            Event::Text(t) => {
                let t = t.as_ref();
                if !self.in_code() {
                    if let Some(top) = self.stack.last_mut() {
                        top.scan_text.push_str(t);
                    } else {
                        self.root_scan.push_str(t);
                    }
                }
                self.append_text(t);
            }
            Event::Code(c) => self.append_text(c.as_ref()),
            Event::SoftBreak => self.append_text(" "),
            Event::HardBreak => self.append_text("\n"),
            Event::InlineMath(m) | Event::DisplayMath(m) => self.append_text(m.as_ref()),
            _ => {}
        }
    }

    fn scan_block(&mut self, scan_text: &str) {
        for tag in scan_tags(scan_text) {
            self.tags.push(tag);
        }
        for (is_embed, target, alias) in scan_wikilinks(scan_text) {
            self.push_wikilink(is_embed, &target, alias.as_deref());
        }
    }

    fn push_wikilink(&mut self, is_embed: bool, target: &str, alias: Option<&str>) {
        if is_embed {
            let is_att = looks_like_attachment(target);
            let text = target.to_string();
            self.links.push(OutLink {
                text: text.clone(),
                kind: LinkKind::Embed,
                target: text.clone(),
                is_attachment: is_att,
            });
            if is_att {
                self.attachments.push(text);
            }
        } else {
            let text = alias
                .map(|a| a.to_string())
                .unwrap_or_else(|| target.to_string());
            self.links.push(OutLink {
                text,
                kind: LinkKind::Wikilink,
                target: target.to_string(),
                is_attachment: false,
            });
        }
    }

    fn on_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => self.push("heading", Some(heading_level(level)), None),
            Tag::CodeBlock(info) => self.push("code", None, Some(code_info(&info))),
            Tag::List(_) => self.push("list", None, None),
            Tag::BlockQuote(_) => self.push("quote", None, None),
            Tag::Table(_) => self.push("table", None, None),
            Tag::Paragraph => {
                if self.stack.is_empty() {
                    self.push("paragraph", None, None);
                }
            }
            Tag::Link { dest_url, .. } => self.on_link(dest_url.as_ref(), false),
            Tag::Image { dest_url, .. } => self.on_link(dest_url.as_ref(), true),
            _ => {}
        }
    }

    fn on_link(&mut self, dest: &str, is_image: bool) {
        let d = dest.to_string();
        if d.starts_with("http://") || d.starts_with("https://") || d.starts_with("mailto:") {
            return;
        }
        let is_att = d.starts_with("attachments/") || looks_like_attachment(&d);
        if is_image {
            if is_att {
                self.attachments.push(d);
            }
        } else {
            self.links.push(OutLink {
                text: d.clone(),
                kind: LinkKind::MarkdownLink,
                target: d.clone(),
                is_attachment: is_att,
            });
            if is_att {
                self.attachments.push(d);
            }
        }
    }

    fn on_end(&mut self, end: TagEnd) {
        let expect = match end {
            TagEnd::Paragraph => Some("paragraph"),
            TagEnd::Heading(_) => Some("heading"),
            TagEnd::CodeBlock => Some("code"),
            TagEnd::List(_) => Some("list"),
            TagEnd::BlockQuote(_) => Some("quote"),
            TagEnd::Table => Some("table"),
            _ => None,
        };
        if let Some(exp) = expect {
            if self.stack.last().map(|f| f.kind) == Some(exp) {
                let frame = self.stack.pop().unwrap();
                self.finalize_frame(frame);
            }
        }
    }

    fn finalize_frame(&mut self, frame: Frame) {
        self.scan_block(&frame.scan_text);
        let body = frame.text.trim().to_string();
        if body.is_empty() {
            return;
        }
        let hp;
        match frame.kind {
            "heading" => {
                let level = frame.level.unwrap_or(1) as usize;
                let title = body.clone();
                self.heading_path.truncate(level.saturating_sub(1));
                self.heading_path.push(title.clone());
                if self.title.is_empty() {
                    self.title = title;
                }
                hp = self.heading_path.join(" > ");
                self.push_chunk("heading", &hp, &body);
            }
            "code" => {
                let info = frame.code_info.unwrap_or_default();
                let kind = code_chunk_kind(&info);
                hp = self.heading_path.join(" > ");
                self.push_chunk(kind, &hp, &body);
            }
            other => {
                hp = self.heading_path.join(" > ");
                self.push_chunk(other, &hp, &body);
            }
        }
    }

    fn push_chunk(&mut self, kind: &str, heading_path: &str, body: &str) {
        let cid = hash::chunk_id(self.note_id, kind, heading_path, body);
        self.plain_parts.push(body.to_string());
        self.chunks.push(Chunk {
            chunk_id: cid,
            kind: kind.to_string(),
            heading_path: heading_path.to_string(),
            body: body.to_string(),
        });
    }

    fn flush_root(&mut self) {
        let root_scan = std::mem::take(&mut self.root_scan);
        self.scan_block(&root_scan);
        let root = std::mem::take(&mut self.root);
        let root = root.trim().to_string();
        if !root.is_empty() {
            let hp = self.heading_path.join(" > ");
            self.push_chunk("paragraph", &hp, &root);
        }
    }
}
