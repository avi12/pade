// Pure file-type badge for a Change Feed card: a file's extension → a short label,
// a colour tone, and (where the language has one) a brand-logo icon name.
// Language-agnostic — an unrecognised extension still yields a chip (its own
// uppercased extension), so every row is tagged even without a logo.

import type { IconName } from "@/lib/Icon.svelte";

/** The closed set of colour tones a badge can carry; each maps to a `.tone-*`
 *  class the card styles. One authoritative home for the tone names. */
export const FileTone = {
  TypeScript: "typescript",
  JavaScript: "javascript",
  Svelte: "svelte",
  Rust: "rust",
  Style: "style",
  Markup: "markup",
  Python: "python",
  Go: "go",
  Data: "data",
  Doc: "doc",
  Shell: "shell",
  Image: "image",
  Neutral: "neutral"
} as const;
export type FileTone = (typeof FileTone)[keyof typeof FileTone];

export interface FileTypeBadge {
  /** 2–4 char label shown in the chip (e.g. `TS`, `CSS`) — the fallback for a
   *  file type without a brand logo, and always present so every row is tagged. */
  label: string;
  tone: FileTone;
  /** Brand-logo icon (an `Icon.svelte` name) when the language has one; the card
   *  renders this in place of the text `label`. Absent for logoless types. */
  icon?: IconName;
}

// Extension → badge. The authoritative extension table; anything absent falls
// back to a neutral chip of the extension itself. Multi-colour marks (TS, JS,
// Python) carry brand fills in their SVG; single-colour marks (Rust, Go) and the
// hand-drawn format glyphs (JSON, YAML, TOML, Markdown, Shell, Image) are tinted
// by the card's `--tone`.
const BADGES: Record<string, FileTypeBadge> = {
  ts: {
    label: "TS",
    tone: FileTone.TypeScript,
    icon: "typescript"
  },
  tsx: {
    label: "TSX",
    tone: FileTone.TypeScript,
    icon: "typescript"
  },
  mts: {
    label: "TS",
    tone: FileTone.TypeScript,
    icon: "typescript"
  },
  cts: {
    label: "TS",
    tone: FileTone.TypeScript,
    icon: "typescript"
  },
  js: {
    label: "JS",
    tone: FileTone.JavaScript,
    icon: "javascript"
  },
  jsx: {
    label: "JSX",
    tone: FileTone.JavaScript,
    icon: "javascript"
  },
  mjs: {
    label: "JS",
    tone: FileTone.JavaScript,
    icon: "javascript"
  },
  cjs: {
    label: "JS",
    tone: FileTone.JavaScript,
    icon: "javascript"
  },
  svelte: {
    label: "SV",
    tone: FileTone.Svelte,
    icon: "svelte"
  },
  rs: {
    label: "RS",
    tone: FileTone.Rust,
    icon: "rust"
  },
  css: {
    label: "CSS",
    tone: FileTone.Style,
    icon: "css"
  },
  scss: {
    label: "SCSS",
    tone: FileTone.Style,
    icon: "css"
  },
  sass: {
    label: "SASS",
    tone: FileTone.Style,
    icon: "css"
  },
  html: {
    label: "HTML",
    tone: FileTone.Markup,
    icon: "html"
  },
  svg: {
    label: "SVG",
    tone: FileTone.Markup,
    icon: "image"
  },
  py: {
    label: "PY",
    tone: FileTone.Python,
    icon: "python"
  },
  go: {
    label: "GO",
    tone: FileTone.Go,
    icon: "go"
  },
  json: {
    label: "JSON",
    tone: FileTone.Data,
    icon: "json"
  },
  jsonc: {
    label: "JSON",
    tone: FileTone.Data,
    icon: "json"
  },
  toml: {
    label: "TOML",
    tone: FileTone.Data,
    icon: "toml"
  },
  yaml: {
    label: "YAML",
    tone: FileTone.Data,
    icon: "yaml"
  },
  yml: {
    label: "YAML",
    tone: FileTone.Data,
    icon: "yaml"
  },
  md: {
    label: "MD",
    tone: FileTone.Doc,
    icon: "markdown"
  },
  mdx: {
    label: "MDX",
    tone: FileTone.Doc,
    icon: "markdown"
  },
  txt: {
    label: "TXT",
    tone: FileTone.Doc
  },
  sh: {
    label: "SH",
    tone: FileTone.Shell,
    icon: "shell"
  },
  bash: {
    label: "SH",
    tone: FileTone.Shell,
    icon: "shell"
  },
  ps1: {
    label: "PS1",
    tone: FileTone.Shell,
    icon: "shell"
  },
  png: {
    label: "IMG",
    tone: FileTone.Image,
    icon: "image"
  },
  jpg: {
    label: "IMG",
    tone: FileTone.Image,
    icon: "image"
  },
  jpeg: {
    label: "IMG",
    tone: FileTone.Image,
    icon: "image"
  },
  gif: {
    label: "IMG",
    tone: FileTone.Image,
    icon: "image"
  },
  webp: {
    label: "IMG",
    tone: FileTone.Image,
    icon: "image"
  }
};

/** The badge for a file path — read from its extension. A dotfile (`.gitignore`)
 *  or extensionless file gets a neutral chip from its own name. */
export function fileTypeBadge(path: string): FileTypeBadge {
  const base = path.split(/[\\/]/).pop() ?? path;
  const dot = base.lastIndexOf(".");
  const hasExtension = dot > 0 && dot < base.length - 1;
  if (!hasExtension) {
    const stem = base.replace(/^\./, "").slice(0, 3).toUpperCase();
    return {
      label: stem.length > 0 ? stem : "FILE",
      tone: FileTone.Neutral
    };
  }

  const extension = base.slice(dot + 1).toLowerCase();
  return BADGES[extension] ?? {
    label: extension.slice(0, 4).toUpperCase(),
    tone: FileTone.Neutral
  };
}
