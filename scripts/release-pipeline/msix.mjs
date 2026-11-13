import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import {
  BUILT_EXECUTABLE,
  CONTEXT_MENU_HANDLER,
  CONTEXT_MENU_HANDLER_FILE,
  EXECUTABLE_FILE,
  fail,
  filled,
  MSIX_ONLY_FLAG,
  OUTPUT_DIRECTORY,
  readPowerShell,
  ROOT,
  run
} from "./environment.mjs";
import { latestReleaseUrl } from "./github.mjs";
import {
  checkSigningCertificate,
  findSigningTools,
  SIGNING_CERTIFICATE_PATH,
  SIGNING_DIGEST,
  SIGNING_SUBJECT,
  SIGNING_THUMBPRINT,
  TIMESTAMP_URL
} from "./signing.mjs";
import { VERSION_PATTERN } from "./version.mjs";

/** The MSIX package's identity name - the same in its manifest and its feed. */
const MSIX_PACKAGE_NAME = "avi12.PADE";
const MSIX_TEMPLATES = join(ROOT, "packaging", "msix");
const MSIX_MANIFEST_TEMPLATE = join(MSIX_TEMPLATES, "AppxManifest.xml");
const APPINSTALLER_TEMPLATE = join(MSIX_TEMPLATES, "PADE.appinstaller");
const MSIX_MANIFEST_FILE = "AppxManifest.xml";
const MSIX_LAYOUT = join(OUTPUT_DIRECTORY, "msix-layout");
const MSIX_ASSETS_DIRECTORY = "Assets";
/** The logos the package manifest names, already generated from the app icon. */
const MSIX_LOGOS = ["StoreLogo.png", "Square44x44Logo.png", "Square71x71Logo.png", "Square150x150Logo.png"];
const APP_ICONS = join(ROOT, "src-tauri", "icons");
/** The names the feed reaches through `/releases/latest/download/`, so one
 *  installed copy follows every later release. */
const STABLE_PACKAGE_NAME = "PADE.msix";
const APPINSTALLER_NAME = "PADE.appinstaller";
const CERTIFICATE_NAME = "PADE.cer";
/** MSIX versions have four parts; a release fills the first three. */
const MSIX_REVISION = "0";

/** Lay out the package folder from the release build, pack it, sign it, and
 *  write the public certificate and the update feed beside it. Answers every
 *  file the release attaches for MSIX. */
export function buildMsix({ version, tools }) {
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
    join(MSIX_LAYOUT, MSIX_MANIFEST_FILE),
    filled({
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
    args: [
      "sign",
      "/fd",
      SIGNING_DIGEST,
      "/sha1",
      SIGNING_THUMBPRINT,
      "/tr",
      TIMESTAMP_URL,
      "/td",
      SIGNING_DIGEST,
      versionedPackage
    ]
  });

  const stablePackage = join(OUTPUT_DIRECTORY, STABLE_PACKAGE_NAME);
  copyFileSync(versionedPackage, stablePackage);
  const certificate = join(OUTPUT_DIRECTORY, CERTIFICATE_NAME);
  readPowerShell(
    `Export-Certificate -Cert '${SIGNING_CERTIFICATE_PATH}' -FilePath '${certificate.replaceAll("'", "''")}' -Type CERT | Out-Null`
  );
  const appinstaller = join(OUTPUT_DIRECTORY, APPINSTALLER_NAME);
  writeFileSync(
    appinstaller,
    filled({
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

/** Build and sign the MSIX from the existing release build, touching nothing
 *  but `release/` - for trying the package without cutting a release. */
export function packMsixOnly(version) {
  const isVersionInvalid = !version || !VERSION_PATTERN.test(version);
  if (isVersionInvalid) {
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
