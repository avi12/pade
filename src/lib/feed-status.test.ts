import { watchSummary } from "@/lib/feed-status";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\pade";
const NOW = 1_760_000_000_000;

describe("watchSummary", () => {
  it("reads as still checking before the first status arrives", () => {
    const summary = watchSummary({
      status: null,
      project: PROJECT,
      now: NOW
    });
    expect(summary.stalled).toBe(false);
    expect(summary.detail).toContain("Checking");
  });

  it("says so — and stalls — when no watch is armed", () => {
    const summary = watchSummary({
      status: {
        root: null,
        armedAt: null,
        surfaced: 0,
        ignored: 0
      },
      project: PROJECT,
      now: NOW
    });
    expect(summary.stalled).toBe(true);
    expect(summary.headline).toBe("Not watching this project.");
  });

  it("names the other project when the watch is armed elsewhere", () => {
    const summary = watchSummary({
      status: {
        root: "C:\\repositories\\avi\\taki",
        armedAt: NOW - 60_000,
        surfaced: 4,
        ignored: 0
      },
      project: PROJECT,
      now: NOW
    });
    expect(summary.stalled).toBe(true);
    expect(summary.detail).toContain("taki");
    expect(summary.detail).toContain("pade");
  });

  it("treats a differently-spelled same path as this project", () => {
    const summary = watchSummary({
      status: {
        root: "c:/repositories/avi/pade/",
        armedAt: NOW - 60_000,
        surfaced: 0,
        ignored: 0
      },
      project: PROJECT,
      now: NOW
    });
    expect(summary.stalled).toBe(false);
    expect(summary.headline).toBe("No changes yet.");
  });

  it("states how long it has been watching a quiet project", () => {
    const summary = watchSummary({
      status: {
        root: PROJECT,
        armedAt: NOW - 12 * 60_000,
        surfaced: 0,
        ignored: 0
      },
      project: PROJECT,
      now: NOW
    });
    expect(summary.detail).toContain("for 12m");
    expect(summary.detail).not.toContain("hidden");
  });

  it("surfaces changes the ignore rules swallowed", () => {
    const summary = watchSummary({
      status: {
        root: PROJECT,
        armedAt: NOW - 5000,
        surfaced: 0,
        ignored: 1284
      },
      project: PROJECT,
      now: NOW
    });
    expect(summary.detail).toContain("1,284 hidden");
  });
});
