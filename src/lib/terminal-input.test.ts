import {
  BRACKETED_PASTE_END,
  BRACKETED_PASTE_START,
  isPromptNewlineShortcut,
  pastedText,
  PROMPT_NEWLINE,
  referencedSnippet,
  submittedPrompt
} from "@/lib/terminal-input";
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

describe("pastedText", () => {
  it("keeps embedded command separators inside one non-submitting paste", () => {
    const delivered = pastedText("review this\r\nrm -rf project");
    expect(delivered).toBe(
      `${BRACKETED_PASTE_START}review this\r\nrm -rf project${BRACKETED_PASTE_END}`
    );
    expect(delivered.endsWith("\r")).toBe(false);
  });
});

describe("referencedSnippet", () => {
  it("prefixes selected code with its path and inclusive line range", () => {
    expect(
      referencedSnippet({
        path: "src/lib/terminal-input.ts",
        anchorLine: 28,
        focusLine: 31,
        snippet: "export function pastedText(text: string): string {\n  return text;\n}"
      })
    ).toBe(
      "src/lib/terminal-input.ts:28-31\nexport function pastedText(text: string): string {\n  return text;\n}"
    );
  });

  it("normalizes an upward selection to an ascending line range", () => {
    expect(
      referencedSnippet({
        path: "src/App.svelte",
        anchorLine: 1541,
        focusLine: 1535,
        snippet: "selected code"
      })
    ).toBe("src/App.svelte:1535-1541\nselected code");
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
