import { afterEach, describe, expect, it, vi } from "vitest";

import { Command } from "@/lib/ipc/messaging";
import type * as messagingModule from "@/lib/ipc/messaging";
import {
  createApiErrorRetry,
  dismissApiError,
  observeApiError,
  parseApiError
} from "@/lib/stores/agent/apiErrorRetry.svelte";
import type { AgentSession } from "@/lib/types/agent";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<(command: Command, data?: unknown) => Promise<unknown>>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...(await importOriginal<typeof messagingModule>()),
  sendMessage: mocks.sendMessage
}));

/** Well past the first retry, so a nudge that was still scheduled would be sent. */
const PAST_FIRST_RETRY_MS = 60_000;

const stoppedSession: AgentSession = {
  id: "api-error-session",
  agent: {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  },
  cwd: "C:/repositories/pade",
  project: "C:/repositories/pade"
};

afterEach(() => vi.useRealTimers());

describe("createApiErrorRetry", () => {
  it("stops nudging once the user submits a prompt of their own", async () => {
    vi.useFakeTimers();
    const retry = createApiErrorRetry({
      sessions: () => [stoppedSession],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      forceHandoff() {}
    });
    observeApiError({
      id: stoppedSession.id,
      chunk: "API Error: 500"
    });
    retry.check();
    expect(retry.note).not.toBe("");

    dismissApiError(stoppedSession.id);
    retry.check();
    await vi.advanceTimersByTimeAsync(PAST_FIRST_RETRY_MS);

    expect(retry.note).toBe("");
    expect(mocks.sendMessage).not.toHaveBeenCalledWith(Command.PTY_WRITE, expect.anything());
    retry.dispose();
  });
});

describe("parseApiError", () => {
  describe("detects an API-side stop", () => {
    it.each([
      '⎿  API Error: 500 {"type":"error"}',
      '{"type":"overloaded_error","message":"Overloaded"}',
      "500 Internal Server Error",
      "502 Bad Gateway",
      "503 Service Unavailable",
      "HTTP 500",
      "Error 529",
      "status: 502",
      "Connection error.",
      "connection reset by peer",
      "read ECONNRESET",
      "network error occurred"
    ])("flags %j", text => {
      expect(parseApiError({ text })).toBe(true);
    });
  });

  it("is case-insensitive", () => {
    expect(parseApiError({ text: "api error" })).toBe(true);
    expect(parseApiError({ text: "API ERROR" })).toBe(true);
    expect(parseApiError({ text: "OVERLOADED_ERROR" })).toBe(true);
    expect(parseApiError({ text: "econnreset".toUpperCase() })).toBe(true);
  });

  // The usage-limit message is auto-resume's to own - this sniffer must never
  // claim it, or the two features would fight over the same stopped session.
  describe("never fires on a usage limit", () => {
    it.each([
      "5-hour limit reached ∙ resets 3pm",
      "Claude usage limit reached. Your limit will reset at 3pm.",
      "weekly limit reached ∙ resets Oct 14",
      "Approaching usage limit · resets at 3pm",
      // Claude Code words a limited request as an API error - still a usage limit.
      "Agent terminated early due to an API error: You've hit your session limit · resets 5:30pm (error type rate_limit, HTTP 429)",
      'API Error: 429 {"type":"error","error":{"type":"rate_limit_error"}}'
    ])("ignores %j", text => {
      expect(parseApiError({ text })).toBe(false);
    });
  });

  describe("never fires on ordinary output", () => {
    it.each([
      "Compiled 500 modules successfully",
      "Listening on port 5000",
      "All 502 tests passed",
      "the connection was established",
      "this method is overloaded",
      "network is reachable",
      "wrote 529 bytes to disk",
      "Reading file 3 of 500",
      "Fetching 502 rows from the table",
      "done"
    ])("ignores %j", text => {
      expect(parseApiError({ text })).toBe(false);
    });
  });
});
