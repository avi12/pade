import { isSyntax, tokenize, tokenizeMarkdown } from "@/lib/highlight";
import type { Token } from "@/lib/highlight";
import { describe, expect, it, vi } from "vitest";

// The color branch resolves swatches through `CSS.supports`, which Node lacks.
// Same minimal stand-in as colors.test.ts, so these tests pin what highlight.ts
// owns: scanning and classification, not the engine's color parser.
vi.stubGlobal("CSS", {
  supports(_property: string, value: string) {
    return value.startsWith("#") || value.startsWith("rgb");
  }
});

function classOf(text: string) {
  return tokenize(text)[0].cls;
}

describe("tokenize", () => {
  it("classifies a // line comment", () => {
    expect(classOf("// a note")).toBe("comment");
  });

  it("classifies a /* block */ comment", () => {
    expect(classOf("/* spans\ntwo lines */")).toBe("comment");
  });

  it("classifies an <!-- html --> comment", () => {
    expect(classOf("<!-- hidden -->")).toBe("comment");
  });

  it("classifies a leading # line as a comment", () => {
    expect(classOf("# shell comment")).toBe("comment");
    expect(classOf("## markdown heading")).toBe("comment");
  });

  it("classifies all three string quote styles", () => {
    expect(classOf(`"double"`)).toBe("string");
    expect(classOf("'single'")).toBe("string");
    expect(classOf("`template`")).toBe("string");
  });

  it("resolves a hex color to its own swatch", () => {
    const [token] = tokenize("#3366ff");

    expect(token.cls).toBe("color");
    expect(token.color).toBe("#3366ff");
  });

  it("resolves an rgb() color", () => {
    const [token] = tokenize("rgb(1, 2, 3)");

    expect(token.cls).toBe("color");
    expect(token.color).toBe("rgb(1, 2, 3)");
  });

  it("traces a var() color through the provided token map", () => {
    const vars = new Map([["--brand", "#123456"]]);
    const [token] = tokenize("var(--brand)", vars);

    expect(token.cls).toBe("color");
    expect(token.color).toBe("#123456");
  });

  it("leaves an unresolvable var() color without a swatch", () => {
    const [token] = tokenize("var(--missing)", new Map());

    expect(token.cls).toBe("color");
    expect(token.color).toBeUndefined();
  });

  it("classifies an at-rule as a keyword", () => {
    expect(classOf("@media")).toBe("keyword");
  });

  it("classifies numbers, including fractions and digit separators", () => {
    expect(classOf("42")).toBe("number");
    expect(classOf("3.14")).toBe("number");
    expect(classOf("1_000")).toBe("number");
  });

  it("classifies keywords from the cross-language set", () => {
    expect(classOf("const")).toBe("keyword");
    expect(classOf("fn")).toBe("keyword");
    expect(classOf("true")).toBe("keyword");
  });

  it("classifies an ordinary identifier as plain", () => {
    expect(classOf("banana")).toBe("plain");
  });

  it("colors an identifier immediately followed by `(` as a function", () => {
    expect(classOf("linear-gradient(0deg, red, blue)")).toBe("function");
    expect(classOf("calc(100% - 24px)")).toBe("function");
  });

  it("colors an identifier immediately followed by `:` as a property/key", () => {
    expect(classOf("align-items: center")).toBe("property");
    expect(classOf("background: red")).toBe("property");
  });

  it("does not treat a `::` pseudo-element or a `://` scheme as a property", () => {
    expect(classOf("div::before")).toBe("plain");
    expect(tokenize("https://x")[0].cls).toBe("plain");
  });

  it("keeps a hyphenated identifier whole", () => {
    expect(tokenize("mask-image")).toEqual([{
      text: "mask-image",
      cls: "plain"
    }]);
  });

  it("does not turn a keyword before `(` into a function", () => {
    expect(classOf("if(x)")).toBe("keyword");
  });

  it("emits the gaps between matches as plain runs", () => {
    const tokens = tokenize("const width = 42;");

    expect(tokens.map(token => token.cls)).toEqual([
      "keyword", "plain", "plain", "plain", "number", "plain"
    ]);
  });

  it("drops no characters: concatenating token texts reproduces the input", () => {
    const input = `const swatch = "#fff" + rgb(1, 2, 3); // done`;
    const tokens = tokenize(input);

    expect(tokens.map(token => token.text).join("")).toBe(input);
  });

  it("returns a single plain token for text with no matches", () => {
    expect(tokenize("=+;")).toEqual([{
      text: "=+;",
      cls: "plain"
    }]);
  });
});

describe("isSyntax", () => {
  it("marks comment, string, number, keyword and function as syntax-colored", () => {
    expect(isSyntax("comment")).toBe(true);
    expect(isSyntax("string")).toBe(true);
    expect(isSyntax("number")).toBe(true);
    expect(isSyntax("keyword")).toBe(true);
    expect(isSyntax("function")).toBe(true);
    expect(isSyntax("property")).toBe(true);
  });

  it("leaves plain and color tokens on the default color", () => {
    expect(isSyntax("plain")).toBe(false);
    expect(isSyntax("color")).toBe(false);
  });
});

describe("tokenizeMarkdown", () => {
  function markdownClassOf(tokens: Token[], text: string): string | undefined {
    return tokens.find(token => token.text.includes(text))?.cls;
  }

  it("leaves prose plain — 'use' and 'for' are words here, not keywords", () => {
    const tokens = tokenizeMarkdown("use pnpm for the extension framework.");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]?.cls).toBe("plain");
  });

  it("colors structure: headings, quotes, list markers, inline spans", () => {
    const tokens = tokenizeMarkdown([
      "# Title",
      "> a quote",
      "- item with `code` and **bold** and [a link](https://x)"
    ].join("\n"));

    expect(markdownClassOf(tokens, "# Title")).toBe("keyword");
    expect(markdownClassOf(tokens, "> a quote")).toBe("comment");
    expect(markdownClassOf(tokens, "- ")).toBe("number");
    expect(markdownClassOf(tokens, "`code`")).toBe("string");
    expect(markdownClassOf(tokens, "**bold**")).toBe("keyword");
    expect(markdownClassOf(tokens, "[a link](https://x)")).toBe("function");
    expect(markdownClassOf(tokens, "item with ")).toBe("plain");
  });

  it("hands fenced code blocks to the generic scanner", () => {
    const tokens = tokenizeMarkdown([
      "```ts",
      "const answer = 42;",
      "```"
    ].join("\n"));

    expect(markdownClassOf(tokens, "```ts")).toBe("comment");
    expect(markdownClassOf(tokens, "const")).toBe("keyword");
    expect(markdownClassOf(tokens, "42")).toBe("number");
  });

  it("round-trips the exact text, newlines included", () => {
    const source = "# H\n\ntext `x`\n";
    const rebuilt = tokenizeMarkdown(source).map(token => token.text).join("");
    expect(rebuilt).toBe(source);
  });
});
