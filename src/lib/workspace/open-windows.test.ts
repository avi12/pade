import { isOpenInAnotherWindow } from "@/lib/workspace/open-windows";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\avi\\poll-avi";

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
