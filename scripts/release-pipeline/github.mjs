import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { CARGO_LOCK, CARGO_MANIFEST, OUTPUT_DIRECTORY, REPOSITORY, run } from "./environment.mjs";

/** The release's page, where its MSIX is downloaded from. */
export function releasePageUrl(tag) {
  return `https://github.com/${REPOSITORY}/releases/tag/${tag}`;
}

/** The address GitHub redirects to that file in the newest release. */
export function latestReleaseUrl(assetName) {
  return `https://github.com/${REPOSITORY}/releases/latest/download/${assetName}`;
}

/** Commit the version bump, tag it and push both. */
export function commitAndTag({ version, tag }) {
  run({
    command: "git",
    args: ["add", CARGO_MANIFEST, CARGO_LOCK]
  });
  run({
    command: "git",
    args: ["commit", "-m", `chore(release): ${version}`]
  });
  run({
    command: "git",
    args: ["tag", tag]
  });
  run({
    command: "git",
    args: ["push", "origin", "main", tag]
  });
}

/** Create the GitHub release with every built file attached. */
export function createGitHubRelease({ version, tag, previousTag, highlights, files }) {
  const releaseNotes = `${highlights}

**All changes:** https://github.com/${REPOSITORY}/compare/${previousTag}...${tag}
`;
  const releaseNotesFile = join(OUTPUT_DIRECTORY, `notes-${version}.md`);
  writeFileSync(releaseNotesFile, releaseNotes);
  run({
    command: "gh",
    args: ["release", "create", tag, ...files, "--title", `PADE ${version}`, "--notes-file", releaseNotesFile]
  });
}
