import {
  baseName,
  displayName,
  isTemporaryWorkspace,
  normalizePath,
  parentDir
} from "@/lib/paths";
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
    expect(
      displayName({
        path: temporary,
        labels: {
          [temporary]: "brave-otter"
        }
      })
    ).toBe("brave-otter");
  });

  it("falls back to the folder name when no label is assigned", () => {
    expect(
      displayName({
        path: temporary,
        labels: {}
      })
    ).toBe("temp-42");
  });
});

describe("parentDir", () => {
  it("returns the containing folder of a Windows path", () => {
    expect(parentDir("C:\\repositories\\avi\\pade")).toBe("C:\\repositories\\avi");
  });

  it("returns the containing folder of a POSIX path", () => {
    expect(parentDir("/home/avi/pade")).toBe("/home/avi");
  });

  it("ignores a trailing separator", () => {
    expect(parentDir("C:\\projects\\demo\\")).toBe("C:\\projects");
  });

  it("returns null for a bare name with no parent", () => {
    expect(parentDir("pade")).toBeNull();
  });

  it("returns null at the top of a POSIX tree", () => {
    expect(parentDir("/home")).toBeNull();
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

describe("normalizePath", () => {
  it("lowercases and forward-slashes a Windows path", () => {
    expect(normalizePath("C:\\Repos\\Avi\\PADE")).toBe("c:/repos/avi/pade");
  });

  it("leaves an already-normal path unchanged", () => {
    expect(normalizePath("c:/repos/avi/pade")).toBe("c:/repos/avi/pade");
  });

  it("drops a trailing separator so C:\\repositories\\ equals C:\\repositories", () => {
    expect(normalizePath("C:\\repositories\\")).toBe(normalizePath("C:\\repositories"));
    expect(normalizePath("c:/repos/avi/")).toBe("c:/repos/avi");
  });

  it("keeps case on a POSIX / WSL path (case-sensitive filesystem)", () => {
    expect(normalizePath("/home/User/Project")).toBe("/home/User/Project");
    // Case-differing POSIX paths are distinct files, so they must not compare equal.
    expect(normalizePath("/home/User/x")).not.toBe(normalizePath("/home/user/x"));
  });

  it("still folds separators and a trailing slash on a POSIX path", () => {
    expect(normalizePath("/mnt/c/Repos/")).toBe("/mnt/c/Repos");
  });
});
