import { namingSignal } from "@/lib/autoName";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\pade\\workspaces\\temp-42";

describe("namingSignal", () => {
  it("returns the normalized path for a real source change", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: "C:\\pade\\workspaces\\temp-42\\src\\main.ts"
    });

    expect(signal).toBe("c:/pade/workspaces/temp-42/src/main.ts");
  });

  it("matches across separator and casing differences", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: "c:/PADE/workspaces/TEMP-42/index.html"
    });

    expect(signal).toBe("c:/pade/workspaces/temp-42/index.html");
  });

  it("ignores changes outside the workspace", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: "C:\\elsewhere\\main.ts"
    });

    expect(signal).toBeNull();
  });

  it("ignores dot-dirs like .git", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: `${PROJECT}\\.git\\HEAD`
    });

    expect(signal).toBeNull();
  });

  it("ignores dotfiles at any depth", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: `${PROJECT}\\src\\.env`
    });

    expect(signal).toBeNull();
  });

  it("ignores the workspace dir itself", () => {
    const signal = namingSignal({
      projectDir: PROJECT,
      changedPath: PROJECT
    });

    expect(signal).toBeNull();
  });
});
