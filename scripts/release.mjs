// Cut a PADE release: one portable exe on GitHub, and the update manifest every
// installed copy polls (`docs/updates.json`, read by `src-tauri/src/update.rs`).
//
//   node scripts/release.mjs <version> <notes-file>
//
// The notes file holds the changelog highlights, written for users; the script
// appends the link to every commit in the release's range. In order:
//
//   1. set the version in `src-tauri/Cargo.toml` - its one home; Tauri and the
//      exe read it from there;
//   2. build the Windows 11 context-menu handler, then PADE itself without an
//      installer, embedding the handler so the exe stands alone;
//   3. commit `chore(release): <version>`, tag `v<version>` and push both;
//   4. create the GitHub release with the exe attached as `PADE-<version>.exe`;
//   5. write the manifest from that exe's SHA-256 and push it - last, so an
//      installed copy never sees a release whose exe isn't uploaded yet;
//   6. fill the winget manifests (`packaging/winget`), validate them and open
//      the pull request to `microsoft/winget-pkgs` with `wingetcreate`, so a
//      winget install sees the release once winget's reviewers merge it.
//      `--no-winget` (after the notes file) skips this step.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync
} from "node:fs";
import { join, resolve } from "node:path";

const REPOSITORY = "avi12/pade";
const ROOT = resolve(import.meta.dirname, "..");
const CARGO_MANIFEST = join(ROOT, "src-tauri", "Cargo.toml");
const CARGO_LOCK = join(ROOT, "src-tauri", "Cargo.lock");
const RELEASE_TARGET = join(ROOT, "src-tauri", "target", "release");
const OUTPUT_DIRECTORY = join(ROOT, "release");
const UPDATE_MANIFEST = join(ROOT, "docs", "updates.json");
const CONTEXT_MENU_HANDLER = join(RELEASE_TARGET, "contextmenu_handler.dll");
const BUILT_EXECUTABLE = join(RELEASE_TARGET, "pade.exe");
const PACKAGE_VERSION_LINE = /^version = "(\d+\.\d+\.\d+)"$/m;
const VERSION_PATTERN = /^\d+\.\d+\.\d+$/;
const IS_WINDOWS = process.platform === "win32";
/** Commands installed as a `.cmd` shim on Windows, which only a shell can
 *  start. Their arguments here never hold a space, so the shell can't split one. */
const SHIM_COMMANDS = new Set(["pnpm"]);
const WINGET_TEMPLATES = join(ROOT, "packaging", "winget");
const WINGET_PACKAGE = "avi12.PADE";
const WINGET_PACKAGE_PATH = "repos/microsoft/winget-pkgs/contents/manifests/a/avi12/PADE";
const NO_WINGET_FLAG = "--no-winget";

/** Run a command in the repository, its output passed through; throws on failure. */
function run({ command, args, environment = {} }) {
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
function read({ command, args }) {
  return execFileSync(command, args, {
    cwd: ROOT,
    encoding: "utf8"
  }).trim();
}

function versionParts(version) {
  return version.split(".").map(part => parseInt(part, 10));
}

function isNewer({ candidate, current }) {
  const candidateParts = versionParts(candidate);
  const currentParts = versionParts(current);
  for (const [i, part] of candidateParts.entries()) {
    if (part !== currentParts[i]) {
      return part > currentParts[i];
    }
  }

  return false;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

/** Whether winget already lists PADE - a first submission is a new package. */
function isOnWinget() {
  try {
    read({
      command: "gh",
      args: ["api", WINGET_PACKAGE_PATH, "--silent"]
    });
    return true;
  } catch {
    return false;
  }
}

/** Fill the winget manifest templates for this release, validate them and open
 *  the winget-pkgs pull request. The token rides an environment variable, which
 *  wingetcreate reads, rather than an argument it may log. */
function submitToWinget({ version, url, sha256 }) {
  const directory = join(OUTPUT_DIRECTORY, "winget", version);
  mkdirSync(directory, { recursive: true });
  const values = {
    "{{VERSION}}": version,
    "{{RELEASE_DATE}}": new Date().toISOString().slice(0, 10),
    "{{INSTALLER_URL}}": url,
    "{{INSTALLER_SHA256}}": sha256.toUpperCase()
  };
  for (const file of readdirSync(WINGET_TEMPLATES)) {
    let manifest = readFileSync(join(WINGET_TEMPLATES, file), "utf8");
    for (const [placeholder, value] of Object.entries(values)) {
      manifest = manifest.replaceAll(placeholder, value);
    }
    writeFileSync(join(directory, file), manifest);
  }

  run({
    command: "winget",
    args: ["validate", "--manifest", directory]
  });
  const kind = isOnWinget() ? "New version" : "New package";
  run({
    command: "wingetcreate",
    args: ["submit", "--no-open", "--prtitle", `${kind}: ${WINGET_PACKAGE} version ${version}`, directory],
    environment: {
      WINGET_CREATE_GITHUB_TOKEN: read({
        command: "gh",
        args: ["auth", "token"]
      })
    }
  });
}

const [version, notesFile, ...flags] = process.argv.slice(2);
const isWingetSkipped = flags.includes(NO_WINGET_FLAG);
if (!version || !notesFile || !VERSION_PATTERN.test(version)) {
  fail(`usage: node scripts/release.mjs <major.minor.patch> <notes-file> [${NO_WINGET_FLAG}]`);
}

const isWorkingTreeClean = read({
  command: "git",
  args: ["status", "--porcelain", "--untracked-files=no"]
}) === "";
if (!isWorkingTreeClean) {
  fail("commit or stash your changes first - a release is built from what is committed");
}

const cargoManifest = readFileSync(CARGO_MANIFEST, "utf8");
const currentVersion = PACKAGE_VERSION_LINE.exec(cargoManifest)?.[1];
if (!currentVersion || !isNewer({
  candidate: version,
  current: currentVersion
})) {
  fail(`${version} must be newer than the current ${currentVersion ?? "(unreadable)"}`);
}

const previousTag = read({
  command: "git",
  args: ["describe", "--tags", "--abbrev=0"]
});
const tag = `v${version}`;
const highlights = readFileSync(notesFile, "utf8").trim();

writeFileSync(CARGO_MANIFEST, cargoManifest.replace(PACKAGE_VERSION_LINE, `version = "${version}"`));

run({
  command: "cargo",
  args: ["build", "--release", "--manifest-path", CARGO_MANIFEST, "-p", "contextmenu-handler"]
});
run({
  command: "pnpm",
  args: ["tauri", "build", "--no-bundle"],
  environment: {
    PADE_CONTEXT_MENU_HANDLER: CONTEXT_MENU_HANDLER
  }
});

mkdirSync(OUTPUT_DIRECTORY, { recursive: true });
const assetName = `PADE-${version}.exe`;
const asset = join(OUTPUT_DIRECTORY, assetName);
copyFileSync(BUILT_EXECUTABLE, asset);
const sha256 = createHash("sha256").update(readFileSync(asset)).digest("hex");

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

const releaseNotes = `${highlights}

**All changes:** https://github.com/${REPOSITORY}/compare/${previousTag}...${tag}
`;
const releaseNotesFile = join(OUTPUT_DIRECTORY, `notes-${version}.md`);
writeFileSync(releaseNotesFile, releaseNotes);
run({
  command: "gh",
  args: ["release", "create", tag, asset, "--title", `PADE ${version}`, "--notes-file", releaseNotesFile]
});

const manifest = {
  version,
  url: `https://github.com/${REPOSITORY}/releases/download/${tag}/${assetName}`,
  sha256
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

if (!isWingetSkipped) {
  submitToWinget(manifest);
}

console.log(`Released ${tag}: https://github.com/${REPOSITORY}/releases/tag/${tag}`);
