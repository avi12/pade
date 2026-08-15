import type { RtlCell, RtlPart, RtlTextPart } from "@/lib/terminal-rtl";
import {
  containsRtl,
  rowSelection,
  RtlPartKind,
  rtlRow,
  rtlRuns
} from "@/lib/terminal-rtl";
import { describe, expect, it } from "vitest";

/** A row of cells from a plain string, one cell per code point (which is how a
 *  terminal buffer stores them), all painted with the defaults. */
function row(text: string): RtlCell[] {
  return Array.from(text, character => ({
    text: character,
    foreground: "var(--code-foreground)",
    background: "var(--code-background)",
    bold: false,
    dim: false,
    italic: false,
    underline: false,
    strikethrough: false
  }));
}

function columnsOf(text: string) {
  return rtlRuns({ cells: Array.from(text) });
}

function isText(part: RtlPart): part is RtlTextPart {
  return part.kind === RtlPartKind.Text;
}

/** The run's parts as a readable sequence — text verbatim, the caret as `|`. */
function sequence(parts: readonly RtlPart[]): string[] {
  return parts.map(part => isText(part) ? part.text : "|");
}

describe("containsRtl", () => {
  it("spots Arabic, Hebrew and Arabic presentation forms", () => {
    expect(containsRtl("مرحبا")).toBe(true);
    expect(containsRtl("מידע נוסף")).toBe(true);
    expect(containsRtl("ﻫ")).toBe(true);
  });

  it("stays quiet for text that reads left to right", () => {
    expect(containsRtl("● Read 90 lines")).toBe(false);
    expect(containsRtl("╭─ hello ─╮")).toBe(false);
    expect(containsRtl("")).toBe(false);
  });
});

describe("rtlRuns", () => {
  it("covers exactly the right-to-left columns", () => {
    expect(columnsOf("ab عربي cd")).toEqual([{
      startColumn: 3,
      endColumn: 7
    }]);
  });

  it("keeps a whole phrase in one run so it is reordered as a sentence", () => {
    expect(columnsOf("מידע נוסף")).toEqual([{
      startColumn: 0,
      endColumn: 9
    }]);
  });

  it("trims the neutrals at a run's edges — they belong to the text beside them", () => {
    expect(columnsOf("  عربي  ")).toEqual([{
      startColumn: 2,
      endColumn: 6
    }]);
  });

  it("ends a run at a letter that reads left to right", () => {
    expect(columnsOf("عربي hello عربي")).toEqual([
      {
        startColumn: 0,
        endColumn: 4
      },
      {
        startColumn: 11,
        endColumn: 15
      }
    ]);
  });

  it("ends a run at a box-drawing glyph, so a TUI's frame is never reordered", () => {
    expect(columnsOf("│عربي│")).toEqual([{
      startColumn: 1,
      endColumn: 5
    }]);
  });

  it("splits across column padding but not across a word space", () => {
    expect(columnsOf("عربي عربي")).toEqual([{
      startColumn: 0,
      endColumn: 9
    }]);
    expect(columnsOf("عربي          عربي")).toEqual([
      {
        startColumn: 0,
        endColumn: 4
      },
      {
        startColumn: 14,
        endColumn: 18
      }
    ]);
  });

  it("carries a number the phrase pulls along with it", () => {
    expect(columnsOf("מועד: 11.01.2027")).toEqual([{
      startColumn: 0,
      endColumn: 16
    }]);
  });

  it("leaves a number that comes before any right-to-left text alone", () => {
    expect(columnsOf("2027 עדכון")).toEqual([{
      startColumn: 5,
      endColumn: 10
    }]);
  });

  it("finds nothing in a row with no right-to-left text", () => {
    expect(columnsOf("● Read 90 lines")).toEqual([]);
  });
});

describe("rtlRow", () => {
  it("keeps one uniformly styled run as a single part, so the letters can join", () => {
    const [run] = rtlRow({ cells: row("hi مرحبا") });
    expect(run?.startColumn).toBe(3);
    expect(run?.columns).toBe(5);
    expect(sequence(run?.parts ?? [])).toEqual(["مرحبا"]);
  });

  it("splits a part only where the styling actually changes", () => {
    const emphasised = row("مرحبا").map((cell, index) => index < 2 ? {
      ...cell,
      bold: true
    } : cell);

    const [run] = rtlRow({ cells: emphasised });
    expect(sequence(run?.parts ?? [])).toEqual(["مر", "حبا"]);
    expect(run?.parts.filter(isText).map(part => part.bold)).toEqual([true, false]);
  });

  it("marks the selected part rather than a column, so the browser places it", () => {
    const [run] = rtlRow({
      cells: row("مرحبا"),
      selection: {
        startColumn: 0,
        endColumn: 2
      }
    });

    expect(sequence(run?.parts ?? [])).toEqual(["مر", "حبا"]);
    expect(run?.parts.filter(isText).map(part => part.selected)).toEqual([true, false]);
  });

  it("places the caret between parts, including past the last character", () => {
    const atStart = rtlRow({
      cells: row("مرحبا"),
      cursorColumn: 0
    });
    expect(sequence(atStart[0]?.parts ?? [])).toEqual(["|", "مرحبا"]);

    const insideText = rtlRow({
      cells: row("مرحبا"),
      cursorColumn: 2
    });
    expect(sequence(insideText[0]?.parts ?? [])).toEqual(["مر", "|", "حبا"]);

    const pastTheEnd = rtlRow({
      cells: row("مرحبا"),
      cursorColumn: 5
    });
    expect(sequence(pastTheEnd[0]?.parts ?? [])).toEqual(["مرحبا", "|"]);
  });

  it("leaves the caret out of a row it is not on", () => {
    const [run] = rtlRow({ cells: row("مرحبا") });
    expect(sequence(run?.parts ?? [])).toEqual(["مرحبا"]);
  });

  it("takes its backdrop from the run's own first cell", () => {
    const cells = row("مرحبا").map(cell => ({
      ...cell,
      background: "#102030"
    }));

    expect(rtlRow({ cells })[0]?.background).toBe("#102030");
  });
});

describe("rowSelection", () => {
  const selection = {
    start: {
      x: 4,
      y: 10
    },
    end: {
      x: 7,
      y: 12
    }
  };

  it("clips to the selection's own ends on the rows that hold them", () => {
    expect(
      rowSelection({
        selection,
        absoluteRow: 10,
        columns: 80
      })
    ).toEqual({
      startColumn: 4,
      endColumn: 80
    });

    expect(
      rowSelection({
        selection,
        absoluteRow: 12,
        columns: 80
      })
    ).toEqual({
      startColumn: 0,
      endColumn: 7
    });
  });

  it("selects a row in the middle end to end", () => {
    expect(
      rowSelection({
        selection,
        absoluteRow: 11,
        columns: 80
      })
    ).toEqual({
      startColumn: 0,
      endColumn: 80
    });
  });

  it("answers nothing for a row outside it, or with no selection at all", () => {
    expect(
      rowSelection({
        selection,
        absoluteRow: 9,
        columns: 80
      })
    ).toBeUndefined();
    expect(
      rowSelection({
        selection: undefined,
        absoluteRow: 11,
        columns: 80
      })
    ).toBeUndefined();
  });
});
