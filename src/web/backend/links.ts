// Link commands for the web client. Links come from the native extractor (WASM);
// target resolution mirrors index.rs `resolve_note_target` and snippets mirror
// commands.rs `snippet_around`.
import type {
  BacklinkDto,
  LinkReferenceDto,
  LinkRepairResultDto,
  OutlinkDto,
} from "../../lib/ipc/types.ts";
import { docJson, normalizeTitle, rewriteNoteLinks } from "../core.ts";
import { infoOf, titleOf, type WebNote } from "./state.ts";

/** A note's resolvable names, built once per call over the live notes. */
function resolver(notes: WebNote[], pathOf: (n: WebNote) => string) {
  const byId = new Map<string, WebNote>();
  const byTitle = new Map<string, WebNote>();
  const byPath = new Map<string, WebNote>();
  const byNorm = new Map<string, WebNote>();
  for (const n of notes) {
    const title = titleOf(n);
    if (!byId.has(n.id)) byId.set(n.id, n);
    if (!byTitle.has(title)) byTitle.set(title, n);
    if (!byPath.has(pathOf(n))) byPath.set(pathOf(n), n);
    const norm = normalizeTitle(title);
    if (!byNorm.has(norm)) byNorm.set(norm, n);
  }
  // shortcut: frontmatter aliases are not resolved yet (the native index does).
  return (target: string): WebNote | undefined => {
    const t = target.split("#")[0].trim();
    if (!t) return undefined;
    return byId.get(t) ?? byTitle.get(t) ?? byPath.get(t) ?? byNorm.get(normalizeTitle(t));
  };
}

/** Window of `body` around the first case-insensitive match of `needle`. */
export function snippetAround(body: string, needle: string): string {
  const chars = [...body];
  const n = needle.trim();
  if (!n) return chars.slice(0, 120).join("");
  const lower = chars.map((c) => c.toLowerCase()[0] ?? c);
  const target = [...n].map((c) => c.toLowerCase()[0] ?? c);
  for (let i = 0; i + target.length <= lower.length; i++) {
    if (target.every((c, j) => lower[i + j] === c)) {
      const start = Math.max(0, i - 40);
      const end = Math.min(chars.length, i + target.length + 60);
      return (
        (start > 0 ? "…" : "") + chars.slice(start, end).join("") + (end < chars.length ? "…" : "")
      );
    }
  }
  return chars.slice(0, 120).join("");
}

const noteLinks = (n: WebNote) => infoOf(n).out_links.filter((l) => !l.is_attachment);

export function backlinks(
  notes: WebNote[],
  pathOf: (n: WebNote) => string,
  noteId: string,
): BacklinkDto[] {
  const resolve = resolver(notes, pathOf);
  const out: BacklinkDto[] = [];
  for (const src of notes) {
    for (const link of noteLinks(src)) {
      if (resolve(link.target)?.id !== noteId) continue;
      out.push({
        source_id: src.id,
        source_title: titleOf(src),
        link_text: link.text,
        kind: link.kind,
        snippet: snippetAround(infoOf(src).plain_text, link.text),
      });
    }
  }
  return out.sort(
    (a, b) =>
      a.source_title.localeCompare(b.source_title) || a.link_text.localeCompare(b.link_text),
  );
}

export function outlinks(
  notes: WebNote[],
  pathOf: (n: WebNote) => string,
  note: WebNote,
  blobs: Record<string, string>,
): OutlinkDto[] {
  const resolve = resolver(notes, pathOf);
  return infoOf(note)
    .out_links.map((l): OutlinkDto => {
      if (l.is_attachment) {
        const path = l.target.startsWith("attachments/") ? l.target : `attachments/${l.target}`;
        const known = path in blobs;
        return {
          target_raw: l.target,
          link_text: l.text,
          kind: l.kind,
          is_attachment: true,
          target_id: known ? path : null,
          target_title: known ? path.slice("attachments/".length) : null,
          target_path: known ? path : null,
          is_resolved: known,
        };
      }
      const t = resolve(l.target);
      return {
        target_raw: l.target,
        link_text: l.text,
        kind: l.kind,
        is_attachment: false,
        target_id: t?.id ?? null,
        target_title: t ? titleOf(t) : null,
        target_path: t ? pathOf(t) : null,
        is_resolved: Boolean(t),
      };
    })
    .sort((a, b) => a.target_raw.localeCompare(b.target_raw));
}

/** Notes mentioning `title` in plain text without linking to `noteId`. */
export function unlinkedMentions(
  notes: WebNote[],
  pathOf: (n: WebNote) => string,
  noteId: string,
  title: string,
): BacklinkDto[] {
  const needle = title.trim().toLowerCase();
  if (!needle) return [];
  const linked = new Set(backlinks(notes, pathOf, noteId).map((b) => b.source_id));
  return notes
    .filter((n) => n.id !== noteId && !linked.has(n.id))
    .filter((n) => infoOf(n).plain_text.toLowerCase().includes(needle))
    .map((n) => ({
      source_id: n.id,
      source_title: titleOf(n),
      link_text: title,
      kind: "mention",
      snippet: snippetAround(infoOf(n).plain_text, title),
    }));
}

export function linkReferences(
  notes: WebNote[],
  pathOf: (n: WebNote) => string,
  oldTitle: string,
): LinkReferenceDto[] {
  return notes
    .map((n) => ({ n, count: rewriteNoteLinks(n.doc, oldTitle, "PLACEHOLDER")[1] }))
    .filter(({ count }) => count > 0)
    .map(({ n, count }) => ({ note_id: n.id, title: titleOf(n), path: pathOf(n), count }))
    .sort((a, b) => a.title.localeCompare(b.title));
}

/** Rewritten bodies for every note linking to `oldTitle`; the caller saves them. */
export function repairLinks(
  notes: WebNote[],
  oldTitle: string,
  newTitle: string,
): { edits: { note: WebNote; body: string }[]; result: LinkRepairResultDto } {
  const edits: { note: WebNote; body: string }[] = [];
  let updated = 0;
  for (const note of notes) {
    const [rewritten, count] = rewriteNoteLinks(note.doc, oldTitle, newTitle);
    if (count > 0 && rewritten !== note.doc) {
      edits.push({ note, body: docJson(rewritten) });
      updated += count;
    }
  }
  return {
    edits,
    result: { affected_notes_count: edits.length, updated_links_count: updated },
  };
}
