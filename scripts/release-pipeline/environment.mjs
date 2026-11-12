import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export const ROOT = resolve(import.meta.dirname, "..", "..");
export const REPOSITORY = "avi12/pade";

export const CARGO_MANIFEST = join(ROOT, "src-tauri", "Cargo.toml");
export const CARGO_LOCK = join(ROOT, "src-tauri", "Cargo.lock");
export const RELEASE_TARGET = join(ROOT, "src-tauri", "target", "release");
export const OUTPUT_DIRECTORY = join(ROOT, "release");

export const CONTEXT_MENU_HANDLER_FILE = "contextmenu_handler.dll";
export const CONTEXT_MENU_HANDLER = join(RELEASE_TARGET, CONTEXT_MENU_HANDLER_FILE);
export const EXECUTABLE_FILE = "pade.exe";
export const BUILT_EXECUTABLE = join(RELEASE_TARGET, EXECUTABLE_FILE);

const IS_WINDOWS = process.platform === "win32";
/** Commands installed as a `.cmd` shim on Windows, which only a shell can
 *  start. Their arguments here never hold a space, so the shell can't split one. */
const SHIM_COMMANDS = new Set(["pnpm"]);

export const MSIX_ONLY_FLAG = "--msix-only";

/** Environment variable names are case-blind on Windows; compared lowercased. */
const MODULE_PATH_VARIABLE = "psmodulepath";

/** Run a command in the repository, its output passed through; throws on failure. */
export function run({ command, args, environment = {} }) {
  execFileSync(command, args, {
    cwd: ROOT,
    stdio: "inherit",
    env: {
      ...process.env,
      ...environment
    },
    shell: IS_WINDOWS && SHIM_COMMANDS.has(command)
  });
}

/** A command's trimmed output. */
export function read({ command, args }) {
  return execFileSync(command, args, {
    cwd: ROOT,
    encoding: "utf8"
  }).trim();
}

/** A Windows PowerShell one-liner's trimmed output. It starts without the
 *  caller's `PSModulePath`: one inherited from PowerShell 7 hides Windows
 *  PowerShell's own modules, and with them the `Cert:` drive and
 *  `Export-Certificate`. */
export function readPowerShell(script) {
  const inherited = Object.entries(process.env);
  const environment = Object.fromEntries(inherited.filter(([name]) => name.toLowerCase() !== MODULE_PATH_VARIABLE));
  return execFileSync("powershell", ["-NoProfile", "-NonInteractive", "-Command", script], {
    cwd: ROOT,
    encoding: "utf8",
    env: environment
  }).trim();
}

export function fail(message) {
  console.error(message);
  process.exit(1);
}

/** A template with every `{{PLACEHOLDER}}` in `values` filled in. */
export function filled({ template, values }) {
  let text = readFileSync(template, "utf8");
  for (const [placeholder, value] of Object.entries(values)) {
    text = text.replaceAll(placeholder, value);
  }

  return text;
}
