# Fork roadmap: `experiment/html-editor`

Parallel experiment: Tiptap (WYSIWYG) editor with notes stored as Tiptap JSON.
Nothing here ships to the web, the sync server or `main` until the user decides
the editor is worth it (see the fork exception in `AGENTS.md`).

## Done

- Tiptap 3 replaces CodeMirror 6 (`src/components/editors/noteEngine.ts`).
- Notes are `.json` files under `notes/tiptap/`: an envelope
  `{ notium_id, created, updated, tags?, doc }` around the Tiptap doc
  (`crates/notium-core/src/envelope.rs`). Markdown support was removed.
- Rust extractor for the JSON (`extract/tiptap.rs`): titles, tags, `[[links]]`,
  embeds, attachments and chunks for search and backlinks.
- LaTeX (`@tiptap/extension-mathematics` + KaTeX) and Mermaid diagrams
  (`mermaid` code blocks, loaded on demand). `$…$` / `$$…$$` text from old notes
  becomes math nodes on open (`noteMigration.ts`).
- Callouts are a `callout` node (type, title, fold) and code blocks are highlighted
  (lowlight) with a language label and a copy button; `> [!type]` quotes from old
  notes become callouts on open.
- Desktop, Android and web backends save and read the doc; demo notes and the
  dev mock are JSON (`src/lib/demoNotes.json`).

## Pending before sync can be turned on

- [ ] **Structural merge for JSON notes.** `sync::merge::merge_md` merges line by
      line; on pretty-printed JSON a merge that breaks the file is treated as a
      conflict (winner + copy). Replace it with a merge over Tiptap blocks
      (match top-level blocks by content/position, 3-way per block).
- [ ] Remove the fork bridge in `src-tauri/src/sync.rs` (`load_sync_config` forces
      sync off; config file `sync_config.html-fork.json`, keyring service
      `dev.notium.html.sync`) and point the fork at a test sync server or account.
- [ ] Sync server (`server/cloudflare-sync`): check that nothing assumes `.md`
      paths or Markdown content before deploying anything from this branch.

## Editor gaps (vs. the CodeMirror editor on `main`)

- [ ] LaTeX and diagram sources are edited with `window.prompt` (math) and the
      raw code block (Mermaid); an inline editor would be nicer, especially on Android.
- [ ] Note embeds (`![[Nota]]`) show a chip, not the embedded content.
- [ ] Source mode shows the same WYSIWYG view (there is no Markdown source now;
      decide whether to drop the setting or show the JSON).
- [ ] `#tag` autocompletion.
- [ ] External links use `window.prompt`; Android may not show it. Use an
      in-app popover.
- [ ] Unused `.cm-*` rules in `src/app.css`; the "Marcaciones Markdown" help in
      `SettingsModal.svelte` describes Markdown syntax.

## Not verified yet

- [ ] Android build with the new package `dev.notium.html` (Tauri regenerates
      the `generated/` Kotlin on the first build).
- [ ] Desktop build `NotiumHTML.exe` end to end (create, link, rename, search).
