// Plain-text extraction + word/character counting for the backlinks panel footer.

export function mdToPlainText(md: string): string {
  let text = md;
  // Strip YAML frontmatter.
  const fm = /^---\s*\n[\s\S]*?\n---\s*\n?/.exec(text);
  if (fm) text = text.slice(fm[0].length);
  // Strip fenced code markers (keep the code content itself).
  text = text.replace(/```[a-zA-Z0-9_-]*\n?/g, " ");
  // Resolve wiki links / markdown links to their label text.
  text = text
    .replace(/!\[\[([^\]]+)\]\]/g, "$1")
    .replace(/\[\[([^\]]+)\]\]/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
  // Strip remaining markdown punctuation.
  text = text.replace(/[#>*_~`\-+=\[\]()|]/g, " ");
  return text.replace(/\s+/g, " ").trim();
}

export function plainTextOf(content: string): string {
  return mdToPlainText(content);
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
