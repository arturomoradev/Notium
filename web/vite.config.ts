// Web build of Notium (served by the Cloudflare sync Worker). Separate from the
// root vite.config.ts so the Tauri build (dist/) is never affected.
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const here = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  root: here,
  publicDir: "../public",
  plugins: [svelte()],
  define: {
    // Read by shared code (App.svelte) to skip native-only behaviour.
    "import.meta.env.VITE_NOTIUM_WEB": JSON.stringify("1"),
  },
  server: {
    port: 1430,
    strictPort: true,
    fs: { allow: [".."] },
    // `npm run dev:web` expects `wrangler dev` on 8787 (server/cloudflare-sync).
    proxy: { "/sync": "http://127.0.0.1:8787" },
  },
  build: {
    outDir: "../dist-web",
    emptyOutDir: true,
    target: "es2022",
  },
});
