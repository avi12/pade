import { buildApplication, linkInstaller } from "./build.mjs";
import { BUILT_EXECUTABLE, CARGO_MANIFEST, OUTPUT_DIRECTORY, run } from "./environment.mjs";
import { buildMsix } from "./msix.mjs";
import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

/** Build the exe, the MSI and the MSIX for `version` into `release/`. */
export function buildReleaseFiles({ version, tools }) {
  run({
    command: "cargo",
    args: ["build", "--release", "--manifest-path", CARGO_MANIFEST, "-p", "contextmenu-handler"]
  });
  buildApplication();

  mkdirSync(OUTPUT_DIRECTORY, { recursive: true });
  const assetName = `PADE-${version}.exe`;
  const asset = join(OUTPUT_DIRECTORY, assetName);
  copyFileSync(BUILT_EXECUTABLE, asset);
  const installerName = `PADE-${version}-setup.msi`;
  const installer = join(OUTPUT_DIRECTORY, installerName);
  linkInstaller(installer);
  return {
    assetName,
    asset,
    installerName,
    installer,
    msixFiles: buildMsix({
      version,
      tools
    })
  };
}
