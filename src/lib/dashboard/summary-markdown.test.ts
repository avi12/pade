import { InlineKind, inlineMarkdown } from "@/lib/dashboard/summary-markdown";
import { describe, expect, it } from "vitest";

describe("inlineMarkdown", () => {
  it("splits bold, code and a link out of the plain text, in order", () => {
    expect(inlineMarkdown("**Done**: merged [PR #12](https://github.com/a/b/pull/12) after `pnpm test`")).toEqual([
      {
        kind: InlineKind.Strong,
        text: "Done"
      },
      {
        kind: InlineKind.Text,
        text: ": merged "
      },
      {
        kind: InlineKind.Link,
        text: "PR #12",
        url: "https://github.com/a/b/pull/12"
      },
      {
        kind: InlineKind.Text,
        text: " after "
      },
      {
        kind: InlineKind.Code,
        text: "pnpm test"
      }
    ]);
  });

  it("links a bare web address and reads emphasis either way", () => {
    expect(inlineMarkdown("see https://example.com/x and *soon* or _now_")).toEqual([
      {
        kind: InlineKind.Text,
        text: "see "
      },
      {
        kind: InlineKind.Link,
        text: "https://example.com/x",
        url: "https://example.com/x"
      },
      {
        kind: InlineKind.Text,
        text: " and "
      },
      {
        kind: InlineKind.Emphasis,
        text: "soon"
      },
      {
        kind: InlineKind.Text,
        text: " or "
      },
      {
        kind: InlineKind.Emphasis,
        text: "now"
      }
    ]);
  });

  it("never makes a link of anything but a web address, and leaves snake_case alone", () => {
    expect(inlineMarkdown("[x](javascript:alert(1)) in my_var_name")).toEqual([
      {
        kind: InlineKind.Text,
        text: "[x](javascript:alert(1)) in my_var_name"
      }
    ]);
  });

  it("keeps code spans literal", () => {
    expect(inlineMarkdown("ran `**not bold**`")).toEqual([
      {
        kind: InlineKind.Text,
        text: "ran "
      },
      {
        kind: InlineKind.Code,
        text: "**not bold**"
      }
    ]);
  });
});
