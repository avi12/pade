import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

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
    port: 1420,
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
    sourcemap: true
  },
  // Unit tests share this config, so `@/` resolves in tests exactly as in the app.
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node"
  }
});
