// Cut a PADE release: a portable exe, an MSI and a signed MSIX on GitHub, and
// the update manifest every self-updating copy polls (`docs/updates.json`, read
// by `src-tauri/src/update.rs`).
//
//   node scripts/release.mjs <version> <notes-file> [--no-winget]
//   node scripts/release.mjs --msix-only <version>
//
// The notes file holds the changelog highlights, written for users; the script
// appends the link to every commit in the release's range. In order:
//
//   1. check that the MSIX tools (`makeappx`, `signtool`) and the code-signing
//      certificate are here, before anything is changed;
//   2. set the version in `src-tauri/Cargo.toml` - its one home; Tauri and the
//      exe read it from there;
//   3. build the Windows 11 context-menu handler, then PADE itself and its MSI
//      installer (`src-tauri/windows/installer.wxs`), embedding the handler so
//      the exe stands alone. The MSI is linked here with WiX's validation
//      skipped: its ICE checks run on VBScript, which Windows no longer ships
//      by default, so CI runs them instead (`.github/workflows/installer.yml`);
//   4. lay out, pack and sign the MSIX (`packaging/msix`), export the public
//      certificate that trusts it, and fill its `.appinstaller` update feed;
//   5. commit `chore(release): <version>`, tag `v<version>` and push both;
//   6. create the GitHub release with the portable exe attached as
//      `PADE-<version>.exe`, the installer as `PADE-<version>-setup.msi`, the
//      package as `PADE-<version>.msix` and under the stable `PADE.msix` the feed
//      points at, plus `PADE.cer` and `PADE.appinstaller`;
//   7. write the manifest from the exe's and the MSI's SHA-256 and push it -
//      last, so an installed copy never sees a release whose files aren't
//      uploaded yet;
//   8. fill the winget manifests (`packaging/winget`), validate them and open
//      the pull request to `microsoft/winget-pkgs` with `wingetcreate`, so a
//      winget install sees the release once winget's reviewers merge it.
//      `--no-winget` (after the notes file) skips this step.
//
// `--msix-only <version>` runs step 4 alone, from what `src-tauri/target/release`
// already holds, into `release/` - no version bump, git, GitHub or winget.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
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
const CONTEXT_MENU_HANDLER_FILE = "contextmenu_handler.dll";
const CONTEXT_MENU_HANDLER = join(RELEASE_TARGET, CONTEXT_MENU_HANDLER_FILE);
const EXECUTABLE_FILE = "pade.exe";
const BUILT_EXECUTABLE = join(RELEASE_TARGET, EXECUTABLE_FILE);
/** Where Tauri leaves the compiled WiX objects and its locale strings. */
const WIX_INTERMEDIATE = join(RELEASE_TARGET, "wix", "x64");
const WIX_MAIN_OBJECT = join(WIX_INTERMEDIATE, "main.wixobj");
/** The WiX toolset Tauri downloads on its first MSI build. */
const WIX_TOOLS = join(process.env.LOCALAPPDATA ?? "", "tauri", "WixTools314");
const WIX_CULTURE = "en-us";
const PACKAGE_VERSION_LINE = /^version = "(\d+\.\d+\.\d+)"$/m;
const VERSION_PATTERN = /^\d+\.\d+\.\d+$/;
/** What a `Cargo.toml` with no version line is reported as holding. */
const UNREADABLE_VERSION = "(unreadable)";
const IS_WINDOWS = process.platform === "win32";
/** Commands installed as a `.cmd` shim on Windows, which only a shell can
 *  start. Their arguments here never hold a space, so the shell can't split one. */
const SHIM_COMMANDS = new Set(["pnpm"]);
const WINGET_TEMPLATES = join(ROOT, "packaging", "winget");
const WINGET_PACKAGE = "avi12.PADE";
const WINGET_PACKAGE_PATH = "repos/microsoft/winget-pkgs/contents/manifests/a/avi12/PADE";
const NO_WINGET_FLAG = "--no-winget";
const MSIX_ONLY_FLAG = "--msix-only";

/** The MSIX package's identity name - the same in its manifest and its feed. */
const MSIX_PACKAGE_NAME = "avi12.PADE";
const MSIX_TEMPLATES = join(ROOT, "packaging", "msix");
const MSIX_MANIFEST_TEMPLATE = join(MSIX_TEMPLATES, "AppxManifest.xml");
const APPINSTALLER_TEMPLATE = join(MSIX_TEMPLATES, "PADE.appinstaller");
const MSIX_MANIFEST_FILE = "AppxManifest.xml";
const MSIX_LAYOUT = join(OUTPUT_DIRECTORY, "msix-layout");
const MSIX_ASSETS_DIRECTORY = "Assets";
/** The logos the package manifest names, already generated from the app icon. */
const MSIX_LOGOS = [
  "StoreLogo.png",
  "Square44x44Logo.png",
  "Square71x71Logo.png",
  "Square150x150Logo.png"
];
const APP_ICONS = join(ROOT, "src-tauri", "icons");
/** The names the feed reaches through `/releases/latest/download/`, so one
 *  installed copy follows every later release. */
const STABLE_PACKAGE_NAME = "PADE.msix";
const APPINSTALLER_NAME = "PADE.appinstaller";
const CERTIFICATE_NAME = "PADE.cer";
/** MSIX versions have four parts; a release fills the first three. */
const MSIX_REVISION = "0";

/** The code-signing certificate in `Cert:\CurrentUser\My`. Windows requires the
 *  package's Publisher to equal the certificate's subject exactly. */
const DEFAULT_SIGNING_THUMBPRINT = "DA09BDA44CB503CB519602BD6BD0D1F126D98B8A";
const SIGNING_THUMBPRINT_VARIABLE = "PADE_SIGNING_THUMBPRINT";
const SIGNING_THUMBPRINT = (process.env[SIGNING_THUMBPRINT_VARIABLE] || DEFAULT_SIGNING_THUMBPRINT).toUpperCase();
const SIGNING_SUBJECT = "CN=avi12";
const THUMBPRINT_PATTERN = /^[0-9A-F]{40}$/;
const SIGNING_CERTIFICATE_PATH = `Cert:\\CurrentUser\\My\\${SIGNING_THUMBPRINT}`;
const TIMESTAMP_URL = "http://timestamp.digicert.com";
const SIGNING_DIGEST = "SHA256";
const WINDOWS_KITS_BIN = join(process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Windows Kits", "10", "bin");
/** A Windows SDK's bin folder is named for its four-part version. */
const SDK_VERSION_PATTERN = /^\d+(\.\d+){3}$/;
const SDK_ARCHITECTURE = "x64";
/** Environment variable names are case-blind on Windows; compared lowercased. */
const MODULE_PATH_VARIABLE = "psmodulepath";
const MAKEAPPX = "makeappx.exe";
const SIGNTOOL = "signtool.exe";

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

/** A Windows PowerShell one-liner's trimmed output. It starts without the
 *  caller's `PSModulePath`: one inherited from PowerShell 7 hides Windows
 *  PowerShell's own modules, and with them the `Cert:` drive and
 *  `Export-Certificate`. */
function readPowerShell(script) {
  const inherited = Object.entries(process.env);
  const environment = Object.fromEntries(inherited.filter(([name]) => name.toLowerCase() !== MODULE_PATH_VARIABLE));
  return execFileSync("powershell", ["-NoProfile", "-NonInteractive", "-Command", script], {
    cwd: ROOT,
    encoding: "utf8",
    env: environment
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

function sha256Of(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

/** A template with every `{{PLACEHOLDER}}` in `values` filled in. */
function filled({ template, values }) {
  let text = readFileSync(template, "utf8");
  for (const [placeholder, value] of Object.entries(values)) {
    text = text.replaceAll(placeholder, value);
  }

  return text;
}

/** The installed Windows SDK versions, newest first. */
function sdkVersionsNewestFirst() {
  if (!existsSync(WINDOWS_KITS_BIN)) {
    return [];
  }

  const sdkVersions = readdirSync(WINDOWS_KITS_BIN).filter(name => SDK_VERSION_PATTERN.test(name));
  return sdkVersions.toSorted((left, right) => isNewer({
    candidate: left,
    current: right
  }) ? -1 : 1);
}

/** One SDK version's `makeappx` and `signtool`. */
function sdkTools(sdkVersion) {
  const directory = join(WINDOWS_KITS_BIN, sdkVersion, SDK_ARCHITECTURE);
  return {
    makeappx: join(directory, MAKEAPPX),
    signtool: join(directory, SIGNTOOL)
  };
}

function hasBothTools({ makeappx, signtool }) {
  return existsSync(makeappx) && existsSync(signtool);
}

/** The newest Windows SDK's `makeappx` and `signtool`, or a clear stop. */
function findSigningTools() {
  const tools = sdkVersionsNewestFirst().map(sdkTools).find(hasBothTools);
  return tools ?? fail(`${MAKEAPPX} and ${SIGNTOOL} were not found under ${WINDOWS_KITS_BIN} - install the Windows SDK's signing tools first`);
}

/** The signing certificate's subject, private-key presence and expiry, or a
 *  clear stop when the store has no certificate with that thumbprint. */
function readSigningCertificate() {
  if (!THUMBPRINT_PATTERN.test(SIGNING_THUMBPRINT)) {
    fail(`${SIGNING_THUMBPRINT_VARIABLE} must be a 40-digit hex thumbprint, not "${SIGNING_THUMBPRINT}"`);
  }

  const certificate = readPowerShell(`$certificate = Get-Item -LiteralPath '${SIGNING_CERTIFICATE_PATH}' -ErrorAction SilentlyContinue; if ($certificate) { [pscustomobject]@{ subject = $certificate.Subject; hasPrivateKey = $certificate.HasPrivateKey; expires = $certificate.NotAfter.ToString('o') } | ConvertTo-Json -Compress }; exit 0`);
  if (!certificate) {
    fail(`the code-signing certificate ${SIGNING_THUMBPRINT} is not in ${SIGNING_CERTIFICATE_PATH} - install it or set ${SIGNING_THUMBPRINT_VARIABLE}`);
  }

  return JSON.parse(certificate);
}

/** Stop unless the signing certificate is installed, carries its private key,
 *  is still valid and has the subject the package is published under. */
function checkSigningCertificate() {
  const { subject, hasPrivateKey, expires } = readSigningCertificate();
  const problems = [
    [subject !== SIGNING_SUBJECT, `the signing certificate's subject is "${subject}", but the package is published as "${SIGNING_SUBJECT}"`],
    [!hasPrivateKey, `the signing certificate ${SIGNING_THUMBPRINT} has no private key here, so it cannot sign`],
    [new Date(expires) <= new Date(), `the signing certificate ${SIGNING_THUMBPRINT} expired on ${expires}`]
  ];
  const firstProblem = problems.find(([isProblem]) => isProblem);
  if (firstProblem) {
    fail(firstProblem[1]);
  }
}

/** Build PADE and compile its WiX source. Tauri's own link of the MSI runs the
 *  ICE checks, so it fails wherever VBScript is missing; only a fresh compiled
 *  object matters here, and `linkInstaller` links it without them. */
function buildApplication() {
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

function linkInstaller(destination) {
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

function releaseUrl({ tag, assetName }) {
  return `https://github.com/${REPOSITORY}/releases/download/${tag}/${assetName}`;
}

/** The address GitHub redirects to that file in the newest release. */
function latestReleaseUrl(assetName) {
  return `https://github.com/${REPOSITORY}/releases/latest/download/${assetName}`;
}

/** Lay out the package folder from the release build, pack it, sign it, and
 *  write the public certificate and the update feed beside it. Answers every
 *  file the release attaches for MSIX. */
function buildMsix({ version, tools }) {
  for (const built of [BUILT_EXECUTABLE, CONTEXT_MENU_HANDLER]) {
    if (!existsSync(built)) {
      fail(`${built} is missing - build PADE and the context-menu handler first`);
    }
  }

  const values = {
    "{{PACKAGE_NAME}}": MSIX_PACKAGE_NAME,
    "{{PUBLISHER}}": SIGNING_SUBJECT,
    "{{VERSION}}": `${version}.${MSIX_REVISION}`,
    "{{PACKAGE_URL}}": latestReleaseUrl(STABLE_PACKAGE_NAME),
    "{{APPINSTALLER_URL}}": latestReleaseUrl(APPINSTALLER_NAME)
  };
  rmSync(MSIX_LAYOUT, {
    recursive: true,
    force: true
  });
  const assets = join(MSIX_LAYOUT, MSIX_ASSETS_DIRECTORY);
  mkdirSync(assets, { recursive: true });
  copyFileSync(BUILT_EXECUTABLE, join(MSIX_LAYOUT, EXECUTABLE_FILE));
  copyFileSync(CONTEXT_MENU_HANDLER, join(MSIX_LAYOUT, CONTEXT_MENU_HANDLER_FILE));
  for (const logo of MSIX_LOGOS) {
    copyFileSync(join(APP_ICONS, logo), join(assets, logo));
  }
  writeFileSync(
    join(MSIX_LAYOUT, MSIX_MANIFEST_FILE), filled({
      template: MSIX_MANIFEST_TEMPLATE,
      values
    })
  );

  const versionedPackage = join(OUTPUT_DIRECTORY, `PADE-${version}.msix`);
  run({
    command: tools.makeappx,
    args: ["pack", "/o", "/d", MSIX_LAYOUT, "/p", versionedPackage]
  });
  run({
    command: tools.signtool,
    args: ["sign", "/fd", SIGNING_DIGEST, "/sha1", SIGNING_THUMBPRINT, "/tr", TIMESTAMP_URL, "/td", SIGNING_DIGEST, versionedPackage]
  });

  const stablePackage = join(OUTPUT_DIRECTORY, STABLE_PACKAGE_NAME);
  copyFileSync(versionedPackage, stablePackage);
  const certificate = join(OUTPUT_DIRECTORY, CERTIFICATE_NAME);
  readPowerShell(`Export-Certificate -Cert '${SIGNING_CERTIFICATE_PATH}' -FilePath '${certificate.replaceAll("'", "''")}' -Type CERT | Out-Null`);
  const appinstaller = join(OUTPUT_DIRECTORY, APPINSTALLER_NAME);
  writeFileSync(
    appinstaller, filled({
      template: APPINSTALLER_TEMPLATE,
      values
    })
  );
  rmSync(MSIX_LAYOUT, {
    recursive: true,
    force: true
  });

  return [versionedPackage, stablePackage, certificate, appinstaller];
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
    writeFileSync(
      join(directory, file), filled({
        template: join(WINGET_TEMPLATES, file),
        values
      })
    );
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

/** Build and sign the MSIX from the existing release build, touching nothing
 *  but `release/` - for trying the package without cutting a release. */
function packMsixOnly(version) {
  if (!version || !VERSION_PATTERN.test(version)) {
    fail(`usage: node scripts/release.mjs ${MSIX_ONLY_FLAG} <major.minor.patch>`);
  }

  const tools = findSigningTools();
  checkSigningCertificate();
  mkdirSync(OUTPUT_DIRECTORY, { recursive: true });
  const files = buildMsix({
    version,
    tools
  });
  console.log(`Packed and signed:\n${files.join("\n")}`);
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
function checkReleaseArguments({ version, notesFile }) {
  const isUsageRight = Boolean(version && notesFile) && VERSION_PATTERN.test(version);
  if (!isUsageRight) {
    fail(`usage: node scripts/release.mjs <major.minor.patch> <notes-file> [${NO_WINGET_FLAG}]`);
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

/** Build the exe, the MSI and the MSIX for `version` into `release/`. */
function buildReleaseFiles({ version, tools }) {
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

/** Commit the version bump, tag it and push both. */
function commitAndTag({ version, tag }) {
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
function createGitHubRelease({ version, tag, previousTag, highlights, files }) {
  const releaseNotes = `${highlights}

**All changes:** https://github.com/${REPOSITORY}/compare/${previousTag}...${tag}
`;
  const releaseNotesFile = join(OUTPUT_DIRECTORY, `notes-${version}.md`);
  writeFileSync(releaseNotesFile, releaseNotes);
  run({
    command: "gh",
    args: ["release", "create", tag, files.asset, files.installer, ...files.msixFiles, "--title", `PADE ${version}`, "--notes-file", releaseNotesFile]
  });
}

/** Write the manifest self-updating copies poll, commit it and push it. */
function publishUpdateManifest({ version, tag, files }) {
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

function release({ version, notesFile, isWingetSkipped }) {
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
  const manifest = publishUpdateManifest({
    version,
    tag,
    files
  });
  if (!isWingetSkipped) {
    submitToWinget(manifest);
  }

  console.log(`Released ${tag}: https://github.com/${REPOSITORY}/releases/tag/${tag}`);
}

const [firstArgument, secondArgument, ...flags] = process.argv.slice(2);
if (firstArgument === MSIX_ONLY_FLAG) {
  packMsixOnly(secondArgument);
} else {
  release({
    version: firstArgument,
    notesFile: secondArgument,
    isWingetSkipped: flags.includes(NO_WINGET_FLAG)
  });
}
