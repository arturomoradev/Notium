// Plain-text extraction + word/character counting for the backlinks panel footer.

interface DocNode {
  type?: string;
  text?: string;
  attrs?: { target?: string; embed?: boolean; latex?: string };
  marks?: unknown[];
  content?: DocNode[];
}

/** Text of a Tiptap doc (JSON string): text nodes and link targets, one line per block. */
export function plainTextOf(content: string): string {
  let doc: DocNode;
  try {
    doc = JSON.parse(content) as DocNode;
  } catch {
    return content.trim();
  }
  const out: string[] = [];
  const walk = (n: DocNode): void => {
    if (n.type === "text") out.push(n.text ?? "");
    else if (n.type === "wikiLink") out.push(n.attrs?.target ?? "");
    else if (n.type === "inlineMath") out.push(n.attrs?.latex ?? "");
    else if (n.type === "blockMath") out.push(n.attrs?.latex ?? "", "\n");
    else {
      n.content?.forEach(walk);
      out.push("\n");
    }
  };
  walk(doc);
  return out.join("").replace(/\s+/g, " ").trim();
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

export function countWords(text: string): number {
  const t = text.trim();
  if (!t) return 0;
  return t.split(/\s+/).length;
}

export interface Stats {
  words: number;
  chars: number;
}

export function statsOf(content: string): Stats {
  const text = plainTextOf(content);
  return { words: countWords(text), chars: text.length };
}
