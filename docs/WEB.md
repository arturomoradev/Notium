# Notium Web (`docs/WEB.md`)

The same Svelte app, run in a browser with no PC or phone. It is served by the
`notium-web` Worker (`server/notium-web`), which sits behind Cloudflare Access
and forwards `/sync/*` to `notium-sync` (see `docs/CLOUDFLARE_SYNC_WORKER.md`
§6). Instead of Tauri it uses the sync Worker as its backend:

- notes are rebuilt from the D1 journal (`GET /sync/pull`);
- edits go back as ordinary ops (`POST /sync/push`).

The PC and the phone apply web edits on their next sync, like a change from any
other device.

## Isolation from the native app

| Piece | Native (Tauri) | Web |
|---|---|---|
| HTML entry | `index.html` → `src/main.ts` | `web/index.html` → `src/web/main.ts` |
| Vite config | `vite.config.ts` → `dist/` | `web/vite.config.ts` → `dist-web/` |
| Type check | `npm run check` (excludes `src/web`) | `npm run check:web` (`src/web/tsconfig.json`) |
| notium-core | `native` feature (default) | `crates/notium-wasm` with `default-features = false` |

The only shared-code switch is in `App.svelte`. Its check
`import.meta.env.VITE_NOTIUM_WEB` is only defined by the web build, and it skips
seeding the demo notes into the synced vault.

## Layout of `src/web`

- `main.ts`: login (`WebLogin.svelte`, `session.ts`), the "Salir" button and backend startup.
- `cache.ts`: copy of the vault in IndexedDB, only when "Recordar en este dispositivo" is ticked.
- `core.ts`: typed wrapper over the WASM. It provides extraction (title, tags, links), BLAKE3, frontmatter, `normalize_title`, wikilink rewriting, `merge_md` and the path helpers. Its bindings are generated in `wasm/pkg` by `npm run build:wasm` and are not committed.
- `backend/index.ts`: the table of IPC command handlers, type-checked against `Commands` (`src/lib/ipc/commands.ts`). It uses `mockIPC`; `vault_import_attachment` is intercepted so its `x-filename` header is not lost.
- `backend/state.ts`: pure state (ops → notes, revs, HLC, paths). It has no DOM and is tested.
- `backend/sync.ts`: push/pull cycle (30 s, on returning to the tab, 1.5 s after each save).
- `backend/notes.ts`, `links.ts`, `attachments.ts`: the commands, mirroring the native implementations in `src-tauri/src/commands.rs`.

### Adding a command to the web

1. Implement it in the matching `backend/*.ts` module. Reuse `core.ts` when the logic already exists in Rust.
2. Register it in `handlers` in `backend/index.ts`. TypeScript checks its arguments and result.
3. Add a test in `tools/web-*.test.mjs`.
4. Commands with no handler respond "No disponible en la versión web (comando)".

If the logic lives in Rust and is pure, expose it from `crates/notium-wasm` instead of rewriting it in TypeScript.

## Paths, renames and folders

A sync op can carry `path`. Only create, rename, move and folder-rename ops set it.

- **Ordering:** the latest change by `(HLC, device_id)` wins on every device (`SyncDb::claim_path`, and `claimPath` in the web). This makes all devices converge on the same path.
- **Native app:** it only applies safe paths (`paths::is_safe_note_rel_path`) that are not already taken.
- **Older versions:** they ignore the field, and the op behaves as a content update with no changes.
- **Empty folders:** a folder only syncs if it contains notes. A folder created empty on the web lives only in that tab.

## Trash

The trash is shared by every device and the web:

- **Delete:** each device moves the note to its own trash. The web keeps the deleted content in its state.
- **Restore:** a content op with the path, built on the tombstone rev (`base_rev` ≥ the delete's `new_rev`). Peers restore it from their trash, or recreate it if they never had it. Stale edits (lower `base_rev`) stay rejected.
- **Permanent delete / empty trash:** a delete op with `purge: true`. Peers drop their trash copies (never a live note), and the Worker erases the note's content history, keeping only its delete ops.
- **30 days** (`TRASH_RETENTION_SECS`): devices purge their local trash themselves, the web hides older items, and the Worker's daily cron erases the history of notes deleted over 30 days ago.

## Scripts

```bash
npm run build:wasm   # cargo (wasm32) + wasm-bindgen -> src/web/wasm/pkg
npm run dev:web      # http://localhost:1430, proxies /sync to 127.0.0.1:8787
npm run build:web    # dist-web/
npm run check:web    # svelte-check of src/web
npm run test:web     # tools/web-*.test.mjs (pure state, links, WASM)
```

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked   # same version as crates/notium-wasm
```

### Local Worker for development

Run this without touching `.dev.vars` or the local D1:

```bash
cd server/cloudflare-sync
npx wrangler d1 execute notium-db --local --persist-to ../../target/d1-dev --file=schema.sql
npx wrangler dev --persist-to ../../target/d1-dev --var AUTH_TOKEN:test-token
```

## Not supported on the web yet

- Deleting or moving folders to another vault.
- Multiple vaults.
- File sizes in the attachments panel.
- Frontmatter aliases when resolving links.
- Merging inside the web itself: an edit made on the web while another device also edited the note is merged by that device.
