import {
  didFailToStart,
  EmptyWorkspaceOutcome,
  emptyWorkspaceOutcome,
  RESPAWN_MINIMUM_LIFETIME_MS
} from "@/lib/session-exit";
import { describe, expect, it } from "vitest";

describe("didFailToStart", () => {
  it("reads an exit within the minimum lifetime as a failed start", () => {
    expect(
      didFailToStart({
        launchedAt: 1_000,
        now: 1_000 + RESPAWN_MINIMUM_LIFETIME_MS
      })
    ).toBe(true);
  });

  it("reads a later exit as a deliberate quit", () => {
    expect(
      didFailToStart({
        launchedAt: 1_000,
        now: 1_001 + RESPAWN_MINIMUM_LIFETIME_MS
      })
    ).toBe(false);
  });
});

describe("emptyWorkspaceOutcome", () => {
  it("sends an unnamed temp workspace back to the picker, even after a failed start", () => {
    expect(
      emptyWorkspaceOutcome({
        isUnnamedTemp: true,
        isFailedStart: true
      })
    ).toBe(EmptyWorkspaceOutcome.picker);
  });

  it("respawns the agent in a real project", () => {
    expect(
      emptyWorkspaceOutcome({
        isUnnamedTemp: false,
        isFailedStart: false
      })
    ).toBe(EmptyWorkspaceOutcome.respawn);
  });

  it("offers the agent picker after a failed start", () => {
    expect(
      emptyWorkspaceOutcome({
        isUnnamedTemp: false,
        isFailedStart: true
      })
    ).toBe(EmptyWorkspaceOutcome.onboarding);
  });
});
