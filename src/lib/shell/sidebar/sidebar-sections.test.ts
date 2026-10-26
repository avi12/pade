import type { SidebarProject } from "@/lib/session/chats/chat-registry";
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

describe("sidebarProjectOrder", () => {
  it("leads with the pins, then this window's project and the recents", () => {
    expect(
      sidebarProjectOrder({
        current: PADE,
        pinned: [POLL],
        recent: [PADE, POLL]
      })
    ).toEqual([POLL, PADE, PADE, POLL]);
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

  it("lists the unpinned projects with a running session first, each part in recent order", () => {
    const live = {
      id: "a",
      project: TEMP,
      label: "Claude Code",
      agentId: "claude",
      isLive: true
    };
    const sections = sidebarSections({
      groups: [
        group(POLL),
        group(PADE),
        {
          ...group(TEMP),
          chats: [live]
        }
      ],
      pinned: [],
      query: "",
      labels: {}
    });

    expect(sections.recent.map(entry => entry.project)).toEqual([TEMP, POLL, PADE]);
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
