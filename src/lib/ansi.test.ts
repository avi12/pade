import { parseAnsi, stripAnsi } from "@/lib/ansi";
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

describe("parseAnsi", () => {
  it("returns one plain segment for text with no codes", () => {
    expect(parseAnsi("plain text")).toEqual([{ text: "plain text" }]);
  });

  it("splits a coloured run into a plain and a coloured segment", () => {
    expect(parseAnsi(`before${ESC}[31mred`)).toEqual([
      { text: "before" },
      {
        text: "red",
        color: "var(--terminal-red)"
      }
    ]);
  });

  it("maps foreground, bright, and background colours to palette tokens", () => {
    expect(parseAnsi(`${ESC}[32mg`)).toEqual([{
      text: "g",
      color: "var(--terminal-green)"
    }]);
    expect(parseAnsi(`${ESC}[91mr`)).toEqual([{
      text: "r",
      color: "var(--terminal-bright-red)"
    }]);
    expect(parseAnsi(`${ESC}[44mb`)).toEqual([{
      text: "b",
      background: "var(--terminal-blue)"
    }]);
  });

  it("carries bold/italic/underline styles onto the segment", () => {
    expect(parseAnsi(`${ESC}[1;3;4mx`)).toEqual([
      {
        text: "x",
        bold: true,
        italic: true,
        underline: true
      }
    ]);
  });

  it("reproduces Vite's banner as coloured segments (the reported bug)", () => {
    const banner = `${ESC}[32m${ESC}[1mVITE${ESC}[22m v8.0.13${ESC}[39m ready`;
    expect(parseAnsi(banner)).toEqual([
      {
        text: "VITE",
        color: "var(--terminal-green)",
        bold: true
      },
      {
        text: " v8.0.13",
        color: "var(--terminal-green)"
      },
      { text: " ready" }
    ]);
  });

  it("clears all state on reset", () => {
    expect(parseAnsi(`${ESC}[31;1mred${ESC}[0mplain`)).toEqual([
      {
        text: "red",
        color: "var(--terminal-red)",
        bold: true
      },
      { text: "plain" }
    ]);
  });

  it("clears a single attribute without touching the others", () => {
    expect(parseAnsi(`${ESC}[1;31mrb${ESC}[22mr`)).toEqual([
      {
        text: "rb",
        color: "var(--terminal-red)",
        bold: true
      },
      {
        text: "r",
        color: "var(--terminal-red)"
      }
    ]);
  });

  it("maps a 256-colour palette index inside 0–15, and skips one above it", () => {
    expect(parseAnsi(`${ESC}[38;5;12mx`)).toEqual([{
      text: "x",
      color: "var(--terminal-bright-blue)"
    }]);
    expect(parseAnsi(`${ESC}[38;5;200mx`)).toEqual([{ text: "x" }]);
  });

  it("consumes a truecolor sequence without emitting a colour", () => {
    expect(parseAnsi(`${ESC}[38;2;10;20;30mx`)).toEqual([{ text: "x" }]);
  });

  it("skips unmodelled codes (cursor moves) and keeps the text", () => {
    expect(parseAnsi(`a${ESC}[2Kb${ESC}[5mc`)).toEqual([
      { text: "a" },
      { text: "b" },
      { text: "c" }
    ]);
  });

  it("returns a single empty segment for empty input", () => {
    expect(parseAnsi("")).toEqual([{ text: "" }]);
  });

  it("returns an empty segment for an all-escape line", () => {
    expect(parseAnsi(`${ESC}[32m${ESC}[0m`)).toEqual([{ text: "" }]);
  });
});
