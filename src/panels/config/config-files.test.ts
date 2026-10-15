import type { ConfigFile } from "@/lib/types";
import { isListedConfigFile } from "@/panels/config/config-files";
import { describe, expect, it } from "vitest";

const FILES: ConfigFile[] = [
  {
    name: "CLAUDE.md",
    rel: "CLAUDE.md",
    kind: "instructions",
    exists: true
  },
  {
    name: "settings.json",
    rel: ".claude/settings.json",
    kind: "settings",
    exists: false
  }
];

describe("isListedConfigFile", () => {
  it("matches a listed file by its relative path", () => {
    expect(
      isListedConfigFile({
        changedPath: "CLAUDE.md",
        files: FILES
      })
    ).toBe(true);
  });

  it("matches an absolute Windows path ending in a listed file, ignoring case", () => {
    expect(
      isListedConfigFile({
        changedPath: String.raw`C:\Work\App\.Claude\Settings.json`,
        files: FILES
      })
    ).toBe(true);
  });

  it("ignores a file that only shares a name suffix", () => {
    expect(
      isListedConfigFile({
        changedPath: "/work/app/NOTCLAUDE.md",
        files: FILES
      })
    ).toBe(false);
  });

  it("ignores everything when nothing is listed", () => {
    expect(
      isListedConfigFile({
        changedPath: "CLAUDE.md",
        files: []
      })
    ).toBe(false);
  });
});
