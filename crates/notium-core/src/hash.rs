//! BLAKE3 content hashing and deterministic chunk-id derivation.

/// Lowercase hex encoding of bytes.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Full BLAKE3 hash of a file's bytes, hex-encoded (64 chars).
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Deterministic chunk id: `hex( blake3(note_id ‖ kind ‖ heading_path ‖ body)[..16] )`.
///
/// Stable across identical extractions and unaffected by unrelated edits elsewhere
/// in the note. Used for RAG cache invalidation and citation anchors.
pub fn chunk_id(note_id: &str, kind: &str, heading_path: &str, body: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(note_id.as_bytes());
    hasher.update(&[0u8]);
    hasher.update(kind.as_bytes());
    hasher.update(&[0u8]);
    hasher.update(heading_path.as_bytes());
    hasher.update(&[0u8]);
    hasher.update(body.as_bytes());
    let h = hasher.finalize();
    hex(&h.as_bytes()[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_hex_and_stable() {
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_eq!(content_hash(b"hello").len(), 64);
        assert_ne!(content_hash(b"hello"), content_hash(b"world"));
    }

    #[test]
    fn chunk_id_is_deterministic() {
        let a = chunk_id("note_1", "paragraph", "H1", "body");
        let b = chunk_id("note_1", "paragraph", "H1", "body");
        assert_eq!(a, b);
        assert_ne!(a, chunk_id("note_1", "paragraph", "H1", "body!"));
        assert_eq!(a.len(), 32);
    }
}
