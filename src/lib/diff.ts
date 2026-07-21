// Shared unified-diff pipeline (DRY — the Change Feed and the Git panel both
// render diffs). Pure, dependency-free: classify each line of a `git diff` body,
// derive the side-by-side rows for a split view, and — for the Change Feed's
// git-free preview — generate a unified diff between two whole texts so the same
// parse+render path draws a session-baseline diff exactly like a git one.

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
  /** 1-based line number in the OLD file for del/context lines; undefined for
   *  additions, hunk headers, and file meta. Drives the diff gutter's old column. */
  oldLine?: number;
  /** 1-based line number in the NEW file for add/context lines; undefined for
   *  deletions, hunk headers, and file meta. Lets a reveal open at that line, and
   *  drives the diff gutter's new column. */
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
  /** Old-file line for the left (old) side — drives the split gutter's old column. */
  oldLine?: number;
  /** New-file line for the right (new) side — lets a click open at that line, and
   *  drives the split gutter's new column. */
  newLine?: number;
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

// A hunk header — `@@ -old,n +new,n @@` — where capture 1 is the old file's and
// capture 2 the new file's starting line, from which we count lines forward.
const HUNK_HEADER = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** Parse a raw `git diff` body into classified lines (trailing blank dropped).
 *  Context lines carry both 1-based line numbers, additions the new-file number,
 *  deletions the old-file number — the diff gutter reads them per side. */
export function parseDiff(raw: string): DiffLine[] {
  const lines = raw.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }

  let oldLine = 0; // 0 until the first hunk header sets the old-file cursor
  let newLine = 0; // 0 until the first hunk header sets the new-file cursor
  return lines.map(text => {
    const kind = classify(text);
    if (kind === DiffKind.meta) {
      const hunk = HUNK_HEADER.exec(text);
      if (hunk) {
        oldLine = Number(hunk[1]);
        newLine = Number(hunk[2]);
      }

      return {
        text,
        kind
      };
    }

    // Deletions exist only in the old file; additions only in the new; context
    // in both. Advance each side's cursor only where the line is present there.
    if (kind === DiffKind.del) {
      const number = oldLine;
      oldLine += 1;
      return number > 0 ? {
        text,
        kind,
        oldLine: number
      } : {
        text,
        kind
      };
    }

    if (kind === DiffKind.add) {
      const number = newLine;
      newLine += 1;
      return number > 0 ? {
        text,
        kind,
        newLine: number
      } : {
        text,
        kind
      };
    }

    const oldNumber = oldLine;
    const newNumber = newLine;
    oldLine += 1;
    newLine += 1;
    return newNumber > 0 ? {
      text,
      kind,
      oldLine: oldNumber,
      newLine: newNumber
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
        rightFilled: true,
        newLine: line.newLine
      };
    }

    if (line.kind === DiffKind.del) {
      return {
        ...blank,
        left: source,
        leftFilled: true,
        oldLine: line.oldLine
      };
    }

    return {
      ...blank,
      left: source,
      right: source,
      oldLine: line.oldLine,
      newLine: line.newLine
    };
  });
}

// Unified-diff generation ---------------------------------------------------
// The Change Feed's preview is git-free: the backend hands over two whole texts
// (a first-touch baseline and the current file) and this turns them into the
// same unified-diff string `parseDiff` already reads, so the render path is
// shared with git diffs (DRY) and needs no diff dependency.

/** What happened to one line going from `before` to `after`. One authoritative
 *  set, no bare strings. */
const LineOp = {
  keep: "keep",
  insert: "insert",
  delete: "delete"
} as const;
type LineOp = (typeof LineOp)[keyof typeof LineOp];

interface LineChange {
  operation: LineOp;
  text: string;
}

/** Lines of surrounding context kept around each change, matching git's default. */
const DIFF_CONTEXT = 3;

/** Beyond this many DP cells (rows × columns of the *changed* middle region),
 *  skip the line-level LCS and treat the region as a wholesale replace. Bounds
 *  the worst case (two large, wholly different files) to a fixed budget; the
 *  common "edit a few lines in a big file" case never reaches it because the
 *  shared prefix and suffix are peeled off first. */
const MAX_DIFF_CELLS = 4_000_000;

/** Split text into lines, treating a file as newline-terminated: a trailing
 *  newline adds no spurious empty final line, and empty text is zero lines (so an
 *  empty baseline diffs as a pure addition). Mirrors Rust's `str::lines`, keeping
 *  the two sides of a baseline diff consistent. */
function splitLines(text: string): string[] {
  if (text === "") {
    return [];
  }

  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") {
    lines.pop();
  }

  return lines;
}

/** The line-level edit script for a wholly-different region: every old line
 *  removed, then every new line inserted. The fallback when the region is too
 *  large to align, and the base case when one side is empty. */
function replaceChanges({ before, after }: {
  before: string[];
  after: string[];
}): LineChange[] {
  return [
    ...before.map(text => ({
      operation: LineOp.delete,
      text
    })),
    ...after.map(text => ({
      operation: LineOp.insert,
      text
    }))
  ];
}

/** Align two changed regions by longest common subsequence (classic DP +
 *  backtrack), or fall back to a wholesale replace when the region is too large. */
function middleChanges({ before, after }: {
  before: string[];
  after: string[];
}): LineChange[] {
  if (before.length === 0 || after.length === 0) {
    return replaceChanges({
      before,
      after
    });
  }

  const isTooLarge = before.length * after.length > MAX_DIFF_CELLS;
  if (isTooLarge) {
    return replaceChanges({
      before,
      after
    });
  }

  const rows = before.length;
  const columns = after.length;
  // lengths[row][column] = LCS length of before[row..] and after[column..].
  const lengths = Array.from({ length: rows + 1 }, () =>
    Array.from({ length: columns + 1 }, () => 0));
  for (let row = rows - 1; row >= 0; row -= 1) {
    for (let column = columns - 1; column >= 0; column -= 1) {
      lengths[row][column] =
        before[row] === after[column]
          ? lengths[row + 1][column + 1] + 1
          : Math.max(lengths[row + 1][column], lengths[row][column + 1]);
    }
  }

  const changes: LineChange[] = [];
  let row = 0;
  let column = 0;
  while (row < rows && column < columns) {
    if (before[row] === after[column]) {
      changes.push({
        operation: LineOp.keep,
        text: before[row]
      });
      row += 1;
      column += 1;
    } else if (lengths[row + 1][column] >= lengths[row][column + 1]) {
      changes.push({
        operation: LineOp.delete,
        text: before[row]
      });
      row += 1;
    } else {
      changes.push({
        operation: LineOp.insert,
        text: after[column]
      });
      column += 1;
    }
  }
  while (row < rows) {
    changes.push({
      operation: LineOp.delete,
      text: before[row]
    });
    row += 1;
  }
  while (column < columns) {
    changes.push({
      operation: LineOp.insert,
      text: after[column]
    });
    column += 1;
  }

  return changes;
}

/** The full edit script from `before` to `after`, peeling the shared prefix and
 *  suffix off as context before aligning only the region that differs. */
function lineChanges({ before, after }: {
  before: string[];
  after: string[];
}): LineChange[] {
  let head = 0;
  const sharedLimit = Math.min(before.length, after.length);
  while (head < sharedLimit && before[head] === after[head]) {
    head += 1;
  }

  let tail = 0;
  const tailLimit = sharedLimit - head;
  while (tail < tailLimit && before[before.length - 1 - tail] === after[after.length - 1 - tail]) {
    tail += 1;
  }

  const middle = middleChanges({
    before: before.slice(head, before.length - tail),
    after: after.slice(head, after.length - tail)
  });

  return [
    ...before.slice(0, head).map(text => ({
      operation: LineOp.keep,
      text
    })),
    ...middle,
    ...before.slice(before.length - tail).map(text => ({
      operation: LineOp.keep,
      text
    }))
  ];
}

/** A change tagged with its 1-based line number on each side: `oldLine` is real
 *  for keep/delete, `newLine` for keep/insert (the other is the position it sits
 *  at). Lets the hunk header read its start and counts straight off the run. */
interface PlacedChange extends LineChange {
  oldLine: number;
  newLine: number;
}

function markerFor(operation: LineOp): string {
  if (operation === LineOp.insert) {
    return "+";
  }

  if (operation === LineOp.delete) {
    return "-";
  }

  return " ";
}

/** The `@@ -old,n +new,n @@` header for one hunk. A side with no lines starts at
 *  0 (git's convention for a pure addition or deletion), which `parseDiff` reads
 *  as "no new-file cursor" for that side. */
function hunkHeader(hunk: PlacedChange[]): string {
  const oldLines = hunk.filter(change => change.operation !== LineOp.insert);
  const newLines = hunk.filter(change => change.operation !== LineOp.delete);
  const oldStart = oldLines.length > 0 ? oldLines[0].oldLine : 0;
  const newStart = newLines.length > 0 ? newLines[0].newLine : 0;
  return `@@ -${oldStart},${oldLines.length} +${newStart},${newLines.length} @@`;
}

/** Generate a git-style unified diff between two whole texts — the Change Feed's
 *  git-free preview (session baseline vs current content). Groups changes into
 *  hunks with up to `DIFF_CONTEXT` lines of surrounding context and emits the
 *  `@@` headers `parseDiff` reads, so the shared parse+render path draws it.
 *  Returns "" when the texts are identical (the card shows "No preview"). */
export function unifiedDiff({ before, after }: {
  before: string;
  after: string;
}): string {
  const changes = lineChanges({
    before: splitLines(before),
    after: splitLines(after)
  });

  let oldLine = 0;
  let newLine = 0;
  const placed: PlacedChange[] = changes.map(change => {
    if (change.operation !== LineOp.insert) {
      oldLine += 1;
    }

    if (change.operation !== LineOp.delete) {
      newLine += 1;
    }

    return {
      ...change,
      oldLine,
      newLine
    };
  });

  // Keep every changed line, plus the context window around each — an untouched
  // line survives only when it sits within `DIFF_CONTEXT` of some change.
  const kept = Array.from({ length: placed.length }, () => false);
  let anyChange = false;
  placed.forEach((change, index) => {
    if (change.operation === LineOp.keep) {
      return;
    }

    anyChange = true;
    const from = Math.max(0, index - DIFF_CONTEXT);
    const to = Math.min(placed.length - 1, index + DIFF_CONTEXT);
    for (let cursor = from; cursor <= to; cursor += 1) {
      kept[cursor] = true;
    }
  });

  if (!anyChange) {
    return "";
  }

  // Emit each contiguous run of kept lines as one hunk.
  const lines: string[] = [];
  let index = 0;
  while (index < placed.length) {
    if (!kept[index]) {
      index += 1;
      continue;
    }

    let end = index;
    while (end + 1 < placed.length && kept[end + 1]) {
      end += 1;
    }

    const hunk = placed.slice(index, end + 1);
    lines.push(hunkHeader(hunk));
    for (const change of hunk) {
      lines.push(markerFor(change.operation) + change.text);
    }

    index = end + 1;
  }

  return lines.join("\n");
}
