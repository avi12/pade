import { AgentId } from "@/lib/agent-icon";
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
        agent: AgentId.Claude
      })
    ).toBeUndefined();
  });

  it("reads the workspace's pick for that agent only", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.Claude]: {
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
        agent: AgentId.Claude
      })
    ).toEqual({
      model: "claude-opus-5-5",
      effort: "high"
    });
    expect(
      projectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.Codex
      })
    ).toBeUndefined();
  });
});

describe("withProjectModelDefault", () => {
  it("sets one agent without touching the others", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.Codex]: {
            model: "gpt-5.6-terra"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.Claude,
        choice: {
          model: "claude-fable-5-1"
        }
      })
    ).toEqual({
      [PROJECT]: {
        [AgentId.Codex]: {
          model: "gpt-5.6-terra"
        },
        [AgentId.Claude]: {
          model: "claude-fable-5-1"
        }
      }
    });
  });

  it("drops the agent, then the workspace, when cleared", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.Claude]: {
            model: "claude-opus-5-5"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.Claude,
        choice: {}
      })
    ).toEqual({});
  });
});
