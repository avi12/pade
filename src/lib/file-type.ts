// Pure file-type badge for a Change Feed card: a file's extension → a short label,
// a colour tone, and (where the language has one) a brand-logo icon name.
// Language-agnostic — an unrecognised extension still yields a chip (its own
// uppercased extension), so every row is tagged even without a logo.

import type { IconName } from "@/lib/Icon.svelte";
import { baseName } from "@/lib/paths";

/** The closed set of colour tones a badge can carry; each maps to a `.tone-*`
 *  class the card styles. One authoritative home for the tone names. */
export const FileTone = {
  TypeScript: "typescript",
  JavaScript: "javascript",
  Svelte: "svelte",
  Vue: "vue",
  React: "react",
  Angular: "angular",
  Astro: "astro",
  Laravel: "laravel",
  Rails: "rails",
  Rust: "rust",
  Cpp: "cpp",
  C: "c",
  Style: "style",
  Markup: "markup",
  Python: "python",
  Go: "go",
  Php: "php",
  Java: "java",
  CSharp: "csharp",
  Ruby: "ruby",
  Kotlin: "kotlin",
  Swift: "swift",
  Dart: "dart",
  Lua: "lua",
  Scala: "scala",
  Elixir: "elixir",
  Haskell: "haskell",
  R: "r",
  Zig: "zig",
  Perl: "perl",
  PowerShell: "powershell",
  Xml: "xml",
  Sql: "sql",
  Csv: "csv",
  Env: "env",
  Lock: "lock",
  Batch: "batch",
  Docker: "docker",
  GraphQL: "graphql",
  Prisma: "prisma",
  Gradle: "gradle",
  Npm: "npm",
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

// Badges reached from several extensions and/or name rules — one home each.
const XML_BADGE: FileTypeBadge = {
  label: "XML",
  tone: FileTone.Xml,
  icon: "xml"
};
const SQL_BADGE: FileTypeBadge = {
  label: "SQL",
  tone: FileTone.Sql,
  icon: "sql"
};
const BATCH_BADGE: FileTypeBadge = {
  label: "BAT",
  tone: FileTone.Batch,
  icon: "batch"
};
const GRAPHQL_BADGE: FileTypeBadge = {
  label: "GQL",
  tone: FileTone.GraphQL,
  icon: "graphql"
};
const GRADLE_BADGE: FileTypeBadge = {
  label: "GRDL",
  tone: FileTone.Gradle,
  icon: "gradle"
};
const DOCKER_BADGE: FileTypeBadge = {
  label: "DOCK",
  tone: FileTone.Docker,
  icon: "docker"
};
const LOCK_BADGE: FileTypeBadge = {
  label: "LOCK",
  tone: FileTone.Lock,
  icon: "lock"
};
const NPM_BADGE: FileTypeBadge = {
  label: "NPM",
  tone: FileTone.Npm,
  icon: "npm"
};
const ANGULAR_BADGE: FileTypeBadge = {
  label: "NG",
  tone: FileTone.Angular,
  icon: "angular"
};

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
    tone: FileTone.React,
    icon: "react"
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
    tone: FileTone.React,
    icon: "react"
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
  vue: {
    label: "VUE",
    tone: FileTone.Vue,
    icon: "vue"
  },
  astro: {
    label: "ASTRO",
    tone: FileTone.Astro,
    icon: "astro"
  },
  erb: {
    label: "ERB",
    tone: FileTone.Rails,
    icon: "rails"
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
  php: {
    label: "PHP",
    tone: FileTone.Php,
    icon: "php"
  },
  java: {
    label: "JAVA",
    tone: FileTone.Java,
    icon: "java"
  },
  cs: {
    label: "C#",
    tone: FileTone.CSharp,
    icon: "csharp"
  },
  rb: {
    label: "RB",
    tone: FileTone.Ruby,
    icon: "ruby"
  },
  rake: {
    label: "RAKE",
    tone: FileTone.Ruby,
    icon: "ruby"
  },
  kt: {
    label: "KT",
    tone: FileTone.Kotlin,
    icon: "kotlin"
  },
  kts: {
    label: "KTS",
    tone: FileTone.Kotlin,
    icon: "kotlin"
  },
  swift: {
    label: "SWIFT",
    tone: FileTone.Swift,
    icon: "swift"
  },
  dart: {
    label: "DART",
    tone: FileTone.Dart,
    icon: "dart"
  },
  lua: {
    label: "LUA",
    tone: FileTone.Lua,
    icon: "lua"
  },
  scala: {
    label: "SCALA",
    tone: FileTone.Scala,
    icon: "scala"
  },
  sc: {
    label: "SC",
    tone: FileTone.Scala,
    icon: "scala"
  },
  ex: {
    label: "EX",
    tone: FileTone.Elixir,
    icon: "elixir"
  },
  exs: {
    label: "EXS",
    tone: FileTone.Elixir,
    icon: "elixir"
  },
  hs: {
    label: "HS",
    tone: FileTone.Haskell,
    icon: "haskell"
  },
  lhs: {
    label: "LHS",
    tone: FileTone.Haskell,
    icon: "haskell"
  },
  r: {
    label: "R",
    tone: FileTone.R,
    icon: "r"
  },
  rmd: {
    label: "RMD",
    tone: FileTone.R,
    icon: "r"
  },
  zig: {
    label: "ZIG",
    tone: FileTone.Zig,
    icon: "zig"
  },
  pl: {
    label: "PL",
    tone: FileTone.Perl,
    icon: "perl"
  },
  pm: {
    label: "PM",
    tone: FileTone.Perl,
    icon: "perl"
  },
  psm1: {
    label: "PS",
    tone: FileTone.PowerShell,
    icon: "powershell"
  },
  psd1: {
    label: "PS",
    tone: FileTone.PowerShell,
    icon: "powershell"
  },
  cpp: {
    label: "C++",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  cc: {
    label: "CC",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  cxx: {
    label: "CXX",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  // C++ headers take the C++ mark; the plain `.h` header below stays with C.
  hpp: {
    label: "HPP",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  hh: {
    label: "HH",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  hxx: {
    label: "HXX",
    tone: FileTone.Cpp,
    icon: "cplusplus"
  },
  c: {
    label: "C",
    tone: FileTone.C,
    icon: "c"
  },
  h: {
    label: "H",
    tone: FileTone.C,
    icon: "c"
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
    tone: FileTone.PowerShell,
    icon: "powershell"
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
  },
  xml: XML_BADGE,
  xsd: XML_BADGE,
  xsl: XML_BADGE,
  xslt: XML_BADGE,
  plist: XML_BADGE,
  csproj: XML_BADGE,
  props: XML_BADGE,
  sql: SQL_BADGE,
  psql: SQL_BADGE,
  mysql: SQL_BADGE,
  csv: {
    label: "CSV",
    tone: FileTone.Csv,
    icon: "csv"
  },
  tsv: {
    label: "TSV",
    tone: FileTone.Csv,
    icon: "csv"
  },
  bat: BATCH_BADGE,
  cmd: BATCH_BADGE,
  graphql: GRAPHQL_BADGE,
  gql: GRAPHQL_BADGE,
  graphqls: GRAPHQL_BADGE,
  prisma: {
    label: "PRSM",
    tone: FileTone.Prisma,
    icon: "prisma"
  },
  gradle: GRADLE_BADGE,
  dockerfile: DOCKER_BADGE,
  lock: LOCK_BADGE,
  lockb: LOCK_BADGE
};

/** How a name rule compares its `pattern` against a lower-cased file name. */
const NameMatch = {
  Exact: "exact",
  Prefix: "prefix",
  Suffix: "suffix"
} as const;
type NameMatch = (typeof NameMatch)[keyof typeof NameMatch];

interface NameRule {
  match: NameMatch;
  pattern: string;
  badge: FileTypeBadge;
}

// Files identified by their whole name rather than their extension: tool files
// (`Dockerfile`, `.env.local`, lockfiles named `.yaml`/`.json`) and framework
// files that share a plain extension (`.blade.php`, `.component.ts`). Checked
// before the extension table, so `pnpm-lock.yaml` is a lockfile, not YAML.
const NAME_RULES: readonly NameRule[] = [
  {
    match: NameMatch.Suffix,
    pattern: ".blade.php",
    badge: {
      label: "BLADE",
      tone: FileTone.Laravel,
      icon: "laravel"
    }
  },
  {
    match: NameMatch.Suffix,
    pattern: ".component.ts",
    badge: ANGULAR_BADGE
  },
  {
    match: NameMatch.Suffix,
    pattern: ".component.html",
    badge: ANGULAR_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "dockerfile",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: "dockerfile.",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".dockerignore",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "docker-compose.yml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "docker-compose.yaml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "compose.yml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "compose.yaml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: ".env",
    badge: {
      label: "ENV",
      tone: FileTone.Env,
      icon: "env"
    }
  },
  {
    match: NameMatch.Exact,
    pattern: "package-lock.json",
    badge: LOCK_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "pnpm-lock.yaml",
    badge: LOCK_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "package.json",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".npmrc",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".npmignore",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.Suffix,
    pattern: ".gradle.kts",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "gradle.properties",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "gradlew",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "gradlew.bat",
    badge: GRADLE_BADGE
  }
];

function nameRuleMatches({ rule, lowerName }: {
  rule: NameRule;
  lowerName: string;
}): boolean {
  switch (rule.match) {
    case NameMatch.Exact:
      return lowerName === rule.pattern;
    case NameMatch.Prefix:
      return lowerName.startsWith(rule.pattern);
    case NameMatch.Suffix:
      return lowerName.endsWith(rule.pattern);
  }
}

/** A path's lower-case final extension without its dot, or null for a dotfile,
 * extensionless name, or trailing dot. Shared by all extension classifiers. */
export function pathExtension(path: string): string | null {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  const hasExtension = dot > 0 && dot < name.length - 1;
  return hasExtension ? name.slice(dot + 1).toLowerCase() : null;
}

/** A file path's type key for grouping/counting: its extension with the leading
 *  dot, lowercased (e.g. `.ts`, `.css`). A dotfile (`.gitignore`) or
 *  extensionless file keys on its own base name, so every path yields one key.
 *  The single source both the file-type filter's counts and its filtering read. */
export function fileExtension(path: string): string {
  const name = baseName(path);
  const extension = pathExtension(path);
  return extension ? `.${extension}` : name;
}

/** The badge for a file path — read from its extension. A dotfile (`.gitignore`)
 *  or extensionless file gets a neutral chip from its own name. */
export function fileTypeBadge(path: string): FileTypeBadge {
  const name = baseName(path);
  const lowerName = name.toLowerCase();
  const nameRule = NAME_RULES.find(rule => nameRuleMatches({
    rule,
    lowerName
  }));
  if (nameRule) {
    return nameRule.badge;
  }

  const extension = pathExtension(path);
  if (!extension) {
    const stem = name.replace(/^\./, "").slice(0, 3).toUpperCase();
    return {
      label: stem.length > 0 ? stem : "FILE",
      tone: FileTone.Neutral
    };
  }

  return BADGES[extension] ?? {
    label: extension.slice(0, 4).toUpperCase(),
    tone: FileTone.Neutral
  };
}
