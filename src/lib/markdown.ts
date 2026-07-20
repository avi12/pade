// A focused CommonMark-subset renderer: markdown text → a safe HTML fragment,
// plus `markdownDocument`, which wraps that fragment in a self-contained,
// sandbox-ready HTML document for the Change Feed's Preview iframe.
//
// This is the web translation of PowerToys' Markdown preview handler, which
// renders with Markdig (advanced extensions: tables, autolinks, task lists) and
// displays the result in a locked-down WebView2 (scripts disabled, all external
// requests refused, external images blocked). PADE has no equivalent .NET
// library, so it translates the *approach*, not the code:
//   • Parser — a hand-rolled subset rather than a new dependency. CLAUDE.md
//     rule 10 admits a third-party parser only when it is already load-bearing;
//     a markdown library would be brand-new supply-chain surface, so — as with
//     the repo's hand-rolled base64 and gitignore parser — the subset is
//     authored here. It covers the CommonMark structures a project's docs use:
//     headings, emphasis, inline/fenced code, lists, links, images, blockquotes,
//     rules, paragraphs, and GFM pipe tables (which PowerToys enables too).
//   • Safety — every run of source text is HTML-escaped before it reaches the
//     output, so raw HTML embedded in the markdown is shown as text, never
//     injected as markup (the equivalent of Markdig's `html: false`). Link and
//     image URLs are scheme-checked, so `javascript:`/`vbscript:` can't ride in.
//   • Isolation — the fragment is never placed in the app DOM. It rides in a
//     sandboxed iframe (see ChangeFeed), and `markdownDocument` stamps the srcdoc
//     with a restrictive CSP (`default-src 'none'; img-src data:`) so, exactly
//     like PowerToys, no script runs and no external image or font loads.

/** Escape the five characters that would otherwise be read as HTML markup, so a
 *  run of source text renders as literal text. Ampersand first, so the entity
 *  replacements it introduces aren't re-escaped. */
function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\"", "&quot;")
    .replaceAll("'", "&#39;");
}

/** URL schemes that carry executable content — never allowed in a rendered
 *  link or image, so a crafted document can't smuggle a script in via `href`. */
const DANGEROUS_SCHEME = /^\s*(javascript|vbscript|data|file):/i;

/** A link/image URL made safe for an attribute: a dangerous scheme collapses to
 *  `#`, and the survivor is HTML-escaped. `data:` is refused for links (only the
 *  image renderer, and only the CSP-permitted `img-src data:`, may use one). */
function safeUrl(url: string): string {
  const trimmed = url.trim();
  if (DANGEROUS_SCHEME.test(trimmed)) {
    return "#";
  }

  return escapeHtml(trimmed);
}

/** Turn a bare `http(s)://…` run into a link (PowerToys enables autolinking).
 *  Runs on already-escaped text, and its `[^\s<]+` stops at the first `<`, so it
 *  never reaches into an emphasis tag or an existing anchor. */
function linkify(escaped: string): string {
  return escaped.replaceAll(
    /(https?:\/\/[^\s<]+)/g,
    match => `<a href="${match}">${match}</a>`
  );
}

/** Inline emphasis/strikethrough on a run of plain (link-free) text: escape it
 *  first, then apply the delimiter forms, then autolink. Strong before emphasis
 *  so `**bold**` isn't first eaten as two `*italic*` markers. */
function formatPlain(text: string): string {
  let html = escapeHtml(text);
  html = html.replaceAll(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replaceAll(/__([^_]+)__/g, "<strong>$1</strong>");
  html = html.replaceAll(/(^|[^*])\*([^*\s][^*]*?)\*/g, "$1<em>$2</em>");
  html = html.replaceAll(/\b_([^_\s][^_]*?)_\b/g, "<em>$1</em>");
  html = html.replaceAll(/~~([^~]+)~~/g, "<del>$1</del>");
  return linkify(html);
}

/** A link/image span `[label](url)` or `![alt](url)`, tolerating a trailing
 *  `"title"` inside the parens (captured, then dropped). */
const LINK = /(!?)\[([^\]]*)\]\(\s*([^)\s]*)(?:\s+"[^"]*")?\s*\)/g;

/** Inline markup for a run of text that sits *outside* any code span: link and
 *  image spans become anchors/images with scheme-checked URLs, and the text
 *  between them is emphasis-formatted. */
function formatText(text: string): string {
  let out = "";
  let last = 0;
  LINK.lastIndex = 0;
  for (let match = LINK.exec(text); match !== null; match = LINK.exec(text)) {
    out += formatPlain(text.slice(last, match.index));
    const isImage = match[1] === "!";
    const label = match[2];
    const url = safeUrl(match[3]);
    if (isImage) {
      out += `<img src="${url}" alt="${escapeHtml(label)}">`;
    } else {
      out += `<a href="${url}">${formatPlain(label)}</a>`;
    }

    last = LINK.lastIndex;
  }

  out += formatPlain(text.slice(last));
  return out;
}

/** Full inline rendering: split off code spans first — their content is only
 *  escaped, never emphasis- or link-formatted — then format the rest. A run of
 *  N backticks opens a span that the next run of exactly N backticks closes. */
function parseInline(text: string): string {
  let out = "";
  let i = 0;
  while (i < text.length) {
    if (text[i] !== "`") {
      const nextTick = text.indexOf("`", i);
      const end = nextTick === -1 ? text.length : nextTick;
      out += formatText(text.slice(i, end));
      i = end;
      continue;
    }

    let fenceLength = 0;
    while (text[i + fenceLength] === "`") {
      fenceLength += 1;
    }

    const fence = "`".repeat(fenceLength);
    const closeAt = text.indexOf(fence, i + fenceLength);
    if (closeAt === -1) {
      out += formatText(fence);
      i += fenceLength;
      continue;
    }

    const code = text.slice(i + fenceLength, closeAt).trim();
    out += `<code>${escapeHtml(code)}</code>`;
    i = closeAt + fenceLength;
  }

  return out;
}

const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const HORIZONTAL_RULE = /^ {0,3}([-*_])( *\1){2,} *$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const BLOCKQUOTE = /^ {0,3}>\s?(.*)$/;
const LIST_ITEM = /^( *)([-*+]|\d+[.)])\s+(.*)$/;
const TABLE_DELIMITER = /^ *\|?[ :]*-+[ :]*(\|[ :]*-+[ :]*)*\|? *$/;

/** A cursor over the source lines, advanced by each block reader. A tiny class
 *  keeps the shared index out of every reader's signature. */
class Cursor {
  private index = 0;

  constructor(private readonly lines: string[]) {}

  get done(): boolean {
    return this.index >= this.lines.length;
  }

  peek(): string {
    return this.lines[this.index] ?? "";
  }

  ahead(offset: number): string | undefined {
    return this.lines[this.index + offset];
  }

  take(): string {
    const line = this.peek();
    this.index += 1;
    return line;
  }

  skip(): void {
    this.index += 1;
  }
}

/** Split a GFM table row into its cell texts, dropping the optional leading and
 *  trailing pipes and honoring `\|` as a literal pipe inside a cell. */
function tableCells(row: string): string[] {
  const trimmed = row.trim().replace(/^\|/, "").replace(/\|$/, "");
  const cells: string[] = [];
  let current = "";
  for (let i = 0; i < trimmed.length; i += 1) {
    if (trimmed[i] === "\\" && trimmed[i + 1] === "|") {
      current += "|";
      i += 1;
      continue;
    }

    if (trimmed[i] === "|") {
      cells.push(current.trim());
      current = "";
      continue;
    }

    current += trimmed[i];
  }

  cells.push(current.trim());
  return cells;
}

function renderTable(cursor: Cursor): string {
  const headerCells = tableCells(cursor.take());
  cursor.skip(); // the delimiter row
  const header = headerCells.map(cell => `<th>${parseInline(cell)}</th>`).join("");
  let body = "";
  while (!cursor.done && cursor.peek().includes("|") && cursor.peek().trim() !== "") {
    const cells = tableCells(cursor.take());
    body += `<tr>${cells.map(cell => `<td>${parseInline(cell)}</td>`).join("")}</tr>`;
  }

  return `<table><thead><tr>${header}</tr></thead><tbody>${body}</tbody></table>`;
}

interface FenceRenderInput {
  cursor: Cursor;
  fence: string;
}

function renderFence({ cursor, fence }: FenceRenderInput): string {
  cursor.skip(); // opening fence
  const marker = fence[0];
  const body: string[] = [];
  while (!cursor.done) {
    const line = cursor.peek();
    if (line.trimStart().startsWith(marker.repeat(3))) {
      cursor.skip();
      break;
    }

    body.push(cursor.take());
  }

  return `<pre><code>${escapeHtml(body.join("\n"))}</code></pre>`;
}

function renderBlockquote(cursor: Cursor): string {
  const inner: string[] = [];
  while (!cursor.done && BLOCKQUOTE.test(cursor.peek())) {
    inner.push(cursor.take().replace(BLOCKQUOTE, "$1"));
  }

  return `<blockquote>${renderBlocks(inner)}</blockquote>`;
}

/** The whitespace width a nested block must clear to belong to the current list
 *  item: the item's own indent plus its marker and the space after it. */
interface ListItemIndentInput {
  indent: string;
  marker: string;
}

function itemContentIndent({ indent, marker }: ListItemIndentInput): number {
  return indent.length + marker.length + 1;
}

/** The lines belonging to one list item beyond its first: continuation and
 *  nested-block lines dedented to the item's content column, stopping at a blank
 *  line or the next same-or-shallower list marker. Advances `cursor` past them. */
interface ItemLineCollectionInput {
  contentIndent: number;
  cursor: Cursor;
  firstLine: string;
}

function collectItemLines({ cursor, firstLine, contentIndent }: ItemLineCollectionInput): string[] {
  const itemLines: string[] = [firstLine];
  cursor.skip();
  while (!cursor.done) {
    const line = cursor.peek();
    const startsNextItem =
      LIST_ITEM.test(line) && (line.length - line.trimStart().length) < contentIndent;
    if (line.trim() === "" || startsNextItem) {
      break;
    }

    itemLines.push(line.slice(contentIndent));
    cursor.skip();
  }

  return itemLines;
}

function renderList(cursor: Cursor): string {
  const opener = LIST_ITEM.exec(cursor.peek());
  if (!opener) {
    return "";
  }

  const ordered = /\d/.test(opener[2]);
  const items: string[] = [];
  while (!cursor.done) {
    const match = LIST_ITEM.exec(cursor.peek());
    if (!match) {
      break;
    }

    const contentIndent = itemContentIndent({
      indent: match[1],
      marker: match[2]
    });
    const itemLines = collectItemLines({
      cursor,
      firstLine: match[3],
      contentIndent
    });
    const body = itemLines.length > 1 ? renderBlocks(itemLines) : parseInline(itemLines[0]);
    items.push(`<li>${body}</li>`);
  }

  const tag = ordered ? "ol" : "ul";
  return `<${tag}>${items.join("")}</${tag}>`;
}

function renderParagraph(cursor: Cursor): string {
  const lines: string[] = [];
  while (!cursor.done) {
    const line = cursor.peek();
    const endsParagraph =
      line.trim() === "" ||
      HEADING.test(line) ||
      HORIZONTAL_RULE.test(line) ||
      FENCE.test(line) ||
      BLOCKQUOTE.test(line) ||
      LIST_ITEM.test(line) ||
      startsTable(cursor);
    if (endsParagraph) {
      break;
    }

    lines.push(cursor.take());
  }

  return `<p>${parseInline(lines.join("\n"))}</p>`;
}

/** Whether the line at the cursor opens a GFM table (a header row followed by a
 *  `---|---` delimiter row with a matching pipe shape). */
function startsTable(cursor: Cursor): boolean {
  const header = cursor.peek();
  const delimiter = cursor.ahead(1);
  return (
    header.includes("|") &&
    delimiter !== undefined &&
    TABLE_DELIMITER.test(delimiter) &&
    delimiter.includes("-")
  );
}

function renderBlocks(lines: string[]): string {
  const cursor = new Cursor(lines);
  let html = "";
  while (!cursor.done) {
    const line = cursor.peek();
    if (line.trim() === "") {
      cursor.skip();
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      html += renderFence({
        cursor,
        fence: fence[1]
      });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      const level = heading[1].length;
      html += `<h${level}>${parseInline(heading[2])}</h${level}>`;
      cursor.skip();
      continue;
    }

    if (HORIZONTAL_RULE.test(line)) {
      html += "<hr>";
      cursor.skip();
      continue;
    }

    if (BLOCKQUOTE.test(line)) {
      html += renderBlockquote(cursor);
      continue;
    }

    if (startsTable(cursor)) {
      html += renderTable(cursor);
      continue;
    }

    if (LIST_ITEM.test(line)) {
      html += renderList(cursor);
      continue;
    }

    html += renderParagraph(cursor);
  }

  return html;
}

/** Render markdown `source` to a safe HTML fragment (no document scaffolding).
 *  Every text run is HTML-escaped, so embedded raw HTML is shown, not executed;
 *  the fragment is only ever mounted inside the sandboxed Preview iframe. */
export function renderMarkdown(source: string): string {
  const lines = source.replaceAll("\r\n", "\n").replaceAll("\r", "\n").split("\n");
  return renderBlocks(lines);
}

/** The vendored, GitHub-flavoured stylesheet the Preview iframe wears — modelled
 *  on PowerToys' bundled markdown CSS, retuned to PADE's M3 palette and made
 *  theme-aware with `prefers-color-scheme` (the srcdoc is an isolated document
 *  and can't read the parent app's tokens, so the colours are declared here). */
const MARKDOWN_STYLE = `
  :root {
    color-scheme: light dark;
    --md-surface: #fdfcff;
    --md-on-surface: #1a1c1e;
    --md-muted: #43474e;
    --md-code-surface: #f1f2f6;
    --md-border: #c3c6cf;
    --md-primary: #2b6bd6;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --md-surface: #1a1c1e;
      --md-on-surface: #e3e2e6;
      --md-muted: #c3c6cf;
      --md-code-surface: #26282b;
      --md-border: #43474e;
      --md-primary: #a9c7ff;
    }
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    padding: 20px;
    background: var(--md-surface);
    color: var(--md-on-surface);
    font-family: "Google Sans", Roboto, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 14px;
    line-height: 1.6;
    overflow-wrap: anywhere;
  }
  h1, h2, h3, h4, h5, h6 { margin: 24px 0 16px; font-weight: 600; line-height: 1.25; }
  h1, h2 { padding-bottom: 0.3em; border-bottom: 1px solid var(--md-border); }
  h1 { font-size: 1.7em; }
  h2 { font-size: 1.4em; }
  h3 { font-size: 1.2em; }
  h4 { font-size: 1em; }
  h5 { font-size: 0.9em; }
  h6 { font-size: 0.85em; color: var(--md-muted); }
  :first-child { margin-top: 0; }
  a { color: var(--md-primary); text-decoration: none; }
  a:hover { text-decoration: underline; }
  p { margin: 0 0 16px; }
  strong { font-weight: 600; }
  code {
    padding: 0.2em 0.4em;
    border-radius: 6px;
    background: var(--md-code-surface);
    font-family: "Roboto Mono", ui-monospace, SFMono-Regular, Consolas, monospace;
    font-size: 0.9em;
  }
  pre {
    margin: 0 0 16px;
    padding: 14px 16px;
    border-radius: 12px;
    background: var(--md-code-surface);
    overflow-x: auto;
  }
  pre code { padding: 0; border-radius: 0; background: none; font-size: 0.85em; }
  blockquote {
    margin: 0 0 16px;
    padding: 0 1em;
    border-inline-start: 3px solid var(--md-border);
    color: var(--md-muted);
  }
  ul, ol { margin: 0 0 16px; padding-inline-start: 1.6em; }
  li { margin: 4px 0; }
  hr { margin: 24px 0; border: 0; border-top: 1px solid var(--md-border); }
  img { max-width: 100%; height: auto; border-radius: 8px; }
  table { margin: 0 0 16px; border-collapse: collapse; overflow: auto; display: block; }
  th, td { padding: 6px 13px; border: 1px solid var(--md-border); }
  th { font-weight: 600; background: var(--md-code-surface); }
`;

/** Wrap a rendered markdown fragment in a self-contained HTML document for the
 *  Preview iframe's `srcdoc`. Everything is inlined (the strict CSP forbids
 *  external refs), and the CSP `<meta>` — the belt to the sandbox's braces —
 *  refuses every request except inline styles and `data:` images, so no script
 *  runs and no external image or font loads (PowerToys' locked-down WebView2,
 *  in a `<meta>`). */
export function markdownDocument(source: string): string {
  const body = renderMarkdown(source);
  const contentSecurityPolicy =
    "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:";
  return (
    "<!doctype html><html><head><meta charset=\"utf-8\">" +
    `<meta http-equiv="Content-Security-Policy" content="${contentSecurityPolicy}">` +
    `<style>${MARKDOWN_STYLE}</style></head><body>${body}</body></html>`
  );
}
