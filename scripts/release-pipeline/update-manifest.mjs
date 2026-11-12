import { ROOT, run } from "./environment.mjs";
import { releaseUrl } from "./github.mjs";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const UPDATE_MANIFEST = join(ROOT, "docs", "updates.json");

function sha256Of(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

/** Write the manifest self-updating copies poll, commit it and push it. */
export function publishUpdateManifest({ version, tag, files }) {
  const manifest = {
    version,
    url: releaseUrl({
      tag,
      assetName: files.assetName
    }),
    sha256: sha256Of(files.asset),
    installer: {
      url: releaseUrl({
        tag,
        assetName: files.installerName
      }),
      sha256: sha256Of(files.installer)
    }
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
  return manifest;
}
