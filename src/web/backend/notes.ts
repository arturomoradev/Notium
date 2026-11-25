// Note commands answered from the in-memory vault (see ./state.ts).
import type {
  FolderDto,
  NoteContentDto,
  NoteSummaryDto,
  SaveResultDto,
  SearchHitDto,
  TitleSuggestionDto,
  TrashItemDto,
} from "../../lib/ipc/types.ts";
import { sanitizeFolderPath, sanitizeTitle, titleFromPath } from "../core.ts";
import {
  folderOp,
  localOp,
  newDoc,
  nowSecs,
  splitDoc,
  stitchDoc,
  tagsOf,
  titleOf,
  trashed,
  TRASH_RETENTION_SECS,
  type WebNote,
} from "./state.ts";
import { deviceId, persist, schedulePush, vault } from "./sync.ts";

export const liveNotes = (): WebNote[] =>
  Object.values(vault.state.notes).filter((n) => !n.deleted);

const NOTES_DIR = "notes/markdown";

/** Vault-relative path: the synced one, or where devices place older notes. */
export const pathOf = (n: WebNote): string => n.path ?? `${NOTES_DIR}/${titleOf(n)}.md`;

/** Same as `LocalVault::dedup_path`: `Title.md`, then `Title 2.md`, … */
function dedupPath(dir: string, title: string, exceptId?: string): string {
  const taken = new Set(
    liveNotes()
      .filter((n) => n.id !== exceptId)
      .map(pathOf),
  );
  let path = `${dir}/${title}.md`;
  for (let n = 2; taken.has(path); n++) path = `${dir}/${title} ${n}.md`;
  return path;
}

const folderDir = (folder: string): string => {
  const clean = sanitizeFolderPath(folder);
  return clean ? `${NOTES_DIR}/${clean}` : NOTES_DIR;
};

export function summary(n: WebNote): NoteSummaryDto {
  return {
    note_id: n.id,
    title: titleOf(n),
    format: "markdown",
    path: pathOf(n),
    updated_at: String(n.updated),
    tags: tagsOf(n),
  };
}

export function find(noteId: string): WebNote {
  const n = vault.state.notes[noteId];
  if (!n || n.deleted) throw new Error(`Nota no encontrada: ${noteId}`);
  return n;
}

const contentDto = (n: WebNote): NoteContentDto => ({
  note: summary(n),
  content: splitDoc(n.doc)[1],
  rev: n.rev,
});

/** Record a local op and push it shortly. */
async function commit(
  noteId: string,
  kind: "create" | "update" | "delete",
  doc: string | null,
  baseRev: number,
  path?: string,
  purge = false,
): Promise<WebNote> {
  const n = localOp(vault.state, deviceId, noteId, kind, doc, baseRev, path, purge);
  await persist();
  schedulePush();
  return n;
}

export function listNotes(tag?: string | null): NoteSummaryDto[] {
  return liveNotes()
    .sort((a, b) => b.updated - a.updated)
    .map(summary)
    .filter((s) => !tag || s.tags.includes(tag));
}

export const readNote = (noteId: string): NoteContentDto => contentDto(find(noteId));

export async function saveNote(
  noteId: string,
  content: string,
  baseRev: number,
): Promise<SaveResultDto> {
  const n = find(noteId);
  // Saves without edits (e.g. on blur) must not create ops.
  if (content === splitDoc(n.doc)[1]) return { rev: n.rev, note: summary(n) };
  const saved = await commit(n.id, "update", stitchDoc(n.doc, content, n.id), baseRev);
  return { rev: saved.rev, note: summary(saved) };
}

/** Like the native `create_impl`: sanitized title, deduplicated file in `folder`. */
export async function createNote(
  title?: string | null,
  folder?: string | null,
): Promise<NoteContentDto> {
  const id = `note_${crypto.randomUUID().replaceAll("-", "")}`;
  const name = sanitizeTitle(title?.trim() || `Sin título ${Math.floor(Date.now() / 1000)}`);
  const path = dedupPath(folderDir(folder ?? ""), name);
  return contentDto(await commit(id, "create", newDoc(id), 0, path));
}

/** A path-only change: same content, new path (old peers see a no-op update). */
async function setPath(n: WebNote, path: string): Promise<WebNote> {
  if (path === pathOf(n)) return n;
  return commit(n.id, "update", n.doc, n.rev, path);
}

/** Like `LocalVault::rename_note`: same folder, sanitized and deduplicated. */
export async function renameNote(noteId: string, newTitle: string): Promise<NoteSummaryDto> {
  const n = find(noteId);
  const dir = pathOf(n).slice(0, pathOf(n).lastIndexOf("/"));
  return summary(await setPath(n, dedupPath(dir, sanitizeTitle(newTitle), n.id)));
}

/** Like `LocalVault::move_note`: keep the file name, change the folder. */
export async function moveNote(noteId: string, folder: string): Promise<NoteSummaryDto> {
  const n = find(noteId);
  return summary(await setPath(n, dedupPath(folderDir(folder), titleFromPath(pathOf(n)), n.id)));
}

/** Every folder holding notes (with its ancestors), plus the synced folder set (empty ones too). */
export function listFolders(): FolderDto[] {
  const dirs = new Set(Object.keys(vault.state.folders ?? {}).map((f) => `${NOTES_DIR}/${f}`));
  for (const n of liveNotes()) {
    const parts = pathOf(n).split("/").slice(0, -1);
    for (let i = 3; i <= parts.length; i++) dirs.add(parts.slice(0, i).join("/"));
  }
  return [...dirs].sort().map((path) => ({ path }));
}

/** Like `LocalVault::create_folder` (parents included); synced as folder ops. */
export async function createFolder(name: string): Promise<string> {
  const clean = sanitizeFolderPath(name) || sanitizeTitle(name);
  const parts = clean.split("/");
  for (let i = 1; i <= parts.length; i++) {
    const dir = parts.slice(0, i).join("/");
    if (!vault.state.folders?.[dir]) folderOp(vault.state, deviceId, dir, "create");
  }
  await persist();
  schedulePush();
  return `${NOTES_DIR}/${clean}`;
}

/** Like `LocalVault::rename_folder`: moves every note under it; returns the new folder. */
export async function renameFolder(folder: string, newName: string): Promise<string> {
  const oldClean = sanitizeFolderPath(folder);
  const input = sanitizeFolderPath(newName);
  if (!oldClean || !input) throw new Error("Nombre de carpeta no válido");
  const parent = oldClean.includes("/") ? oldClean.slice(0, oldClean.lastIndexOf("/")) : "";
  const newClean = input.includes("/") || !parent ? input : `${parent}/${input}`;
  if (newClean === oldClean) return oldClean;
  if (newClean.startsWith(`${oldClean}/`))
    throw new Error("No se puede mover una carpeta dentro de sí misma");
  const oldPrefix = `${NOTES_DIR}/${oldClean}/`;
  const newPrefix = `${NOTES_DIR}/${newClean}/`;
  if (liveNotes().some((n) => pathOf(n).startsWith(newPrefix))) {
    throw new Error("La carpeta de destino ya existe");
  }
  for (const n of liveNotes().filter((n) => pathOf(n).startsWith(oldPrefix))) {
    await setPath(n, newPrefix + pathOf(n).slice(oldPrefix.length));
  }
  const moved = Object.keys(vault.state.folders ?? {})
    .filter((dir) => dir === oldClean || dir.startsWith(`${oldClean}/`))
    .sort();
  // Children first, as the devices do, so each folder is empty when its delete lands.
  for (const dir of [...moved].reverse()) folderOp(vault.state, deviceId, dir, "delete");
  for (const dir of moved) {
    folderOp(vault.state, deviceId, newClean + dir.slice(oldClean.length), "create");
  }
  await persist();
  schedulePush();
  return newClean;
}

/** Like `LocalVault::delete_folder`: notes go to the parent folder (keep) or to the trash. */
export async function deleteFolder(folder: string, keepContent: boolean): Promise<number> {
  const clean = sanitizeFolderPath(folder);
  if (!clean) throw new Error("No se puede borrar la carpeta raíz");
  const prefix = `${NOTES_DIR}/${clean}/`;
  const parent = clean.includes("/") ? clean.slice(0, clean.lastIndexOf("/")) : "";
  const inside = liveNotes().filter((n) => pathOf(n).startsWith(prefix));
  for (const n of inside) {
    if (keepContent) {
      await setPath(n, dedupPath(folderDir(parent), titleFromPath(pathOf(n)), n.id));
    } else {
      await commit(n.id, "delete", null, n.rev);
    }
  }
  const known = Object.keys(vault.state.folders ?? {}).filter((d) => d.startsWith(`${clean}/`));
  // Children first; the folder itself always, even if no device announced it yet.
  for (const dir of [...known.sort().reverse(), clean]) {
    folderOp(vault.state, deviceId, dir, "delete");
  }
  await persist();
  schedulePush();
  return inside.length;
}

export async function deleteNote(noteId: string): Promise<void> {
  const n = find(noteId);
  await commit(n.id, "delete", null, n.rev);
}

/** Notes deleted on any device, while their content is still kept (30 days). */
export function listTrash(): TrashItemDto[] {
  const now = nowSecs();
  return trashed(vault.state, now).map((n) => {
    const left = TRASH_RETENTION_SECS - (now - (n.deletedAt ?? now));
    return {
      trash_id: n.id,
      note_id: n.id,
      title: titleOf(n),
      deleted_at: n.deletedAt ?? now,
      days_remaining: Math.min(30, Math.max(1, Math.ceil(left / 86_400))),
      size: new TextEncoder().encode(n.doc).length,
    };
  });
}

function findTrashed(trashId: string): WebNote {
  const n = vault.state.notes[trashId];
  if (!n?.deleted || !n.doc) throw new Error(`No está en la papelera: ${trashId}`);
  return n;
}

/**
 * Restore on every device: the content and path on top of the tombstone rev
 * (devices treat that as a restore). Keeps its path unless a live note took it.
 */
export async function restoreTrash(trashId: string): Promise<NoteSummaryDto> {
  const n = findTrashed(trashId);
  const old = pathOf(n);
  const path = liveNotes().some((o) => pathOf(o) === old)
    ? dedupPath(old.slice(0, old.lastIndexOf("/")), titleOf(n))
    : old;
  return summary(await commit(n.id, "update", n.doc, n.rev, path));
}

/** Permanent delete: gone from every trash and from the server. */
export async function deleteTrashItem(trashId: string): Promise<void> {
  const n = findTrashed(trashId);
  await commit(n.id, "delete", null, n.rev, undefined, true);
}

export async function emptyTrash(): Promise<void> {
  for (const n of trashed(vault.state)) await deleteTrashItem(n.id);
}

export function searchNotes(query: string): SearchHitDto[] {
  const q = query.toLowerCase();
  return liveNotes()
    .filter((n) => n.doc.toLowerCase().includes(q))
    .map(summary)
    .map(({ note_id, title, path, format }) => ({ note_id, title, path, format }));
}

/** All matches: `noteTitleSource` fetches the full list once with an empty prefix. */
export function suggestTitles(prefix: string): TitleSuggestionDto[] {
  const p = prefix.toLowerCase();
  return liveNotes()
    .map(summary)
    .filter((s) => s.title.toLowerCase().includes(p))
    .map(({ note_id, title, format }) => ({ note_id, title, format }));
}
