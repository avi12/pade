import { eventsForLiveWatch, type FeedSnapshot, readFeedSnapshot, saveFeedSnapshot } from "@/lib/feed-snapshot";
import { ChangeKind } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

// Minimal localStorage and Tauri-window doubles — vitest runs in node, which
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
const ARMED_AT = 1_791_004_514_273;

const SNAPSHOT: FeedSnapshot = {
  root: PROJECT,
  armedAt: ARMED_AT,
  events: [{
    id: "1",
    path: `${PROJECT}\\src\\main.ts`,
    kind: ChangeKind.enum.modified,
    added: 3,
    removed: 1,
    summary: "Edited",
    ts: ARMED_AT + 1_000
  }]
};

describe("eventsForLiveWatch", () => {
  it("restores the events into the same watch, still live after a reload", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      watch: {
        root: PROJECT,
        armedAt: ARMED_AT
      },
      project: PROJECT
    });
    expect(events).toEqual(SNAPSHOT.events);
  });

  it("drops them for a watch armed afresh (a later app run)", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      watch: {
        root: PROJECT,
        armedAt: ARMED_AT + 60_000
      },
      project: PROJECT
    });
    expect(events).toEqual([]);
  });

  it("drops them when the window opens another project on the old watch", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      watch: {
        root: PROJECT,
        armedAt: ARMED_AT
      },
      project: "C:\\repositories\\avi\\other"
    });
    expect(events).toEqual([]);
  });

  it("drops them when nothing is watched", () => {
    const events = eventsForLiveWatch({
      snapshot: SNAPSHOT,
      watch: {
        root: null,
        armedAt: null
      },
      project: PROJECT
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
