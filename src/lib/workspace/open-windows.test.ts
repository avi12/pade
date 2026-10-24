import { isNewWindowClick, isOpenInAnotherWindow, otherWindowWith } from "@/lib/workspace/open-windows";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\poll-avi";

describe("isNewWindowClick", () => {
  const PLAIN = {
    ctrlKey: false,
    metaKey: false,
    shiftKey: false
  };

  it("opens in a new window with Ctrl, Cmd or Shift held, never on a plain click", () => {
    expect(isNewWindowClick(PLAIN)).toBe(false);
    expect(
      isNewWindowClick({
        ...PLAIN,
        ctrlKey: true
      })
    ).toBe(true);
    expect(
      isNewWindowClick({
        ...PLAIN,
        metaKey: true
      })
    ).toBe(true);
    expect(
      isNewWindowClick({
        ...PLAIN,
        shiftKey: true
      })
    ).toBe(true);
  });
});

describe("isOpenInAnotherWindow", () => {
  it("finds the project in another window, whatever the path's spelling", () => {
    expect(
      isOpenInAnotherWindow({
        windowRows: [{
          label: "main",
          path: "c:/repositories/avi/poll-avi/",
          isCurrent: false
        }],
        path: PROJECT
      })
    ).toBe(true);
  });

  it("ignores this window's own project", () => {
    expect(
      isOpenInAnotherWindow({
        windowRows: [{
          label: "main",
          path: PROJECT,
          isCurrent: true
        }],
        path: PROJECT
      })
    ).toBe(false);
  });

  it("ignores other projects", () => {
    expect(
      isOpenInAnotherWindow({
        windowRows: [{
          label: "second",
          path: "C:\\repositories\\avi\\pade",
          isCurrent: false
        }],
        path: PROJECT
      })
    ).toBe(false);
  });
});

describe("otherWindowWith", () => {
  it("names the window to focus for a project open elsewhere", () => {
    expect(
      otherWindowWith({
        windowRows: [{
          label: "main",
          path: "C:\\repositories\\avi\\pade",
          isCurrent: true
        }, {
          label: "second",
          path: "c:/repositories/avi/poll-avi",
          isCurrent: false
        }],
        path: PROJECT
      })?.label
    ).toBe("second");
  });
});
