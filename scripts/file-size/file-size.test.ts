import { describe, expect, it } from "vitest";

import { countCodeLines, FILE_SIZE_LIMIT, isMeasuredPath, judgeSizes, tightenBaseline, Verdict } from "./file-size";

describe("isMeasuredPath", () => {
  it("measures source files and skips tests, fixtures and icons", () => {
    expect(isMeasuredPath("src/lib/a.ts")).toBe(true);
    expect(isMeasuredPath("src-tauri/src/a.rs")).toBe(true);
    expect(isMeasuredPath("src/lib/a.test.ts")).toBe(false);
    expect(isMeasuredPath("src-tauri/src/agents/conversations/tests.rs")).toBe(false);
    expect(isMeasuredPath("src-tauri/src/pacing/fixtures/golden_fixtures.rs")).toBe(false);
    expect(isMeasuredPath("src/lib/icons/status/a.ts")).toBe(false);
    expect(isMeasuredPath("src/lib/tests/helper.ts")).toBe(false);
    expect(isMeasuredPath("docs/a.ts")).toBe(false);
    expect(isMeasuredPath("src/a.json")).toBe(false);
  });
});

describe("countCodeLines", () => {
  it("skips blank lines and comment-only lines", () => {
    const text = [
      "// note",
      "",
      "const a = 1;",
      "/* block",
      "still comment */",
      "const b = 2; // trailing",
      "/** doc */"
    ].join("\n");
    expect(
      countCodeLines({
        path: "a.ts",
        text
      })
    ).toBe(2);
  });

  it("drops cfg(test) modules and declarations from Rust", () => {
    const text = [
      "fn a() {}",
      "#[cfg(test)]",
      "mod tests {",
      "    fn t() {",
      "        assert!(true);",
      "    }",
      "}",
      "#[cfg(test)]",
      "mod more_tests;",
      "fn b() {}"
    ].join("\n");
    expect(
      countCodeLines({
        path: "a.rs",
        text
      })
    ).toBe(2);
  });

  it("counts a Svelte file's script, template and style together", () => {
    const text = [
      "<script>",
      "  let a = 1;",
      "</script>",
      "<!-- note -->",
      "<p>{a}</p>",
      "<style>",
      "  p { color: red; }",
      "</style>"
    ].join("\n");
    expect(
      countCodeLines({
        path: "A.svelte",
        text
      })
    ).toBe(7);
  });

  it("keeps a CSS selector line that starts with a slash-free rule", () => {
    expect(
      countCodeLines({
        path: "a.css",
        text: "/* c */\na { color: red; }"
      })
    ).toBe(1);
  });
});

describe("judgeSizes", () => {
  const baselined = FILE_SIZE_LIMIT + 100;

  it("fails a new file over the limit", () => {
    const findings = judgeSizes({
      sizes: {
        "a.ts": FILE_SIZE_LIMIT + 1
      },
      baseline: {}
    });
    expect(findings).toEqual([
      {
        path: "a.ts",
        size: FILE_SIZE_LIMIT + 1,
        recorded: undefined,
        kind: Verdict.FAIL
      }
    ]);
  });

  it("passes a new file at the limit", () => {
    expect(
      judgeSizes({
        sizes: {
          "a.ts": FILE_SIZE_LIMIT
        },
        baseline: {}
      })
    ).toEqual([]);
  });

  it("fails a baselined file that grew", () => {
    const findings = judgeSizes({
      sizes: {
        "a.ts": baselined + 1
      },
      baseline: {
        "a.ts": baselined
      }
    });
    expect(findings.map(finding => finding.kind)).toEqual([Verdict.FAIL]);
  });

  it("reports a shrunk baselined file without failing", () => {
    const findings = judgeSizes({
      sizes: {
        "a.ts": baselined - 1
      },
      baseline: {
        "a.ts": baselined
      }
    });
    expect(findings.map(finding => finding.kind)).toEqual([Verdict.SHRUNK]);
  });
});

describe("tightenBaseline", () => {
  it("lowers sizes, drops files at or under the limit, and never adds or raises", () => {
    const tightened = tightenBaseline({
      sizes: {
        "shrunk.ts": 600,
        "fixed.ts": 400,
        "grown.ts": 900,
        "new.ts": 700
      },
      baseline: {
        "shrunk.ts": 800,
        "fixed.ts": 800,
        "grown.ts": 800
      }
    });
    expect(tightened).toEqual({
      "shrunk.ts": 600,
      "grown.ts": 800
    });
  });
});
