import { CARGO_MANIFEST, MSIX_ONLY_FLAG, read, REPOSITORY } from "./release-pipeline/environment.mjs";
import { commitAndTag, createGitHubRelease } from "./release-pipeline/github.mjs";
import { packMsixOnly } from "./release-pipeline/msix.mjs";
import { buildReleaseFiles } from "./release-pipeline/release-files.mjs";
import { checkSigningCertificate, findSigningTools } from "./release-pipeline/signing.mjs";
import { publishUpdateManifest } from "./release-pipeline/update-manifest.mjs";
import { checkReleaseArguments, PACKAGE_VERSION_LINE } from "./release-pipeline/version.mjs";
// Cut a PADE release: a signed MSIX on GitHub, and the update manifest every
// unpackaged copy polls for its "install the MSIX" notice (`docs/updates.json`,
// read by `src-tauri/src/update.rs`).
//
//   node scripts/release.mjs <version> <notes-file>
//   node scripts/release.mjs --msix-only <version>
//
// The notes file holds the changelog highlights, written for users; the script
// appends the link to every commit in the release's range. In order:
//
//   1. check that the MSIX tools (`makeappx`, `signtool`) and the code-signing
//      certificate are here, before anything is changed;
//   2. set the version in `src-tauri/Cargo.toml` - its one home; Tauri and the
//      exe read it from there;
//   3. build the Windows 11 context-menu handler, then PADE itself, embedding
//      the handler (the MSIX packs the exe; nothing else is bundled);
//   4. lay out, pack and sign the MSIX (`packaging/msix`), export the public
//      certificate that trusts it, and fill its `.appinstaller` update feed;
//   5. commit `chore(release): <version>`, tag `v<version>` and push both;
//   6. create the GitHub release with the package as `PADE-<version>.msix` and
//      under the stable `PADE.msix` the feed points at, plus `PADE.cer` and
//      `PADE.appinstaller`;
//   7. write the manifest (the version and the release page) and push it -
//      last, so an unpackaged copy never sees a release whose files aren't
//      uploaded yet.
//
// `--msix-only <version>` runs step 4 alone, from what `src-tauri/target/release`
// already holds, into `release/` - no version bump, git or GitHub.
import { readFileSync, writeFileSync } from "node:fs";

function release({ version, notesFile }) {
  const cargoManifest = checkReleaseArguments({
    version,
    notesFile
  });
  const tools = findSigningTools();
  checkSigningCertificate();
  const previousTag = read({
    command: "git",
    args: ["describe", "--tags", "--abbrev=0"]
  });
  const tag = `v${version}`;
  const highlights = readFileSync(notesFile, "utf8").trim();

  writeFileSync(CARGO_MANIFEST, cargoManifest.replace(PACKAGE_VERSION_LINE, `version = "${version}"`));
  const files = buildReleaseFiles({
    version,
    tools
  });
  commitAndTag({
    version,
    tag
  });
  createGitHubRelease({
    version,
    tag,
    previousTag,
    highlights,
    files
  });
  publishUpdateManifest({
    version,
    tag
  });

  console.log(`Released ${tag}: https://github.com/${REPOSITORY}/releases/tag/${tag}`);
}

const [firstArgument, secondArgument] = process.argv.slice(2);
if (firstArgument === MSIX_ONLY_FLAG) {
  packMsixOnly(secondArgument);
} else {
  release({
    version: firstArgument,
    notesFile: secondArgument
  });
}
