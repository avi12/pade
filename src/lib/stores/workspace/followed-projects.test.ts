import { isFollowedProject, watchedProjectPaths } from "@/lib/stores/workspace/followed-projects";
import { describe, expect, it } from "vitest";

describe("watchedProjectPaths", () => {
  it("puts the followed projects first and the rows on screen after them", () => {
    expect(
      watchedProjectPaths({
        followed: ["C:\\work\\app"],
        visible: ["C:\\work\\site", "C:\\work\\tools"]
      })
    ).toEqual(["C:\\work\\app", "C:\\work\\site", "C:\\work\\tools"]);
  });

  it("watches a project once however each list spelled it", () => {
    expect(
      watchedProjectPaths({
        followed: ["C:\\Work\\App"],
        visible: ["c:/work/app/", "C:\\work\\site"]
      })
    ).toEqual(["C:\\Work\\App", "C:\\work\\site"]);
  });

  it("watches nothing when nothing is followed or on screen", () => {
    expect(
      watchedProjectPaths({
        followed: [],
        visible: []
      })
    ).toEqual([]);
  });
});

describe("isFollowedProject", () => {
  it("recognizes a followed project by any spelling of its path", () => {
    expect(
      isFollowedProject({
        followed: ["C:\\work\\app", "C:\\work\\api"],
        path: "c:/work/api"
      })
    ).toBe(true);
  });

  it("does not mistake a sibling for a followed project", () => {
    expect(
      isFollowedProject({
        followed: ["C:\\work\\app"],
        path: "C:\\work\\app-two"
      })
    ).toBe(false);
  });
});
