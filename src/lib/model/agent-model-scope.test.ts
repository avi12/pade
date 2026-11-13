import { describe, expect, it } from "vitest";

import { AgentId } from "@/lib/agent/agent-icon";
import {
  projectModelDefault,
  startingModelChoice,
  withGlobalModelDefault,
  withProjectModelDefault
} from "@/lib/model/agent-model-scope";
import type { Prefs } from "@/lib/types/settings";

const PROJECT = "C:/repositories/avi/pade";

describe("projectModelDefault", () => {
  it("is undefined until the workspace picks one", () => {
    expect(
      projectModelDefault({
        prefs: {},
        project: PROJECT,
        agent: AgentId.CLAUDE
      })
    ).toBeUndefined();
  });

  it("reads the workspace's pick for that agent only", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.CLAUDE]: {
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
        agent: AgentId.CLAUDE
      })
    ).toEqual({
      model: "claude-opus-5-5",
      effort: "high"
    });
    expect(
      projectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.CODEX
      })
    ).toBeUndefined();
  });
});

describe("withProjectModelDefault", () => {
  it("sets one agent without touching the others", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.CODEX]: {
            model: "gpt-5.6-terra"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.CLAUDE,
        choice: {
          model: "claude-fable-5-1"
        }
      })
    ).toEqual({
      [PROJECT]: {
        [AgentId.CODEX]: {
          model: "gpt-5.6-terra"
        },
        [AgentId.CLAUDE]: {
          model: "claude-fable-5-1"
        }
      }
    });
  });

  it("drops the agent, then the workspace, when cleared", () => {
    const prefs: Prefs = {
      agentModelProjects: {
        [PROJECT]: {
          [AgentId.CLAUDE]: {
            model: "claude-opus-5-5"
          }
        }
      }
    };

    expect(
      withProjectModelDefault({
        prefs,
        project: PROJECT,
        agent: AgentId.CLAUDE,
        choice: {}
      })
    ).toEqual({});
  });
});

describe("startingModelChoice", () => {
  const opus = { model: "claude-opus-5-5" };
  const fable = { model: "claude-fable-5-2" };
  const sonnet = { model: "claude-sonnet-5-5" };
  const prefs: Prefs = {
    agentModelGlobal: {
      [AgentId.CLAUDE]: opus
    },
    agentModelProjects: {
      [PROJECT]: {
        [AgentId.CLAUDE]: fable
      }
    }
  };

  it("takes the workspace's pick over every workspace's", () => {
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: PROJECT,
        agent: AgentId.CLAUDE
      })
    ).toEqual(fable);
  });

  it("falls back to every workspace's pick, then the agent's own default", () => {
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: "C:/elsewhere",
        agent: AgentId.CLAUDE
      })
    ).toEqual(opus);
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: "C:/elsewhere",
        agent: AgentId.CODEX
      })
    ).toBeUndefined();
  });

  it("keeps a handoff's own choice over both", () => {
    expect(
      startingModelChoice({
        explicit: sonnet,
        prefs,
        project: PROJECT,
        agent: AgentId.CLAUDE
      })
    ).toEqual(sonnet);
  });
});

describe("withGlobalModelDefault", () => {
  it("sets an agent's pick and drops it when cleared", () => {
    const set = withGlobalModelDefault({
      prefs: {},
      agent: AgentId.CLAUDE,
      choice: {
        model: "claude-opus-5-5"
      }
    });
    expect(set).toEqual({
      [AgentId.CLAUDE]: {
        model: "claude-opus-5-5"
      }
    });
    expect(
      withGlobalModelDefault({
        prefs: {
          agentModelGlobal: set
        },
        agent: AgentId.CLAUDE,
        choice: {}
      })
    ).toEqual({});
  });
});
