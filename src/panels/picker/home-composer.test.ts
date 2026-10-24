import { resolveHomeStart } from "@/panels/picker/home-composer";
import { describe, expect, it } from "vitest";

const ROOT = "C:\\repositories";

function base(): {
  selectedPath: string;
  isCreating: boolean;
  root: string;
  rawName: string;
  rawPrompt: string;
  agent: string | null;
} {
  return {
    selectedPath: "",
    isCreating: false,
    root: ROOT,
    rawName: "",
    rawPrompt: "",
    agent: "claude"
  };
}

describe("resolveHomeStart", () => {
  it("opens the chosen project with its prompt and agent", () => {
    expect(
      resolveHomeStart({
        ...base(),
        selectedPath: `${ROOT}\\poll`,
        rawPrompt: "scaffold an app"
      })
    ).toEqual({
      kind: "open",
      target: {
        path: `${ROOT}\\poll`,
        initialPrompt: "scaffold an app",
        agent: "claude"
      }
    });
  });

  it("opens without a prompt or agent when neither was picked", () => {
    expect(
      resolveHomeStart({
        ...base(),
        selectedPath: `${ROOT}\\poll`,
        rawPrompt: "   ",
        agent: null
      })
    ).toEqual({
      kind: "open",
      target: {
        path: `${ROOT}\\poll`,
        initialPrompt: undefined,
        agent: undefined
      }
    });
  });

  it("does nothing with no project and no new-project form", () => {
    expect(resolveHomeStart(base())).toEqual({ kind: "none" });
  });

  it("creates the named project in its root", () => {
    expect(
      resolveHomeStart({
        ...base(),
        isCreating: true,
        rawName: "  poll  ",
        rawPrompt: "scaffold it"
      })
    ).toEqual({
      kind: "create",
      root: ROOT,
      name: "poll",
      prompt: "scaffold it",
      agent: "claude"
    });
  });

  it("needs both a root and a valid name to create", () => {
    expect(
      resolveHomeStart({
        ...base(),
        isCreating: true,
        rawName: "poll",
        root: ""
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        isCreating: true,
        rawName: "  "
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        isCreating: true,
        rawName: "has/slash"
      })
    ).toEqual({ kind: "none" });
  });
});
