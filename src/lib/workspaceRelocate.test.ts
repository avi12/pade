import { isUnderDir, remapDir } from "@/lib/workspaceRelocate";
import { describe, expect, it } from "vitest";

describe("isUnderDir", () => {
  it("matches the base directory itself", () => {
    const under = isUnderDir({
      dir: "C:\\pade\\workspaces\\temp-1",
      base: "C:\\pade\\workspaces\\temp-1"
    });

    expect(under).toBe(true);
  });

  it("matches a nested directory", () => {
    const under = isUnderDir({
      dir: "C:\\pade\\workspaces\\temp-1\\src",
      base: "C:\\pade\\workspaces\\temp-1"
    });

    expect(under).toBe(true);
  });

  it("compares across separator and casing differences", () => {
    const under = isUnderDir({
      dir: "c:/PADE/workspaces/TEMP-1/src",
      base: "C:\\pade\\workspaces\\temp-1"
    });

    expect(under).toBe(true);
  });

  it("rejects a sibling whose name merely shares the prefix", () => {
    const under = isUnderDir({
      dir: "C:\\pade\\workspaces\\temp-10",
      base: "C:\\pade\\workspaces\\temp-1"
    });

    expect(under).toBe(false);
  });

  it("rejects a directory outside the base", () => {
    const under = isUnderDir({
      dir: "C:\\elsewhere",
      base: "C:\\pade\\workspaces\\temp-1"
    });

    expect(under).toBe(false);
  });
});

describe("remapDir", () => {
  it("re-points the base directory itself", () => {
    const remapped = remapDir({
      dir: "C:\\ws\\old-name",
      from: "C:\\ws\\old-name",
      to: "C:\\ws\\new-name"
    });

    expect(remapped).toBe("C:\\ws\\new-name");
  });

  it("keeps the suffix under the new base", () => {
    const remapped = remapDir({
      dir: "C:\\ws\\old-name\\packages\\core",
      from: "C:\\ws\\old-name",
      to: "D:\\projects\\moved"
    });

    expect(remapped).toBe("D:\\projects\\moved\\packages\\core");
  });
});
