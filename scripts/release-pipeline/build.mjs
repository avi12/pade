import { CONTEXT_MENU_HANDLER, run } from "./environment.mjs";

/** Build PADE's exe, with the context-menu handler embedded, and no bundle: the
 *  MSIX packs the exe itself. */
export function buildApplication() {
  run({
    command: "pnpm",
    args: ["tauri", "build", "--no-bundle"],
    environment: {
      PADE_CONTEXT_MENU_HANDLER: CONTEXT_MENU_HANDLER
    }
  });
}
