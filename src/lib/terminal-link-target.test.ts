import { terminalLinkDestination, TerminalLinkTarget } from "@/lib/terminal-link-target";
import { describe, expect, it } from "vitest";

describe("terminalLinkDestination", () => {
  it("keeps web links in the browser", () => {
    expect(terminalLinkDestination("https://example.com/docs?q=pade")).toEqual({
      kind: TerminalLinkTarget.browser,
      value: "https://example.com/docs?q=pade"
    });
  });

  it("opens a Windows OSC-8 file link in Explorer", () => {
    expect(terminalLinkDestination("file:///C:/repositories/avi/pade/src/App.svelte")).toEqual({
      kind: TerminalLinkTarget.explorer,
      value: "C:\\repositories\\avi\\pade\\src\\App.svelte"
    });
  });

  it("decodes a local POSIX file link", () => {
    expect(terminalLinkDestination("file:///tmp/project%20name/readme.md")).toEqual({
      kind: TerminalLinkTarget.explorer,
      value: "/tmp/project name/readme.md"
    });
  });

  it("rejects remote file shares and non-web schemes", () => {
    expect(terminalLinkDestination("file://server/share/project")).toBeNull();
    expect(terminalLinkDestination("javascript:alert(1)")).toBeNull();
  });
});
