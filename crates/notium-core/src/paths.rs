//! Pure note-path helpers shared by the native vault and the web build (WASM):
//! title sanitizing, folder paths, and the vault layout rules for synced paths.

use std::io;
use std::path::Path;

/// Derive a note title from a vault-relative path (filename without extension).
pub fn title_from_path(rel_path: &str) -> String {
    let name = Path::new(rel_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel_path.to_string());
    name
}

/// Sanitize a note title into a cross-platform-safe filename (no reserved
/// Windows characters, no control chars, no trailing dots/spaces).
pub fn sanitize_title(title: &str) -> String {
    let s: String = title
        .chars()
        .filter(|c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()
        })
        .collect();
    let s = s.trim().trim_end_matches('.').trim_end();
    if s.is_empty() {
        "Sin título".to_string()
    } else {
        s.to_string()
    }
}

/// Sanitize a relative folder path (possibly nested via `/` or `\`), rejecting
/// `.` and `..` path traversal segments with [`io::ErrorKind::InvalidInput`].
pub fn sanitize_folder_path(folder: &str) -> io::Result<String> {
    let normalized = folder.replace('\\', "/");
    let trimmed = normalized.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::new();
    for raw_seg in trimmed.split('/') {
        let seg = raw_seg.trim();
        if seg.is_empty() {
            continue;
        }
        if seg == "." || seg == ".." {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid folder path segment",
            ));
        }
        parts.push(sanitize_title(seg));
    }
    Ok(parts.join("/"))
}

/// Whether `path` is a vault-relative Markdown note path a remote op may write:
/// under `notes/markdown/`, ending in `.md`, with no traversal, backslashes,
/// empty segments or reserved characters in any segment.
pub fn is_safe_note_rel_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("notes/markdown/") else {
        return false;
    };
    if !rest.ends_with(".md") || path.contains('\\') {
        return false;
    }
    rest.split('/')
        .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && sanitize_title(seg) == seg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_note_paths() {
        assert!(is_safe_note_rel_path("notes/markdown/Idea.md"));
        assert!(is_safe_note_rel_path("notes/markdown/Proyectos/Plan 2.md"));
        assert!(!is_safe_note_rel_path("notes/markdown/../x.md"));
        assert!(!is_safe_note_rel_path("notes/markdown//x.md"));
        assert!(!is_safe_note_rel_path("attachments/x.md"));
        assert!(!is_safe_note_rel_path("notes/markdown/x.txt"));
        assert!(!is_safe_note_rel_path("notes/markdown/a:b.md"));
        assert!(!is_safe_note_rel_path("notes/markdown/a\\b.md"));
    }

    #[test]
    fn title_and_sanitize() {
        assert_eq!(title_from_path("notes/markdown/Dir/Mi nota.md"), "Mi nota");
        assert_eq!(sanitize_folder_path(" a//b\\c ").unwrap(), "a/b/c");
        assert!(sanitize_folder_path("a/../b").is_err());
    }
}
