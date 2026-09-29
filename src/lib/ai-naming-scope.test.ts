import { aiNamingEnabled, globalNaming, projectNamingChoice, withProjectNamingChoice } from "@/lib/ai-naming-scope";
import type { Prefs } from "@/lib/types";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\pade";
const OTHER = "C:\\repositories\\avi\\any-stt";

describe("aiNamingEnabled", () => {
  it("is off until something turns it on", () => {
    expect(
      aiNamingEnabled({
        prefs: {},
        project: PROJECT
      })
    ).toBe(false);
  });

  it("follows the global default when the project has no choice", () => {
    expect(
      aiNamingEnabled({
        prefs: {
          aiSessionNaming: true
        },
        project: PROJECT
      })
    ).toBe(true);
  });

  it("lets a project opt out of a global yes", () => {
    const prefs: Prefs = {
      aiSessionNaming: true,
      aiSessionNamingProjects: {
        [PROJECT]: false
      }
    };

    expect(
      aiNamingEnabled({
        prefs,
        project: PROJECT
      })
    ).toBe(false);
    expect(
      aiNamingEnabled({
        prefs,
        project: OTHER
      })
    ).toBe(true);
  });

  it("lets a project opt in to a global no", () => {
    expect(
      aiNamingEnabled({
        prefs: {
          aiSessionNamingProjects: {
            [PROJECT]: true
          }
        },
        project: PROJECT
      })
    ).toBe(true);
  });

  // `false` is a real choice, not an absent one — the classic ?? / || trap, and
  // the reason a project can pin itself off while the global is on.
  it("keeps a project's explicit off distinct from having no choice", () => {
    const prefs: Prefs = {
      aiSessionNamingProjects: {
        [PROJECT]: false
      }
    };
    expect(
      projectNamingChoice({
        prefs,
        project: PROJECT
      })
    ).toBe(false);
    expect(
      projectNamingChoice({
        prefs,
        project: OTHER
      })
    ).toBeUndefined();
  });
});

describe("globalNaming", () => {
  it("reads off when unset", () => {
    expect(globalNaming({})).toBe(false);
    expect(globalNaming({ aiSessionNaming: null })).toBe(false);
    expect(globalNaming({ aiSessionNaming: true })).toBe(true);
  });
});

describe("withProjectNamingChoice", () => {
  it("records a choice without touching other projects", () => {
    const next = withProjectNamingChoice({
      prefs: {
        aiSessionNamingProjects: {
          [OTHER]: true
        }
      },
      project: PROJECT,
      choice: false
    });

    expect(next).toEqual({
      [OTHER]: true,
      [PROJECT]: false
    });
  });

  it("drops the key when the project goes back to following the global", () => {
    const next = withProjectNamingChoice({
      prefs: {
        aiSessionNamingProjects: {
          [PROJECT]: true,
          [OTHER]: true
        }
      },
      project: PROJECT,
      choice: undefined
    });

    expect(next).toEqual({ [OTHER]: true });
    expect(PROJECT in next).toBe(false);
  });

  it("starts a record when there is none", () => {
    expect(
      withProjectNamingChoice({
        prefs: {},
        project: PROJECT,
        choice: true
      })
    ).toEqual({ [PROJECT]: true });
  });
});
