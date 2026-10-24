import type { SidebarProject, StoredChatRegistry } from "@/lib/session/chat-registry";
import { firstListedProject, sidebarProjectOrder, sidebarSections } from "@/lib/shell/sidebar/sidebar-sections";
import { describe, expect, it } from "vitest";

const PADE = "C:\\repositories\\pade";
const POLL = "C:\\repositories\\poll";
const TEMP = "C:\\temp\\pade-workspace-1";

function group(project: string): SidebarProject {
  return {
    project,
    isCurrent: false,
    chats: []
  };
}

const EMPTY_REGISTRY: StoredChatRegistry = {
  chats: [],
  activeChatByProject: {},
  lastProject: ""
};

describe("sidebarProjectOrder", () => {
  it("leads with the pins, then this window's project, recents and chat holders", () => {
    const registry: StoredChatRegistry = {
      ...EMPTY_REGISTRY,
      chats: [
        {
          id: "a",
          project: TEMP,
          agent: {
            id: "claude",
            label: "Claude Code",
            command: "claude",
            reordersBidi: false
          },
          updatedAt: 1
        }
      ]
    };

    expect(
      sidebarProjectOrder({
        current: PADE,
        pinned: [POLL],
        recent: [PADE, POLL],
        registry
      })
    ).toEqual([POLL, PADE, PADE, POLL, TEMP]);
  });
});

describe("firstListedProject", () => {
  it("prefers a pin, falls back to the rest, and is empty with no rows", () => {
    expect(
      firstListedProject({
        pinned: [group(POLL)],
        recent: [group(PADE)]
      })
    ).toBe(POLL);
    expect(
      firstListedProject({
        pinned: [],
        recent: [group(PADE)]
      })
    ).toBe(PADE);
    expect(
      firstListedProject({
        pinned: [],
        recent: []
      })
    ).toBeUndefined();
  });
});

describe("sidebarSections", () => {
  const groups = [group(POLL), group(PADE), group(TEMP)];

  it("splits pinned projects from the rest, whatever the path's spelling", () => {
    const sections = sidebarSections({
      groups,
      pinned: ["c:/repositories/poll"],
      query: "",
      labels: {}
    });

    expect(sections.pinned.map(entry => entry.project)).toEqual([POLL]);
    expect(sections.recent.map(entry => entry.project)).toEqual([PADE, TEMP]);
  });

  it("keeps only the projects whose name or path matches the search", () => {
    const sections = sidebarSections({
      groups,
      pinned: [POLL],
      query: "  ",
      labels: {}
    });
    expect(sections.recent).toHaveLength(2);

    const byPath = sidebarSections({
      groups,
      pinned: [POLL],
      query: "temp",
      labels: {}
    });
    expect(byPath.pinned).toEqual([]);
    expect(byPath.recent.map(entry => entry.project)).toEqual([TEMP]);
  });

  it("matches a temp workspace by its label", () => {
    const sections = sidebarSections({
      groups,
      pinned: [],
      query: "death stranding",
      labels: {
        [TEMP]: "Death Stranding with DLSS 4.5"
      }
    });

    expect(sections.recent.map(entry => entry.project)).toEqual([TEMP]);
  });
});
