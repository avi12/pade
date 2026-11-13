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

describe("startingModelChoice", () => {
  const OPUS = { model: "claude-opus-5-5" };
  const FABLE = { model: "claude-fable-5-2" };
  const SONNET = { model: "claude-sonnet-5-5" };
  const prefs: Prefs = {
    agentModelGlobal: {
      [AgentId.Claude]: OPUS
    },
    agentModelProjects: {
      [PROJECT]: {
        [AgentId.Claude]: FABLE
      }
    }
  };

  it("takes the workspace's pick over every workspace's", () => {
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: PROJECT,
        agent: AgentId.Claude
      })
    ).toEqual(FABLE);
  });

  it("falls back to every workspace's pick, then the agent's own default", () => {
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: "C:/elsewhere",
        agent: AgentId.Claude
      })
    ).toEqual(OPUS);
    expect(
      startingModelChoice({
        explicit: undefined,
        prefs,
        project: "C:/elsewhere",
        agent: AgentId.Codex
      })
    ).toBeUndefined();
  });

  it("keeps a handoff's own choice over both", () => {
    expect(
      startingModelChoice({
        explicit: SONNET,
        prefs,
        project: PROJECT,
        agent: AgentId.Claude
      })
    ).toEqual(SONNET);
  });
});

describe("withGlobalModelDefault", () => {
  it("sets an agent's pick and drops it when cleared", () => {
    const set = withGlobalModelDefault({
      prefs: {},
      agent: AgentId.Claude,
      choice: {
        model: "claude-opus-5-5"
      }
    });
    expect(set).toEqual({
      [AgentId.Claude]: {
        model: "claude-opus-5-5"
      }
    });
    expect(
      withGlobalModelDefault({
        prefs: {
          agentModelGlobal: set
        },
        agent: AgentId.Claude,
        choice: {}
      })
    ).toEqual({});
  });
});
