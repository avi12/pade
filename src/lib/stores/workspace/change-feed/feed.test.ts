import { beforeEach, describe, expect, it, vi } from "vitest";

import { Command, EventType } from "@/lib/ipc/messaging";
import type * as messagingModule from "@/lib/ipc/messaging";
import { setFrontProjectSource } from "@/lib/stores/workspace/active-project/frontProject.svelte";
import { feedStore, keepFeedsOf } from "@/lib/stores/workspace/change-feed/feed.svelte";
import { ChangeKind } from "@/lib/types/feed";
import type { ChangeEvent } from "@/lib/types/feed";

const FRONT = "C:/repositories/avi/pade";
const BEHIND = "C:/repositories/avi/youtube-time-manager";

const handlers = vi.hoisted(() => new Map<string, (message: { data: unknown }) => unknown>());
const ignoredPaths = vi.hoisted(() => {
  const value: string[] = [];
  return { value };
});

vi.stubGlobal("addEventListener", vi.fn());
vi.stubGlobal("removeEventListener", vi.fn());

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof messagingModule>();
  return {
    ...messaging,
    sendMessage: vi.fn(async (command: string) => {
      if (command === messaging.Command.FEED_IGNORED) {
        return ignoredPaths.value;
      }

      throw new Error("no backend in tests");
    }),
    onMessage: vi.fn(async (event: string, handler: (message: { data: unknown }) => unknown) => {
      handlers.set(event, handler);
      return () => handlers.delete(event);
    })
  };
});

let front = FRONT;

function change({ root, name }: { root: string; name: string }) {
  return {
    id: `${root}-${name}`,
    root,
    path: `${root}/src/${name}`,
    kind: ChangeKind.enum.modified,
    added: 1,
    removed: 0,
    summary: "",
    ts: 0
  } satisfies ChangeEvent;
}

async function land(event: ChangeEvent) {
  await vi.waitFor(() => expect(handlers.has(EventType.FEED_CHANGE)).toBe(true));
  handlers.get(EventType.FEED_CHANGE)?.({ data: event });
}

function shownIds() {
  return feedStore.events.map(event => event.id);
}

beforeEach(() => {
  front = FRONT;
  setFrontProjectSource(() => front);
  keepFeedsOf([]);
});

describe("feed per project", () => {
  it("files each change under the project its event names and shows the front one's", async () => {
    keepFeedsOf([FRONT, BEHIND]);
    await land(
      change({
        root: BEHIND,
        name: "behind.ts"
      })
    );
    await land(
      change({
        root: FRONT,
        name: "front.ts"
      })
    );

    expect(shownIds()).toEqual([`${FRONT}-front.ts`]);

    front = BEHIND;
    expect(shownIds()).toEqual([`${BEHIND}-behind.ts`]);
  });

  it("ignores a change for a project the window does not hold, and editor scratch", async () => {
    keepFeedsOf([FRONT]);
    await land(
      change({
        root: BEHIND,
        name: "stray.ts"
      })
    );
    await land(
      change({
        root: FRONT,
        name: "main.ts.tmp.123"
      })
    );

    expect(shownIds()).toEqual([]);
  });

  it("drops the feed of a project the window no longer holds", async () => {
    keepFeedsOf([FRONT, BEHIND]);
    await land(
      change({
        root: BEHIND,
        name: "behind.ts"
      })
    );

    keepFeedsOf([FRONT]);
    keepFeedsOf([FRONT, BEHIND]);
    front = BEHIND;

    expect(shownIds()).toEqual([]);
  });

  it("re-filters only the project whose ignore rules changed", async () => {
    keepFeedsOf([FRONT, BEHIND]);
    const frontChange = change({
      root: FRONT,
      name: "generated.ts"
    });
    const behindChange = change({
      root: BEHIND,
      name: "generated.ts"
    });
    await land(frontChange);
    await land(behindChange);
    ignoredPaths.value = [frontChange.path, behindChange.path];

    await handlers.get(EventType.FEED_IGNORE_CHANGED)?.({
      data: {
        root: BEHIND
      }
    });

    expect(shownIds()).toEqual([frontChange.id]);
    front = BEHIND;
    expect(shownIds()).toEqual([]);
    ignoredPaths.value = [];
  });

  it("asks the watch status of each newly held project by its own root", async () => {
    const { sendMessage } = await import("@/lib/ipc/messaging");
    keepFeedsOf([FRONT, BEHIND]);

    expect(sendMessage).toHaveBeenCalledWith(Command.WATCH_STATUS, { root: FRONT });
    expect(sendMessage).toHaveBeenCalledWith(Command.WATCH_STATUS, { root: BEHIND });
  });
});
