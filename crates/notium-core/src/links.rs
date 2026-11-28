//! Link-resolution helpers: title normalization for cross-format resolution.

/// Normalize a title/path for case- and diacritic-insensitive matching.
pub fn normalize_title(s: &str) -> String {
    let folded: String = s.chars().map(fold_diacritic).collect();
    folded
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fold_diacritic(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' => 'A',
        'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' | 'Ē' => 'E',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' => 'I',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ō' => 'O',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' => 'U',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ç' => 'c',
        'Ç' => 'C',
        other => other,
    }
}

/// Rewrite wikilink targets in Markdown text when a note is renamed.
/// Replaces targets matching `old_title` (by normalized title comparison) with `new_title`.
/// Preserves any anchor `#section`, alias `|label`, and leading `!`.
/// Returns `(updated_content, occurrences_count)`.
pub fn rewrite_wikilink_targets(
    content: &str,
    old_title: &str,
    new_title: &str,
) -> (String, usize) {
    let old_norm = normalize_title(old_title);
    if old_norm.is_empty() {
        return (content.to_string(), 0);
    }

    let mut out = String::with_capacity(content.len());
    let mut count = 0;
    let b = content.as_bytes();
    let mut i = 0;

    while i < b.len() {
        if i + 1 < b.len() && b[i] == b'[' && b[i + 1] == b'[' {
            if let Some(close_rel) = content[i + 2..].find("]]") {
                let inner_start = i + 2;
                let inner_end = inner_start + close_rel;
                let inner = &content[inner_start..inner_end];

                // Split by | to separate target from alias
                let (target, alias_part) = match inner.split_once('|') {
                    Some((t, a)) => (t, Some(a)),
                    None => (inner, None),
                };

                // Split target by # to separate note target from heading
                let (note_part, heading_part) = match target.split_once('#') {
                    Some((n, h)) => (n, Some(h)),
                    None => (target, None),
                };

                let note_norm = normalize_title(note_part);
                if !note_norm.is_empty() && note_norm == old_norm {
                    count += 1;
                    out.push_str("[[");
                    out.push_str(new_title);
                    if let Some(h) = heading_part {
                        out.push('#');
                        out.push_str(h);
                    }
                    if let Some(a) = alias_part {
                        out.push('|');
                        out.push_str(a);
                    }
                    out.push_str("]]");
                    i = inner_end + 2;
                    continue;
                }
            }
        }
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    (out, count)
}

/// Rewrite links to a renamed note inside a note file (Tiptap JSON): the
/// `target` of `wikiLink` nodes and any `[[..]]` typed as plain text.
/// Text that is not a JSON note is returned unchanged with a count of 0.
pub fn rewrite_note_links(file: &str, old_title: &str, new_title: &str) -> (String, usize) {
    fn walk(node: &mut serde_json::Value, old: &str, new: &str, count: &mut usize) {
        let kind = node
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if kind == "wikiLink" {
            if let Some(t) = node.pointer_mut("/attrs/target") {
                let target = t.as_str().unwrap_or("").to_string();
                let (out, n) = rewrite_wikilink_targets(&format!("[[{target}]]"), old, new);
                if n > 0 {
                    *t = serde_json::Value::String(out[2..out.len() - 2].to_string());
                    *count += n;
                }
            }
        } else if kind == "text" {
            if let Some(t) = node.get_mut("text") {
                let (out, n) = rewrite_wikilink_targets(t.as_str().unwrap_or(""), old, new);
                if n > 0 {
                    *t = serde_json::Value::String(out);
                    *count += n;
                }
            }
        }
        match node {
            serde_json::Value::Object(m) => m.values_mut().for_each(|v| walk(v, old, new, count)),
            serde_json::Value::Array(a) => a.iter_mut().for_each(|v| walk(v, old, new, count)),
            _ => {}
        }
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(file) else {
        return (file.to_string(), 0);
    };
    let mut count = 0;
    if let Some(doc) = value.get_mut("doc") {
        walk(doc, old_title, new_title, &mut count);
    }
    if count == 0 {
        return (file.to_string(), 0);
    }
    let mut out = serde_json::to_string_pretty(&value).unwrap_or_default();
    out.push('\n');
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_folds_case_and_diacritics() {
        assert_eq!(normalize_title("Canción"), "cancion");
        assert_eq!(normalize_title("  Mi   Nota  "), "mi nota");
        assert_eq!(normalize_title("Inglés"), normalize_title("ingles"));
    }

    #[test]
    fn rewrite_wikilink_targets_variants() {
        let input = "Ver [[Notas]], [[notas#Capítulo 1]], [[Notas|Mis Notas]], ![[Notas#Intro|Intro]], [[#Local]] y [[Notas Antiguas]].";
        let (output, count) = rewrite_wikilink_targets(input, "Notas", "Apuntes");
        assert_eq!(count, 4);
        assert_eq!(
            output,
            "Ver [[Apuntes]], [[Apuntes#Capítulo 1]], [[Apuntes|Mis Notas]], ![[Apuntes#Intro|Intro]], [[#Local]] y [[Notas Antiguas]]."
        );
    }

    #[test]
    fn rewrite_wikilink_targets_preserves_accents() {
        let input = "Consulta [[Diseño Gráfico]] y [[diseño grafico#Herramientas]].";
        let (output, count) = rewrite_wikilink_targets(input, "Diseño Gráfico", "Diseño Visual");
        assert_eq!(count, 2);
        assert_eq!(
            output,
            "Consulta [[Diseño Visual]] y [[Diseño Visual#Herramientas]]."
        );
    }

    #[test]
    fn rewrite_note_links_updates_nodes_and_text() {
        let file = r#"{"notium_id":"n","doc":{"type":"doc","content":[{"type":"paragraph","content":[
            {"type":"wikiLink","attrs":{"target":"Notas#Intro|Mis notas","embed":false}},
            {"type":"text","text":" y [[notas]] y [[Otra]]"}]}]}}"#;
        let (out, count) = rewrite_note_links(file, "Notas", "Apuntes");
        assert_eq!(count, 2);
        assert!(out.contains("\"Apuntes#Intro|Mis notas\""));
        assert!(out.contains("y [[Apuntes]] y [[Otra]]"));
        assert_eq!(
            rewrite_note_links("not json", "Notas", "X"),
            ("not json".into(), 0)
        );
    }
}
