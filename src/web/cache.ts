// IndexedDB copy of the vault state, so a remembered session opens instantly
// and pulls only what changed. One record per vault id.
import type { VaultState } from "./backend/state.ts";

const DB = "notium-web";
const STORE = "vaults";

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function run<T>(
  mode: IDBTransactionMode,
  fn: (s: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await open();
  return new Promise((resolve, reject) => {
    const req = fn(db.transaction(STORE, mode).objectStore(STORE));
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function loadCache(vaultId: string): Promise<VaultState | null> {
  try {
    return ((await run("readonly", (s) => s.get(vaultId))) as VaultState | undefined) ?? null;
  } catch {
    return null; // Private mode or blocked storage: start from a full pull.
  }
}

export async function saveCache(vaultId: string, state: VaultState): Promise<void> {
  try {
    await run("readwrite", (s) => s.put(state, vaultId));
  } catch {
    // Cache only; the server copy is the source of truth.
  }
}

export async function clearCache(): Promise<void> {
  try {
    await run("readwrite", (s) => s.clear());
  } catch {
    // Nothing cached.
  }
}
