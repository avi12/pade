import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// The dev-server port has ONE home: tauri.conf.json's `devUrl` (the port the
// packaged webview loads). Derive it here rather than repeating the literal, so
// the two can never drift. Deliberately NOT Tauri's 1420 default — PADE hosts the
// development of other Tauri apps, which all default to 1420, so the host yields
// that port and takes 1430 to avoid colliding with a project launched inside it.
const tauriConfig: {
  build: { devUrl: string };
} = JSON.parse(
  readFileSync(fileURLToPath(new URL("./src-tauri/tauri.conf.json", import.meta.url)), "utf8")
);
const devServerPort = Number(new URL(tauriConfig.build.devUrl).port);

// Tauri expects a fixed port and no clobbering of its own env vars.
export default defineConfig({
  plugins: [svelte()],
  // `@` → src, so internal modules import as `@/lib/…` not brittle `../../` chains.
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url))
    }
  },
  clearScreen: false,
  server: {
    port: devServerPort,
    strictPort: true,
    // Never watch the Rust build tree — its locked .pdb/.exe artifacts crash
    // the dev-server file watcher (EBUSY on Windows).
    watch: {
      ignored: ["**/src-tauri/**"]
    }
  },
  // Produce assets Tauri can bundle; keep sourcemaps in dev.
  build: {
    target: "esnext",
    // The WebView consumes the minified bundle directly. External production
    // source maps add package and startup I/O but are not used by the app.
    sourcemap: false
  },
  // Unit tests share this config, so `@/` resolves in tests exactly as in the app.
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node"
  }
});
