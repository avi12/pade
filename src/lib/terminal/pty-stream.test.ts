import { createPtyStream } from "@/lib/terminal/pty-stream";
import type { PtyChunk } from "@/lib/types";
import { describe, expect, it } from "vitest";

function chunk(seq: number) {
  return {
    id: "session",
    data: `chunk ${seq} · שלום`,
    seq
  } satisfies PtyChunk;
}

describe("createPtyStream", () => {
  it("parks live chunks until the replay, then writes only the newer ones in order", () => {
    const consumed: number[] = [];
    const stream = createPtyStream(({ seq }) => consumed.push(seq));

    stream.accept(chunk(4));
    stream.accept(chunk(5));
    stream.accept(chunk(6));
    expect(consumed).toEqual([]);

    stream.goLive(4);
    expect(consumed).toEqual([5, 6]);
  });

  it("writes live chunks straight through once replayed", () => {
    const consumed: string[] = [];
    const stream = createPtyStream(({ data }) => consumed.push(data));

    stream.goLive(0);
    stream.accept(chunk(1));
    stream.accept(chunk(2));
    expect(consumed).toEqual([chunk(1).data, chunk(2).data]);
  });

  it("never writes a parked chunk twice", () => {
    const consumed: number[] = [];
    const stream = createPtyStream(({ seq }) => consumed.push(seq));

    stream.accept(chunk(1));
    stream.goLive(0);
    stream.accept(chunk(2));
    expect(consumed).toEqual([1, 2]);
  });
});
