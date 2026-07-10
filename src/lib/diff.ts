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
  /** 1-based line number in the NEW file for add/context lines; undefined for
   *  deletions, hunk headers, and file meta. Lets a reveal open at that line. */
  newLine?: number;
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

// A hunk header — `@@ -old,n +new,n @@` — where capture 1 is the new file's
// starting line, from which we count add/context lines forward.
const HUNK_HEADER = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** Parse a raw `git diff` body into classified lines (trailing blank dropped).
 *  Add/context lines carry their 1-based line number in the new file. */
export function parseDiff(raw: string): DiffLine[] {
  const lines = raw.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  let newLine = 0; // 0 until the first hunk header sets the new-file cursor
  return lines.map(text => {
    const kind = classify(text);
    if (kind === DiffKind.meta) {
      const hunk = HUNK_HEADER.exec(text);
      if (hunk) {
        newLine = Number(hunk[1]);
      }

      return {
        text,
        kind
      };
    }

    // Deletions have no counterpart in the new file, so they carry no number.
    if (kind === DiffKind.del) {
      return {
        text,
        kind
      };
    }

    const lineNumber = newLine;
    newLine += 1;
    return lineNumber > 0 ? {
      text,
      kind,
      newLine: lineNumber
    } : {
      text,
      kind
    };
  });
}

/** The new-file line of the first added line (else the first line that exists in
 *  the new file) — where a "reveal in editor" click should land. */
export function firstChangedLine(lines: DiffLine[]): number | undefined {
  const firstAdd = lines.find(line => line.kind === DiffKind.add && line.newLine !== undefined);
  return (firstAdd ?? lines.find(line => line.newLine !== undefined))?.newLine;
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
