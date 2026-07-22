import { isPromptNewlineShortcut, PROMPT_NEWLINE } from "@/lib/terminal-input";
import { describe, expect, it } from "vitest";

describe("isPromptNewlineShortcut", () => {
  it("accepts only an unmodified Shift+Enter", () => {
    expect(
      isPromptNewlineShortcut({
        key: "Enter",
        shiftKey: true,
        altKey: false,
        ctrlKey: false,
        metaKey: false
      })
    ).toBe(true);
    expect(
      isPromptNewlineShortcut({
        key: "Enter",
        shiftKey: false,
        altKey: false,
        ctrlKey: false,
        metaKey: false
      })
    ).toBe(false);
    expect(
      isPromptNewlineShortcut({
        key: "Enter",
        shiftKey: true,
        altKey: false,
        ctrlKey: true,
        metaKey: false
      })
    ).toBe(false);
  });

  it("uses a single pasted newline", () => {
    expect(PROMPT_NEWLINE).toBe("\n");
  });
});
