import { createPtyWriter } from "@/lib/terminal/pty-writer";
import { describe, expect, it, vi } from "vitest";

function deferred() {
  let resolve: () => void = () => {};
  let reject: (error: Error) => void = () => {};
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return {
    promise,
    resolve,
    reject
  };
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
      onTransportLost: () => {}
    });

    const a = write("a");
    const b = write("b");
    const c = write("c");
    expect(sent).toEqual(["a"]);

    first.resolve();
    await Promise.all([a, b, c]);
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

    const a = write("a");
    const b = write("b");
    first.reject(new Error("gone"));
    await Promise.all([a, b]);
    expect(onTransportLost).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledTimes(1);
  });
});
