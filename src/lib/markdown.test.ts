import { markdownDocument, renderMarkdown } from "@/lib/markdown";
import { describe, expect, it } from "vitest";

describe("renderMarkdown block structures", () => {
  it("renders ATX headings at their level", () => {
    expect(renderMarkdown("# Title")).toBe("<h1>Title</h1>");
    expect(renderMarkdown("### Deep")).toBe("<h3>Deep</h3>");
  });

  it("wraps loose text in a paragraph", () => {
    expect(renderMarkdown("just text")).toBe("<p>just text</p>");
  });

  it("renders a horizontal rule", () => {
    expect(renderMarkdown("---")).toBe("<hr>");
  });

  it("renders unordered and ordered lists", () => {
    expect(renderMarkdown("- a\n- b")).toBe("<ul><li>a</li><li>b</li></ul>");
    expect(renderMarkdown("1. one\n2. two")).toBe("<ol><li>one</li><li>two</li></ol>");
  });

  it("renders a blockquote around its inner blocks", () => {
    expect(renderMarkdown("> quoted")).toBe("<blockquote><p>quoted</p></blockquote>");
  });

  it("renders a GFM pipe table", () => {
    const table = renderMarkdown("| A | B |\n| --- | --- |\n| 1 | 2 |");
    expect(table).toBe(
      "<table><thead><tr><th>A</th><th>B</th></tr></thead>" +
        "<tbody><tr><td>1</td><td>2</td></tr></tbody></table>"
    );
  });
});

describe("renderMarkdown inline formatting", () => {
  it("renders strong, emphasis, and strikethrough", () => {
    expect(renderMarkdown("**bold**")).toBe("<p><strong>bold</strong></p>");
    expect(renderMarkdown("*it*")).toBe("<p><em>it</em></p>");
    expect(renderMarkdown("~~gone~~")).toBe("<p><del>gone</del></p>");
  });

  it("leaves snake_case words untouched", () => {
    expect(renderMarkdown("call read_preview_text now")).toBe(
      "<p>call read_preview_text now</p>"
    );
  });

  it("renders inline code without further formatting inside it", () => {
    expect(renderMarkdown("use `**not bold**` here")).toBe(
      "<p>use <code>**not bold**</code> here</p>"
    );
  });

  it("renders links and autolinks", () => {
    expect(renderMarkdown("[site](https://example.com)")).toBe(
      "<p><a href=\"https://example.com\">site</a></p>"
    );
    expect(renderMarkdown("see https://example.com now")).toContain(
      "<a href=\"https://example.com\">https://example.com</a>"
    );
  });
});

describe("renderMarkdown safety", () => {
  it("escapes raw HTML rather than passing it through", () => {
    const html = renderMarkdown("<script>alert(1)</script>");
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });

  it("escapes HTML inside fenced code and applies no inline formatting", () => {
    const html = renderMarkdown("```\n<b>**x**</b>\n```");
    expect(html).toBe("<pre><code>&lt;b&gt;**x**&lt;/b&gt;</code></pre>");
  });

  it("neutralises javascript: link URLs to #", () => {
    const html = renderMarkdown("[x](javascript:alert(1))");
    expect(html).toContain("href=\"#\"");
    expect(html).not.toContain("javascript:");
  });

  it("neutralises data: link URLs to # (only images may use data:)", () => {
    const html = renderMarkdown("[x](data:text/html,<script>1</script>)");
    expect(html).toContain("href=\"#\"");
  });
});

describe("markdownDocument", () => {
  it("wraps the fragment in a sandbox-ready document with a strict CSP", () => {
    const doc = markdownDocument("# Hi");
    expect(doc.startsWith("<!doctype html>")).toBe(true);
    expect(doc).toContain("http-equiv=\"Content-Security-Policy\"");
    expect(doc).toContain("default-src 'none'");
    expect(doc).toContain("img-src data:");
    expect(doc).toContain("<h1>Hi</h1>");
  });

  it("never emits a live script even from script-shaped source", () => {
    const doc = markdownDocument("<script>alert(1)</script>");
    expect(doc).not.toContain("<script>alert");
  });
});
