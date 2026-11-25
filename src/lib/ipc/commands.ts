// Typed wrappers around the Tauri IPC commands (src-tauri/src/commands.rs).
// Components call these instead of `invoke("…")` with string names, so a
// renamed command or changed payload is a compile error in one place.

import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import type {
  AttachmentItemDto,
  BacklinkDto,
  FolderDto,
  LinkReferenceDto,
  LinkRepairResultDto,
  NoteContentDto,
  NoteSummaryDto,
  OutlinkDto,
  RebuildReportDto,
  SaveResultDto,
  SearchHitDto,
  SettingsDto,
  SyncConfigDto,
  SyncStatusDto,
  TitleSuggestionDto,
  TrashItemDto,
  VaultBackupBundleDto,
  VaultInfoDto,
  VaultProfileDto,
} from "./types";

/**
 * Every JSON command exposed by the backend: argument object (camelCase, as
 * Tauri converts Rust's snake_case parameters) and result type. Keep in sync
 * with the `#[tauri::command]` signatures in src-tauri/src/commands.rs.
 */
export interface Commands {
  // Vault
  vault_init: { args: void; result: VaultInfoDto };
  vault_open: { args: { path: string }; result: VaultInfoDto };
  vault_create_named: { args: { name: string }; result: VaultInfoDto };
  vault_rename_current: { args: { name: string }; result: VaultInfoDto };
  vault_delete_profile: { args: { path: string }; result: VaultInfoDto };
  vault_list_profiles: { args: void; result: VaultProfileDto[] };
  vault_export_bundle: { args: void; result: VaultBackupBundleDto };
  vault_import_bundle: { args: { bundle: VaultBackupBundleDto }; result: VaultInfoDto };
  vault_get_settings: { args: void; result: SettingsDto };
  vault_update_settings: { args: { settings: SettingsDto }; result: void };
  vault_rebuild_index: { args: void; result: RebuildReportDto };
  vault_pick_folder: { args: void; result: string | null };
  // Folders
  vault_create_folder: { args: { name: string }; result: string };
  vault_rename_folder: { args: { folder: string; newName: string }; result: string };
  vault_delete_folder: { args: { folder: string; keepContent: boolean }; result: number };
  vault_move_folder_to_vault: {
    args: { folder: string; targetVaultPath: string };
    result: number;
  };
  vault_list_folders: { args: void; result: FolderDto[] };
  // Attachments (bytes go through readAttachment / importAttachment below)
  attachment_list: { args: void; result: AttachmentItemDto[] };
  attachment_delete: { args: { path: string }; result: void };
  attachment_delete_orphans: { args: void; result: number };
  // Notes
  note_list: { args: { tag?: string | null; format?: string | null }; result: NoteSummaryDto[] };
  note_create: {
    args: { kind: string; title?: string | null; folder?: string | null };
    result: NoteContentDto;
  };
  note_read: { args: { noteId: string }; result: NoteContentDto };
  note_save: { args: { noteId: string; content: string; baseRev: number }; result: SaveResultDto };
  note_rename: { args: { noteId: string; newTitle: string }; result: NoteSummaryDto };
  note_move: { args: { noteId: string; folder: string }; result: NoteSummaryDto };
  note_delete: { args: { noteId: string }; result: void };
  // Trash
  trash_list: { args: void; result: TrashItemDto[] };
  trash_restore: { args: { trashId: string }; result: NoteSummaryDto };
  trash_delete_item: { args: { trashId: string }; result: void };
  trash_empty: { args: void; result: void };
  // Search and links
  note_search_fts: {
    args: { query: string; tag?: string | null; format?: string | null };
    result: SearchHitDto[];
  };
  note_get_backlinks: { args: { noteId: string }; result: BacklinkDto[] };
  note_get_outlinks: { args: { noteId: string }; result: OutlinkDto[] };
  note_unlinked_mentions: { args: { noteId: string; title: string }; result: BacklinkDto[] };
  note_suggest_titles: { args: { prefix: string }; result: TitleSuggestionDto[] };
  note_check_link_references: { args: { oldTitle: string }; result: LinkReferenceDto[] };
  note_repair_links: { args: { oldTitle: string; newTitle: string }; result: LinkRepairResultDto };
  // Sync
  get_sync_status: { args: void; result: SyncStatusDto };
  sync_now: { args: void; result: SyncStatusDto };
  configure_sync_server: { args: { config: SyncConfigDto }; result: SyncStatusDto };
  pause_sync: { args: void; result: SyncStatusDto };
  resume_sync: { args: void; result: SyncStatusDto };
}

type ArgsOf<K extends keyof Commands> = Commands[K]["args"] extends void
  ? []
  : object extends Commands[K]["args"]
    ? [args?: Commands[K]["args"]]
    : [args: Commands[K]["args"]];

/** Invoke a backend command with arguments and result checked against {@link Commands}. */
export function call<K extends keyof Commands>(
  command: K,
  ...args: ArgsOf<K>
): Promise<Commands[K]["result"]> {
  return invoke(command, args[0] as InvokeArgs | undefined);
}

/**
 * Read an attachment's bytes. The backend answers with a raw IPC response
 * (`ArrayBuffer`); the postMessage fallback transport delivers a number array.
 */
export async function readAttachment(path: string): Promise<Uint8Array<ArrayBuffer>> {
  const data = await invoke<ArrayBuffer | number[]>("vault_read_attachment", { path });
  return data instanceof ArrayBuffer ? new Uint8Array(data) : Uint8Array.from(data);
}

/**
 * Import a file into the vault's attachments folder and return its vault-relative
 * path. Bytes are sent as the raw request body; the filename goes in a header
 * (percent-encoded, since header values must be ASCII).
 */
export async function importAttachment(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  return invoke<string>("vault_import_attachment", bytes, {
    headers: { "x-filename": encodeURIComponent(file.name) },
  });
}
