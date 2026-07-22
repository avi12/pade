import { BRACKETED_PASTE_END, BRACKETED_PASTE_START, isPromptNewlineShortcut, PROMPT_NEWLINE, submittedPrompt } from "@/lib/terminal-input";
import { describe, expect, it } from "vitest";

describe("submittedPrompt", () => {
  it("wraps the prompt in paste markers with the submitting Enter OUTSIDE them", () => {
    // The Enter must be a separate keystroke after the closing marker — inside
    // the paste it reads as a soft newline and the prompt sits unsent.
    expect(submittedPrompt("write the handoff")).toBe(
      `${BRACKETED_PASTE_START}write the handoff${BRACKETED_PASTE_END}\r`
    );
  });

  it("keeps a multi-line prompt's own newlines soft inside the paste", () => {
    const delivered = submittedPrompt("line one\nline two");
    const enterCount = delivered.split("\r").length - 1;
    expect(enterCount).toBe(1);
    expect(delivered.endsWith("\r")).toBe(true);
  });
});

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
