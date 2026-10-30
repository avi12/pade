import { Command } from "@/lib/ipc/messaging";
import type { AgentSession, ModelTokenUsage } from "@/lib/types";
import { measuredBurn } from "@/lib/usage/pacing/measured-burn";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<() => Promise<ModelTokenUsage[]>>(async () => [])
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

const NOW = Date.UTC(2026, 9, 30, 12);
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

describe("measuredBurn", () => {
  beforeEach(() => {
    mocks.sendMessage.mockReset();
  });

  it("asks for the session's tokens over the window and prices them per minute", async () => {
    mocks.sendMessage.mockResolvedValue([
      {
        model: "claude-haiku-5-5",
        inputTokens: 2000,
        outputTokens: 0,
        cacheReadTokens: 0,
        cacheWriteFiveMinuteTokens: 0,
        cacheWriteOneHourTokens: 0
      }
    ]);
    expect(
      await measuredBurn({
        session: SESSION,
        now: NOW
      })
    ).toBe(2000 / WINDOW_MINUTES);
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.sessionTokenUsage, {
      command: "claude",
      conversationId: "conversation-1",
      since: NOW - WINDOW_MINUTES * 60_000
    });
  });

  it("measures nothing when the agent logged nothing in the window", async () => {
    mocks.sendMessage.mockResolvedValue([]);
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
