import { popularRoot, watchedDirectories } from "@/panels/picker/project-roots";
import { describe, expect, it } from "vitest";

const REPOSITORIES = "C:\\repositories";
const WORK = "D:\\work";

describe("popularRoot", () => {
  it("is empty with no roots", () => {
    expect(
      popularRoot({
        roots: [],
        recentProjects: []
      })
    ).toBe("");
  });

  it("picks a lone root outright", () => {
    expect(
      popularRoot({
        roots: [WORK],
        recentProjects: [`${REPOSITORIES}\\app`]
      })
    ).toBe(WORK);
  });

  it("picks the root most recent projects live in, ignoring case", () => {
    expect(
      popularRoot({
        roots: [REPOSITORIES, WORK],
        recentProjects: [`${REPOSITORIES}\\app`, "d:\\WORK\\api", "D:\\work\\web"]
      })
    ).toBe(WORK);
  });

  it("keeps the earlier root on a tie", () => {
    expect(
      popularRoot({
        roots: [REPOSITORIES, WORK],
        recentProjects: [`${WORK}\\api`, `${REPOSITORIES}\\app`]
      })
    ).toBe(REPOSITORIES);
  });
});

describe("watchedDirectories", () => {
  it("watches every root, its parent and each recent project's parent, once", () => {
    expect(
      watchedDirectories({
        roots: [`${REPOSITORIES}\\avi`],
        recentProjects: [`${REPOSITORIES}\\avi\\pade`, `${REPOSITORIES}\\avi\\poll`]
      })
    ).toEqual([`${REPOSITORIES}\\avi`, REPOSITORIES]);
  });

  it("drops a path with no parent", () => {
    expect(
      watchedDirectories({
        roots: ["C:"],
        recentProjects: []
      })
    ).toEqual(["C:"]);
  });
});
