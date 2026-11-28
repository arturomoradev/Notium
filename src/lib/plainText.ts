// Small helpers over Tiptap docs (JSON) used outside the editor.

interface DocNode {
  type?: string;
  text?: string;
  attrs?: { target?: string; embed?: boolean; latex?: string; title?: string };
  marks?: unknown[];
  content?: DocNode[];
}

/** A doc (JSON string) whose blocks are `blocks`. */
export function docOf(...blocks: object[]): string {
  return JSON.stringify({ type: "doc", content: blocks });
}

/**
 * Turn the first plain mention of `title` (whole word, any case) into a
 * `[[title]]` link node. Returns the new doc, or null when there is none.
 */
export function linkFirstMention(docJson: string, title: string): string | null {
  const escaped = title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(^|[^\\p{L}\\p{N}])(${escaped})(?=$|[^\\p{L}\\p{N}])`, "iu");
  let doc: DocNode;
  try {
    doc = JSON.parse(docJson) as DocNode;
  } catch {
    return null;
  }
  let done = false;
  const walk = (n: DocNode): void => {
    if (done || !n.content || n.type === "codeBlock") return;
    n.content = n.content.flatMap((child) => {
      const code = child.marks?.some((m) => (m as { type?: string }).type === "code");
      if (done || child.type !== "text" || !child.text || code) return [child];
      const m = re.exec(child.text);
      if (!m) return [child];
      done = true;
      const start = m.index + m[1].length;
      const end = start + m[2].length;
      const part = (text: string): DocNode[] => (text ? [{ ...child, text }] : []);
      return [
        ...part(child.text.slice(0, start)),
        { type: "wikiLink", attrs: { target: title, embed: false } } as DocNode,
        ...part(child.text.slice(end)),
      ];
    });
    n.content.forEach(walk);
  };
  walk(doc);
  return done ? JSON.stringify(doc) : null;
}

export interface Stats {
  words: number;
  chars: number;
}
