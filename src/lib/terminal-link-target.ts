// Classify a terminal hyperlink before handing it to an OS integration. OSC-8
// links can point to either a web page or a local file. The latter is how Codex
// exposes workspace paths, and must go to the file manager rather than the
// browser-only `open_url` command.

export const TerminalLinkTarget = {
  browser: "browser",
  explorer: "explorer"
} as const;
export type TerminalLinkTarget = (typeof TerminalLinkTarget)[keyof typeof TerminalLinkTarget];

export type TerminalLinkDestination = {
  kind: TerminalLinkTarget;
  value: string;
};

/**
 * Return the safe OS destination for an OSC-8 target. Network links remain
 * browser links; local `file:` links with no remote host open in Explorer.
 * Other schemes are intentionally unsupported.
 */
export function terminalLinkDestination(uri: string): TerminalLinkDestination | null {
  try {
    const url = new URL(uri);
    if (url.protocol === "https:" || url.protocol === "http:") {
      return {
        kind: TerminalLinkTarget.browser,
        value: uri
      };
    }

    // A remote `file://server/share` URL is not a local workspace path. Never
    // turn an agent-provided terminal link into a network-share navigation.
    if (url.protocol !== "file:" || (url.hostname !== "" && url.hostname !== "localhost")) {
      return null;
    }

    let path = decodeURIComponent(url.pathname);
    // A file URL on Windows is `/C:/…`; Explorer expects the drive to lead.
    if (/^\/[A-Za-z]:\//.test(path)) {
      path = path.slice(1).replaceAll("/", "\\");
    }

    return path
      ? {
        kind: TerminalLinkTarget.explorer,
        value: path
      }
      : null;
  } catch {
    return null;
  }
}
