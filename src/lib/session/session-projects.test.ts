import { frontProject, sessionsOfProject, sessionToFocus } from "@/lib/session/session-projects";
import { describe, expect, it } from "vitest";

const PADE = "C:\\repositories\\pade";
const POLL = "C:\\repositories\\poll";

const SESSIONS = [
  {
    id: "a",
    project: PADE
  },
  {
    id: "b",
    project: POLL
  },
  {
    id: "c",
    project: "C:/repositories/pade"
  }
];

describe("frontProject", () => {
  it("follows the active session's project", () => {
    expect(
      frontProject({
        activeSession: {
          project: POLL
        },
        fallbackProject: PADE
      })
    ).toBe(POLL);
  });

  it("falls back to the entered project while no session is in front", () => {
    expect(
      frontProject({
        activeSession: undefined,
        fallbackProject: PADE
      })
    ).toBe(PADE);
  });
});

describe("sessionsOfProject", () => {
  it("keeps one project's sessions in window order, whatever the path spelling", () => {
    expect(
      sessionsOfProject({
        sessions: SESSIONS,
        project: PADE
      }).map(session => session.id)
    ).toEqual(["a", "c"]);
  });
});

describe("sessionToFocus", () => {
  it("brings back the session the project last had in front while it is live", () => {
    expect(
      sessionToFocus({
        sessions: SESSIONS,
        project: PADE,
        lastActiveId: "a"
      })
    ).toBe("a");
  });

  it("falls back to the project's newest session", () => {
    expect(
      sessionToFocus({
        sessions: SESSIONS,
        project: PADE,
        lastActiveId: "b"
      })
    ).toBe("c");
  });

  it("has nothing for a project with no session here", () => {
    expect(
      sessionToFocus({
        sessions: SESSIONS,
        project: "C:\\elsewhere",
        lastActiveId: undefined
      })
    ).toBeUndefined();
  });
});
