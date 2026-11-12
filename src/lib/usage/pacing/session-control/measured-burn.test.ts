import { Command } from "@/lib/ipc/messaging";
import type { AgentSession, ModelTokenUsage, SessionTokenUsage } from "@/lib/types/agent";
import { measuredBurn, splitBurn } from "@/lib/usage/pacing/session-control/measured-burn";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const NOTHING: SessionTokenUsage = {
  own: [],
  subagents: []
};

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<() => Promise<SessionTokenUsage>>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

const NOW = Date.UTC(2026, 9, 30, 12);
const MINUTE_MS = 60_000;
const WINDOW_MINUTES = 20;

const SESSION = {
  id: "session-1",
  agent: {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  },
  project: "C:\\repositories\\pade",
  conversationId: "conversation-1"
} satisfies AgentSession;

function haikuInput(inputTokens: number): ModelTokenUsage[] {
  return [{
    model: "claude-haiku-5-5",
    inputTokens,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheWriteFiveMinuteTokens: 0,
    cacheWriteOneHourTokens: 0
  }];
}

describe("measuredBurn", () => {
  beforeEach(() => {
    mocks.sendMessage.mockReset();
  });

  it("asks for the session's tokens over the window and prices them per minute", async () => {
    mocks.sendMessage.mockResolvedValue({
      own: haikuInput(2000),
      subagents: []
    });
    expect(
      (await measuredBurn({
        session: SESSION,
        now: NOW
      }))?.total
    ).toBe(2000 / WINDOW_MINUTES);
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.sessionTokenUsage, {
      command: "claude",
      conversationId: "conversation-1",
      since: NOW - WINDOW_MINUTES * MINUTE_MS
    });
  });

  it("measures nothing when the agent logged nothing in the window", async () => {
    mocks.sendMessage.mockResolvedValue(NOTHING);
    expect(
      await measuredBurn({
        session: SESSION,
        now: NOW
      })
    ).toBeNull();
  });

  it("measures nothing, without throwing, when the read fails", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    mocks.sendMessage.mockRejectedValue(new Error("backend gone"));
    expect(
      await measuredBurn({
        session: SESSION,
        now: NOW
      })
    ).toBeNull();
  });
});

describe("splitBurn", () => {
  it("keeps each running subagent apart and a finished one in the session's own share", () => {
    const burn = splitBurn({
      usage: {
        own: haikuInput(200),
        subagents: [{
          id: "running",
          description: "Explore",
          writtenAt: NOW - MINUTE_MS,
          usage: haikuInput(400)
        }, {
          id: "finished",
          description: null,
          writtenAt: NOW - 10 * MINUTE_MS,
          usage: haikuInput(600)
        }]
      },
      now: NOW
    });
    expect(burn).toEqual({
      total: 1200 / WINDOW_MINUTES,
      own: 800 / WINDOW_MINUTES,
      subagents: [{
        id: "running",
        description: "Explore",
        load: 400 / WINDOW_MINUTES
      }]
    });
  });
});
