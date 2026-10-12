import { projectModelDefault, withProjectModelDefault } from "@/lib/agent-model-scope";
import type { Prefs } from "@/lib/types";
import { describe, expect, it } from "vitest";

const PROJECT = "C:/repositories/avi/pade";

describe("projectModelDefault", () => {
  it("is undefined until the workspace picks one", () => {
    expect(
      projectModelDefault({
        prefs: {},
        project: PROJECT,
        agent: "claude"
      })
    ).toBeUndefined();
  });

  it("reads the workspace's pick for that agent only", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          claude: {
            model: "claude-opus-5-5",
            effort: "high"
          }
        }
      }
    };

    expect(
      projectModelDefault({
        prefs,
        project: PROJECT,
        agent: "claude"
      })
    ).toEqual({
      model: "claude-opus-5-5",
      effort: "high"
    });
    expect(
      projectModelDefault({
        prefs,
        project: PROJECT,
        agent: "codex"
      })
    ).toBeUndefined();
  });
});

describe("withProjectModelDefault", () => {
  it("sets one agent without touching the others", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          codex: {
            model: "gpt-5.6-terra"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: "claude",
        choice: {
          model: "claude-fable-5-1"
        }
      })
    ).toEqual({
      [PROJECT]: {
        codex: {
          model: "gpt-5.6-terra"
        },
        claude: {
          model: "claude-fable-5-1"
        }
      }
    });
  });

  it("drops the agent, then the workspace, when cleared", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          claude: {
            model: "claude-opus-5-5"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: "claude",
        choice: {}
      })
    ).toEqual({});
  });
});
