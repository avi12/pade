import { describe, expect, it } from "vitest";

import { encodeChord, normalizeChord } from "@/lib/terminal/input/key-chord";
import { ENTER, ESCAPE } from "@/lib/terminal/input/terminal-sequences";

const CONTROL_X = String.fromCharCode(0x18);
const CONTROL_K = String.fromCharCode(0x0b);

describe("encodeChord", () => {
  it("sends each keystroke of a chord on its own, ctrl clearing the letter's top bits", () => {
    expect(encodeChord("ctrl+x ctrl+k")).toEqual([CONTROL_X, CONTROL_K]);
    expect(encodeChord("Ctrl+X   Ctrl+K")).toEqual([CONTROL_X, CONTROL_K]);
  });

  it("names the keys configs spell out", () => {
    expect(encodeChord("escape")).toEqual([ESCAPE]);
    expect(encodeChord("esc")).toEqual([ESCAPE]);
    expect(encodeChord("enter")).toEqual([ENTER]);
  });

  it("prefixes alt, meta and option with Esc and capitalises a shifted letter", () => {
    expect(encodeChord("meta+p")).toEqual([`${ESCAPE}p`]);
    expect(encodeChord("alt+shift+k")).toEqual([`${ESCAPE}K`]);
    expect(encodeChord("ctrl+x 1")).toEqual([CONTROL_X, "1"]);
  });

  it("refuses what a terminal can't send apart from another key", () => {
    expect(encodeChord("ctrl+shift+k")).toBeNull();
    expect(encodeChord("cmd+k")).toBeNull();
    expect(encodeChord("ctrl+x cmd+k")).toBeNull();
    expect(encodeChord("ctrl+enter")).toBeNull();
    expect(encodeChord("")).toBeNull();
  });
});

describe("normalizeChord", () => {
  it("compares two spellings of one chord", () => {
    expect(normalizeChord(" Ctrl+X  ctrl+K ")).toBe(normalizeChord("ctrl+x ctrl+k"));
  });
});
