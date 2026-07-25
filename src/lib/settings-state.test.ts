import { createSerialQueue, createVersionedAdopter, replaceRecord } from "@/lib/settings-state";
import { describe, expect, it } from "vitest";

describe("settings adoption", () => {
  it("replaces records so removed optional keys do not survive", () => {
    const current: Record<string, unknown> = {
      themeMode: "dark",
      monoFont: "Fira Code"
    };

    replaceRecord(current, { themeMode: "light" });

    expect(current).toEqual({ themeMode: "light" });
  });

  it("rejects a response older than the latest adopted response", () => {
    let current = "initial";
    const versions = createVersionedAdopter<string>({
      adopt: fresh => (current = fresh)
    });
    const older = versions.begin();
    const newer = versions.begin();

    expect(versions.adopt("newer", newer)).toBe(true);
    expect(versions.adopt("older", older)).toBe(false);
    expect(current).toBe("newer");
  });

  it("starts settings requests in invocation order", async () => {
    const enqueue = createSerialQueue();
    const events: string[] = [];
    function noRelease(): void {}
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
});
