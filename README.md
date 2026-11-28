<h1 align="center">
  <img src="notium-icon.svg" alt="" height="40" align="absmiddle">
  Notium
</h1>

<p align="center"><strong>English</strong> | <a href="README.es.md">Español</a></p>

Local-first Markdown note app — **Phase 1 complete** (production monorepo, Obsidian-style Markdown editor with live preview, formatting toolbar, watcher, Android).

- **Framework:** Tauri 2 (Rust) + Svelte 5 / TypeScript
- **Core:** `crates/notium-core` — headless vault, Markdown extractor (`.md`), SQLite FTS5 index, sync primitives
- **Shell:** `src-tauri` — typed IPC commands (vault/notes/links/search/settings)

## Prerequisites

- Rust (MSVC toolchain on Windows)
- Node.js ≥ 20 and npm
- Android: JDK, Android SDK + NDK 28.2 (for the APK)

## Development (desktop shell)

```powershell
npm install          # once
npm run tauri dev    # launches vite (port 1420) + the Tauri desktop window
```

## Checks

```powershell
cargo test --release     # Rust unit + integration tests
npm run check            # svelte-check (0 errors / 0 warnings)
npm run build            # vite production build (outputs dist/)
```

## Android release APK

```powershell
$env:ANDROID_HOME = "<ruta del Android SDK>"; $env:NDK_HOME = "$env:ANDROID_HOME\ndk\<versión del NDK>"
npm run tauri android build -- --apk --target aarch64 --ci
adb install -r "src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk"
adb shell am start -n dev.notium.app/.MainActivity
```

Gradle signs the release APK using `src-tauri/gen/android/keystore.properties`
(untracked — create it with `password`, `keyAlias` and `storeFile`; see the
[Tauri Android signing guide](https://v2.tauri.app/distribute/sign/android/)).

## Features (Phase 1)

- **Obsidian-style `.md` editor** — CodeMirror 6 with Live Preview (cursor-aware syntax hiding),
  Source and Reading view modes, `[[Wiki Link]]` autocomplete + click navigation.
- **Formatting toolbar** (desktop + mobile) with tables, tasks, links, embeds, tags, and attachments.
- **FTS search palette** (Ctrl+K), unified **backlinks panel**, filesystem watcher, responsive shell.

