import {
  DiffKind,
  firstChangedLine,
  parseDiff,
  toSplitRows,
  unifiedDiff
} from "@/lib/diff";
import { describe, expect, it } from "vitest";

// A small but realistic `git diff` body: file meta, one hunk, a context line,
// a replaced line (deletion + addition), a second addition, then context.
const SAMPLE = [
  "diff --git a/src/app.ts b/src/app.ts",
  "index 1111111..2222222 100644",
  "--- a/src/app.ts",
  "+++ b/src/app.ts",
  "@@ -1,3 +1,4 @@",
  " const one = 1;",
  "-const two = 3;",
  "+const two = 2;",
  "+const three = 3;",
  " export {};"
].join("\n");

describe("parseDiff", () => {
  it("classifies meta, context, deletion and addition lines", () => {
    const kinds = parseDiff(SAMPLE).map(line => line.kind);

    expect(kinds).toEqual([
      DiffKind.meta,
      DiffKind.meta,
      DiffKind.meta,
      DiffKind.meta,
      DiffKind.meta,
      DiffKind.context,
      DiffKind.del,
      DiffKind.add,
      DiffKind.add,
      DiffKind.context
    ]);
  });

  it("numbers add and context lines against the new file", () => {
    const numbers = parseDiff(SAMPLE).map(line => line.newLine);

    expect(numbers).toEqual([undefined, undefined, undefined, undefined, undefined, 1, undefined, 2, 3, 4]);
  });

  it("numbers deletion and context lines against the old file", () => {
    const numbers = parseDiff(SAMPLE).map(line => line.oldLine);

    expect(numbers).toEqual([undefined, undefined, undefined, undefined, undefined, 1, 2, undefined, undefined, 3]);
  });

  it("restarts numbering at every hunk header", () => {
    const lines = parseDiff("@@ -10,2 +20,2 @@\n context\n+add\n@@ -40 +50 @@\n context");

    expect(lines.map(line => line.newLine)).toEqual([undefined, 20, 21, undefined, 50]);
  });

  it("drops the trailing blank line of a diff body", () => {
    expect(parseDiff("@@ -1 +1 @@\n+x\n")).toHaveLength(2);
  });

  it("gives lines before any hunk header no line number", () => {
    const [line] = parseDiff("plain text");

    expect(line.kind).toBe(DiffKind.context);
    expect(line.newLine).toBeUndefined();
  });
});

describe("firstChangedLine", () => {
  it("lands on the first added line", () => {
    expect(firstChangedLine(parseDiff(SAMPLE))).toBe(2);
  });

  it("falls back to the first line present in the new file", () => {
    const lines = parseDiff("@@ -1,2 +1 @@\n context\n-gone");

    expect(firstChangedLine(lines)).toBe(1);
  });

  it("is undefined when nothing maps to the new file", () => {
    expect(firstChangedLine(parseDiff("diff --git a/x b/x"))).toBeUndefined();
  });
});

describe("toSplitRows", () => {
  const rows = toSplitRows(parseDiff(SAMPLE));

  it("renders meta lines as full-width hunk rows", () => {
    expect(rows[4].hunk).toBe(true);
    expect(rows[4].hunkText).toBe("@@ -1,3 +1,4 @@");
  });

  it("puts context on both sides, unfilled", () => {
    expect(rows[5].left).toBe("const one = 1;");
    expect(rows[5].right).toBe("const one = 1;");
    expect(rows[5].leftFilled).toBe(false);
    expect(rows[5].rightFilled).toBe(false);
  });

  it("puts deletions on the left only, wash-tinted", () => {
    expect(rows[6].left).toBe("const two = 3;");
    expect(rows[6].leftFilled).toBe(true);
    expect(rows[6].right).toBe("");
  });

  it("puts additions on the right only, wash-tinted", () => {
    expect(rows[7].right).toBe("const two = 2;");
    expect(rows[7].rightFilled).toBe(true);
    expect(rows[7].left).toBe("");
  });
});

describe("unifiedDiff", () => {
  it("is empty when the two texts are identical", () => {
    expect(
      unifiedDiff({
        before: "a\nb\n",
        after: "a\nb\n"
      })
    ).toBe("");
  });

  it("treats an empty baseline as a full addition", () => {
    expect(
      unifiedDiff({
        before: "",
        after: "one\ntwo"
      })
    ).toBe("@@ -0,0 +1,2 @@\n+one\n+two");
  });

  it("treats an emptied file as a full removal", () => {
    expect(
      unifiedDiff({
        before: "one\ntwo",
        after: ""
      })
    ).toBe("@@ -1,2 +0,0 @@\n-one\n-two");
  });

  it("a trailing newline is not a change on its own", () => {
    expect(
      unifiedDiff({
        before: "one\ntwo",
        after: "one\ntwo\n"
      })
    ).toBe("");
  });

  it("shows a middle edit with bounded context, deletions before additions", () => {
    const diff = unifiedDiff({
      before: "one\ntwo\nthree",
      after: "one\nTWO\nthree"
    });

    expect(diff).toBe("@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three");
  });

  it("keeps only DIFF_CONTEXT lines around a change in a longer file", () => {
    const before = ["a", "b", "c", "d", "e", "f", "g", "h", "i"].join("\n");
    const after = ["a", "b", "c", "d", "E", "f", "g", "h", "i"].join("\n");

    // The edit is on line 5, so lines 1 (a) and 9 (i) fall outside the ±3 window.
    expect(
      unifiedDiff({
        before,
        after
      })
    ).toBe(
      "@@ -2,7 +2,7 @@\n b\n c\n d\n-e\n+E\n f\n g\n h"
    );
  });

  it("round-trips through parseDiff into classified, new-file-numbered lines", () => {
    const lines = parseDiff(
      unifiedDiff({
        before: "one\ntwo\nthree",
        after: "one\nTWO\nthree"
      })
    );

    expect(lines.map(line => line.kind)).toEqual([
      DiffKind.meta,
      DiffKind.context,
      DiffKind.del,
      DiffKind.add,
      DiffKind.context
    ]);
    expect(lines.map(line => line.newLine)).toEqual([undefined, 1, undefined, 2, 3]);
    expect(firstChangedLine(lines)).toBe(2);
  });
});
