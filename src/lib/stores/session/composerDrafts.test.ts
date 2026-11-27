import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { isComposing, noteComposerInput } from "@/lib/stores/session/composerDrafts.svelte";
import { ENTER } from "@/lib/terminal/input/terminal-sequences";

const SESSION = "session-composing";
const SECOND_MS = 1_000;
const MINUTE_MS = 60 * SECOND_MS;

describe("isComposing", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("holds PADE's prompt back while the user types, and over a draft they leave", () => {
    noteComposerInput({
      id: SESSION,
      data: "you also need"
    });
    expect(isComposing(SESSION)).toBe(true);
    vi.advanceTimersByTime(MINUTE_MS);
    expect(isComposing(SESSION)).toBe(true);
  });

  it("frees the composer once the user sends what they wrote and pauses", () => {
    noteComposerInput({
      id: SESSION,
      data: "fix it"
    });
    noteComposerInput({
      id: SESSION,
      data: ENTER
    });
    expect(isComposing(SESSION)).toBe(true);
    vi.advanceTimersByTime(6 * SECOND_MS);
    expect(isComposing(SESSION)).toBe(false);
  });

  it("trusts a draft untouched for five minutes no more", () => {
    noteComposerInput({
      id: SESSION,
      data: "half a thought"
    });
    vi.advanceTimersByTime(5 * MINUTE_MS);
    expect(isComposing(SESSION)).toBe(false);
  });

  it("never holds back a session the user hasn't typed in", () => {
    expect(isComposing("untouched")).toBe(false);
  });
});
