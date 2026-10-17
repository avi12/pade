import {
  isCurrentProject,
  isSameMembership,
  matchesProjectFilter,
  rowMenuId,
  rowTransitionName,
  windowRowsInOrder,
  windowRowTransitionName
} from "@/lib/workspace/switcher/switcher-rows";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\poll-avi";

describe("row identifiers", () => {
  it("turns a path into a valid popover id", () => {
    expect(rowMenuId(PROJECT)).toBe("switcher-C--repositories-avi-poll-avi");
  });

  it("keeps project and window transition names in separate namespaces", () => {
    expect(rowTransitionName(PROJECT)).toBe("row-C--repositories-avi-poll-avi");
    expect(windowRowTransitionName("main")).toBe("window-row-main");
  });
});

describe("isCurrentProject", () => {
  it("matches the window's project whatever the path's spelling", () => {
    expect(
      isCurrentProject({
        project: "c:/repositories/avi/poll-avi/",
        current: PROJECT
      })
    ).toBe(true);
  });

  it("rejects another project", () => {
    expect(
      isCurrentProject({
        project: "C:\\repositories\\avi\\other",
        current: PROJECT
      })
    ).toBe(false);
  });
});

describe("matchesProjectFilter", () => {
  const labels = { [PROJECT]: "Poll Board" };

  it("keeps every project for an empty filter", () => {
    expect(
      matchesProjectFilter({
        project: PROJECT,
        query: "",
        labels
      })
    ).toBe(true);
  });

  it("matches the shown label case-insensitively", () => {
    expect(
      matchesProjectFilter({
        project: PROJECT,
        query: "BOARD",
        labels
      })
    ).toBe(true);
  });

  it("matches the path", () => {
    expect(
      matchesProjectFilter({
        project: PROJECT,
        query: "repositories",
        labels
      })
    ).toBe(true);
  });

  it("rejects a project neither name nor path contains", () => {
    expect(
      matchesProjectFilter({
        project: PROJECT,
        query: "terminal",
        labels
      })
    ).toBe(false);
  });
});

describe("windowRowsInOrder", () => {
  const windowRows = [
    {
      label: "main",
      path: PROJECT,
      isCurrent: true
    },
    {
      label: "second",
      path: "C:\\other",
      isCurrent: false
    }
  ];

  it("re-orders the rows by the committed labels", () => {
    expect(
      windowRowsInOrder({
        windowRows,
        order: ["second", "main"]
      }).map(row => row.label)
    ).toEqual(["second", "main"]);
  });

  it("drops a label whose window is gone", () => {
    expect(
      windowRowsInOrder({
        windowRows,
        order: ["closed", "main"]
      }).map(row => row.label)
    ).toEqual(["main"]);
  });
});

describe("isSameMembership", () => {
  const before = {
    pinned: ["C:/a"],
    recent: ["C:/b", "C:/c"]
  };

  it("ignores a reorder", () => {
    expect(
      isSameMembership({
        before,
        after: {
          pinned: ["C:/a"],
          recent: ["C:/c", "C:/b"]
        }
      })
    ).toBe(true);
  });

  it("sees a project leave", () => {
    expect(
      isSameMembership({
        before,
        after: {
          pinned: ["C:/a"],
          recent: ["C:/b"]
        }
      })
    ).toBe(false);
  });

  it("sees a project move between sections", () => {
    expect(
      isSameMembership({
        before,
        after: {
          pinned: ["C:/a", "C:/b"],
          recent: ["C:/c"]
        }
      })
    ).toBe(false);
  });
});
