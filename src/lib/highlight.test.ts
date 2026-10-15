import { isSyntax, TokenClass, tokenize, tokenizeMarkdown } from "@/lib/highlight";
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
  return tokenize(text)[0].tokenClass;
}

describe("tokenize", () => {
  it("classifies a // line comment", () => {
    expect(classOf("// a note")).toBe(TokenClass.Comment);
  });

  it("classifies a /* block */ comment", () => {
    expect(classOf("/* spans\ntwo lines */")).toBe(TokenClass.Comment);
  });

  it("classifies an <!-- html --> comment", () => {
    expect(classOf("<!-- hidden -->")).toBe(TokenClass.Comment);
  });

  it("classifies a leading # line as a comment", () => {
    expect(classOf("# shell comment")).toBe(TokenClass.Comment);
    expect(classOf("## markdown heading")).toBe(TokenClass.Comment);
  });

  it("classifies all three string quote styles", () => {
    expect(classOf(`"double"`)).toBe(TokenClass.String);
    expect(classOf("'single'")).toBe(TokenClass.String);
    expect(classOf("`template`")).toBe(TokenClass.String);
  });

  it("resolves a hex color to its own swatch", () => {
    const [token] = tokenize("#3366ff");

    expect(token.tokenClass).toBe(TokenClass.Color);
    expect(token.color).toBe("#3366ff");
  });

  it("resolves an rgb() color", () => {
    const [token] = tokenize("rgb(1, 2, 3)");

    expect(token.tokenClass).toBe(TokenClass.Color);
    expect(token.color).toBe("rgb(1, 2, 3)");
  });

  it("traces a var() color through the provided token map", () => {
    const variables = new Map([["--brand", "#123456"]]);
    const [token] = tokenize("var(--brand)", variables);

    expect(token.tokenClass).toBe(TokenClass.Color);
    expect(token.color).toBe("#123456");
  });

  it("leaves an unresolvable var() color without a swatch", () => {
    const [token] = tokenize("var(--missing)", new Map());

    expect(token.tokenClass).toBe(TokenClass.Color);
    expect(token.color).toBeUndefined();
  });

  it("classifies an at-rule as a keyword", () => {
    expect(classOf("@media")).toBe(TokenClass.Keyword);
  });

  it("classifies numbers, including fractions and digit separators", () => {
    expect(classOf("42")).toBe(TokenClass.Number);
    expect(classOf("3.14")).toBe(TokenClass.Number);
    expect(classOf("1_000")).toBe(TokenClass.Number);
  });

  it("classifies keywords from the cross-language set", () => {
    expect(classOf("const")).toBe(TokenClass.Keyword);
    expect(classOf("fn")).toBe(TokenClass.Keyword);
    expect(classOf("true")).toBe(TokenClass.Keyword);
  });

  it("classifies an ordinary identifier as plain", () => {
    expect(classOf("banana")).toBe(TokenClass.Plain);
  });

  it("colors an identifier immediately followed by `(` as a function", () => {
    expect(classOf("linear-gradient(0deg, red, blue)")).toBe(TokenClass.Function);
    expect(classOf("calc(100% - 24px)")).toBe(TokenClass.Function);
  });

  it("colors an identifier immediately followed by `:` as a property/key", () => {
    expect(classOf("align-items: center")).toBe(TokenClass.Property);
    expect(classOf("background: red")).toBe(TokenClass.Property);
  });

  it("does not treat a `::` pseudo-element or a `://` scheme as a property", () => {
    expect(classOf("div::before")).toBe(TokenClass.Plain);
    expect(tokenize("https://x")[0].tokenClass).toBe(TokenClass.Plain);
  });

  it("keeps a hyphenated identifier whole", () => {
    expect(tokenize("mask-image")).toEqual([{
      text: "mask-image",
      tokenClass: TokenClass.Plain
    }]);
  });

  it("does not turn a keyword before `(` into a function", () => {
    expect(classOf("if(x)")).toBe(TokenClass.Keyword);
  });

  it("emits the gaps between matches as plain runs", () => {
    const tokens = tokenize("const width = 42;");

    expect(tokens.map(token => token.tokenClass)).toEqual([
      TokenClass.Keyword, TokenClass.Plain, TokenClass.Plain, TokenClass.Plain, TokenClass.Number, TokenClass.Plain
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
      tokenClass: TokenClass.Plain
    }]);
  });
});

describe("isSyntax", () => {
  it("marks comment, string, number, keyword and function as syntax-colored", () => {
    expect(isSyntax(TokenClass.Comment)).toBe(true);
    expect(isSyntax(TokenClass.String)).toBe(true);
    expect(isSyntax(TokenClass.Number)).toBe(true);
    expect(isSyntax(TokenClass.Keyword)).toBe(true);
    expect(isSyntax(TokenClass.Function)).toBe(true);
    expect(isSyntax(TokenClass.Property)).toBe(true);
  });

  it("leaves plain and color tokens on the default color", () => {
    expect(isSyntax(TokenClass.Plain)).toBe(false);
    expect(isSyntax(TokenClass.Color)).toBe(false);
  });
});

describe("tokenizeMarkdown", () => {
  function markdownClassOf(tokens: Token[], text: string) {
    return tokens.find(token => token.text.includes(text))?.tokenClass;
  }

  it("leaves prose plain - 'use' and 'for' are words here, not keywords", () => {
    const tokens = tokenizeMarkdown("use pnpm for the extension framework.");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]?.tokenClass).toBe(TokenClass.Plain);
  });

  it("colors structure: headings, quotes, list markers, inline spans", () => {
    const tokens = tokenizeMarkdown(
      [
        "# Title",
        "> a quote",
        "- item with `code` and **bold** and [a link](https://x)"
      ].join("\n")
    );

    expect(markdownClassOf(tokens, "# Title")).toBe(TokenClass.Keyword);
    expect(markdownClassOf(tokens, "> a quote")).toBe(TokenClass.Comment);
    expect(markdownClassOf(tokens, "- ")).toBe(TokenClass.Number);
    expect(markdownClassOf(tokens, "`code`")).toBe(TokenClass.String);
    expect(markdownClassOf(tokens, "**bold**")).toBe(TokenClass.Keyword);
    expect(markdownClassOf(tokens, "[a link](https://x)")).toBe(TokenClass.Function);
    expect(markdownClassOf(tokens, "item with ")).toBe(TokenClass.Plain);
  });

  it("hands fenced code blocks to the generic scanner", () => {
    const tokens = tokenizeMarkdown(
      [
        "```ts",
        "const answer = 42;",
        "```"
      ].join("\n")
    );

    expect(markdownClassOf(tokens, "```ts")).toBe(TokenClass.Comment);
    expect(markdownClassOf(tokens, "const")).toBe(TokenClass.Keyword);
    expect(markdownClassOf(tokens, "42")).toBe(TokenClass.Number);
  });

  it("round-trips the exact text, newlines included", () => {
    const source = "# H\n\ntext `x`\n";
    const rebuilt = tokenizeMarkdown(source).map(token => token.text).join("");
    expect(rebuilt).toBe(source);
  });
});
