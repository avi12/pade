import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";
import { relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// The dev-server port has ONE home: tauri.conf.json's `devUrl` (the port the
// packaged webview loads). Derive it here rather than repeating the literal, so
// the two can never drift. Deliberately NOT Tauri's 1420 default - PADE hosts the
// development of other Tauri apps, which all default to 1420, so the host yields
// that port and takes 1430 to avoid colliding with a project launched inside it.
const tauriConfig: {
  build: { devUrl: string };
} = JSON.parse(
  readFileSync(fileURLToPath(new URL("./src-tauri/tauri.conf.json", import.meta.url)), "utf8")
);
const devServerPort = Number(new URL(tauriConfig.build.devUrl).port);

const PROJECT_ROOT = fileURLToPath(new URL(".", import.meta.url));

// The top-level entries the running app is built from: its sources and the
// configuration Vite itself reads.
const APP_SOURCE_ENTRIES = new Set([
  "src",
  "public",
  "index.html",
  "vite.config.ts",
  "svelte.config.js",
  "tsconfig.json",
  "package.json"
]);

function isOutsideAppSources(file: string) {
  const fromRoot = relative(PROJECT_ROOT, file);
  const isProjectRoot = fromRoot === "";
  if (isProjectRoot) {
    return false;
  }

  const [topLevelEntry] = fromRoot.split(sep);
  return !APP_SOURCE_ENTRIES.has(topLevelEntry);
}

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
    watch: {
      // Only what the app loads is watched, never the rest of the repository.
      // Everything else there is written by tools beside a live session, and on
      // Windows a watched file locked or deleted under the watcher kills it for
      // good: the app keeps running but never receives another hot update until
      // `pnpm app` restarts. That is src-tauri's locked .pdb/.exe artifacts
      // (EBUSY), `release/` getting each release's exe and installer, `dist/`
      // emptied by every build, the subagent worktrees under `.claude/` (copies
      // of the whole source, deleted again), and `.svelte-check/`, whose
      // regenerated tsconfig Vite would answer with a FULL reload.
      ignored: (file: string) => isOutsideAppSources(file)
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
