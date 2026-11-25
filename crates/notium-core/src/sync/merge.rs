//! Merge primitives: line-level 3-way merge for `.md` and conflict-copy helpers.

use std::path::Path;

/// A single diff edit over `base`: base[start..end] is replaced by `lines`.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub lines: Vec<String>,
}

fn split_lines(s: &str) -> Vec<String> {
    s.split('\n').map(|l| l.to_string()).collect()
}

fn join_lines(v: &[String]) -> String {
    v.join("\n")
}

/// Line-level LCS diff of `base` -> `x`, producing run-granularity edits.
pub fn diff_lines(base: &[String], x: &[String]) -> Vec<Edit> {
    let n = base.len();
    let m = x.len();
    // Guard against quadratic blowup: fall back to a single replace edit.
    if (n + 1) as u128 * (m + 1) as u128 > 4_000_000 {
        return vec![Edit {
            start: 0,
            end: n,
            lines: x.to_vec(),
        }];
    }
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if base[i] == x[j] {
                dp[(i + 1) * w + (j + 1)] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + (j + 1)])
            };
        }
    }
    let mut edits = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if base[i] == x[j] {
            i += 1;
            j += 1;
            continue;
        }
        let start = i;
        let mut repl = Vec::new();
        loop {
            if i < n && j < m && base[i] == x[j] {
                break;
            }
            if i < n && (j >= m || dp[(i + 1) * w + j] >= dp[i * w + (j + 1)]) {
                i += 1; // delete base[i]
            } else if j < m {
                repl.push(x[j].clone());
                j += 1;
            } else {
                break; // both sequences exhausted
            }
        }
        edits.push(Edit {
            start,
            end: i,
            lines: repl,
        });
    }
    while i < n {
        edits.push(Edit {
            start: i,
            end: i + 1,
            lines: Vec::new(),
        });
        i += 1;
    }
    while j < m {
        edits.push(Edit {
            start: n,
            end: n,
            lines: vec![x[j].clone()],
        });
        j += 1;
    }
    edits
}

/// Closed-interval overlap between two edit lists (inserts are points).
fn edits_overlap(a: &[Edit], b: &[Edit]) -> bool {
    for x in a {
        for y in b {
            if x.start.max(y.start) <= x.end.min(y.end) {
                return true;
            }
        }
    }
    false
}

fn apply_edits(base: &[String], edits: &[Edit]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    for e in edits {
        if e.start > i {
            out.extend_from_slice(&base[i..e.start]);
        }
        out.extend(e.lines.iter().cloned());
        i = i.max(e.end);
    }
    if i < base.len() {
        out.extend_from_slice(&base[i..]);
    }
    out
}

/// Three-way line merge for Markdown.
///
/// Returns `(merged, conflict)`. Disjoint edits auto-merge; any overlap produces a
/// conservative whole-file conflict with git-style markers (both sides preserved
/// verbatim). Identical concurrent edits are a clean no-op.
pub fn merge_md(base: &str, mine: &str, theirs: &str) -> (String, bool) {
    if mine == theirs {
        return (mine.to_string(), false);
    }
    let b = split_lines(base);
    let m = split_lines(mine);
    let t = split_lines(theirs);
    let dm = diff_lines(&b, &m);
    let dt = diff_lines(&b, &t);
    if !edits_overlap(&dm, &dt) {
        let mut all = dm;
        all.extend(dt);
        all.sort_by_key(|e| (e.start, e.end));
        let out = apply_edits(&b, &all);
        (join_lines(&out), false)
    } else {
        let mut out = String::from("<<<<<<< mine\n");
        out.push_str(mine);
        if !mine.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("=======\n");
        out.push_str(theirs);
        if !theirs.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(">>>>>>> theirs\n");
        (out, true)
    }
}

/// Build a conflict-copy file name: `<stem> (conflict <device> <ts>).<ext>`.
pub fn conflict_copy_name(path: &str, device: &str, ts_millis: u64) -> String {
    let p = Path::new(path);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "note".to_string());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    format!("{stem} (conflict {device} {ts_millis}){ext}")
}

/// Vault-relative path of the conflict copy (same directory as the note).
pub fn conflict_copy_path(path: &str, device: &str, ts_millis: u64) -> String {
    let name = conflict_copy_name(path, device, ts_millis);
    match Path::new(path).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            format!("{}/{}", parent.to_string_lossy().replace('\\', "/"), name)
        }
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_insert_delete_replace() {
        let b = vec!["a".into(), "b".into(), "c".into()];
        // insert "x" before "b"
        let x = vec!["a".into(), "x".into(), "b".into(), "c".into()];
        let edits = diff_lines(&b, &x);
        assert_eq!(
            edits,
            vec![Edit {
                start: 1,
                end: 1,
                lines: vec!["x".into()]
            }]
        );
    }

    #[test]
    fn merge_disjoint_is_clean() {
        let base = "line1\nline2\nline3\n";
        let mine = "MINE\nline2\nline3\n";
        let theirs = "line1\nline2\nTHEIRS\n";
        let (out, conflict) = merge_md(base, mine, theirs);
        assert!(!conflict);
        assert_eq!(out, "MINE\nline2\nTHEIRS\n");
    }

    #[test]
    fn merge_same_line_conflicts() {
        let base = "line1\nline2\n";
        let mine = "mine edit\nline2\n";
        let theirs = "theirs edit\nline2\n";
        let (out, conflict) = merge_md(base, mine, theirs);
        assert!(conflict);
        assert!(out.contains("<<<<<<< mine"));
        assert!(out.contains("mine edit"));
        assert!(out.contains("theirs edit"));
        assert!(out.contains(">>>>>>> theirs"));
    }

    #[test]
    fn merge_identical_is_clean() {
        let base = "a\n";
        let mine = "changed\n";
        let theirs = "changed\n";
        let (out, conflict) = merge_md(base, mine, theirs);
        assert!(!conflict);
        assert_eq!(out, "changed\n");
    }

    #[test]
    fn conflict_copy_name_format() {
        assert_eq!(
            conflict_copy_path("notes/markdown/Proyecto.md", "Arturo", 1234),
            "notes/markdown/Proyecto (conflict Arturo 1234).md"
        );
    }
}
