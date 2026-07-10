// Shared unified-diff parser (DRY — the Change Feed and the Git panel both
// render diffs). Pure, dependency-free: classify each line of a `git diff` body,
// and derive the side-by-side rows for a split view.

/** What a single diff line represents. One authoritative set, no bare strings. */
export const DiffKind = {
  context: "context",
  add: "add",
  del: "del",
  /** Hunk headers (@@ …) and file meta (diff/index/---/+++) — rendered muted. */
  meta: "meta"
} as const;
export type DiffKind = (typeof DiffKind)[keyof typeof DiffKind];

export interface DiffLine {
  text: string;
  kind: DiffKind;
}

/** One row of a side-by-side (split) view: either a full-width hunk header, or a
 *  left (old) / right (new) cell pair. `*Filled` marks the wash-tinted side. */
export interface SplitRow {
  hunk: boolean;
  hunkText: string;
  left: string;
  leftFilled: boolean;
  right: string;
  rightFilled: boolean;
}

const META_PREFIXES = ["diff ", "index ", "--- ", "+++ ", "new file", "deleted file", "rename ", "similarity "];

function classify(line: string): DiffKind {
  const isHunkHeader = line.startsWith("@@");
  const isFileMeta = META_PREFIXES.some(prefix => line.startsWith(prefix));
  if (isHunkHeader || isFileMeta) {
    return DiffKind.meta;
  }

  if (line.startsWith("+")) {
    return DiffKind.add;
  }

  if (line.startsWith("-")) {
    return DiffKind.del;
  }

  return DiffKind.context;
}

/** Parse a raw `git diff` body into classified lines (trailing blank dropped). */
export function parseDiff(raw: string): DiffLine[] {
  const lines = raw.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  return lines.map(text => ({
    text,
    kind: classify(text)
  }));
}

/** Drop the leading +/-/space marker so split cells show clean source. */
function stripMarker(text: string): string {
  return text.replace(/^[+\-\s]/, "");
}

/** Turn classified lines into side-by-side rows (naive per-line pairing, like the
 *  canvas): additions on the right, deletions on the left, context on both. */
export function toSplitRows(lines: DiffLine[]): SplitRow[] {
  return lines.map(line => {
    const blank: SplitRow = {
      hunk: false,
      hunkText: "",
      left: "",
      leftFilled: false,
      right: "",
      rightFilled: false
    };
    if (line.kind === DiffKind.meta) {
      return {
        ...blank,
        hunk: true,
        hunkText: line.text
      };
    }

    const source = stripMarker(line.text);
    if (line.kind === DiffKind.add) {
      return {
        ...blank,
        right: source,
        rightFilled: true
      };
    }

    if (line.kind === DiffKind.del) {
      return {
        ...blank,
        left: source,
        leftFilled: true
      };
    }

    return {
      ...blank,
      left: source,
      right: source
    };
  });
}
