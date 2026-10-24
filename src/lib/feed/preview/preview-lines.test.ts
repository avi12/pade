import { DiffKind } from "@/lib/code/diff";
import { previewLines } from "@/lib/feed/preview/preview-lines";
import { describe, expect, it } from "vitest";

function kindsOf(lines: ReturnType<typeof previewLines>) {
  return lines.filter(line => line.kind !== DiffKind.Meta).map(line => line.kind);
}

describe("previewLines", () => {
  it("has nothing to show without a preview", () => {
    expect(previewLines(null)).toEqual([]);
  });

  it("diffs the baseline against the current content", () => {
    const lines = previewLines({
      before: "one\ntwo\n",
      after: "one\nthree\n"
    });

    expect(kindsOf(lines)).toEqual([DiffKind.Context, DiffKind.Removed, DiffKind.Added]);
  });

  it("previews the whole file when the baseline already equals it", () => {
    const lines = previewLines({
      before: "late\nbaseline\n",
      after: "late\nbaseline\n"
    });

    expect(kindsOf(lines)).toEqual([DiffKind.Added, DiffKind.Added]);
  });

  it("keeps a deletion's removal diff", () => {
    const lines = previewLines({
      before: "gone\n",
      after: ""
    });

    expect(kindsOf(lines)).toEqual([DiffKind.Removed]);
  });

  it("shows nothing for an empty file that stayed empty", () => {
    expect(
      previewLines({
        before: "",
        after: ""
      })
    ).toEqual([]);
  });
});
