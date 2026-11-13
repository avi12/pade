import { describe, expect, it } from "vitest";

import {
  NEW_PROJECT_PLACEHOLDER,
  newProjectNameProblem,
  ProjectChoice,
  projectTrigger,
  resolveHomeStart
} from "@/panels/picker/home/home-composer";

const ROOT = "C:\\repositories";

function base() {
  return {
    selectedPath: "",
    choice: ProjectChoice.EXISTING,
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
        choice: ProjectChoice.NEW,
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
        choice: ProjectChoice.NEW,
        rawName: "poll",
        root: ""
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.NEW,
        rawName: "  "
      })
    ).toEqual({ kind: "none" });
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.NEW,
        rawName: "has/slash"
      })
    ).toEqual({ kind: "none" });
  });

  it("starts a throwaway workspace whatever else is typed", () => {
    expect(
      resolveHomeStart({
        ...base(),
        choice: ProjectChoice.TEMP,
        rawName: "ignored"
      })
    ).toEqual({ kind: "temp" });
  });
});

describe("projectTrigger", () => {
  function trigger({
    choice,
    root = ROOT,
    selectedName = null,
    selectedPath = ""
  }: {
    choice: ProjectChoice;
    root?: string;
    selectedName?: string | null;
    selectedPath?: string;
  }) {
    return projectTrigger({
      choice,
      root,
      selectedName,
      selectedPath
    });
  }

  it("names the throwaway workspace", () => {
    expect(trigger({ choice: ProjectChoice.TEMP })).toEqual({
      prefix: "",
      name: "Throwaway workspace"
    });
  });

  it("says which root a new project goes under", () => {
    expect(trigger({ choice: ProjectChoice.NEW })).toEqual({
      prefix: "repositories\\",
      name: NEW_PROJECT_PLACEHOLDER
    });
  });

  it("names no root for a new project while none is chosen", () => {
    expect(
      trigger({
        choice: ProjectChoice.NEW,
        root: ""
      }).prefix
    ).toBe("");
  });

  it("names the chosen project with its root", () => {
    expect(
      trigger({
        choice: ProjectChoice.EXISTING,
        selectedName: "pade",
        selectedPath: "C:\\repositories\\avi\\pade"
      })
    ).toEqual({
      prefix: "avi\\",
      name: "pade"
    });
  });

  it("asks for a project while none is chosen", () => {
    expect(trigger({ choice: ProjectChoice.EXISTING }).name).toBe("Choose a project");
  });
});

describe("newProjectNameProblem", () => {
  const clear = {
    nameError: null,
    isTakenByFile: false,
    isExistingFolder: false
  };

  it("has nothing to say about a name that can be created", () => {
    expect(newProjectNameProblem(clear)).toBeNull();
  });

  it("puts the schema's own reason first", () => {
    expect(
      newProjectNameProblem({
        ...clear,
        nameError: "Too long",
        isExistingFolder: true
      })
    ).toBe("Too long");
  });

  it("says what already stands on disk under the name", () => {
    expect(
      newProjectNameProblem({
        ...clear,
        isTakenByFile: true
      })
    ).toBe("That name is taken by a file in this root");
    expect(
      newProjectNameProblem({
        ...clear,
        isExistingFolder: true
      })
    ).toBe("This folder already exists in this root");
  });
});
