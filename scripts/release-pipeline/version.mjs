import { CARGO_MANIFEST, fail, read } from "./environment.mjs";
import { readFileSync } from "node:fs";

export const PACKAGE_VERSION_LINE = /^version = "(\d+\.\d+\.\d+)"$/m;
export const VERSION_PATTERN = /^\d+\.\d+\.\d+$/;
/** What a `Cargo.toml` with no version line is reported as holding. */
const UNREADABLE_VERSION = "(unreadable)";

function versionParts(version) {
  return version.split(".").map(part => parseInt(part, 10));
}

export function isNewer({ candidate, current }) {
  const candidateParts = versionParts(candidate);
  const currentParts = versionParts(current);
  for (const [i, part] of candidateParts.entries()) {
    if (part !== currentParts[i]) {
      return part > currentParts[i];
    }
  }

  return false;
}

function checkWorkingTreeClean() {
  const isWorkingTreeClean = read({
    command: "git",
    args: ["status", "--porcelain", "--untracked-files=no"]
  }) === "";
  if (!isWorkingTreeClean) {
    fail("commit or stash your changes first - a release is built from what is committed");
  }
}

/** Stop unless the arguments name a release newer than the committed one, built
 *  from a clean tree. Answers `Cargo.toml` as read. */
export function checkReleaseArguments({ version, notesFile }) {
  const isUsageRight = Boolean(version && notesFile) && VERSION_PATTERN.test(version);
  if (!isUsageRight) {
    fail(`usage: node scripts/release.mjs <major.minor.patch> <notes-file>`);
  }

  checkWorkingTreeClean();
  return checkNewerThanCommitted(version);
}

function committedVersion(cargoManifest) {
  return PACKAGE_VERSION_LINE.exec(cargoManifest)?.[1] ?? UNREADABLE_VERSION;
}

/** Stop unless `version` is newer than the one `Cargo.toml` holds. Answers
 *  `Cargo.toml` as read. */
function checkNewerThanCommitted(version) {
  const cargoManifest = readFileSync(CARGO_MANIFEST, "utf8");
  const currentVersion = committedVersion(cargoManifest);
  const isNewerRelease = currentVersion !== UNREADABLE_VERSION && isNewer({
    candidate: version,
    current: currentVersion
  });
  if (!isNewerRelease) {
    fail(`${version} must be newer than the current ${currentVersion}`);
  }

  return cargoManifest;
}
