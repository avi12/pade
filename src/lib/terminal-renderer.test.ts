import { shouldUseWebgl } from "@/lib/terminal-renderer";
import { describe, expect, it } from "vitest";

describe("shouldUseWebgl", () => {
  it("uses WebGL for a visible terminal in the focused window", () => {
    expect(
      shouldUseWebgl({
        shown: true,
        windowFocused: true
      })
    ).toBe(true);
  });

  it("releases WebGL for a background tab", () => {
    expect(
      shouldUseWebgl({
        shown: false,
        windowFocused: true
      })
    ).toBe(false);
  });

  it("releases WebGL while PADE is not the foreground app", () => {
    expect(
      shouldUseWebgl({
        shown: true,
        windowFocused: false
      })
    ).toBe(false);
  });
});
