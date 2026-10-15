// Classify a terminal hyperlink before handing it to an OS integration. OSC-8
// links can point to either a web page or a local file. The latter is how Codex
// exposes workspace paths, and must go to the file manager rather than the
// browser-only `open_url` command.

export const TerminalLinkTarget = {
  Browser: "browser",
  Explorer: "explorer"
} as const;
type TerminalLinkTarget = (typeof TerminalLinkTarget)[keyof typeof TerminalLinkTarget];

type TerminalLinkDestination = {
  kind: TerminalLinkTarget;
  value: string;
};

const WEB_PROTOCOLS = new Set(["https:", "http:"]);
const FILE_PROTOCOL = "file:";
const LOCAL_HOSTS = new Set(["", "localhost"]);
/** A file URL's pathname on Windows: `/C:/…`, the drive behind a leading slash. */
const WINDOWS_DRIVE_PATHNAME = /^\/[A-Za-z]:\//;

/**
 * Return the safe OS destination for an OSC-8 target. Network links remain
 * browser links; local `file:` links with no remote host open in Explorer.
 * Other schemes are intentionally unsupported.
 */
export function terminalLinkDestination(uri: string) {
  try {
    const url = new URL(uri);
    if (WEB_PROTOCOLS.has(url.protocol)) {
      return {
        kind: TerminalLinkTarget.Browser,
        value: uri
      } satisfies TerminalLinkDestination;
    }

    // A remote `file://server/share` URL is not a local workspace path. Never
    // turn an agent-provided terminal link into a network-share navigation.
    const isLocalFile = url.protocol === FILE_PROTOCOL && LOCAL_HOSTS.has(url.hostname);
    if (!isLocalFile) {
      return null;
    }

    let path = decodeURIComponent(url.pathname);
    // Explorer expects the drive to lead, with Windows separators.
    if (WINDOWS_DRIVE_PATHNAME.test(path)) {
      path = path.slice(1).replaceAll("/", "\\");
    }

    return path
      ? {
        kind: TerminalLinkTarget.Explorer,
        value: path
      } satisfies TerminalLinkDestination
      : null;
  } catch {
    return null;
  }
}
