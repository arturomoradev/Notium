// Pure vault state for the web client, rebuilt from the sync journal in D1.
// Mirrors the device semantics in crates/notium-core/src/sync (Op, Hlc, revs);
// no DOM or network here so it can be unit-tested (tools/web-vault-state.test.mjs).
import * as core from "../core.ts";

export interface Hlc {
  ts: number;
  cnt: number;
}

export type OpKind = "create" | "update" | "delete";

/** Wire format of `/sync/push` and `/sync/pull` (content is a byte array). */
export interface WireOp {
  op_id: string;
  device_id: string;
  note_id: string;
  kind: OpKind | "Create" | "Update" | "Delete";
  base_rev: number;
  new_rev: number;
  content: number[] | null;
  hlc: Hlc;
  ts: number;
  /** Path set by a create/rename/move op (absent on older ops). */
  path?: string;
  /** On a delete: permanent delete, removes the note from every trash. */
  purge?: boolean;
  /** Hash of the content the op was built on (merge base for devices). */
  base_hash?: string;
}

export interface WebNote {
  id: string;
  /** Full file contents: the JSON envelope with the Tiptap doc. */
  doc: string;
  rev: number;
  /** Unix seconds of the last change seen. */
  updated: number;
  deleted: boolean;
  /** Vault-relative path from the latest path op; older notes have none. */
  path?: string;
  /** Unix seconds when it went to the trash (deleted notes only). */
  deletedAt?: number;
}

/** Same retention as the apps' trash (notium-core `TRASH_RETENTION_SECS`). */
export const TRASH_RETENTION_SECS = 30 * 24 * 60 * 60;

/** Deleted notes whose content can still be restored, newest first. */
export function trashed(state: VaultState, now = nowSecs()): WebNote[] {
  return Object.values(state.notes)
    .filter((n) => n.deleted && n.doc && now - (n.deletedAt ?? 0) < TRASH_RETENTION_SECS)
    .sort((a, b) => (b.deletedAt ?? 0) - (a.deletedAt ?? 0));
}

export interface VaultState {
  cursor: string;
  notes: Record<string, WebNote>;
  /** Vault-relative attachment path -> blob hash. */
  blobs: Record<string, string>;
  /** Local ops not yet accepted by the server, in order. */
  pending: WireOp[];
  clock: Hlc;
  /** Note id -> stamp of its latest path change (see `pathStamp`). */
  pathStamps: Record<string, string>;
  /** Folders announced by devices (path under notes/tiptap); absent in older saved states. */
  folders?: Record<string, true>;
}

/** Same as the devices' `FOLDER_OP_PREFIX`: folder ops carry no content. */
export const FOLDER_OP_PREFIX = "folder:";

export const emptyState = (): VaultState => ({
  cursor: "0",
  notes: {},
  blobs: {},
  pending: [],
  clock: { ts: 0, cnt: 0 },
  pathStamps: {},
});

/** Same order as notium-core `path_stamp`: HLC, then device id. */
export const pathStamp = (op: Pick<WireOp, "hlc" | "device_id">): string =>
  `${String(op.hlc.ts).padStart(20, "0")}:${String(op.hlc.cnt).padStart(10, "0")}:${op.device_id}`;

/** Last path change wins (`SyncDb::claim_path`); records the winner. */
function claimPath(state: VaultState, op: WireOp): boolean {
  if (!op.path || !core.isSafeNotePath(op.path)) return false;
  const stamp = pathStamp(op);
  if ((state.pathStamps[op.note_id] ?? "") >= stamp) return false;
  state.pathStamps[op.note_id] = stamp;
  return true;
}

const utf8 = new TextDecoder();
const encoder = new TextEncoder();

export const nowSecs = (): number => Math.floor(Date.now() / 1000);

/** Same as notium-core `MAX_CLOCK_SKEW_SECS`: clocks further ahead are not adopted. */
const MAX_CLOCK_SKEW_SECS = 24 * 60 * 60;

/** Same rule as `Hlc::tick`: devices stamp ops in Unix seconds. */
export function tick(clock: Hlc, wall = nowSecs()): Hlc {
  return wall > clock.ts ? { ts: wall, cnt: 0 } : { ts: clock.ts, cnt: clock.cnt + 1 };
}

/** Apply ops pulled from the server. Returns the ids of notes that changed. */
export function applyRemote(state: VaultState, ops: WireOp[], deviceId: string): string[] {
  const changed: string[] = [];
  for (let op of ops) {
    // Older peers may send NFD names; the devices store NFC ones.
    if (op.path) op = { ...op, path: op.path.normalize("NFC") };
    const kind = op.kind.toLowerCase() as OpKind;
    const note = state.notes[op.note_id];
    if (op.hlc.ts > state.clock.ts && op.hlc.ts <= nowSecs() + MAX_CLOCK_SKEW_SECS) {
      state.clock = { ...op.hlc };
    }
    if (op.note_id.startsWith(FOLDER_OP_PREFIX)) {
      const folder = op.note_id.slice(FOLDER_OP_PREFIX.length).normalize("NFC");
      state.folders ??= {};
      if (kind === "delete") delete state.folders[folder];
      else state.folders[folder] = true;
      changed.push(op.note_id);
      continue;
    }
    // Our own ops are already applied, unless this state was rebuilt from
    // scratch (e.g. a new tab without a remembered session).
    if (op.device_id === deviceId && note && note.rev >= op.new_rev) continue;
    const rev = Math.max(note?.rev ?? 0, op.new_rev);
    if (kind === "delete") {
      // A delete always wins and leaves a tombstone (with the content, for the
      // trash), so a stale or re-delivered content op can't bring the note back
      // (devices reject those too: `ApplyOutcome::RejectedTombstone`). A purge
      // also drops the content: it is gone from every trash.
      if (!note?.deleted || op.purge) changed.push(op.note_id);
      const base = note ?? { id: op.note_id, doc: "", updated: op.hlc.ts };
      state.notes[op.note_id] = {
        ...base,
        doc: op.purge ? "" : base.doc,
        rev,
        deleted: true,
        deletedAt: note?.deleted ? note.deletedAt : op.hlc.ts,
      };
      continue;
    }
    if (!op.content) continue;
    // Only a restore (built on the tombstone rev) brings a deleted note back.
    if (note?.deleted && op.base_rev < note.rev) continue;
    const path = claimPath(state, op) ? op.path : note?.path;
    // Already seen (pull pages can repeat ops) or older than what we show: keep
    // ours. Concurrent edits are merged by a device, which pushes the result
    // (sync.rs `journal_merge_result`).
    // An unpushed local edit also wins here; the device merges both when it
    // pulls our op (base_rev keeps the common ancestor).
    if (
      note &&
      !note.deleted &&
      (op.new_rev <= note.rev || state.pending.some((p) => p.note_id === op.note_id))
    ) {
      if (path !== note.path) changed.push(op.note_id);
      note.rev = rev;
      note.path = path;
      continue;
    }
    state.notes[op.note_id] = {
      id: op.note_id,
      doc: utf8.decode(new Uint8Array(op.content)),
      rev,
      updated: op.hlc.ts || nowSecs(),
      deleted: false,
      path,
    };
    changed.push(op.note_id);
  }
  return changed;
}

/** Record a local op: applied to the state now, queued for the next push. */
export function localOp(
  state: VaultState,
  deviceId: string,
  noteId: string,
  kind: OpKind,
  doc: string | null,
  baseRev: number,
  path?: string,
  purge = false,
): WebNote {
  state.clock = tick(state.clock);
  const prev = state.notes[noteId];
  const newRev = kind === "create" ? 0 : Math.max(prev?.rev ?? 0, baseRev) + 1;
  // The version this op replaces, so devices merge against the right base.
  const baseHash =
    kind !== "create" && prev && !prev.deleted && prev.doc
      ? core.contentHash(encoder.encode(prev.doc))
      : undefined;
  const op: WireOp = {
    op_id: `op_web${crypto.randomUUID().replaceAll("-", "")}`,
    device_id: deviceId,
    note_id: noteId,
    kind,
    base_rev: kind === "create" ? 0 : baseRev,
    new_rev: newRev,
    content: doc === null ? null : Array.from(encoder.encode(doc)),
    hlc: state.clock,
    ts: state.clock.ts,
    ...(path ? { path } : {}),
    ...(purge ? { purge } : {}),
    ...(baseHash ? { base_hash: baseHash } : {}),
  };
  if (path) state.pathStamps[noteId] = pathStamp(op);
  state.pending.push(op);
  const deleted = kind === "delete";
  const note: WebNote = {
    id: noteId,
    doc: purge ? "" : (doc ?? prev?.doc ?? ""),
    rev: newRev,
    updated: state.clock.ts,
    deleted,
    path: path ?? prev?.path,
    ...(deleted ? { deletedAt: prev?.deleted ? prev.deletedAt : state.clock.ts } : {}),
  };
  state.notes[noteId] = note;
  return note;
}

/** Record a local folder create/delete (path under notes/tiptap), queued like `localOp`. */
export function folderOp(
  state: VaultState,
  deviceId: string,
  folder: string,
  kind: "create" | "delete",
): void {
  state.clock = tick(state.clock);
  state.pending.push({
    op_id: `op_web${crypto.randomUUID().replaceAll("-", "")}`,
    device_id: deviceId,
    note_id: FOLDER_OP_PREFIX + folder,
    kind,
    base_rev: 0,
    new_rev: kind === "create" ? 0 : 1,
    content: null,
    hlc: state.clock,
    ts: state.clock.ts,
  });
  state.folders ??= {};
  if (kind === "delete") delete state.folders[folder];
  else state.folders[folder] = true;
}

/** The editor's part of a note file: its Tiptap doc as JSON. */
export const docOf = (file: string): string => core.docJson(file);

/** Same as the native save (commands.rs `save_impl`): swap the doc, keep the id, bump `updated`. Throws on a malformed doc. */
export function stitchDoc(previous: string, doc: string, noteId: string, ts = nowSecs()): string {
  const full = core.injectNotiumId(core.withDoc(previous, doc), noteId);
  return core.upsertField(full, "updated", String(ts));
}

/** A new, empty note file (vault.rs `create_note_in`). */
export const newDoc = (noteId: string, ts = nowSecs()): string => core.newNote(noteId, String(ts));

const extracted = new Map<string, { doc: string; info: core.Extracted }>();
const unreadable: core.Extracted = {
  title: "",
  tags: [],
  plain_text: "",
  out_links: [],
  attachment_refs: [],
};

/** Native extraction of a note (title, tags, links, plain text), cached per content. */
export function infoOf(note: WebNote): core.Extracted {
  const hit = extracted.get(note.id);
  if (hit?.doc === note.doc) return hit.info;
  // A file that is not a note (e.g. a malformed op) must not break the listing.
  let info = unreadable;
  try {
    info = core.extractNote(note.id, note.doc);
  } catch {
    /* listed by its path, without tags or links */
  }
  extracted.set(note.id, { doc: note.doc, info });
  return info;
}

/**
 * Display title: the file name, as in the native app. Notes whose ops predate
 * synced paths fall back to the first heading, which is what devices name
 * such files after (sync.rs `derive_note_title`).
 */
export function titleOf(note: WebNote): string {
  if (note.path) return core.titleFromPath(note.path);
  const heading = infoOf(note).title.trim();
  if (heading) return heading;
  const line = infoOf(note)
    .plain_text.split("\n")
    .find((l) => l.trim())
    ?.trim();
  return line ? line.slice(0, 80) : `Nota ${note.id.slice(0, 8)}`;
}

export const tagsOf = (note: WebNote): string[] => infoOf(note).tags;
