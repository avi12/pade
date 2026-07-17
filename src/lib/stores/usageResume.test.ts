import { LimitWindow, nextOccurrence, parseLimitHit, parseResetClock } from "@/lib/stores/usageResume.svelte";
import { describe, expect, it } from "vitest";

// A fixed "now": 2026-07-17 10:00 local time.
const NOW = new Date(2026, 6, 17, 10, 0, 0, 0).getTime();

describe("parseLimitHit", () => {
  it("matches the CLI's session stop message", () => {
    const hit = parseLimitHit({
      text: "5-hour limit reached ∙ resets 3pm",
      now: NOW
    });
    expect(hit?.window).toBe(LimitWindow.session);
    expect(hit?.inlineResetAt).toBe(new Date(2026, 6, 17, 15, 0, 0, 0).getTime());
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
    expect(hit?.inlineResetAt).toBeNull(); // a date, not a clock — API supplies it
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
