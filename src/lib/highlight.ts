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

interface ClassifyInput {
  raw: string;
  vars?: ReadonlyMap<string, string>;
}

function classify({ raw, vars }: ClassifyInput): Token {
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
      color: resolveColor({
        token: raw,
        vars
      }) ?? undefined
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
export interface TokenizeInput {
  text: string;
  vars?: ReadonlyMap<string, string>;
}

export function tokenize({ text, vars }: TokenizeInput): Token[] {
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

    const token = classify({
      raw: match[0],
      vars
    });
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
