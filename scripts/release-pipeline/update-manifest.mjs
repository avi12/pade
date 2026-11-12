import { ROOT, run } from "./environment.mjs";
import { releasePageUrl } from "./github.mjs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const UPDATE_MANIFEST = join(ROOT, "docs", "updates.json");

/** Write the manifest unpackaged copies poll for their "install the MSIX"
 *  notice, commit it and push it. */
export function publishUpdateManifest({ version, tag }) {
  const manifest = {
    version,
    releaseUrl: releasePageUrl(tag)
  };
  writeFileSync(UPDATE_MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);
  run({
    command: "git",
    args: ["add", UPDATE_MANIFEST]
  });
  run({
    command: "git",
    args: ["commit", "-m", `chore(release): update manifest for ${version}`]
  });
  run({
    command: "git",
    args: ["push", "origin", "main"]
  });
}
