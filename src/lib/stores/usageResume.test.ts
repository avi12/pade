import {
  assessLimit,
  isUsageLimitStop,
  LimitVerdict,
  LimitWindow,
  nextOccurrence,
  parseLimitHit,
  parseResetClock,
  showsTurnRunning
} from "@/lib/stores/usageResume.svelte";
import { CreditsState, UsageWindowKind } from "@/lib/types";
import type { AccountUsage } from "@/lib/types";
import { describe, expect, it } from "vitest";

// A fixed "now": 2026-07-17 10:00 local time.
const NOW = new Date(2026, 6, 17, 10, 0, 0, 0).getTime();

// A fullscreen TUI's "move the cursor one column right" — what it paints between
// words instead of a space.
const ESCAPE = "\u001b";
const CURSOR_FORWARD = `${ESCAPE}[1C`;

describe("parseLimitHit", () => {
  it("matches the CLI's session stop message", () => {
    const hit = parseLimitHit({
      text: "5-hour limit reached ∙ resets 3pm",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
    expect(hit?.resetAt).toBe(new Date(2026, 6, 17, 15, 0, 0, 0).getTime());
  });

  it("matches Claude Code's current session stop, with its reset clock", () => {
    const hit = parseLimitHit({
      text: "⎿  You've hit your session limit · resets 5:30pm (Asia/Jerusalem)",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
    expect(hit?.resetAt).toBe(new Date(2026, 6, 17, 17, 30, 0, 0).getTime());
  });

  it("matches the stop when a fullscreen TUI paints cursor moves instead of spaces", () => {
    const text = ["You've", "hit", "your", "session", "limit", "·", "resets", "5:30pm"].join(CURSOR_FORWARD);
    const hit = parseLimitHit({
      text,
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
    expect(hit?.resetAt).toBe(new Date(2026, 6, 17, 17, 30, 0, 0).getTime());
  });

  it("matches the footer notice and the failed-agent line", () => {
    expect(
      parseLimitHit({
        text: "⚠ Usage limit reached · continuing automatically at 5:30pm · esc to cancel",
        now: NOW
      })?.window
    ).toBe(LimitWindow.session);
    expect(
      parseLimitHit({
        text: "Agent terminated early due to an API error (error type rate_limit, HTTP 429)",
        now: NOW
      })
    ).not.toBeNull();
  });

  it("reads the weekly window only from the stop phrase itself", () => {
    expect(
      parseLimitHit({
        text: "You've hit your weekly limit · resets Sep 19",
        now: NOW
      })?.window
    ).toBe(LimitWindow.weekly);
    // A frame mentioning the weekly limit elsewhere is still a session stop.
    expect(
      parseLimitHit({
        text: "You've hit your session limit · /low-priority uses your weekly limit",
        now: NOW
      })?.window
    ).toBe(LimitWindow.session);
  });

  it("matches a spend-capped stop as the session window it names", () => {
    const hit = parseLimitHit({
      text: "You've hit your monthly spend limit · raise it at claude.ai/settings/usage · your session limit resets 5:30pm (Asia/Jerusalem)",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
    expect(hit?.resetAt).toBe(new Date(2026, 6, 17, 17, 30, 0, 0).getTime());
  });

  it.each([
    "You've hit your fast limit",
    "Your usage limit has reset · press enter to continue"
  ])("ignores %j", text => {
    expect(isUsageLimitStop(text)).toBe(false);
  });

  it("matches the older phrasing without a window name", () => {
    const hit = parseLimitHit({
      text: "Claude usage limit reached. Your limit will reset at 3pm.",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
  });

  it("reads a weekly stop as the weekly window", () => {
    const hit = parseLimitHit({
      text: "weekly limit reached ∙ resets Oct 14",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.weekly);
    expect(hit?.resetAt).toBeNull(); // a date, not a clock — API supplies it
  });

  it("ignores the softer approaching warning", () => {
    expect(
      parseLimitHit({
        text: "Approaching usage limit · resets at 3pm",
        now: NOW
      })
    ).toBeNull();
  });

  it("ignores ordinary output", () => {
    expect(
      parseLimitHit({
        text: "rate limit reached for the API endpoint",
        now: NOW
      })
    ).not.toBeNull(); // "limit reached" alone still counts…
    expect(
      parseLimitHit({
        text: "tokens remaining: 5%",
        now: NOW
      })
    ).toBeNull();
  });
});

describe("showsTurnRunning", () => {
  const transcript = Array.from({ length: 30 }, (_, index) => `line ${index}`).join("\n");

  it("reads a running turn from the agent's status rows", () => {
    const claude = `${transcript}\n❯ \n⏵⏵ bypass permissions on (shift+tab to cycle) · esc to interrupt · ← 1 agent\n○ grounding-finish  2/14 agents done · 8m 44s\n\n`;
    const codex = `${transcript}\n• Working (12s • Esc to interrupt)\n\n› \n`;
    expect(showsTurnRunning(claude)).toBe(true);
    expect(showsTurnRunning(codex)).toBe(true);
  });

  it("sees a stopped session as idle, even with a ticking workflow row", () => {
    const stuck = `${transcript}\n⚠ Usage limit reached · continuing automatically at 5:30pm · esc to cancel\n⏵⏵ bypass permissions on (shift+tab to cycle) · ← 1 agent\n○ grounding-finish  2/14 agents done · 8m 45s\n`;
    expect(showsTurnRunning(stuck)).toBe(false);
  });

  it("ignores the words quoted in the transcript above the status rows", () => {
    const quoted = `const hint = "esc to interrupt";\n${transcript}\n❯ \n⏵⏵ bypass permissions on\n`;
    expect(showsTurnRunning(quoted)).toBe(false);
  });
});

describe("assessLimit", () => {
  const RESET_AT = new Date(2026, 6, 17, 17, 30, 0, 0).getTime();

  function account({ sessionUtilization, credits }: {
    sessionUtilization: number;
    credits?: CreditsState;
  }): AccountUsage {
    return {
      windows: [
        {
          key: "five_hour",
          kind: UsageWindowKind.enum.session,
          label: "Five hour",
          utilization: sessionUtilization,
          resetsAt: new Date(RESET_AT).toISOString()
        }
      ],
      plan: "Claude max",
      source: "oauth:api.anthropic.com",
      credits
    };
  }

  const waiting = {
    window: LimitWindow.session,
    resetAt: RESET_AT,
    confirmed: true
  };
  const freshlySniffed = {
    ...waiting,
    confirmed: false
  };

  it("keeps a session with an exhausted window and no credits blocked", () => {
    expect(
      assessLimit({
        hit: freshlySniffed,
        account: account({
          sessionUtilization: 100,
          credits: CreditsState.enum.unavailable
        }),
        now: NOW
      })
    ).toBe(LimitVerdict.blocked);
  });

  it("frees a waiting session the moment credits are turned on", () => {
    expect(
      assessLimit({
        hit: waiting,
        account: account({
          sessionUtilization: 100,
          credits: CreditsState.enum.available
        }),
        now: NOW
      })
    ).toBe(LimitVerdict.runnable);
  });

  it("frees a waiting session once its window resets", () => {
    expect(
      assessLimit({
        hit: waiting,
        account: account({ sessionUtilization: 3 }),
        now: NOW
      })
    ).toBe(LimitVerdict.runnable);
  });

  it("calls a message the account never backed stale — a healthy window or credits already on", () => {
    expect(
      assessLimit({
        hit: freshlySniffed,
        account: account({ sessionUtilization: 40 }),
        now: NOW
      })
    ).toBe(LimitVerdict.stale);
    expect(
      assessLimit({
        hit: freshlySniffed,
        account: account({
          sessionUtilization: 100,
          credits: CreditsState.enum.available
        }),
        now: NOW
      })
    ).toBe(LimitVerdict.stale);
  });

  it("leaves an unreadable account to the reset clock", () => {
    const afterResetAndBuffer = RESET_AT + 5 * 60_000;
    expect(
      assessLimit({
        hit: waiting,
        account: null,
        now: NOW
      })
    ).toBe(LimitVerdict.unknown);
    expect(
      assessLimit({
        hit: waiting,
        account: null,
        now: afterResetAndBuffer
      })
    ).toBe(LimitVerdict.runnable);
    expect(
      assessLimit({
        hit: {
          ...waiting,
          resetAt: null
        },
        account: null,
        now: afterResetAndBuffer
      })
    ).toBe(LimitVerdict.unknown);
  });
});

describe("parseResetClock", () => {
  it("reads a bare hour", () => {
    expect(
      parseResetClock({
        text: "resets 3pm",
        now: NOW
      })
    ).toBe(new Date(2026, 6, 17, 15, 0, 0, 0).getTime());
  });

  it("reads minutes and the at phrasing", () => {
    expect(
      parseResetClock({
        text: "resets at 3:30am",
        now: NOW
      })
    ).toBe(new Date(2026, 6, 18, 3, 30, 0, 0).getTime()); // 3:30am is past — tomorrow
  });

  it("yields null when no clock is named", () => {
    expect(
      parseResetClock({
        text: "limit reached",
        now: NOW
      })
    ).toBeNull();
  });
});

describe("nextOccurrence", () => {
  it("stays today when the clock is ahead", () => {
    const occurrence = nextOccurrence({
      hour: 11,
      minute: 0,
      meridiem: "am",
      now: NOW
    });
    expect(occurrence).toBe(new Date(2026, 6, 17, 11, 0, 0, 0).getTime());
  });

  it("rolls to tomorrow when the clock has passed", () => {
    const occurrence = nextOccurrence({
      hour: 9,
      minute: 0,
      meridiem: "am",
      now: NOW
    });
    expect(occurrence).toBe(new Date(2026, 6, 18, 9, 0, 0, 0).getTime());
  });

  it("maps 12am to midnight and 12pm to noon", () => {
    const midnight = nextOccurrence({
      hour: 12,
      minute: 0,
      meridiem: "am",
      now: NOW
    });
    expect(new Date(midnight).getHours()).toBe(0);
    const noon = nextOccurrence({
      hour: 12,
      minute: 0,
      meridiem: "pm",
      now: NOW
    });
    expect(new Date(noon).getHours()).toBe(12);
  });
});
