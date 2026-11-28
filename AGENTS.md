# Notium Project Guidelines (`AGENTS.md`)

These guidelines apply permanently and automatically in all sessions on the `Notium_1.0` repository.

Machine-specific values (repo name, production URL, `<INSTALL_DIR>`, `<ANDROID_SDK>`, `<NDK_VERSION>`) live in the untracked `AGENTS.local.md`; read it before deploying.

## 0. Fork exception: `experiment/html-editor` (overrides section 1)

This checkout is a parallel experiment (HTML-based editor) living on the `experiment/html-editor` branch, not `master`/`main`.

- **No automatic deployment.** Do not build or install for PC or Android unless the user explicitly asks for it in that moment. Never deploy the web app or the sync server from this branch (`npm run cloud:deploy:web`, `npm run cloud:deploy`).
- **Never push to `main`** and never merge this branch into `master` unless the user asks. Pushing the branch itself (`git push origin experiment/html-editor`) is fine as a backup.
- **Separate identity:** identifier `dev.notium.html`, name "Notium HTML" (Android package `dev.notium.html`), so it installs beside the main app with its own data folder.
- **PC install (only when asked):** same folder as the main app, different file name:
  ```powershell
  npm run tauri build -- --no-bundle
  Stop-Process -Name NotiumHTML -ErrorAction SilentlyContinue
  Copy-Item "target\release\notium.exe" "<INSTALL_DIR>\NotiumHTML.exe" -Force
  ```
- **Android install (only when asked):** same commands as section 1.4, but restart with `adb shell am force-stop dev.notium.html` and `adb shell am start -n dev.notium.html/.MainActivity`.
- **Sync bridge:** `src-tauri/src/sync.rs` reads/writes `.app/sync_config.html-fork.json` (never the main app's `sync_config.json`), uses keyring service `dev.notium.html.sync`, and forces sync off. Remove the bridge in `load_sync_config` only when the user asks.
- Fork-specific pending work lives in [`docs/FORK_HTML_EDITOR_ROADMAP.md`](docs/FORK_HTML_EDITOR_ROADMAP.md) (structural JSON merge before sync, editor gaps).
- Verification (section 3) and commits (section 2) still apply. `npm run test:markdown` is now `npm run test:editor`.

## 1. Automatic Deployment After Each Modification (MANDATORY)

**Whenever you make a modification to the project code, once it is verified (section 3) and committed, you MUST ship it everywhere: GitHub, web (production), PC and Android.** If a step fails, stop and report it; do not skip it silently.

### 1.1 GitHub

Push the commits to the GitHub repo (see `AGENTS.local.md`). Local `master` maps to remote `main`; a bare `git push` refuses because the names differ:
```powershell
git push origin master:main
```

### 1.2 Web (production)

Build and deploy the web app to Cloudflare (`notium-web` Worker; production URL in `AGENTS.local.md`). Run it whenever the change touches anything the web build includes (`src/`, `crates/notium-core`, `crates/notium-wasm`, `web/`, `server/notium-web`):
```powershell
npm run cloud:deploy:web
```
Changes to the sync server (`server/cloudflare-sync`) deploy with `npm run cloud:deploy`.

### 1.3 PC (Windows desktop)

The installed desktop app is the release executable copied to `<INSTALL_DIR>\Notium.exe` (the desktop shortcut `Notium.lnk` points there). Build it and replace that file; close the running app first, or the copy fails:
```powershell
npm run tauri build -- --no-bundle
Stop-Process -Name Notium -ErrorAction SilentlyContinue
Copy-Item "target\release\notium.exe" "<INSTALL_DIR>\Notium.exe" -Force
```

### 1.4 Android

1. Check if an Android device is connected by running `adb devices`.
2. If a device is connected (`device`), compile the APK for `aarch64` and install it:
   ```powershell
   $env:ANDROID_HOME = "<ANDROID_SDK>"
   $env:NDK_HOME = "<ANDROID_SDK>\ndk\<NDK_VERSION>"
   npm run tauri android build -- --apk --target aarch64 --ci

   adb install -r "src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk"
   adb shell am force-stop dev.notium.app
   adb shell am start -n dev.notium.app/.MainActivity
   ```
3. Gradle signs the release APK (`signingConfigs.release` in `src-tauri/gen/android/app/build.gradle.kts`). It reads the untracked `src-tauri/gen/android/keystore.properties` (`password`, `keyAlias`, `storeFile`); for local builds this points at the Android debug keystore. Never commit that file. Without it the build produces `app-universal-release-unsigned.apk`, which cannot be installed.

### 1.5 macOS and iOS (when working on a Mac)

Requires Xcode (`sudo xcode-select -s /Applications/Xcode.app`, or export `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`), CocoaPods and the Rust targets `aarch64-apple-darwin`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`.

macOS app bundle (`target/release/bundle/macos/Notium.app`):
```sh
npm run tauri build -- --bundles app
```

iOS: the Xcode project lives in `src-tauri/gen/apple` (set your signing team with the `APPLE_DEVELOPMENT_TEAM` environment variable). The iPhone needs Developer Mode enabled (Settings → Privacy & Security).
```sh
npm run tauri ios build -- --target aarch64-sim --debug   # simulator
npm run tauri ios dev -- --open                            # or open in Xcode and Run on the device
```
iOS behaves like Android: no folder picker, no filesystem watcher (resume-reconcile instead), no OS keyring.

The PC and Android builds both compile Rust into `target/`; run them one after the other, not in parallel.

## 2. Git Discipline

- Make a commit (`git commit`) after finishing each task or functional block before moving to the next one.
- Do not push temporary signing or compilation files (`.idsig`, `.apk`, `keystore`) to the repository.

## 3. Verification Before Finishing

- Run `npm run check` (`svelte-check`) and `npm run build` after making frontend changes.
- Run `cargo test --lib` after making changes in `crates/notium-core` or `src-tauri`.
- Run `npm run check:web` and `npm run test:web` after changes in `src/web`, `src/lib/ipc`, `crates/notium-wasm` or the pure modules of `crates/notium-core` (see [`docs/WEB.md`](docs/WEB.md)). `cargo build -p notium-core --no-default-features` must keep compiling.
- Run `npm run test:server` after changes in `server/cloudflare-sync` (the worker against an in-memory D1 stand-in).

## 4. Roadmap and Backlog (`docs/ROADMAP_AND_BACKLOG.md`)

- Consult and keep the [`docs/ROADMAP_AND_BACKLOG.md`](docs/ROADMAP_AND_BACKLOG.md) file up to date to know the current project status, what's next in the planning (Phase 1, Phase 1.5, Phase 2), and which items in sections A and B are completed or pending.

