import { describe, expect, it } from "vitest";

import { cappedFeed } from "@/lib/feed/watch/feed-cap";
import { ChangeKind } from "@/lib/types/feed";
import type { ChangeEvent } from "@/lib/types/feed";

function change({ id, isIgnored }: { id: string; isIgnored: boolean }) {
  return {
    id,
    root: "C:/repo",
    path: `C:/repo/${id}`,
    kind: ChangeKind.enum.modified,
    added: 1,
    removed: 0,
    summary: "",
    ts: 0,
    isIgnored
  } satisfies ChangeEvent;
}

function many({ prefix, count, isIgnored }: { prefix: string; count: number; isIgnored: boolean }) {
  return Array.from({ length: count }, (_, i) =>
    change({
      id: `${prefix}${i}`,
      isIgnored
    })
  );
}

describe("cappedFeed", () => {
  it("never lets ignored churn push out the changes the feed shows", () => {
    const shown = many({
      prefix: "agent-",
      count: 5,
      isIgnored: false
    });
    const churn = many({
      prefix: "profile-",
      count: 1000,
      isIgnored: true
    });
    const capped = cappedFeed([...churn, ...shown]);
    expect(capped.filter(event => !event.isIgnored)).toEqual(shown);
    expect(capped.filter(event => event.isIgnored)).toHaveLength(300);
  });

  it("keeps the newest of each kind, in order", () => {
    const kept = many({
      prefix: "kept-",
      count: 400,
      isIgnored: false
    });
    const capped = cappedFeed(kept);
    expect(capped).toHaveLength(300);
    expect(capped[0].id).toBe("kept-0");
    expect(capped.at(-1)?.id).toBe("kept-299");
  });
});
