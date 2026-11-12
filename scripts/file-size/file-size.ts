import { z } from "zod";

export const FILE_SIZE_LIMIT = 500;

const SOURCE_ROOTS = ["src/", "src-tauri/src/", "scripts/"];
const SOURCE_EXTENSIONS = [".ts", ".svelte", ".rs", ".mjs", ".css"];
const EXCLUDED_PATH_PARTS = ["src-tauri/src/pacing/fixtures/", "src/lib/icons/", "/tests/"];
const EXCLUDED_FILE_NAMES = ["tests.rs", "golden_fixtures.rs"];
const TEST_SUFFIX = ".test.ts";
const RUST_EXTENSION = ".rs";
const CSS_EXTENSION = ".css";
const CFG_TEST_ATTRIBUTE = "#[cfg(test)]";
const BLOCK_COMMENT_OPEN = "/*";
const BLOCK_COMMENT_CLOSE = "*/";
const HTML_COMMENT_OPEN = "<!--";
const HTML_COMMENT_CLOSE = "-->";
const LINE_COMMENT = "//";

export const BaselineSchema = z.record(z.string().min(1), z.number().int().positive());
type Baseline = z.infer<typeof BaselineSchema>;

export function isMeasuredPath(path: string) {
  const isUnderSourceRoot = SOURCE_ROOTS.some((root) => path.startsWith(root));
  const hasSourceExtension = SOURCE_EXTENSIONS.some((extension) => path.endsWith(extension));
  const isTestFile = path.endsWith(TEST_SUFFIX) || EXCLUDED_FILE_NAMES.includes(path.split("/").at(-1) ?? "");
  const isExcludedFolder = EXCLUDED_PATH_PARTS.some((part) => path.includes(part));
  return isUnderSourceRoot && hasSourceExtension && !isTestFile && !isExcludedFolder;
}

function countBraces(line: string) {
  let depth = 0;
  for (const character of line) {
    if (character === "{") depth += 1;
    if (character === "}") depth -= 1;
  }
  return depth;
}

function withoutCfgTestItems(lines: string[]) {
  const kept: string[] = [];
  let isSkipping = false;
  let isInsideItem = false;
  let depth = 0;
  for (const line of lines) {
    const isTestAttribute = line.trim() === CFG_TEST_ATTRIBUTE;
    if (!isSkipping && isTestAttribute) {
      isSkipping = true;
      isInsideItem = false;
      depth = 0;
      continue;
    }
    if (!isSkipping) {
      kept.push(line);
      continue;
    }
    depth += countBraces(line);
    isInsideItem = isInsideItem || line.includes("{");
    const isItemEnd = isInsideItem ? depth <= 0 : line.trimEnd().endsWith(";");
    if (isItemEnd) isSkipping = false;
  }
  return kept;
}

function commentCloserOf(line: string) {
  if (line.startsWith(BLOCK_COMMENT_OPEN)) return BLOCK_COMMENT_CLOSE;
  if (line.startsWith(HTML_COMMENT_OPEN)) return HTML_COMMENT_CLOSE;
  return undefined;
}

function codeLines({ lines, isCss }: { lines: string[]; isCss: boolean }) {
  const code: string[] = [];
  let closer: string | undefined;
  for (const raw of lines) {
    const line = raw.trim();
    if (closer) {
      if (line.includes(closer)) closer = undefined;
      continue;
    }
    if (line === "") continue;
    const opened = commentCloserOf(line);
    if (opened) {
      const isClosedOnSameLine = line.endsWith(opened) && line.length >= 2 * opened.length;
      if (!isClosedOnSameLine) closer = opened;
      continue;
    }
    const isLineComment = !isCss && line.startsWith(LINE_COMMENT);
    if (!isLineComment) code.push(line);
  }
  return code;
}

export function countCodeLines({ path, text }: { path: string; text: string }) {
  const lines = text.split(/\r?\n/);
  const source = path.endsWith(RUST_EXTENSION) ? withoutCfgTestItems(lines) : lines;
  return codeLines({ lines: source, isCss: path.endsWith(CSS_EXTENSION) }).length;
}

export const Verdict = {
  Fail: "fail",
  Shrunk: "shrunk"
} as const;

export function judgeSizes({ sizes, baseline }: { sizes: Baseline; baseline: Baseline }) {
  const findings: { path: string; size: number; recorded: number | undefined; kind: (typeof Verdict)[keyof typeof Verdict] }[] = [];
  for (const [path, size] of Object.entries(sizes)) {
    const recorded = baseline[path];
    const isNewOffender = recorded === undefined && size > FILE_SIZE_LIMIT;
    const hasGrown = recorded !== undefined && size > recorded;
    if (isNewOffender || hasGrown) findings.push({ path, size, recorded, kind: Verdict.Fail });
  }
  for (const [path, recorded] of Object.entries(baseline)) {
    const size = sizes[path] ?? 0;
    if (size < recorded) findings.push({ path, size, recorded, kind: Verdict.Shrunk });
  }
  return findings;
}

export function tightenBaseline({ sizes, baseline }: { sizes: Baseline; baseline: Baseline }) {
  const tightened: Baseline = {};
  for (const [path, recorded] of Object.entries(baseline)) {
    const size = sizes[path] ?? 0;
    if (size > FILE_SIZE_LIMIT) tightened[path] = Math.min(size, recorded);
  }
  return tightened;
}

export function offendersOf(sizes: Baseline) {
  const offenders: Baseline = {};
  for (const [path, size] of Object.entries(sizes)) {
    if (size > FILE_SIZE_LIMIT) offenders[path] = size;
  }
  return offenders;
}
