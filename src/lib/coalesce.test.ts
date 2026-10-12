import { coalesced } from "@/lib/coalesce";
import { describe, expect, it } from "vitest";

const TRANSIENT_ERROR = "transient";

describe("coalesced", () => {
  it("runs once for a single call", async () => {
    let runs = 0;
    const refresh = coalesced(async () => {
      runs += 1;
    });

    await refresh();

    expect(runs).toBe(1);
  });

  it("answers calls made mid-run with exactly one more run", async () => {
    let runs = 0;
    const gate = Promise.withResolvers<void>();
    const refresh = coalesced(async () => {
      runs += 1;

      if (runs === 1) {
        await gate.promise;
      }
    });

    const first = refresh();
    await refresh();
    await refresh();
    gate.resolve();
    await first;

    expect(runs).toBe(2);
  });

  it("recovers after a failing run", async () => {
    let runs = 0;
    const refresh = coalesced(async () => {
      runs += 1;

      if (runs === 1) {
        throw new Error(TRANSIENT_ERROR);
      }
    });

    await expect(refresh()).rejects.toThrow(TRANSIENT_ERROR);
    await refresh();

    expect(runs).toBe(2);
  });
});
