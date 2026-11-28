// Turn LaTeX written as text (notes converted from Markdown) into math nodes:
// a paragraph that is only `$$…$$`, or a `math`/`latex` code block, becomes
// `blockMath`; `$…$` inside text becomes `inlineMath`. Pure JSON, no editor.

interface DocNode {
  type?: string;
  text?: string;
  attrs?: Record<string, unknown>;
  marks?: { type?: string }[];
  content?: DocNode[];
}

// Like Tiptap's mathMigrationRegex, but one line and no `$` inside, so prices
// (`$100`) and the literal `$$` stay text.
const INLINE = /\$(?!\d+\$)([^$\n]+?)\$(?!\d)/g;
const BLOCK = /^\s*\$\$([\s\S]+?)\$\$\s*$/;
const MATH_LANGS = new Set(["math", "latex", "tex", "katex"]);

const textOf = (n: DocNode): string =>
  n.type === "text"
    ? (n.text ?? "")
    : n.type === "hardBreak"
      ? "\n"
      : (n.content ?? []).map(textOf).join("");

const plainInline = (n: DocNode): boolean =>
  (n.type === "text" && !n.marks?.length) || n.type === "hardBreak";

function blockOf(n: DocNode): DocNode | null {
  if (n.type === "codeBlock" && MATH_LANGS.has(String(n.attrs?.language ?? "").toLowerCase()))
    return { type: "blockMath", attrs: { latex: textOf(n).trim() } };
  if (n.type === "paragraph" && n.content?.length && n.content.every(plainInline)) {
    const m = BLOCK.exec(textOf(n));
    if (m && m[1].trim()) return { type: "blockMath", attrs: { latex: m[1].trim() } };
  }
  return null;
}

function splitInline(n: DocNode): DocNode[] {
  const code = n.marks?.some((m) => m.type === "code");
  if (n.type !== "text" || !n.text || code) return [n];
  const out: DocNode[] = [];
  let last = 0;
  for (const m of n.text.matchAll(INLINE)) {
    if (!m[1].trim()) continue;
    if (m.index > last) out.push({ ...n, text: n.text.slice(last, m.index) });
    out.push({ type: "inlineMath", attrs: { latex: m[1].trim() } });
    last = m.index + m[0].length;
  }
  if (!out.length) return [n];
  if (last < n.text.length) out.push({ ...n, text: n.text.slice(last) });
  return out;
}

/** Migrate `doc` in place; returns whether anything changed. */
export function migrateMath(doc: DocNode): boolean {
  let changed = false;
  const walk = (n: DocNode): void => {
    if (!n.content || n.type === "codeBlock") return;
    n.content = n.content.flatMap((child) => {
      const block = blockOf(child);
      if (block) {
        changed = true;
        return [block];
      }
      const parts = splitInline(child);
      if (parts.length !== 1 || parts[0] !== child) changed = true;
      return parts;
    });
    n.content.forEach(walk);
  };
  walk(doc);
  return changed;
}
