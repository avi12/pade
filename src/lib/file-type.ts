// Pure file-type badge for a Change Feed card: a file's extension → a short label
// and a colour tone. Language-agnostic — an unrecognised extension still yields a
// chip (its own uppercased extension), so every row is tagged.

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
  /** 2–4 char label shown in the chip (e.g. `TS`, `CSS`). */
  label: string;
  tone: FileTone;
}

// Extension → badge. The authoritative extension table; anything absent falls
// back to a neutral chip of the extension itself.
const BADGES: Record<string, FileTypeBadge> = {
  ts: {
    label: "TS",
    tone: FileTone.TypeScript
  },
  tsx: {
    label: "TSX",
    tone: FileTone.TypeScript
  },
  mts: {
    label: "TS",
    tone: FileTone.TypeScript
  },
  cts: {
    label: "TS",
    tone: FileTone.TypeScript
  },
  js: {
    label: "JS",
    tone: FileTone.JavaScript
  },
  jsx: {
    label: "JSX",
    tone: FileTone.JavaScript
  },
  mjs: {
    label: "JS",
    tone: FileTone.JavaScript
  },
  cjs: {
    label: "JS",
    tone: FileTone.JavaScript
  },
  svelte: {
    label: "SV",
    tone: FileTone.Svelte
  },
  rs: {
    label: "RS",
    tone: FileTone.Rust
  },
  css: {
    label: "CSS",
    tone: FileTone.Style
  },
  scss: {
    label: "SCSS",
    tone: FileTone.Style
  },
  sass: {
    label: "SASS",
    tone: FileTone.Style
  },
  html: {
    label: "HTML",
    tone: FileTone.Markup
  },
  svg: {
    label: "SVG",
    tone: FileTone.Markup
  },
  py: {
    label: "PY",
    tone: FileTone.Python
  },
  go: {
    label: "GO",
    tone: FileTone.Go
  },
  json: {
    label: "JSON",
    tone: FileTone.Data
  },
  jsonc: {
    label: "JSON",
    tone: FileTone.Data
  },
  toml: {
    label: "TOML",
    tone: FileTone.Data
  },
  yaml: {
    label: "YAML",
    tone: FileTone.Data
  },
  yml: {
    label: "YAML",
    tone: FileTone.Data
  },
  md: {
    label: "MD",
    tone: FileTone.Doc
  },
  mdx: {
    label: "MDX",
    tone: FileTone.Doc
  },
  txt: {
    label: "TXT",
    tone: FileTone.Doc
  },
  sh: {
    label: "SH",
    tone: FileTone.Shell
  },
  bash: {
    label: "SH",
    tone: FileTone.Shell
  },
  ps1: {
    label: "PS1",
    tone: FileTone.Shell
  },
  png: {
    label: "IMG",
    tone: FileTone.Image
  },
  jpg: {
    label: "IMG",
    tone: FileTone.Image
  },
  jpeg: {
    label: "IMG",
    tone: FileTone.Image
  },
  gif: {
    label: "IMG",
    tone: FileTone.Image
  },
  webp: {
    label: "IMG",
    tone: FileTone.Image
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
