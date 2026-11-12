import { fail, readPowerShell } from "./environment.mjs";
import { isNewer } from "./version.mjs";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

/** The code-signing certificate in `Cert:\CurrentUser\My`. Windows requires the
 *  package's Publisher to equal the certificate's subject exactly. */
const DEFAULT_SIGNING_THUMBPRINT = "DA09BDA44CB503CB519602BD6BD0D1F126D98B8A";
const SIGNING_THUMBPRINT_VARIABLE = "PADE_SIGNING_THUMBPRINT";
const CONFIGURED_THUMBPRINT = process.env[SIGNING_THUMBPRINT_VARIABLE] || DEFAULT_SIGNING_THUMBPRINT;
export const SIGNING_THUMBPRINT = CONFIGURED_THUMBPRINT.toUpperCase();
export const SIGNING_SUBJECT = "CN=avi12";
const THUMBPRINT_PATTERN = /^[0-9A-F]{40}$/;
export const SIGNING_CERTIFICATE_PATH = `Cert:\\CurrentUser\\My\\${SIGNING_THUMBPRINT}`;
export const TIMESTAMP_URL = "http://timestamp.digicert.com";
export const SIGNING_DIGEST = "SHA256";
const WINDOWS_KITS_BIN = join(process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Windows Kits", "10", "bin");
/** A Windows SDK's bin folder is named for its four-part version. */
const SDK_VERSION_PATTERN = /^\d+(\.\d+){3}$/;
const SDK_ARCHITECTURE = "x64";

const MAKEAPPX = "makeappx.exe";
const SIGNTOOL = "signtool.exe";

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
export function findSigningTools() {
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
export function checkSigningCertificate() {
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
