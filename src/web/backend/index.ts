/**
 * Browser backend: answers the app's IPC commands (src/lib/ipc/commands.ts)
 * from the Cloudflare sync Worker instead of a local vault. Notes are rebuilt
 * from the D1 journal and edits go back as ops, so the PC and phone apply them
 * like changes from any other device. See docs/WEB.md.
 *
 * To support another command, add it to `handlers`: its arguments and result
 * are type-checked against `Commands`.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Commands } from "../../lib/ipc/commands.ts";
import type { SettingsDto, VaultInfoDto } from "../../lib/ipc/types.ts";
import { loadCache } from "../cache.ts";
import type { Session } from "../session.ts";
import { initCore } from "../core.ts";
import { importAttachment, listAttachments, readAttachment } from "./attachments.ts";
import { backlinks, linkReferences, outlinks, repairLinks, unlinkedMentions } from "./links.ts";
import {
  createFolder,
  createNote,
  deleteFolder,
  deleteNote,
  deleteTrashItem,
  emptyTrash,
  find,
  listFolders,
  listNotes,
  listTrash,
  liveNotes,
  pathOf,
  moveNote,
  readNote,
  renameFolder,
  renameNote,
  restoreTrash,
  saveNote,
  searchNotes,
  suggestTitles,
} from "./notes.ts";
import { emptyState, titleOf } from "./state.ts";
import { status, sync, vault } from "./sync.ts";

const SYNC_INTERVAL_MS = 30_000;
const SETTINGS_KEY = "notium-web-settings";

function loadSettings(): SettingsDto {
  const defaults: SettingsDto = {
    source_mode: false,
    theme: "system",
    font_size: 15,
    image_quality: "1080p",
  };
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") };
  } catch {
    return defaults;
  }
}

type Handlers = {
  [K in keyof Commands]?: (
    args: Commands[K]["args"] extends void ? Record<string, never> : Commands[K]["args"],
  ) => Commands[K]["result"] | Promise<Commands[K]["result"]>;
};

const statusCopy = () => ({ ...status });

const handlers: Handlers = {
  vault_init: (): VaultInfoDto => ({
    root: "web",
    name: "Notium Web",
    notes: liveNotes().length,
    attachments: Object.keys(vault.state.blobs).length,
    folders: 0,
    opened: true,
    is_first_run: false,
  }),
  vault_get_settings: loadSettings,
  vault_update_settings: ({ settings }) => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Blocked storage: settings last for this tab only.
    }
  },
  vault_list_profiles: () => [{ name: "Notium Web", path: "web", active: true }],
  note_list: ({ tag }) => listNotes(tag),
  note_read: ({ noteId }) => readNote(noteId),
  note_save: ({ noteId, content, baseRev }) => saveNote(noteId, content, baseRev),
  note_create: ({ title, folder }) => createNote(title, folder),
  note_rename: ({ noteId, newTitle }) => renameNote(noteId, newTitle),
  note_move: ({ noteId, folder }) => moveNote(noteId, folder),
  vault_list_folders: listFolders,
  vault_create_folder: ({ name }) => createFolder(name),
  vault_rename_folder: ({ folder, newName }) => renameFolder(folder, newName),
  vault_delete_folder: ({ folder, keepContent }) => deleteFolder(folder, keepContent),
  note_delete: ({ noteId }) => deleteNote(noteId),
  note_search_fts: ({ query }) => searchNotes(query),
  note_suggest_titles: ({ prefix }) => suggestTitles(prefix),
  sync_now: async () => (await sync(), statusCopy()),
  get_sync_status: statusCopy,
  configure_sync_server: statusCopy,
  pause_sync: statusCopy,
  resume_sync: statusCopy,
  note_get_backlinks: ({ noteId }) => backlinks(liveNotes(), pathOf, noteId),
  note_get_outlinks: ({ noteId }) => outlinks(liveNotes(), pathOf, find(noteId), vault.state.blobs),
  note_unlinked_mentions: ({ noteId, title }) =>
    unlinkedMentions(liveNotes(), pathOf, noteId, title),
  note_check_link_references: ({ oldTitle }) => linkReferences(liveNotes(), pathOf, oldTitle),
  note_repair_links: async ({ oldTitle, newTitle }) => {
    const { edits, result } = repairLinks(liveNotes(), oldTitle, newTitle);
    for (const { note, body } of edits) await saveNote(note.id, body, note.rev);
    return result;
  },
  attachment_list: () => listAttachments(liveNotes(), titleOf),
  trash_list: listTrash,
  trash_restore: ({ trashId }) => restoreTrash(trashId),
  trash_delete_item: ({ trashId }) => deleteTrashItem(trashId),
  trash_empty: emptyTrash,
};

/** Commands sent with raw `invoke` (bytes), outside `Commands`. */
const rawHandlers: Record<string, (args: Record<string, unknown>) => Promise<unknown>> = {
  vault_read_attachment: ({ path }) => readAttachment(String(path)),
};

/**
 * `mockIPC` drops invoke options, but `importAttachment` (lib/ipc/commands.ts)
 * sends the filename as a header and the bytes as the payload: intercept it.
 */
function interceptAttachmentImport(): void {
  type Invoke = (cmd: string, args?: unknown, options?: { headers?: unknown }) => Promise<unknown>;
  const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
    .__TAURI_INTERNALS__;
  const inner = internals.invoke;
  internals.invoke = (cmd, args, options) => {
    if (cmd !== "vault_import_attachment") return inner(cmd, args, options);
    const headers = new Headers(options?.headers as HeadersInit | undefined);
    const name = decodeURIComponent(headers.get("x-filename") ?? "attachment");
    return importAttachment(args as Uint8Array<ArrayBuffer>, name);
  };
}

async function handle(cmd: string, payload: unknown): Promise<unknown> {
  const args = (payload ?? {}) as Record<string, unknown>;
  const handler = (handlers as Record<string, ((a: unknown) => unknown) | undefined>)[cmd];
  if (handler) return handler(args);
  if (rawHandlers[cmd]) return rawHandlers[cmd](args);
  throw new Error(`No disponible en la versión web (${cmd})`);
}

/** Load the cached vault (if any), sync once, and answer IPC from then on. */
export async function installWebBackend(session: Session): Promise<void> {
  vault.session = session;
  status.vault_id = session.vaultId;
  // Spread over the defaults so caches from older versions gain new fields.
  const cached = session.remember ? await loadCache(session.vaultId) : null;
  vault.state = { ...emptyState(), ...cached };
  await initCore();
  mockIPC(handle, { shouldMockEvents: true });
  interceptAttachmentImport();
  await sync();
  setInterval(() => document.visibilityState === "visible" && void sync(), SYNC_INTERVAL_MS);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void sync();
  });
  // Unpushed edits only live in this tab when the session isn't remembered.
  window.addEventListener("beforeunload", (e) => {
    if (vault.state.pending.length) e.preventDefault();
  });
}
