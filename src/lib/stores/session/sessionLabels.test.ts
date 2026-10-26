import {
  forgetSession,
  rekeySession,
  restoreSessionState,
  savedSessionState
} from "@/lib/session/lifecycle/session-scope";
import { sessionLabel, setSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { afterEach, describe, expect, it } from "vitest";

afterEach(() => {
  forgetSession("a");
  forgetSession("b");
});

describe("session labels", () => {
  it("follows a restart to the new id", () => {
    setSessionLabel({
      id: "a",
      label: "fix-the-rail"
    });

    rekeySession({
      fromId: "a",
      toId: "b"
    });

    expect(sessionLabel("a")).toBeUndefined();
    expect(sessionLabel("b")).toBe("fix-the-rail");
  });

  it("is saved and restored, and ignores a saved value of the wrong shape", () => {
    setSessionLabel({
      id: "a",
      label: "fix-the-rail"
    });
    const saved = savedSessionState("a");
    forgetSession("a");

    restoreSessionState({
      id: "a",
      saved
    });
    restoreSessionState({
      id: "b",
      saved: {
        label: 42
      }
    });

    expect(sessionLabel("a")).toBe("fix-the-rail");
    expect(sessionLabel("b")).toBeUndefined();
  });
});
