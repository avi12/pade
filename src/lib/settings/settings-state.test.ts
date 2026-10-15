import { createSerialQueue, createVersionedAdopter, replaceRecord } from "@/lib/settings/settings-state";
import { describe, expect, it } from "vitest";

describe("settings adoption", () => {
  it("replaces records so removed optional keys do not survive", () => {
    const current: Record<string, unknown> = {
      themeMode: "dark",
      monoFont: "Fira Code"
    };

    replaceRecord({
      target: current,
      fresh: {
        themeMode: "light"
      }
    });

    expect(current).toEqual({ themeMode: "light" });
  });

  it("rejects a response older than the latest adopted response", () => {
    let current = "initial";
    const versions = createVersionedAdopter<string>({
      adopt: fresh => (current = fresh)
    });
    const older = versions.begin();
    const newer = versions.begin();

    expect(
      versions.adopt({
        fresh: "newer",
        version: newer
      })
    ).toBe(true);
    expect(
      versions.adopt({
        fresh: "older",
        version: older
      })
    ).toBe(false);
    expect(current).toBe("newer");
  });

  it("starts settings requests in invocation order", async () => {
    const enqueue = createSerialQueue();
    const events: string[] = [];
    function noRelease() {}
    let releaseFirst: () => void = noRelease;
    const firstGate = new Promise<void>(resolve => (releaseFirst = resolve));
    const first = enqueue(async () => {
      events.push("first started");
      await firstGate;
      events.push("first finished");
    });
    const second = enqueue(async () => {
      events.push("second started");
    });
    await Promise.resolve();

    expect(events).toEqual(["first started"]);
    releaseFirst();
    await Promise.all([first, second]);
    expect(events).toEqual(["first started", "first finished", "second started"]);
  });

  it("keeps the queue moving past a failed request", async () => {
    const enqueue = createSerialQueue();
    const failed = enqueue(async () => {
      throw new Error("refused");
    });
    const next = enqueue(async () => "ran");

    await expect(failed).rejects.toThrow("refused");
    await expect(next).resolves.toBe("ran");
  });
});
