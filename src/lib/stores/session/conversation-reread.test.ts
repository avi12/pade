import { projectsToReread } from "@/lib/stores/session/conversation-reread";
import { describe, expect, it } from "vitest";

const LISTED = ["C:\\repositories\\pade", "C:/repositories/other"];

describe("projectsToReread", () => {
  it("re-reads every listed project for a change that names none", () => {
    expect(
      projectsToReread({
        listed: LISTED,
        changedProject: null
      })
    ).toEqual(LISTED);
  });

  it("re-reads only the listed project a change names, however it is spelled", () => {
    expect(
      projectsToReread({
        listed: LISTED,
        changedProject: "c:/Repositories/pade/"
      })
    ).toEqual([
      "C:\\repositories\\pade"
    ]);
  });

  it("re-reads nothing for a project no longer listed", () => {
    expect(
      projectsToReread({
        listed: LISTED,
        changedProject: "C:/elsewhere"
      })
    ).toEqual([]);
  });
});
