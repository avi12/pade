import {
  forgetSession,
  rekeySession,
  restoreSessionState,
  savedSessionState
} from "@/lib/session/lifecycle/session-scope";
import { isNaming, setNaming } from "@/lib/stores/session/sessionNaming.svelte";
import { afterEach, describe, expect, it } from "vitest";

afterEach(() => {
  forgetSession("a");
  forgetSession("b");
});

describe("session naming", () => {
  it("keeps a hand-toggled ✦ across a reload or a hand-off", () => {
    setNaming({
      id: "a",
      agent: "claude",
      isOn: true
    });
    const saved = savedSessionState("a");
    forgetSession("a");
    expect(isNaming("a")).toBe(false);

    restoreSessionState({
      id: "a",
      saved
    });

    expect(saved.naming).toBe("claude");
    expect(isNaming("a")).toBe(true);
  });

  it("saves nothing while naming is off", () => {
    expect(savedSessionState("a").naming).toBeUndefined();
  });

  it("follows a restart to the new id", () => {
    setNaming({
      id: "a",
      agent: "claude",
      isOn: true
    });

    rekeySession({
      fromId: "a",
      toId: "b"
    });

    expect(isNaming("a")).toBe(false);
    expect(isNaming("b")).toBe(true);
    expect(savedSessionState("b").naming).toBe("claude");
  });
});
