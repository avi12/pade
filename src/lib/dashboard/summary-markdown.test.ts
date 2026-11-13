import { describe, expect, it } from "vitest";

import { InlineKind, inlineMarkdown } from "@/lib/dashboard/summary-markdown";

describe("inlineMarkdown", () => {
  it("splits bold, code and a link out of the plain text, in order", () => {
    expect(inlineMarkdown("**Done**: merged [PR #12](https://github.com/a/b/pull/12) after `pnpm test`")).toEqual([
      {
        kind: InlineKind.STRONG,
        text: "Done"
      },
      {
        kind: InlineKind.TEXT,
        text: ": merged "
      },
      {
        kind: InlineKind.LINK,
        text: "PR #12",
        url: "https://github.com/a/b/pull/12"
      },
      {
        kind: InlineKind.TEXT,
        text: " after "
      },
      {
        kind: InlineKind.CODE,
        text: "pnpm test"
      }
    ]);
  });

  it("links a bare web address and reads emphasis either way", () => {
    expect(inlineMarkdown("see https://example.com/x and *soon* or _now_")).toEqual([
      {
        kind: InlineKind.TEXT,
        text: "see "
      },
      {
        kind: InlineKind.LINK,
        text: "https://example.com/x",
        url: "https://example.com/x"
      },
      {
        kind: InlineKind.TEXT,
        text: " and "
      },
      {
        kind: InlineKind.EMPHASIS,
        text: "soon"
      },
      {
        kind: InlineKind.TEXT,
        text: " or "
      },
      {
        kind: InlineKind.EMPHASIS,
        text: "now"
      }
    ]);
  });

  it("never makes a link of anything but a web address, and leaves snake_case alone", () => {
    expect(inlineMarkdown("[x](javascript:alert(1)) in my_var_name")).toEqual([
      {
        kind: InlineKind.TEXT,
        text: "[x](javascript:alert(1)) in my_var_name"
      }
    ]);
  });

  it("keeps code spans literal", () => {
    expect(inlineMarkdown("ran `**not bold**`")).toEqual([
      {
        kind: InlineKind.TEXT,
        text: "ran "
      },
      {
        kind: InlineKind.CODE,
        text: "**not bold**"
      }
    ]);
  });
});
