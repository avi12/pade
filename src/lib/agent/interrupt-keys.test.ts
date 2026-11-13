import { describe, expect, it } from "vitest";

import { AgentId } from "@/lib/agent/agent-icon";
import { interruptKeysOf } from "@/lib/agent/interrupt-keys";
import { ESCAPE } from "@/lib/terminal/input/terminal-sequences";

const CONTROL_X = String.fromCharCode(0x18);
const CONTROL_K = String.fromCharCode(0x0b);
const CONTROL_G = String.fromCharCode(0x07);

function chat(bindings: Record<string, string | null>) {
  return {
    bindings: [
      {
        context: "Chat",
        bindings
      }
    ]
  };
}

describe("interruptKeysOf", () => {
  it("presses Claude's stop-all chord twice and its Esc once by default", () => {
    expect(
      interruptKeysOf({
        command: AgentId.CLAUDE,
        keybindings: null
      })
    ).toEqual({
      interrupt: {
        keystrokes: [ESCAPE],
        presses: 1
      },
      stopAll: {
        keystrokes: [CONTROL_X, CONTROL_K],
        presses: 2
      },
      hasMidTurnStopHook: true,
      canHoldToolCalls: true
    });
  });

  it("keeps the default when the user only adds a chord for the action", () => {
    const keys = interruptKeysOf({
      command: AgentId.CLAUDE,
      keybindings: chat({ "ctrl+g": "chat:killAgents" })
    });
    expect(keys?.stopAll?.keystrokes).toEqual([CONTROL_X, CONTROL_K]);
  });

  it("follows the user's chord once the default is unbound or taken", () => {
    const unbound = interruptKeysOf({
      command: AgentId.CLAUDE,
      keybindings: chat({
        "ctrl+x ctrl+k": null,
        "ctrl+g": "chat:killAgents"
      })
    });
    expect(unbound?.stopAll?.keystrokes).toEqual([CONTROL_G]);

    const taken = interruptKeysOf({
      command: AgentId.CLAUDE,
      keybindings: chat({ "ctrl+x ctrl+k": "chat:clearInput" })
    });
    expect(taken?.stopAll).toBeNull();
  });

  it("ignores bindings of another context", () => {
    const keys = interruptKeysOf({
      command: AgentId.CLAUDE,
      keybindings: {
        bindings: [
          {
            context: "Global",
            bindings: {
              escape: null
            }
          }
        ]
      }
    });
    expect(keys?.interrupt?.keystrokes).toEqual([ESCAPE]);
  });

  it("knows OpenCode's double Esc and no stop-all for agents nobody verified", () => {
    expect(
      interruptKeysOf({
        command: AgentId.OPENCODE,
        keybindings: null
      })?.interrupt?.presses
    ).toBe(2);
    for (const command of [AgentId.CODEX, AgentId.COPILOT, AgentId.OPENCODE]) {
      expect(
        interruptKeysOf({
          command,
          keybindings: null
        })?.stopAll ?? null
      ).toBeNull();
    }

    expect(
      interruptKeysOf({
        command: AgentId.CLAUDE,
        keybindings: null
      })?.stopAll
    ).not.toBeNull();
    expect(
      interruptKeysOf({
        command: AgentId.AIDER,
        keybindings: null
      })
    ).toBeNull();
  });
});
