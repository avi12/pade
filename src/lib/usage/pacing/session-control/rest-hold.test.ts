import {
  canHoldRest,
  heldRestUntil,
  isHoldFallbackDue,
  MAXIMUM_REST_HOLD_MS
} from "@/lib/usage/pacing/session-control/rest-hold";
import { describe, expect, it } from "vitest";

const HOLDING_KEYS = { canHoldToolCalls: true };
const STOPPING_KEYS = { canHoldToolCalls: false };

describe("canHoldRest", () => {
  it("holds a rest the agent's hook can hold through", () => {
    expect(
      canHoldRest({
        keys: HOLDING_KEYS,
        restMs: MAXIMUM_REST_HOLD_MS
      })
    ).toBe(true);
  });

  it("stops a rest longer than the longest hold", () => {
    expect(
      canHoldRest({
        keys: HOLDING_KEYS,
        restMs: MAXIMUM_REST_HOLD_MS + 1
      })
    ).toBe(false);
  });

  it("stops when the agent can't hold or the session is only stopped", () => {
    expect(
      canHoldRest({
        keys: STOPPING_KEYS,
        restMs: 1000
      })
    ).toBe(false);
    expect(
      canHoldRest({
        keys: null,
        restMs: 1000
      })
    ).toBe(false);
    expect(
      canHoldRest({
        keys: HOLDING_KEYS,
        restMs: null
      })
    ).toBe(false);
  });
});

describe("heldRestUntil", () => {
  it("is the rest end of a held pause only", () => {
    expect(
      heldRestUntil({
        isHeld: true,
        restUntil: 5000
      })
    ).toBe(5000);
    expect(
      heldRestUntil({
        isHeld: false,
        restUntil: 5000
      })
    ).toBeNull();
    expect(heldRestUntil(undefined)).toBeNull();
  });
});

describe("isHoldFallbackDue", () => {
  it("falls back when a working session's hook never noted the rest", () => {
    expect(
      isHoldFallbackDue({
        isWorking: true,
        stopSeenAt: null,
        restWrittenAt: 1000
      })
    ).toBe(true);
    expect(
      isHoldFallbackDue({
        isWorking: true,
        stopSeenAt: 999,
        restWrittenAt: 1000
      })
    ).toBe(true);
  });

  it("stays held once the hook noted a call at or after the rest", () => {
    expect(
      isHoldFallbackDue({
        isWorking: true,
        stopSeenAt: 1000,
        restWrittenAt: 1000
      })
    ).toBe(false);
  });

  it("leaves an idle session alone", () => {
    expect(
      isHoldFallbackDue({
        isWorking: false,
        stopSeenAt: null,
        restWrittenAt: 1000
      })
    ).toBe(false);
  });
});
