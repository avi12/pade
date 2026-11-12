import {
  filled,
  OUTPUT_DIRECTORY,
  read,
  ROOT,
  run
} from "./environment.mjs";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const WINGET_TEMPLATES = join(ROOT, "packaging", "winget");
const WINGET_PACKAGE = "avi12.PADE";
const WINGET_PACKAGE_PATH = "repos/microsoft/winget-pkgs/contents/manifests/a/avi12/PADE";

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
export function submitToWinget({ version, url, sha256 }) {
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
