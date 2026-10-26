import { EventType } from "@/lib/ipc/messaging";
import { feedStore, keepFeedsOf, retargetFeed } from "@/lib/stores/workspace/feed.svelte";
import { ChangeKind } from "@/lib/types";
import type { ChangeEvent } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

const FRONT = "C:/repositories/avi/pade";
const BEHIND = "C:/repositories/avi/youtube-time-manager";

const handlers = vi.hoisted(() => new Map<string, (message: { data: unknown }) => void>());

vi.stubGlobal("addEventListener", vi.fn());
vi.stubGlobal("removeEventListener", vi.fn());

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async () => {
      throw new Error("no backend in tests");
    }),
    onMessage: vi.fn(async (event: string, handler: (message: { data: unknown }) => void) => {
      handlers.set(event, handler);
      return () => handlers.delete(event);
    })
  };
});

function change(project: string) {
  return {
    id: `${project}-change`,
    path: `${project}/src/main.ts`,
    kind: ChangeKind.enum.modified,
    added: 1,
    removed: 0,
    summary: "",
    ts: 0
  } satisfies ChangeEvent;
}

async function landChange(project: string) {
  await vi.waitFor(() => expect(handlers.has(EventType.feedChange)).toBe(true));
  handlers.get(EventType.feedChange)?.({ data: change(project) });
}

describe("feeds behind", () => {
  it("keeps a held project's feed and drops one the window no longer holds", async () => {
    retargetFeed(BEHIND);
    await landChange(BEHIND);
    retargetFeed(FRONT);

    keepFeedsOf([FRONT, BEHIND]);
    retargetFeed(BEHIND);
    expect(feedStore.events.map(event => event.id)).toEqual([`${BEHIND}-change`]);

    retargetFeed(FRONT);
    keepFeedsOf([FRONT]);
    retargetFeed(BEHIND);
    expect(feedStore.events).toEqual([]);
  });
});
