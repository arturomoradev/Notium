// IPC DTOs — mirror src-tauri/src/dto.rs (serde snake_case output).

export interface VaultInfoDto {
  root: string;
  name: string;
  notes: number;
  attachments: number;
  folders: number;
  opened: boolean;
  is_first_run?: boolean;
}

export interface VaultProfileDto {
  name: string;
  path: string;
  active: boolean;
}

export interface VaultBackupEntryDto {
  rel_path: string;
  /** Base64. Bundles exported by older versions carry a number array instead. */
  bytes: string | number[];
}

export interface VaultBackupBundleDto {
  vault_name: string;
  exported_at: string;
  entries: VaultBackupEntryDto[];
}

export interface FolderDto {
  path: string;
}

export interface NoteSummaryDto {
  note_id: string;
  title: string;
  format: "tiptap";
  path: string;
  updated_at: string | null;
  tags: string[];
}

export interface NoteContentDto {
  note: NoteSummaryDto;
  content: string;
  rev: number;
}

export interface SaveResultDto {
  rev: number;
  note: NoteSummaryDto;
}

export interface BacklinkDto {
  source_id: string;
  source_title: string;
  link_text: string;
  kind: string;
  snippet: string;
}

export interface OutlinkDto {
  target_raw: string;
  link_text: string;
  kind: string;
  is_attachment: boolean;
  target_id: string | null;
  target_title: string | null;
  target_path: string | null;
  is_resolved: boolean;
}

export interface LinkReferenceDto {
  note_id: string;
  title: string;
  path: string;
  count: number;
}

export interface LinkRepairResultDto {
  affected_notes_count: number;
  updated_links_count: number;
}

export interface SearchHitDto {
  note_id: string;
  title: string;
  path: string;
  format: string;
}

export interface TitleSuggestionDto {
  note_id: string;
  title: string;
  format: string;
}

export interface TrashItemDto {
  trash_id: string;
  note_id: string | null;
  title: string;
  deleted_at: number;
  days_remaining: number;
  size: number;
}

export interface SettingsDto {
  source_mode: boolean;
  /** `"system"` follows the device's light/dark setting. */
  theme: "dark" | "light" | "system";
  font_size: number;
  /** Resolution new image attachments are scaled down to. */
  image_quality: ImageQuality;
}

export type ImageQuality = "720p" | "1080p" | "1440p";

export interface RebuildReportDto {
  scanned: number;
  adopted: number;
  updated: number;
  tombstoned: number;
}

export interface AttachmentNoteRefDto {
  note_id: string;
  title: string;
}

export interface AttachmentItemDto {
  att_id: string;
  path: string;
  filename: string;
  file_size: number;
  content_hash: string;
  is_orphan: boolean;
  referencing_notes: AttachmentNoteRefDto[];
}

export interface SyncConfigDto {
  server_url: string;
  auth_token: string;
  device_id: string;
  vault_id?: string;
  enabled: boolean;
  auto_sync: boolean;
  interval_secs: number;
}

export type SyncState = "synced" | "syncing" | "offline" | "error" | "paused";

export interface SyncStatusDto {
  enabled: boolean;
  server_url: string;
  device_id: string;
  vault_id: string;
  state: SyncState;
  last_synced_at: number | null;
  pending_ops: number;
  last_error: string | null;
}
