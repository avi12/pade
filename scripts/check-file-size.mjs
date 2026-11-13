// Ratchet against bloated files: counts the non-test, non-blank, non-comment
// lines of every source file and fails when a file over the limit is new or
// has grown past the size recorded in scripts/file-size-baseline.json.
// Usage: pnpm lint:size [--update | --init]. --update only ever lowers the
// baseline; --init writes the first one from the current tree.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

import {
  BaselineSchema,
  countCodeLines,
  FILE_SIZE_LIMIT,
  isMeasuredPath,
  judgeSizes,
  offendersOf,
  tightenBaseline,
  Verdict
} from "./file-size/file-size.ts";

const BASELINE_PATH = "scripts/file-size-baseline.json";
const JSON_INDENT = 2;
const GIT_OUTPUT_LIMIT = 64 * 1024 * 1024;
const IS_UPDATE = process.argv.includes("--update");
const IS_INITIALIZE = process.argv.includes("--init");

function readBaseline() {
  const parsed = BaselineSchema.safeParse(JSON.parse(readFileSync(BASELINE_PATH, "utf8")));
  if (parsed.success) {
    return parsed.data;
  }

  console.error(`${BASELINE_PATH} is malformed: ${parsed.error.message}`);
  process.exit(1);
}

function measureSources() {
  const listing = execFileSync("git", ["ls-files", "--cached", "--others", "--exclude-standard"], {
    encoding: "utf8",
    maxBuffer: GIT_OUTPUT_LIMIT
  });
  const sizes = {};
  for (const path of listing.split("\n")) {
    if (!isMeasuredPath(path)) {
      continue;
    }

    sizes[path] = countCodeLines({
      path,
      text: readFileSync(path, "utf8")
    });
  }
  return sizes;
}

function writeBaseline(baseline) {
  const largestFirst = Object.fromEntries(Object.entries(baseline).sort(([, sizeA], [, sizeB]) => sizeB - sizeA));
  writeFileSync(BASELINE_PATH, `${JSON.stringify(largestFirst, null, JSON_INDENT)}\n`);
}

const sizes = measureSources();
if (IS_INITIALIZE) {
  writeBaseline(offendersOf(sizes));
  console.log(`Wrote ${BASELINE_PATH}`);
  process.exit(0);
}

const baseline = readBaseline();
if (IS_UPDATE) {
  writeBaseline(
    tightenBaseline({
      sizes,
      baseline
    })
  );
  console.log(`Tightened ${BASELINE_PATH}`);
  process.exit(0);
}

const findings = judgeSizes({
  sizes,
  baseline
});
for (const { path, size, recorded } of findings.filter(finding => finding.kind === Verdict.Shrunk)) {
  console.log(`shrunk: ${path} is ${size} lines (baseline ${recorded}) - run pnpm lint:size --update to tighten`);
}

const failures = findings.filter(finding => finding.kind === Verdict.Fail);
for (const { path, size, recorded } of failures) {
  const allowed = recorded ?? FILE_SIZE_LIMIT;
  console.error(
    `${path}: ${size} lines of non-test code, allowed ${allowed} - split it by concern into a semantic subfolder`
  );
}
process.exit(failures.length === 0 ? 0 : 1);
