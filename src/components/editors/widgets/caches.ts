import { call } from "../../../lib/ipc/commands";
import { noteTitleSource } from "../../../lib/links/noteTitleSource";

export interface WikiCompletionEntry {
  label: string;
  insertValue: string;
  keepOpenForSection?: boolean;
}

/** Object URLs per attachment path; storing the promise shares loads still in flight. */
export const attachmentUrlCache = new Map<string, Promise<string>>();
const HEADING_CACHE_TTL_MS = 15_000;

/** Drop cached headings of other notes (after a save, rename or sync). */
export function invalidateEmbedCache(): void {
  headingCache = null;
}

export function imageMime(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase();
  return ext === "svg"
    ? "image/svg+xml"
    : `image/${ext === "png" ? "png" : ext === "gif" ? "gif" : ext === "webp" ? "webp" : "jpeg"}`;
}

export function normalizeHeading(s: string): string {
  return s
    .replace(/[*_`~=]+/g, "")
    .trim()
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/\s+/g, " ");
}

interface DocNode {
  type?: string;
  text?: string;
  attrs?: { level?: number; target?: string };
  content?: DocNode[];
}

const textOf = (n: DocNode): string =>
  n.type === "text" ? (n.text ?? "") : (n.content ?? []).map(textOf).join("");

/** Headings of a Tiptap doc (JSON string), in order. */
export function headingsOf(docJson: string): { level: number; text: string }[] {
  let doc: DocNode;
  try {
    doc = JSON.parse(docJson) as DocNode;
  } catch {
    return [];
  }
  return (doc.content ?? [])
    .filter((n) => n.type === "heading")
    .map((n) => ({ level: n.attrs?.level ?? 1, text: textOf(n).trim() }))
    .filter((h) => h.text);
}

let headingCache: {
  noteId: string;
  ts: number;
  headings: { level: number; text: string }[];
} | null = null;

export async function buildWikiCompletionEntries(
  query: string,
  currentDocText: string,
): Promise<WikiCompletionEntry[]> {
  const hashIdx = query.indexOf("#");
  if (hashIdx === 0) {
    const filter = normalizeHeading(query.slice(1));
    const headings = headingsOf(currentDocText);
    return headings
      .filter((h) => !filter || normalizeHeading(h.text).includes(filter))
      .slice(0, 15)
      .map((h) => ({
        label: `${"#".repeat(h.level)} ${h.text}`,
        insertValue: `#${h.text}`,
      }));
  }
  if (hashIdx > 0) {
    const noteName = query.slice(0, hashIdx).trim();
    const filter = normalizeHeading(query.slice(hashIdx + 1));
    const candidates = await noteTitleSource(noteName);
    const exact =
      candidates.find((c) => normalizeHeading(c.title) === normalizeHeading(noteName)) ??
      candidates[0];
    if (!exact) return [];
    // One note_read per target note every 10 s, not one per keystroke after `#`.
    if (
      headingCache?.noteId !== exact.note_id ||
      Date.now() - headingCache.ts > HEADING_CACHE_TTL_MS
    ) {
      const full = await call("note_read", { noteId: exact.note_id });
      headingCache = {
        noteId: exact.note_id,
        ts: Date.now(),
        headings: headingsOf(full.content),
      };
    }
    const headings = headingCache.headings;
    return headings
      .filter((h) => !filter || normalizeHeading(h.text).includes(filter))
      .slice(0, 15)
      .map((h) => ({
        label: `📄 ${exact.title} › ${"#".repeat(h.level)} ${h.text}`,
        insertValue: `${exact.title}#${h.text}`,
      }));
  }
  const notes = await noteTitleSource(query);
  const entries: WikiCompletionEntry[] = [];
  const currentHeadings = headingsOf(currentDocText);
  if (!query.trim() && currentHeadings.length > 0) {
    entries.push({
      label: "§ Enlazar sección de esta nota (#…)",
      insertValue: "#",
      keepOpenForSection: true,
    });
  }
  for (const n of notes.slice(0, 12)) {
    entries.push({
      label: n.title,
      insertValue: n.title,
    });
  }
  return entries;
}
