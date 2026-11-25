import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri sets `TAURI_DEV_HOST` to the PC's LAN IP when running on a physical
// device (on Windows it defaults to the public network address), so the webview
// can reach the dev server and its HMR websocket over the network.
const host = process.env.TAURI_DEV_HOST;
// The Tauri CLI sets TAURI_ENV_* for the dev/build commands it spawns.
const platform = process.env.TAURI_ENV_PLATFORM;
const debug = !!process.env.TAURI_ENV_DEBUG;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Ignore Rust build output (workspace target/ lives at the repo root)
      // so cargo rebuilds don't trigger spurious full page reloads in the webview.
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
  // Expose TAURI_ENV_* (platform, arch, debug…) to the app via import.meta.env.
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri renders with Chromium on Windows (WebView2) and Android (System
    // WebView), and with WebKit on macOS, iOS and Linux.
    target: platform === "windows" || platform === "android" ? "chrome105" : "safari13",
    // Debug builds stay readable; release builds are minified.
    minify: !debug,
    sourcemap: debug,
  },
});
