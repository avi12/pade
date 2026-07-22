// Lightweight, dependency-free syntax highlighting for the code / config / diff
// viewers. Language-agnostic: it classifies the token shapes common across the
// languages PADE shows (JSON, CSS, JS/TS, Rust, Markdown, TOML, shell) — comments,
// strings, numbers, keywords — and hands color values to the swatch resolver.
// Good-enough highlighting without a heavyweight grammar dependency (principle #10).

import { resolveColor } from "@/lib/colors";

export type TokenClass = "comment" | "string" | "number" | "keyword" | "function" | "property" | "color" | "plain";

export type Token = {
  text: string;
  cls: TokenClass;
  /** Resolved swatch color, for `color` tokens. */
  color?: string;
};

// A broad, cross-language keyword set. Mis-coloring an identifier that happens to
// be a keyword in another language is a cosmetic-only cost, so this stays generous.
const KEYWORDS = new Set([
  "const", "let", "var", "function", "fn", "def", "return", "if", "else", "elif",
  "for", "while", "loop", "match", "switch", "case", "default", "break", "continue",
  "import", "export", "from", "use", "pub", "mod", "package", "require",
  "class", "struct", "enum", "interface", "type", "trait", "impl", "new",
  "extends", "implements", "async", "await", "yield", "in", "of", "as", "is",
  "and", "or", "not", "this", "self", "super", "void", "static", "public",
  "private", "protected", "throw", "try", "catch", "finally", "do", "then", "end",
  "true", "false", "null", "undefined", "True", "False", "None"
]);

const RGB_HSL_FN = /^(?:rgba?|hsla?)\(/;

const SCANNER = new RegExp(
  [
    "\\/\\/[^\\n]*", // // line comment
    "\\/\\*[\\s\\S]*?\\*\\/", // /* block comment */
    "<!--[\\s\\S]*?-->", // <!-- html/md comment -->
    "#+[ \\t][^\\n]*", // # shell/toml comment · md heading
    "\"(?:\\\\.|[^\"\\\\])*\"", // "double string"
    "'(?:\\\\.|[^'\\\\])*'", // 'single string'
    "`(?:\\\\.|[^`\\\\])*`", // `template string`
    "#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})\\b", // hex color
    "\\b(?:rgb|hsl)a?\\([^)]*\\)", // rgb()/hsl() color
    "var\\(\\s*--[\\w-]+\\s*\\)", // var(--x) color
    "@[\\w-]+", // @at-rule / decorator
    "\\b\\d[\\d_]*(?:\\.\\d+)?\\b", // number
    "[A-Za-z_$][\\w$-]*" // identifier / keyword / function (hyphens kept whole so
    //                     `mask-image`, `linear-gradient` stay one token)
  ].join("|"),
  "g"
);

function classify(raw: string, vars?: Map<string, string>): Token {
  const head = raw[0];
  if (raw.startsWith("//") || raw.startsWith("/*") || raw.startsWith("<!--") || /^#+[ \t]/.test(raw)) {
    return {
      text: raw,
      cls: "comment"
    };
  }

  if (head === "\"" || head === "'" || head === "`") {
    return {
      text: raw,
      cls: "string"
    };
  }

  if (head === "#" || RGB_HSL_FN.test(raw) || raw.startsWith("var(")) {
    return {
      text: raw,
      cls: "color",
      color: resolveColor(raw, vars) ?? undefined
    };
  }

  if (head === "@") {
    return {
      text: raw,
      cls: "keyword"
    };
  }

  const code = raw.charCodeAt(0);
  if (code >= 48 && code <= 57) {
    return {
      text: raw,
      cls: "number"
    };
  }

  if (KEYWORDS.has(raw)) {
    return {
      text: raw,
      cls: "keyword"
    };
  }

  return {
    text: raw,
    cls: "plain"
  };
}

/** Split a snippet (a line, or a whole file) into highlighted tokens; color
 *  tokens carry a resolved swatch. Gaps between matches are plain runs. */
export function tokenize(text: string, vars?: Map<string, string>): Token[] {
  const tokens: Token[] = [];
  let last = 0;

  for (const match of text.matchAll(SCANNER)) {
    const start = match.index ?? 0;
    if (start > last) {
      tokens.push({
        text: text.slice(last, start),
        cls: "plain"
      });
    }

    const token = classify(match[0], vars);
    const end = start + match[0].length;
    // A plain identifier takes a role from what immediately follows: `foo(` is a
    // call; `foo:` a property/key (but not a `::` pseudo-element or a `://`
    // scheme). This is what makes a CSS diff read — its property names and calls
    // colour, not just the values that already carry a swatch.
    if (token.cls === "plain") {
      const next = text[end];
      if (next === "(") {
        token.cls = "function";
      } else if (next === ":" && text[end + 1] !== ":" && text[end + 1] !== "/") {
        token.cls = "property";
      }
    }

    tokens.push(token);
    last = end;
  }

  if (last < text.length) {
    tokens.push({
      text: text.slice(last),
      cls: "plain"
    });
  }

  return tokens.length > 0 ? tokens : [{
    text,
    cls: "plain"
  }];
}

// ── Markdown ──────────────────────────────────────────────────────────────────
// Prose is not code: running the generic scanner over a README colors every
// "use", "for" and "type" like a keyword and every word before a colon like a
// CSS property. Markdown gets its own line-based pass that colors STRUCTURE —
// headings, fences, quotes, list markers, inline code/bold/links — and leaves
// the prose itself plain. Fenced code blocks fall through to the generic
// scanner, so a ```ts block still reads like code.

const FENCE_LINE = /^\s*(?:```|~~~)/;
const HEADING_LINE = /^#{1,6}[ \t]/;
const BLOCKQUOTE_LINE = /^\s*>/;
const LIST_MARKER = /^(\s*)([-*+]|\d+\.)([ \t])/;
const INLINE_SPAN = /`[^`\n]+`|\*\*[^*\n]+\*\*|\[[^\]\n]+\]\([^)\n]+\)/g;

/** Tokens for one line of markdown prose: inline code spans read as strings,
 *  bold as keywords, links as functions; everything between stays plain. */
function markdownProseTokens(line: string): Token[] {
  const tokens: Token[] = [];
  let last = 0;
  for (const match of line.matchAll(INLINE_SPAN)) {
    const start = match.index ?? 0;
    if (start > last) {
      tokens.push({
        text: line.slice(last, start),
        cls: "plain"
      });
    }

    const span = match[0];
    let cls: TokenClass = "function";
    if (span.startsWith("`")) {
      cls = "string";
    } else if (span.startsWith("**")) {
      cls = "keyword";
    }

    tokens.push({
      text: span,
      cls
    });
    last = start + span.length;
  }

  if (last < line.length) {
    tokens.push({
      text: line.slice(last),
      cls: "plain"
    });
  }

  return tokens;
}

/** Split a markdown document into highlighted tokens (structure-only coloring;
 *  fenced code blocks are handed to the generic scanner). */
export function tokenizeMarkdown(text: string, vars?: Map<string, string>): Token[] {
  const tokens: Token[] = [];
  let inFence = false;

  const lines = text.split("\n");
  for (const [index, line] of lines.entries()) {
    if (FENCE_LINE.test(line)) {
      inFence = !inFence;
      tokens.push({
        text: line,
        cls: "comment"
      });
    } else if (inFence) {
      tokens.push(...tokenize(line, vars));
    } else if (HEADING_LINE.test(line)) {
      tokens.push({
        text: line,
        cls: "keyword"
      });
    } else if (BLOCKQUOTE_LINE.test(line)) {
      tokens.push({
        text: line,
        cls: "comment"
      });
    } else {
      const listMarker = LIST_MARKER.exec(line);
      if (listMarker) {
        tokens.push({
          text: listMarker[0],
          cls: "number"
        });
        tokens.push(...markdownProseTokens(line.slice(listMarker[0].length)));
      } else {
        tokens.push(...markdownProseTokens(line));
      }
    }

    const isLastLine = index === lines.length - 1;
    if (!isLastLine) {
      tokens.push({
        text: "\n",
        cls: "plain"
      });
    }
  }

  return tokens.length > 0 ? tokens : [{
    text,
    cls: "plain"
  }];
}

/** Whether a token class carries a syntax color (applied via a themed CSS class
 *  in ColorText — plain text and color tokens keep the default color). */
export function isSyntax(cls: TokenClass): boolean {
  return (
    cls === "comment"
    || cls === "string"
    || cls === "number"
    || cls === "keyword"
    || cls === "function"
    || cls === "property"
  );
}
