import { ProjectChoice, projectTriggerLabel, resolveHomeStart } from "@/panels/picker/home-composer";
import { describe, expect, it } from "vitest";

const ROOT = "C:\\repositories";

function base(): {
  selectedPath: string;
  choice: ProjectChoice;
  root: string;
  rawName: string;
  rawPrompt: string;
  agent: string | null;
} {
  return {
    selectedPath: "",
    choice: ProjectChoice.Existing,
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
        choice: ProjectChoice.New,
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
        choice: ProjectChoice.New,
        rawName: "poll",
        root: ""
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.New,
        rawName: "  "
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.New,
        rawName: "has/slash"
      })
    ).toEqual({ kind: "none" });
  });

  it("starts a throwaway workspace whatever else is typed", () => {
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.Temp,
        rawName: "ignored"
      })
    ).toEqual({ kind: "temp" });
  });
});

describe("projectTriggerLabel", () => {
  it("names what the menu picked", () => {
    function label(choice: ProjectChoice, typedName = "", selectedName: string | null = null) {
      return projectTriggerLabel({
        choice,
        typedName,
        selectedName
      });
    }

    expect(label(ProjectChoice.Temp)).toBe("Throwaway workspace");
    expect(label(ProjectChoice.New)).toBe("New project");
    expect(label(ProjectChoice.New, "poll")).toBe("New: poll");
    expect(label(ProjectChoice.Existing, "", "pade")).toBe("pade");
    expect(label(ProjectChoice.Existing)).toBe("Choose a project");
  });
});
