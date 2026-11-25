import { StateEffect } from "@codemirror/state";
import { call } from "../../../lib/ipc/commands";
import { noteTitleSource } from "../../../lib/links/noteTitleSource";

export interface WikiCompletionEntry {
  label: string;
  insertValue: string;
  keepOpenForSection?: boolean;
}

export const forceDecorationsEffect = StateEffect.define<null>();

export const embedExcerptCache = new Map<string, { text: string; muted: boolean; ts: number }>();
/** Object URLs per attachment path; storing the promise shares loads still in flight. */
export const attachmentUrlCache = new Map<string, Promise<string>>();
export const calloutCollapsedState = new Map<string, boolean>();
export const EMBED_CACHE_TTL_MS = 15_000;

export function invalidateEmbedCache(): void {
  embedExcerptCache.clear();
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

export function extractHeadingsFromMarkdown(content: string): { level: number; text: string }[] {
  const out: { level: number; text: string }[] = [];
  const lines = content.split(/\r?\n/);
  let inFence = false;
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const m = /^(#{1,6})\s+(.+)$/.exec(line);
    if (m) {
      const cleaned = m[2].replace(/[*_`~=]+/g, "").trim();
      if (cleaned) {
        out.push({ level: m[1].length, text: cleaned });
      }
    }
  }
  return out;
}

export function extractHeadingSection(content: string, heading: string): string {
  const target = normalizeHeading(heading);
  if (!target) return content;
  const lines = content.split(/\r?\n/);
  let startIdx = -1;
  let startLevel = 7;
  for (let i = 0; i < lines.length; i++) {
    const m = /^(#{1,6})\s+(.+)$/.exec(lines[i]);
    if (m && normalizeHeading(m[2]) === target) {
      startIdx = i;
      startLevel = m[1].length;
      break;
    }
  }
  if (startIdx < 0) return content;
  const section: string[] = [lines[startIdx]];
  for (let i = startIdx + 1; i < lines.length; i++) {
    const m = /^(#{1,6})\s+/.exec(lines[i]);
    if (m && m[1].length <= startLevel) break;
    section.push(lines[i]);
  }
  return section.join("\n");
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
    const headings = extractHeadingsFromMarkdown(currentDocText);
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
      Date.now() - headingCache.ts > EMBED_CACHE_TTL_MS
    ) {
      const full = await call("note_read", { noteId: exact.note_id });
      headingCache = {
        noteId: exact.note_id,
        ts: Date.now(),
        headings: extractHeadingsFromMarkdown(full.content),
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
  const currentHeadings = extractHeadingsFromMarkdown(currentDocText);
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
