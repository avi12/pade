import { describe, expect, it } from "vitest";

import type { InterruptKeys } from "@/lib/agent/interrupt-keys";
import {
  interruptPresses,
  isHookTakingStop,
  MAXIMUM_STOP_ATTEMPTS,
  stillWritingSubagents,
  StopVerdict,
  stopVerdict
} from "@/lib/usage/pacing/session-control/terminal-stop";

const ESCAPE_PRESS = {
  keystrokes: ["esc"],
  presses: 1
};
const STOP_ALL_PRESS = {
  keystrokes: ["x", "k"],
  presses: 2
};
const CLAUDE_KEYS: InterruptKeys = {
  interrupt: ESCAPE_PRESS,
  stopAll: STOP_ALL_PRESS,
  hasMidTurnStopHook: true,
  canHoldToolCalls: true
};

describe("isHookTakingStop", () => {
  it("trusts a hook that answered after the stop was written", () => {
    expect(
      isHookTakingStop({
        keys: CLAUDE_KEYS,
        stopSeenAt: 2000,
        stopWrittenAt: 1000
      })
    ).toBe(true);
  });

  it("falls back when the hook never answered, answered an older stop, or doesn't exist", () => {
    expect(
      isHookTakingStop({
        keys: CLAUDE_KEYS,
        stopSeenAt: null,
        stopWrittenAt: 1000
      })
    ).toBe(false);
    expect(
      isHookTakingStop({
        keys: CLAUDE_KEYS,
        stopSeenAt: 500,
        stopWrittenAt: 1000
      })
    ).toBe(false);
    expect(
      isHookTakingStop({
        keys: {
          hasMidTurnStopHook: false
        },
        stopSeenAt: 2000,
        stopWrittenAt: 1000
      })
    ).toBe(false);
  });
});

describe("interruptPresses", () => {
  it("stops every agent first, then interrupts, while a subagent runs", () => {
    expect(
      interruptPresses({
        keys: CLAUDE_KEYS,
        hasRunningSubagents: true,
        isWorking: false
      })
    ).toEqual([STOP_ALL_PRESS, ESCAPE_PRESS]);
  });

  it("only interrupts a working session with no subagent", () => {
    expect(
      interruptPresses({
        keys: CLAUDE_KEYS,
        hasRunningSubagents: false,
        isWorking: true
      })
    ).toEqual([ESCAPE_PRESS]);
  });

  it("presses nothing into an idle session", () => {
    expect(
      interruptPresses({
        keys: CLAUDE_KEYS,
        hasRunningSubagents: false,
        isWorking: false
      })
    ).toEqual([]);
  });

  it("still interrupts an agent without a stop-all action", () => {
    expect(
      interruptPresses({
        keys: {
          ...CLAUDE_KEYS,
          stopAll: null
        },
        hasRunningSubagents: true,
        isWorking: true
      })
    ).toEqual([ESCAPE_PRESS]);
  });
});

describe("verifying the stop", () => {
  it("counts only writes made once the keys had time to land", () => {
    const subagents = [
      {
        id: "settled",
        writtenAt: 1500
      },
      {
        id: "still",
        writtenAt: 20_000
      }
    ];
    expect(
      stillWritingSubagents({
        subagents,
        interruptedAt: 1000
      }).map(subagent => subagent.id)
    ).toEqual(["still"]);
  });

  it("retries once, then hands a subagent still writing to the user", () => {
    expect(
      stopVerdict({
        stillWritingCount: 0,
        attempt: 1
      })
    ).toBe(StopVerdict.enum.stopped);
    expect(
      stopVerdict({
        stillWritingCount: 1,
        attempt: 1
      })
    ).toBe(StopVerdict.enum.retry);
    expect(
      stopVerdict({
        stillWritingCount: 1,
        attempt: MAXIMUM_STOP_ATTEMPTS
      })
    ).toBe(StopVerdict.enum.unstoppable);
  });
});
