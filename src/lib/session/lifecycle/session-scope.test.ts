import {
  forgetSession,
  registerSessionState,
  rekeySession,
  restoreSessionState,
  savedSessionState,
  SessionStateKey
} from "@/lib/session/lifecycle/session-scope";
import { describe, expect, it } from "vitest";

// A stand-in store: a name per session, kept across restarts and reloads.
const names = new Map<string, string>();
registerSessionState(SessionStateKey.enum.label, {
  forget: id => names.delete(id),
  carry({ fromId, toId }) {
    const name = names.get(fromId);
    if (name !== undefined) {
      names.set(toId, name);
    }
  },
  save: id => names.get(id),
  restore({ id, saved }) {
    names.set(id, String(saved));
  }
});

// One that forgets only.
const statuses = new Map<string, string>();
registerSessionState(SessionStateKey.enum.status, {
  forget: id => statuses.delete(id)
});

describe("session scope", () => {
  it("forgets a session in every registered store", () => {
    names.set("a", "Fix the rail");
    statuses.set("a", "working");

    forgetSession("a");

    expect(names.has("a")).toBe(false);
    expect(statuses.has("a")).toBe(false);
  });

  it("carries what outlives a restart to the new id and forgets the old one", () => {
    names.set("a", "Fix the rail");
    statuses.set("a", "working");

    rekeySession({
      fromId: "a",
      toId: "b"
    });

    expect(names.get("b")).toBe("Fix the rail");
    expect(names.has("a")).toBe(false);
    expect(statuses.has("a")).toBe(false);
    expect(statuses.has("b")).toBe(false);
  });

  it("saves only what a store keeps, and restores it under the session", () => {
    names.set("a", "Fix the rail");
    statuses.set("a", "working");

    const saved = savedSessionState("a");
    expect(saved).toEqual({ [SessionStateKey.enum.label]: "Fix the rail" });

    forgetSession("a");
    restoreSessionState({
      id: "a",
      saved
    });
    expect(names.get("a")).toBe("Fix the rail");
    expect(statuses.has("a")).toBe(false);
  });
});
