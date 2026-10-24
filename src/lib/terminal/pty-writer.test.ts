import { createPtyWriter } from "@/lib/terminal/pty-writer";
import { describe, expect, it, vi } from "vitest";

function deferred() {
  return Promise.withResolvers<void>();
}

describe("createPtyWriter", () => {
  it("sends the first write at once and merges the burst behind it in order", async () => {
    const first = deferred();
    const sent: string[] = [];
    const send = vi.fn(async (data: string) => {
      sent.push(data);

      if (sent.length === 1) {
        await first.promise;
      }
    });
    const write = createPtyWriter({
      send,
      onTransportLost() {}
    });

    const writes = [write("a"), write("b"), write("c")];
    expect(sent).toEqual(["a"]);

    first.resolve();
    await Promise.all(writes);
    expect(sent).toEqual(["a", "bc"]);
  });

  it("settles every waiter and reports the lost transport when a send fails", async () => {
    const first = deferred();
    const onTransportLost = vi.fn();
    const send = vi.fn(async () => {
      await first.promise;
    });
    const write = createPtyWriter({
      send,
      onTransportLost
    });

    const writes = [write("a"), write("b")];
    first.reject(new Error("gone"));
    await Promise.all(writes);
    expect(onTransportLost).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledTimes(1);
  });
});
