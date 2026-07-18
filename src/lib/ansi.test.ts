import { stripAnsi } from "@/lib/ansi";
import { describe, expect, it } from "vitest";

// ESC (0x1b) built from its code point so no raw control byte sits in the source.
const ESC = String.fromCharCode(0x1b);

describe("stripAnsi", () => {
  it("removes CSI colour and cursor-move sequences", () => {
    expect(stripAnsi(`${ESC}[31mred${ESC}[0m${ESC}[2A`)).toBe("red");
  });

  it("leaves ordinary text untouched", () => {
    expect(stripAnsi("plain 1. text")).toBe("plain 1. text");
  });
});
