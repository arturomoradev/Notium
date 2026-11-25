export interface MarkdownEdit {
  doc: string;
  anchor: number;
  head: number;
}

export function insertMarkdown(
  doc: string,
  from: number,
  to: number,
  insert: string,
  cursorOffset: number,
): MarkdownEdit {
  return {
    doc: doc.slice(0, from) + insert + doc.slice(to),
    anchor: from + cursorOffset,
    head: from + cursorOffset,
  };
}

export function wrapMarkdownSelection(
  doc: string,
  from: number,
  to: number,
  marker: string,
): MarkdownEdit {
  const selected = doc.slice(from, to);
  const size = marker.length;
  if (selected.startsWith(marker) && selected.endsWith(marker) && selected.length >= size * 2) {
    const inner = selected.slice(size, -size);
    return {
      doc: doc.slice(0, from) + inner + doc.slice(to),
      anchor: from,
      head: from + inner.length,
    };
  }
  if (!selected) return insertMarkdown(doc, from, to, marker + marker, size);
  const text = `${marker}${selected}${marker}`;
  return { doc: doc.slice(0, from) + text + doc.slice(to), anchor: from, head: from + text.length };
}

export function toggleMarkdownPrefix(
  doc: string,
  from: number,
  to: number,
  prefix: string,
): MarkdownEdit {
  const start = doc.lastIndexOf("\n", Math.max(0, from - 1)) + 1;
  const newline = doc.indexOf("\n", to);
  const end = newline < 0 ? doc.length : newline;
  const lines = doc.slice(start, end).split("\n");
  const strip = /^(?:#{1,6}\s+|-\s\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+|>\s?)+/;
  const remove = lines.every((line) => line.startsWith(prefix));
  const prefixes = lines.map((line) =>
    remove ? prefix.length : (strip.exec(line)?.[0].length ?? 0),
  );
  const changed = lines.map((line) =>
    remove
      ? line.slice(prefix.length)
      : strip.test(line)
        ? line.replace(strip, prefix)
        : prefix + line,
  );
  const text = changed.join("\n");
  const next = doc.slice(0, start) + text + doc.slice(end);
  const map = (position: number) => {
    let adjusted = position;
    let lineStart = start;
    for (let i = 0; i < prefixes.length; i += 1) {
      const oldPrefix = prefixes[i];
      const newPrefix = remove ? 0 : prefix.length;
      const lineEnd = lineStart + lines[i].length;
      if (position >= lineStart + oldPrefix) adjusted += newPrefix - oldPrefix;
      else if (position >= lineStart && position < lineStart + oldPrefix)
        adjusted = lineStart + Math.min(position - lineStart, newPrefix);
      if (position <= lineEnd) break;
      lineStart = lineEnd + 1;
    }
    return Math.max(0, Math.min(next.length, adjusted));
  };
  return { doc: next, anchor: map(from), head: map(to) };
}

export function isolateMarkdownFrontmatter(text: string): { envelope: string; body: string } {
  const newline = text.includes("\r\n") ? "\r\n" : "\n";
  if (!text.startsWith(`---${newline}`)) return { envelope: "", body: text };
  const start = 3 + newline.length;
  let index = start;
  while (index < text.length) {
    const next = text.indexOf("\n", index);
    if (next < 0) break;
    const line = text.slice(index, next).replace(/\r$/, "");
    if (line === "---" || line === "...")
      return { envelope: text.slice(0, next + 1), body: text.slice(next + 1) };
    index = next + 1;
  }
  return { envelope: "", body: text };
}

export function stitchMarkdownFrontmatter(existing: string, body: string, id: string): string {
  const split = isolateMarkdownFrontmatter(existing);
  if (split.envelope) return split.envelope + body;
  const newline = body.includes("\r\n") ? "\r\n" : "\n";
  return `---${newline}notium_id: ${id}${newline}---${newline}${body}`;
}

export type MarkdownTableOperation =
  "add-row" | "add-column" | "delete-row" | "delete-column" | "delete-table";

export function editMarkdownTable(
  doc: string,
  position: number,
  operation: MarkdownTableOperation,
): MarkdownEdit | null {
  const lines = doc.split("\n");
  const starts: number[] = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    offset += line.length + 1;
  }
  const row = starts.findIndex(
    (start, i) => position >= start && position <= start + lines[i].length,
  );
  if (row < 0 || !lines[row].trimStart().startsWith("|")) return null;
  let first = row;
  let last = row;
  while (first > 0 && lines[first - 1].trimStart().startsWith("|")) first--;
  while (last + 1 < lines.length && lines[last + 1].trimStart().startsWith("|")) last++;
  const table = lines.slice(first, last + 1);
  const separator = (line: string) => /^\s*\|?\s*:?-{3,}/.test(line);
  const split = (line: string) => line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
  const join = (cells: string[]) => `|${cells.join("|")}|`;
  const cursorRow = row - first;
  const rowCells = split(lines[row]);
  const cursorColumn = Math.max(
    0,
    lines[row].slice(0, position - starts[row]).split("|").length - 2,
  );
  if (operation === "add-row") {
    const index = Math.max(table.findIndex(separator) + 1, table.length);
    table.splice(index, 0, join(Array.from({ length: rowCells.length }, () => "  ")));
  } else if (operation === "add-column") {
    table.forEach((line, i) => {
      const values = split(line);
      values.push(separator(line) ? " --- " : "  ");
      table[i] = join(values);
    });
  } else if (operation === "delete-row") {
    if (table.length <= 2 || cursorRow === 0 || separator(lines[row])) return null;
    table.splice(cursorRow, 1);
  } else if (operation === "delete-column") {
    if (rowCells.length <= 1) return null;
    table.forEach((line, i) => {
      table[i] = join(
        split(line).filter(
          (_cell, column) => column !== Math.min(cursorColumn, split(line).length - 1),
        ),
      );
    });
  } else {
    table.splice(0, table.length);
  }
  lines.splice(first, last - first + 1, ...table);
  const result = lines.join("\n");
  return {
    doc: result,
    anchor: Math.min(result.length, starts[first] + Math.max(0, position - starts[row])),
    head: Math.min(result.length, starts[first] + Math.max(0, position - starts[row])),
  };
}
