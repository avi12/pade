import { errorHeadline } from "@/lib/error-text";
import { describe, expect, it } from "vitest";

describe("errorHeadline", () => {
  it("keeps the line that names the obstacle and drops git's advice", () => {
    const refusal = new Error(
      [
        "error: Your local changes to the following files would be overwritten by checkout:",
        "\tsrc/App.svelte",
        "Please commit your changes or stash them before you switch branches."
      ].join("\n")
    );

    expect(
      errorHeadline({
        error: refusal,
        fallback: "Could not switch."
      })
    )
      .toBe("Your local changes to the following files would be overwritten by checkout:");
  });

  it("strips the severity word a toast already implies", () => {
    expect(
      errorHeadline({
        error: new Error("fatal: 'work' is already used by worktree at 'C:/repo/work'"),
        fallback: "Could not switch."
      })
    )
      .toBe("'work' is already used by worktree at 'C:/repo/work'");
    expect(
      errorHeadline({
        error: new Error("warning: nothing to fetch"),
        fallback: "Sync failed."
      })
    )
      .toBe("nothing to fetch");
  });

  it("reads a thrown non-Error, and falls back when it says nothing", () => {
    expect(
      errorHeadline({
        error: "invalid Git object id",
        fallback: "Could not switch."
      })
    )
      .toBe("invalid Git object id");
    expect(
      errorHeadline({
        error: new Error("   "),
        fallback: "Could not switch."
      })
    )
      .toBe("Could not switch.");
  });
});
