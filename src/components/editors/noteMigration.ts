// Upgrade notes converted from Markdown, where some syntax arrived as text.
// - Math: a paragraph that is only `$$…$$`, or a `math`/`latex` code block,
//   becomes `blockMath`; `$…$` inside text becomes `inlineMath`.
// - Callouts: a blockquote paragraph starting with `[!type]` starts a `callout`
//   (several merged into one quote are split). Pure JSON, no editor.

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

const CALLOUT = /^\[!([A-Za-z0-9_-]+)\]([+-]?)[ \t]*/;

/** A paragraph's inline content split at its first line break: [first line, rest]. */
function splitFirstLine(p: DocNode): [DocNode[], DocNode[]] {
  const first: DocNode[] = [];
  const rest: DocNode[] = [];
  let broken = false;
  for (const n of p.content ?? []) {
    if (broken) rest.push(n);
    else if (n.type === "hardBreak") broken = true;
    else {
      const text = n.type === "text" ? (n.text ?? "") : "";
      const i = text.indexOf("\n");
      if (i < 0) first.push(n);
      else {
        broken = true;
        if (i > 0) first.push({ ...n, text: text.slice(0, i) });
        if (i + 1 < text.length) rest.push({ ...n, text: text.slice(i + 1) });
      }
    }
  }
  return [first, rest];
}

const startsCallout = (b: DocNode): boolean =>
  b.type === "paragraph" &&
  b.content?.[0]?.type === "text" &&
  CALLOUT.test(b.content[0].text ?? "");

/** The callouts in a blockquote (blocks before the first marker stay a quote), or null. */
function calloutsOf(quote: DocNode): DocNode[] | null {
  const blocks = quote.content ?? [];
  if (!blocks.some(startsCallout)) return null;
  const lead: DocNode[] = [];
  const callouts: DocNode[] = [];
  for (const b of blocks) {
    if (startsCallout(b)) {
      const [firstLine, rest] = splitFirstLine(b);
      const head = textOf({ content: firstLine });
      const m = CALLOUT.exec(head)!;
      callouts.push({
        type: "callout",
        attrs: { type: m[1].toLowerCase(), title: head.slice(m[0].length).trim(), fold: m[2] },
        content: rest.length ? [{ type: "paragraph", content: rest }] : [],
      });
    } else if (callouts.length) callouts[callouts.length - 1].content!.push(b);
    else lead.push(b);
  }
  for (const c of callouts) if (!c.content!.length) c.content = [{ type: "paragraph" }];
  return lead.length ? [{ ...quote, content: lead }, ...callouts] : callouts;
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
export function migrateNote(doc: DocNode): boolean {
  let changed = false;
  const walk = (n: DocNode): void => {
    if (!n.content || n.type === "codeBlock") return;
    n.content = n.content.flatMap((child) => {
      const callouts = child.type === "blockquote" ? calloutsOf(child) : null;
      if (callouts) {
        changed = true;
        return callouts;
      }
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
