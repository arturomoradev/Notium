// Sync engine of the web client: owns the in-memory vault and talks to the
// Cloudflare Worker (`/sync/push`, `/sync/pull`, `/sync/blob`). Same protocol
// as the devices (src-tauri/src/sync.rs), so web edits are ordinary ops.
import { emit } from "@tauri-apps/api/event";
import type { SyncStatusDto } from "../../lib/ipc/types.ts";
import { saveCache } from "../cache.ts";
import { clearSession, type Session } from "../session.ts";
import { applyRemote, emptyState, type VaultState, type WireOp } from "./state.ts";

const PUSH_DELAY_MS = 1500;
/** Server page size of `/sync/pull` (server/cloudflare-sync/src/index.ts). */
const PULL_PAGE = 500;

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
    if (state.pending.length) {
      const sent = state.pending.slice();
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
      if (rejected_op_ids.length) rejected = rejected_op_ids.length;
    }
    const changed: string[] = [];
    for (;;) {
      const res = await api(`/sync/pull?since=${state.cursor}&${vaultQuery()}`);
      const page = (await res.json()) as {
        cursor: string;
        ops: WireOp[];
        blobs: { hash: string; path: string }[];
      };
      changed.push(...applyRemote(state, page.ops, deviceId));
      for (const b of page.blobs) state.blobs[b.path] = b.hash;
      const done =
        page.cursor === state.cursor ||
        (page.ops.length < PULL_PAGE && page.blobs.length < PULL_PAGE);
      state.cursor = page.cursor;
      if (done) break;
    }
    await persist();
    status.state = rejected ? "error" : "synced";
    status.last_error = rejected
      ? `El servidor rechazó ${rejected} cambios (¿nota demasiado grande?); siguen pendientes`
      : null;
    status.last_synced_at = Math.floor(Date.now() / 1000);
    if (changed.length) void emit("vault-changed", []);
  } catch (e) {
    status.state = navigator.onLine ? "error" : "offline";
    status.last_error = e instanceof Error ? e.message : String(e);
  }
  void emit("sync://status-changed", { ...status });
}

let pushTimer: ReturnType<typeof setTimeout> | undefined;

/** Push soon, batching a burst of autosaves into one request. */
export function schedulePush(): void {
  clearTimeout(pushTimer);
  pushTimer = setTimeout(() => void sync(), PUSH_DELAY_MS);
}
