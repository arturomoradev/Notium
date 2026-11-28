// Sync engine of the web client: owns the in-memory vault and talks to the
// Cloudflare Worker (`/sync/push`, `/sync/pull`, `/sync/blob`). Same protocol
// as the devices (src-tauri/src/sync.rs), so web edits are ordinary ops.
import { emit } from "@tauri-apps/api/event";
import type { SyncStatusDto } from "../../lib/ipc/types.ts";
import { saveCache } from "../cache.ts";
import { contentHash } from "../core.ts";
import { clearSession, type Session } from "../session.ts";
import { applyRemote, emptyState, type VaultState, type WireOp } from "./state.ts";

const PUSH_DELAY_MS = 1500;
/** Server page size of `/sync/pull` (server/cloudflare-sync/src/index.ts). */
const PULL_PAGE = 500;
/** Same limits as the devices (notium-core `INLINE_CONTENT_MAX`, `MAX_NOTE_BYTES`). */
const INLINE_CONTENT_MAX = 1_000_000;
const MAX_NOTE_BYTES = 5_000_000;
/** Inline content per push request: JSON inflates it, and the Worker holds it all. */
const PUSH_CHUNK_BYTES = 4_000_000;

export const deviceId = (() => {
  const key = "notium-web-device";
  try {
    let id = localStorage.getItem(key);
    if (!id) localStorage.setItem(key, (id = `web-${crypto.randomUUID().slice(0, 8)}`));
    return id;
  } catch {
    return `web-${crypto.randomUUID().slice(0, 8)}`;
  }
})();

/** The open vault. `session` is set once by `installWebBackend`. */
export const vault: { session: Session; state: VaultState } = {
  session: { token: "", vaultId: "default", remember: false },
  state: emptyState(),
};

export const status: SyncStatusDto = {
  enabled: true,
  server_url: globalThis.location?.origin ?? "",
  device_id: deviceId,
  vault_id: "default",
  state: "syncing",
  last_synced_at: null,
  pending_ops: 0,
  last_error: null,
};

/** Authenticated request to the Worker; a 401 ends the session. */
export async function api(path: string, init: RequestInit = {}): Promise<Response> {
  const res = await fetch(path, {
    ...init,
    headers: { ...init.headers, Authorization: `Bearer ${vault.session.token}` },
  });
  if (res.status === 401) {
    clearSession();
    location.reload();
  }
  if (!res.ok) throw new Error(`Servidor: ${res.status} ${await res.text()}`);
  return res;
}

export const vaultQuery = (): string => `vault_id=${encodeURIComponent(vault.session.vaultId)}`;

/** Save the state to IndexedDB (remembered sessions only). */
export async function persist(): Promise<void> {
  status.pending_ops = vault.state.pending.length;
  if (vault.session.remember) await saveCache(vault.session.vaultId, vault.state);
}

let cycle: Promise<void> = Promise.resolve();

/** Push queued ops, then pull until caught up. Cycles never overlap. */
export function sync(): Promise<void> {
  cycle = cycle.then(runSync, runSync);
  return cycle;
}

async function runSync(): Promise<void> {
  const { state } = vault;
  let rejected = 0;
  status.state = "syncing";
  void emit("sync://status-changed", { ...status });
  try {
    for (const sent of await pushBatches(state.pending)) {
      const res = await api("/sync/push", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ vault_id: vault.session.vaultId, device_id: deviceId, ops: sent }),
      });
      const { accepted_op_ids, rejected_op_ids = [] } = (await res.json()) as {
        accepted_op_ids: string[];
        rejected_op_ids?: string[];
      };
      const accepted = new Set(accepted_op_ids);
      state.pending = state.pending.filter((op) => !accepted.has(op.op_id));
      await persist();
      // Rejected ops stay queued; say so instead of looking synced.
      rejected += rejected_op_ids.length;
    }
    const tooBig = state.pending.filter((op) => (op.content?.length ?? 0) > MAX_NOTE_BYTES).length;
    const changed: string[] = [];
    for (;;) {
      const res = await api(`/sync/pull?since=${state.cursor}&${vaultQuery()}`);
      const page = (await res.json()) as {
        cursor: string;
        ops: WireOp[];
        blobs: { hash: string; path: string; deleted?: boolean }[];
      };
      // A failed download throws: the cursor stays and the page is read again.
      changed.push(...applyRemote(state, await fetchContents(page.ops), deviceId));
      for (const b of page.blobs) {
        if (b.deleted) delete state.blobs[b.path];
        else state.blobs[b.path] = b.hash;
      }
      const done =
        page.cursor === state.cursor ||
        (page.ops.length < PULL_PAGE && page.blobs.length < PULL_PAGE);
      state.cursor = page.cursor;
      if (done) break;
    }
    await persist();
    status.state = rejected || tooBig ? "error" : "synced";
    status.last_error = tooBig
      ? `${tooBig} cambios no se sincronizan: la nota supera 5 MB`
      : rejected
        ? `El servidor rechazó ${rejected} cambios; siguen pendientes`
        : null;
    status.last_synced_at = Math.floor(Date.now() / 1000);
    if (changed.length) void emit("vault-changed", []);
  } catch (e) {
    status.state = navigator.onLine ? "error" : "offline";
    status.last_error = e instanceof Error ? e.message : String(e);
  }
  void emit("sync://status-changed", { ...status });
}

const contentPath = (noteId: string, hash: string): string =>
  `/sync/content/${encodeURIComponent(noteId)}/${hash}?${vaultQuery()}`;

/**
 * Group queued ops into push requests, uploading contents over
 * INLINE_CONTENT_MAX first (the op then names them in `content_ref`). A note
 * over MAX_NOTE_BYTES stays queued, with its later ops: they build on it.
 */
async function pushBatches(pending: WireOp[]): Promise<WireOp[][]> {
  const batches: WireOp[][] = [];
  const held = new Set<string>();
  let bytes = 0;
  for (const op of pending) {
    const len = op.content?.length ?? 0;
    if (held.has(op.note_id) || len > MAX_NOTE_BYTES) {
      held.add(op.note_id);
      continue;
    }
    let wire = op;
    if (len > INLINE_CONTENT_MAX) {
      const data = new Uint8Array(op.content ?? []);
      const hash = contentHash(data);
      await api(contentPath(op.note_id, hash), {
        method: "PUT",
        headers: { "Content-Type": "application/octet-stream" },
        body: data,
      });
      wire = { ...op, content: null, content_ref: hash };
    }
    const inline = wire.content?.length ?? 0;
    const last = batches.at(-1);
    if (!last || last.length >= 50 || (last.length && bytes + inline > PUSH_CHUNK_BYTES)) {
      batches.push([]);
      bytes = 0;
    }
    bytes += inline;
    batches.at(-1)!.push(wire);
  }
  return batches;
}

/** Fill in contents named by `content_ref`; drops ops whose content is gone (purged). */
async function fetchContents(ops: WireOp[]): Promise<WireOp[]> {
  const out: WireOp[] = [];
  for (const op of ops) {
    if (!op.content_ref || op.content) {
      out.push(op);
      continue;
    }
    const res = await fetch(contentPath(op.note_id, op.content_ref), {
      headers: { Authorization: `Bearer ${vault.session.token}` },
    });
    if (res.status === 404) continue;
    if (!res.ok) throw new Error(`Servidor: ${res.status} ${await res.text()}`);
    const data = new Uint8Array(await res.arrayBuffer());
    if (contentHash(data) !== op.content_ref) continue;
    const { content_ref: _, ...rest } = op;
    out.push({ ...rest, content: Array.from(data) });
  }
  return out;
}

let pushTimer: ReturnType<typeof setTimeout> | undefined;

/** Push soon, batching a burst of autosaves into one request. */
export function schedulePush(): void {
  clearTimeout(pushTimer);
  pushTimer = setTimeout(() => void sync(), PUSH_DELAY_MS);
}
