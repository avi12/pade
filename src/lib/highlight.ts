// Lightweight, dependency-free syntax highlighting for the code / config / diff
// viewers. Language-agnostic: it classifies the token shapes common across the
// languages PADE shows (JSON, CSS, JS/TS, Rust, Markdown, TOML, shell) - comments,
// strings, numbers, keywords - and hands color values to the swatch resolver.
// Good-enough highlighting without a heavyweight grammar dependency (principle #10).

import { resolveColor } from "@/lib/colors";

/** The closed set of roles a token can play; each value is also the themed CSS
 *  class ColorText paints it with. */
const TokenClass = {
  Comment: "comment",
  String: "string",
  Number: "number",
  Keyword: "keyword",
  Function: "function",
  Property: "property",
  Color: "color",
  Plain: "plain"
} as const;
export type TokenClass = (typeof TokenClass)[keyof typeof TokenClass];

export type Token = {
  text: string;
  cls: TokenClass;
  /** Resolved swatch color, for `color` tokens. */
  color?: string;
};

/** The classes that carry a syntax color - plain text and color tokens keep the
 *  default color. */
const SYNTAX_CLASSES: ReadonlySet<TokenClass> = new Set([
  TokenClass.Comment,
  TokenClass.String,
  TokenClass.Number,
  TokenClass.Keyword,
  TokenClass.Function,
  TokenClass.Property
]);

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

const RGB_HSL_FUNCTION = /^(?:rgba?|hsla?)\(/;
const COMMENT_OPENERS = ["//", "/*", "<!--"] as const;
const HASH_COMMENT = /^#+[ \t]/;
const QUOTES = new Set(["\"", "'", "`"]);
const DIGIT = /^\d/;

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

function plain(text: string) {
  return {
    text,
    cls: TokenClass.Plain
  };
}

/** How a scanner match is classified: the first rule whose test passes names
 *  its class, so the order is the precedence (a `# heading` is a comment before
 *  `#` can read as a hex colour). Anything no rule claims is plain. */
const MATCH_RULES: readonly {
  test: (raw: string) => boolean;
  cls: TokenClass;
}[] = [
  {
    test: raw => COMMENT_OPENERS.some(opener => raw.startsWith(opener)) || HASH_COMMENT.test(raw),
    cls: TokenClass.Comment
  },
  {
    test: raw => QUOTES.has(raw[0]),
    cls: TokenClass.String
  },
  {
    test: raw => raw.startsWith("#") || RGB_HSL_FUNCTION.test(raw) || raw.startsWith("var("),
    cls: TokenClass.Color
  },
  {
    test: raw => raw.startsWith("@") || KEYWORDS.has(raw),
    cls: TokenClass.Keyword
  },
  {
    test: raw => DIGIT.test(raw),
    cls: TokenClass.Number
  }
];

function classOfMatch(raw: string) {
  return MATCH_RULES.find(rule => rule.test(raw))?.cls ?? TokenClass.Plain;
}

function classify(raw: string, variables?: Map<string, string>) {
  const cls = classOfMatch(raw);
  if (cls === TokenClass.Color) {
    return {
      text: raw,
      cls,
      color: resolveColor(raw, variables) ?? undefined
    };
  }

  return {
    text: raw,
    cls
  };
}

/** A plain identifier takes a role from what immediately follows: `foo(` is a
 *  call; `foo:` a property/key (but not a `::` pseudo-element or a `://` scheme).
 *  This is what makes a CSS diff read - its property names and calls colour, not
 *  just the values that already carry a swatch. */
function identifierRole({ text, end }: {
  text: string;
  end: number;
}) {
  const next = text[end];
  if (next === "(") {
    return TokenClass.Function;
  }

  const isPropertyKey = next === ":" && text[end + 1] !== ":" && text[end + 1] !== "/";
  return isPropertyKey ? TokenClass.Property : TokenClass.Plain;
}

/** Split a snippet (a line, or a whole file) into highlighted tokens; color
 *  tokens carry a resolved swatch. Gaps between matches are plain runs. */
export function tokenize(text: string, variables?: Map<string, string>) {
  const tokens: Token[] = [];
  let previousEnd = 0;

  for (const match of text.matchAll(SCANNER)) {
    const start = match.index ?? 0;
    if (start > previousEnd) {
      tokens.push(plain(text.slice(previousEnd, start)));
    }

    const token: Token = classify(match[0], variables);
    const end = start + match[0].length;
    if (token.cls === TokenClass.Plain) {
      token.cls = identifierRole({
        text,
        end
      });
    }

    tokens.push(token);
    previousEnd = end;
  }

  if (previousEnd < text.length) {
    tokens.push(plain(text.slice(previousEnd)));
  }

  if (tokens.length === 0) {
    tokens.push(plain(text));
  }

  return tokens;
}

// ── Markdown ──────────────────────────────────────────────────────────────────
// Prose is not code: running the generic scanner over a README colors every
// "use", "for" and "type" like a keyword and every word before a colon like a
// CSS property. Markdown gets its own line-based pass that colors STRUCTURE -
// headings, fences, quotes, list markers, inline code/bold/links - and leaves
// the prose itself plain. Fenced code blocks fall through to the generic
// scanner, so a ```ts block still reads like code.

const FENCE_LINE = /^\s*(?:```|~~~)/;
const HEADING_LINE = /^#{1,6}[ \t]/;
const BLOCKQUOTE_LINE = /^\s*>/;
const LIST_MARKER = /^(\s*)([-*+]|\d+\.)([ \t])/;
const INLINE_SPAN = /`[^`\n]+`|\*\*[^*\n]+\*\*|\[[^\]\n]+\]\([^)\n]+\)/g;

function inlineSpanClass(span: string) {
  if (span.startsWith("`")) {
    return TokenClass.String;
  }

  return span.startsWith("**") ? TokenClass.Keyword : TokenClass.Function;
}

/** Tokens for one line of markdown prose: inline code spans read as strings,
 *  bold as keywords, links as functions; everything between stays plain. */
function markdownProseTokens(line: string) {
  const tokens: Token[] = [];
  let previousEnd = 0;
  for (const match of line.matchAll(INLINE_SPAN)) {
    const start = match.index ?? 0;
    if (start > previousEnd) {
      tokens.push(plain(line.slice(previousEnd, start)));
    }

    const span = match[0];
    tokens.push({
      text: span,
      cls: inlineSpanClass(span)
    });
    previousEnd = start + span.length;
  }

  if (previousEnd < line.length) {
    tokens.push(plain(line.slice(previousEnd)));
  }

  return tokens;
}

/** The tokens of one markdown line outside a fence. */
function markdownLineTokens(line: string) {
  if (HEADING_LINE.test(line)) {
    return [{
      text: line,
      cls: TokenClass.Keyword
    }];
  }

  if (BLOCKQUOTE_LINE.test(line)) {
    return [{
      text: line,
      cls: TokenClass.Comment
    }];
  }

  const listMarker = LIST_MARKER.exec(line);
  if (!listMarker) {
    return markdownProseTokens(line);
  }

  return [
    {
      text: listMarker[0],
      cls: TokenClass.Number
    },
    ...markdownProseTokens(line.slice(listMarker[0].length))
  ];
}

/** Split a markdown document into highlighted tokens (structure-only coloring;
 *  fenced code blocks are handed to the generic scanner). */
export function tokenizeMarkdown(text: string, variables?: Map<string, string>) {
  const tokens: Token[] = [];
  let isInFence = false;

  for (const [i, line] of text.split("\n").entries()) {
    if (i > 0) {
      tokens.push(plain("\n"));
    }

    if (FENCE_LINE.test(line)) {
      isInFence = !isInFence;
      tokens.push({
        text: line,
        cls: TokenClass.Comment
      });
      continue;
    }

    tokens.push(...(isInFence ? tokenize(line, variables) : markdownLineTokens(line)));
  }

  if (tokens.length === 0) {
    tokens.push(plain(text));
  }

  return tokens;
}

/** Whether a token class carries a syntax color (applied via a themed CSS class
 *  in ColorText - plain text and color tokens keep the default color). */
export function isSyntax(cls: TokenClass) {
  return SYNTAX_CLASSES.has(cls);
}
