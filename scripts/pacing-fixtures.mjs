// Runs scripts/pacing-fixtures.ts through Vite's module loader, so the `@/`
// alias and the TypeScript resolve exactly as they do in the app and its tests,
// and writes what it returns to the Rust port's fixture folder.
// Usage: pnpm pacing:fixtures (from the repo root).

import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { createServer } from "vite";

const FIXTURE_DIRECTORY = resolve("src-tauri/src/pacing/fixtures");
const BYTES_PER_KILOBYTE = 1024;

const server = await createServer({
  appType: "custom",
  logLevel: "error",
  server: {
    middlewareMode: true,
    watch: null,
    hmr: false
  }
});

try {
  const { pacingFixtures } = await server.ssrLoadModule("/scripts/pacing-fixtures.ts");
  mkdirSync(FIXTURE_DIRECTORY, { recursive: true });
  for (const [name, contents] of Object.entries(pacingFixtures)) {
    const text = `${JSON.stringify(contents)}\n`;
    writeFileSync(resolve(FIXTURE_DIRECTORY, `${name}.json`), text);
    console.log(`${name}.json: ${(text.length / BYTES_PER_KILOBYTE).toFixed(1)} KB`);
  }
} finally {
  await server.close();
}
