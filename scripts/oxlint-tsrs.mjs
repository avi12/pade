// Runs oxlint with its type-aware rules checked by tsrs (oxc's Rust port of the
// TypeScript 7 checker) in place of tsgolint. oxlint spawns whatever
// OXLINT_TSGOLINT_PATH names; tsrs ships `tsrs-lint` for exactly that. On
// Windows the spawn needs pnpm's .CMD shim - the extensionless one is a shell
// script Windows can't run. Arguments pass straight through to oxlint.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const BIN_FOLDER = resolve("node_modules", ".bin");
const isWindows = process.platform === "win32";
const SHIM_EXTENSION = isWindows ? ".CMD" : "";

const { status } = spawnSync(resolve(BIN_FOLDER, `oxlint${SHIM_EXTENSION}`), process.argv.slice(2), {
  stdio: "inherit",
  // A .CMD shim only runs through the shell.
  shell: isWindows,
  env: {
    ...process.env,
    OXLINT_TSGOLINT_PATH: resolve(BIN_FOLDER, `tsrs-lint${SHIM_EXTENSION}`)
  }
});
process.exit(status ?? 1);
