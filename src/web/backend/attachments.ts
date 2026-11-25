// Attachments on the web: stored as blobs in D1 (`/sync/blob/:hash`), keyed by
// the same BLAKE3 hash the devices check before downloading (sync.rs).
import type { AttachmentItemDto } from "../../lib/ipc/types.ts";
import { contentHash } from "../core.ts";
import { infoOf, type WebNote } from "./state.ts";
import { api, persist, vault, vaultQuery } from "./sync.ts";

/** Worker limit per D1 row (server/cloudflare-sync/src/index.ts). */
const MAX_BLOB_BYTES = 900_000;

/** Same naming as the native `vault_import_attachment`: `<stem>-<uuid>.<ext>`. */
export function attachmentPath(filename: string): string {
  const base = filename.split(/[\\/]/).pop() || "attachment";
  const dot = base.lastIndexOf(".");
  const [stem, ext] = dot > 0 ? [base.slice(0, dot), base.slice(dot + 1)] : [base, "bin"];
  const safe = (s: string) =>
    [...s]
      .filter((c) => /[\p{L}\p{N}_-]/u.test(c))
      .slice(0, 64)
      .join("");
  const unique = crypto.randomUUID().replaceAll("-", "");
  return `attachments/${safe(stem) || "attachment"}-${unique}.${safe(ext) || "bin"}`;
}

export async function importAttachment(
  bytes: Uint8Array<ArrayBuffer>,
  filename: string,
): Promise<string> {
  if (bytes.byteLength > MAX_BLOB_BYTES) {
    throw new Error(
      `El adjunto ocupa ${Math.round(bytes.byteLength / 1000)} KB; el máximo en la nube es 900 KB.`,
    );
  }
  const path = attachmentPath(filename);
  const hash = contentHash(bytes);
  await api(`/sync/blob/${hash}?${vaultQuery()}&path=${encodeURIComponent(path)}`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: bytes,
  });
  vault.state.blobs[path] = hash;
  await persist();
  return path;
}

export async function readAttachment(path: string): Promise<ArrayBuffer> {
  const hash = vault.state.blobs[path];
  if (!hash) throw new Error(`Adjunto no sincronizado: ${path}`);
  return (await api(`/sync/blob/${encodeURIComponent(hash)}?${vaultQuery()}`)).arrayBuffer();
}

/** Synced attachments with the notes that reference them. */
export function listAttachments(notes: WebNote[], titleOf: (n: WebNote) => string) {
  const refs = new Map<string, { note_id: string; title: string }[]>();
  for (const n of notes) {
    for (const p of infoOf(n).attachment_refs) {
      const list = refs.get(p) ?? [];
      list.push({ note_id: n.id, title: titleOf(n) });
      refs.set(p, list);
    }
  }
  return Object.entries(vault.state.blobs)
    .map(([path, hash]): AttachmentItemDto => {
      const referencing = refs.get(path) ?? [];
      return {
        att_id: hash,
        path,
        filename: path.slice(path.lastIndexOf("/") + 1),
        // shortcut: the pull feed carries no sizes; upgrade if the panel needs them.
        file_size: 0,
        content_hash: hash,
        is_orphan: referencing.length === 0,
        referencing_notes: referencing,
      };
    })
    .sort((a, b) => a.filename.localeCompare(b.filename));
}
