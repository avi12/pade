import { CONTEXT_MENU_HANDLER, fail, RELEASE_TARGET, run } from "./environment.mjs";
import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/** Where Tauri leaves the compiled WiX objects and its locale strings. */
const WIX_INTERMEDIATE = join(RELEASE_TARGET, "wix", "x64");
const WIX_MAIN_OBJECT = join(WIX_INTERMEDIATE, "main.wixobj");
/** The WiX toolset Tauri downloads on its first MSI build. */
const WIX_TOOLS = join(process.env.LOCALAPPDATA ?? "", "tauri", "WixTools314");
const WIX_CULTURE = "en-us";

/** Build PADE and compile its WiX source. Tauri's own link of the MSI runs the
 *  ICE checks, so it fails wherever VBScript is missing; only a fresh compiled
 *  object matters here, and `linkInstaller` links it without them. */
export function buildApplication() {
  const startedAt = Date.now();
  try {
    run({
      command: "pnpm",
      args: ["tauri", "build", "--bundles", "msi"],
      environment: {
        PADE_CONTEXT_MENU_HANDLER: CONTEXT_MENU_HANDLER
      }
    });
  } catch {
    console.warn("Tauri's MSI link failed; linking it without ICE validation instead");
  }

  const isWixObjectFresh = statSync(WIX_MAIN_OBJECT, { throwIfNoEntry: false })?.mtimeMs >= startedAt;
  if (!isWixObjectFresh) {
    fail("the build stopped before compiling the installer - see the output above");
  }
}

export function linkInstaller(destination) {
  const objects = readdirSync(WIX_INTERMEDIATE).filter(file => file.endsWith(".wixobj")).map(file => join(WIX_INTERMEDIATE, file));
  run({
    command: join(WIX_TOOLS, "light.exe"),
    args: [
      "-nologo",
      "-sval",
      "-ext",
      join(WIX_TOOLS, "WixUIExtension.dll"),
      "-ext",
      join(WIX_TOOLS, "WixUtilExtension.dll"),
      `-cultures:${WIX_CULTURE}`,
      "-loc",
      join(WIX_INTERMEDIATE, "locale.wxl"),
      "-o",
      destination,
      ...objects
    ]
  });
}
