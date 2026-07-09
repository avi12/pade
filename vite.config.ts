import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and no clobbering of its own env vars.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Never watch the Rust build tree — its locked .pdb/.exe artifacts crash
    // the dev-server file watcher (EBUSY on Windows).
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Produce assets Tauri can bundle; keep sourcemaps in dev.
  build: {
    target: "esnext",
    sourcemap: true,
  },
});
