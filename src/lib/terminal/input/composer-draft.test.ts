import { describe, expect, it } from "vitest";

import { nextDraftLength } from "@/lib/terminal/input/composer-draft";
import { pastedText } from "@/lib/terminal/input/terminal-input";
import { CURSOR_UP, ENTER } from "@/lib/terminal/input/terminal-sequences";

function typed(inputs: string[]) {
  let length = 0;
  for (const data of inputs) {
    length = nextDraftLength({
      length,
      data
    });
  }

  return length;
}

describe("nextDraftLength", () => {
  it("counts what the user types", () => {
    expect(typed(["y", "o", "u ", "also"])).toBe(8);
  });

  it("empties on submit", () => {
    expect(typed(["fix it", ENTER])).toBe(0);
  });

  it("takes a character per backspace, never below empty", () => {
    expect(typed(["ab", "\x7f", "\x7f", "\x7f"])).toBe(0);
  });

  it("empties on the clearing keys", () => {
    expect(typed(["draft", "\x03"])).toBe(0);
    expect(typed(["draft", "\x15"])).toBe(0);
    expect(typed(["draft", "\x1b\x1b"])).toBe(0);
  });

  it("leaves a draft alone for arrows and a lone Esc", () => {
    expect(typed(["draft", CURSOR_UP, "\x1b"])).toBe(5);
  });

  it("adds a paste without submitting it, even with a newline inside", () => {
    expect(typed(["see ", pastedText(`line one${ENTER}line two`)])).toBe(4 + 16);
  });
});
