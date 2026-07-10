import { baseName, displayName, isTemporaryWorkspace } from "@/lib/paths";
import { describe, expect, it } from "vitest";

describe("baseName", () => {
  it("returns the final segment of a Windows path", () => {
    expect(baseName("C:\\repositories\\avi\\pade")).toBe("pade");
  });

  it("returns the final segment of a POSIX path", () => {
    expect(baseName("/home/avi/pade")).toBe("pade");
  });

  it("ignores trailing separators", () => {
    expect(baseName("C:\\projects\\demo\\")).toBe("demo");
  });

  it("returns the input unchanged when it has no separators", () => {
    expect(baseName("pade")).toBe("pade");
  });
});

describe("displayName", () => {
  const temporary = "C:\\pade\\workspaces\\temp-42";

  it("prefers the assigned label", () => {
    expect(displayName(temporary, { [temporary]: "brave-otter" })).toBe("brave-otter");
  });

  it("falls back to the folder name when no label is assigned", () => {
    expect(displayName(temporary, {})).toBe("temp-42");
  });
});

describe("isTemporaryWorkspace", () => {
  it("recognises a stamped temp dir under workspaces", () => {
    expect(isTemporaryWorkspace("C:\\pade\\workspaces\\temp-1720000000")).toBe(true);
  });

  it("recognises the forward-slash form", () => {
    expect(isTemporaryWorkspace("/pade/workspaces/temp-7")).toBe(true);
  });

  it("rejects a named (non-temp) workspace", () => {
    expect(isTemporaryWorkspace("C:\\pade\\workspaces\\brave-otter")).toBe(false);
  });

  it("rejects a temp dir that is not directly under workspaces", () => {
    expect(isTemporaryWorkspace("C:\\pade\\workspaces\\temp-1\\nested")).toBe(false);
  });

  it("rejects a temp name without a numeric stamp", () => {
    expect(isTemporaryWorkspace("C:\\pade\\workspaces\\temp-abc")).toBe(false);
  });
});
