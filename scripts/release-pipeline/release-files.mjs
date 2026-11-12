import { buildApplication } from "./build.mjs";
import { CARGO_MANIFEST, OUTPUT_DIRECTORY, run } from "./environment.mjs";
import { buildMsix } from "./msix.mjs";
import { mkdirSync } from "node:fs";

/** Build the exe and the MSIX for `version` into `release/`. Answers every file
 *  the release attaches. */
export function buildReleaseFiles({ version, tools }) {
  run({
    command: "cargo",
    args: ["build", "--release", "--manifest-path", CARGO_MANIFEST, "-p", "contextmenu-handler"]
  });
  buildApplication();

  mkdirSync(OUTPUT_DIRECTORY, { recursive: true });
  return buildMsix({
    version,
    tools
  });
}
