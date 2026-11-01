// `pnpm app`: `tauri dev` with the webview's DevTools port (9222) open, so the
// live frontend can always be inspected and driven over CDP. The Rust side
// honours the switch only in debug builds (lib.rs), so a release never exposes it.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const TAURI_CLI = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");

const { status } = spawnSync(process.execPath, [TAURI_CLI, "dev", ...process.argv.slice(2)], {
  stdio: "inherit",
  env: {
    ...process.env,
    PADE_ENABLE_CDP: "1"
  }
});
process.exit(status ?? 1);
