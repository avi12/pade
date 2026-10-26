import {
  eventsForLiveWatch,
  type FeedSnapshot,
  readFeedSnapshot,
  saveFeedSnapshot
} from "@/lib/feed/watch/feed-snapshot";
import { ChangeKind } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

// Minimal localStorage and Tauri-window doubles - vitest runs in node, which
// has neither. The label is what the snapshot key is scoped to.
const backing = new Map<string, string>();
function stubWindowLabel(label: string) {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {
      metadata: {
        currentWindow: {
          label
        }
      }
    }
  });
}

beforeEach(() => {
  backing.clear();
  stubWindowLabel("main");
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => backing.get(key) ?? null,
    setItem(key: string, value: string) {
      backing.set(key, value);
    }
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const PROJECT = "C:\\repositories\\avi\\demo";
const BEHIND = "C:\\repositories\\avi\\other";
const ARMED_AT = 1_791_004_514_273;
const BEHIND_ARMED_AT = ARMED_AT + 5_000;

function changeIn(root: string) {
  return {
    id: `${root}-1`,
    root,
    path: `${root}\\src\\main.ts`,
    kind: ChangeKind.enum.modified,
    added: 3,
    removed: 1,
    summary: "Edited",
    ts: ARMED_AT + 1_000
  };
}

const SNAPSHOT: FeedSnapshot = {
  feeds: [
    {
      root: PROJECT,
      armedAt: ARMED_AT,
      events: [changeIn(PROJECT)]
    },
    {
      root: BEHIND,
      armedAt: BEHIND_ARMED_AT,
      events: [changeIn(BEHIND)]
    }
  ]
};

describe("eventsForLiveWatch", () => {
  it("restores each project's events into its same watch, still live after a reload", () => {
    expect(
      eventsForLiveWatch({
        snapshot: SNAPSHOT,
        root: PROJECT,
        watch: {
          armedAt: ARMED_AT
        }
      })
    ).toEqual([changeIn(PROJECT)]);
    expect(
      eventsForLiveWatch({
        snapshot: SNAPSHOT,
        root: BEHIND,
        watch: {
          armedAt: BEHIND_ARMED_AT
        }
      })
    ).toEqual([changeIn(BEHIND)]);
  });

  it("drops them for a watch armed afresh (a later app run, a project held again)", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      root: PROJECT,
      watch: {
        armedAt: ARMED_AT + 60_000
      }
    });
    expect(events).toEqual([]);
  });

  it("never hands one project another project's events", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      root: "C:\\repositories\\avi\\third",
      watch: {
        armedAt: ARMED_AT
      }
    });
    expect(events).toEqual([]);
  });

  it("drops them when the project is not watched", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      root: PROJECT,
      watch: {
        armedAt: null
      }
    });
    expect(events).toEqual([]);
  });
});

describe("feed snapshot storage", () => {
  it("round-trips through this window's key", () => {
    saveFeedSnapshot(SNAPSHOT);
    expect(readFeedSnapshot()).toEqual(SNAPSHOT);
  });

  it("is never read by a sibling window", () => {
    saveFeedSnapshot(SNAPSHOT);
    stubWindowLabel("other");
    expect(readFeedSnapshot()).toBeNull();
  });

  it("rejects a malformed payload", () => {
    backing.set("pade.change-feed:main", "{ not json");
    expect(readFeedSnapshot()).toBeNull();
  });
});
