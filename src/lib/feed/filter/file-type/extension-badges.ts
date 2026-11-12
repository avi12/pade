// Extension → badge data for the file-type lookup in file-type.ts.

import {
  BUILD_BADGE,
  CONFIG_BADGE,
  DOCKER_BADGE,
  FileTone,
  GRADLE_BADGE,
  LOCK_BADGE
} from "@/lib/feed/filter/file-type/badge-model";
import type { FileTypeBadge } from "@/lib/feed/filter/file-type/badge-model";

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
const IMAGE_BADGE: FileTypeBadge = {
  label: "IMG",
  tone: FileTone.Image,
  icon: "image"
};
const HTML_BADGE: FileTypeBadge = {
  label: "HTML",
  tone: FileTone.Markup,
  icon: "html"
};
const STYLE_BADGE: FileTypeBadge = {
  label: "CSS",
  tone: FileTone.Style,
  icon: "css"
};
const JSON_BADGE: FileTypeBadge = {
  label: "JSON",
  tone: FileTone.Data,
  icon: "json"
};
const SHELL_BADGE: FileTypeBadge = {
  label: "SH",
  tone: FileTone.Shell,
  icon: "shell"
};
// GPU shader sources: GLSL and its per-stage extensions, plus HLSL, WGSL and Metal.
const SHADER_BADGE: FileTypeBadge = {
  label: "GLSL",
  tone: FileTone.Shader,
  icon: "shader"
};
// Windows linker inputs: module-definition exports and symbol lists.
const LINKER_BADGE: FileTypeBadge = {
  label: "LINK",
  tone: FileTone.Windows,
  icon: "link"
};

// Extension → badge. The authoritative extension table; anything absent falls
// back to a neutral chip of the extension itself. Multi-colour marks (TS, JS,
// Python) carry brand fills in their SVG; single-colour marks (Rust, Go) and the
// hand-drawn format glyphs (JSON, YAML, TOML, Markdown, Shell, Image) are tinted
// by the card's `--tone`.
export const EXTENSION_BADGES: Record<string, FileTypeBadge> = {
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
    tone: FileTone.Doc,
    icon: "text"
  },
  log: {
    label: "LOG",
    tone: FileTone.Doc,
    icon: "text"
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
  avif: IMAGE_BADGE,
  bmp: IMAGE_BADGE,
  ico: IMAGE_BADGE,
  tif: IMAGE_BADGE,
  tiff: IMAGE_BADGE,
  htm: HTML_BADGE,
  xhtml: HTML_BADGE,
  less: STYLE_BADGE,
  styl: STYLE_BADGE,
  pcss: STYLE_BADGE,
  json5: JSON_BADGE,
  jsonl: JSON_BADGE,
  ndjson: JSON_BADGE,
  geojson: JSON_BADGE,
  webmanifest: JSON_BADGE,
  zsh: SHELL_BADGE,
  fish: SHELL_BADGE,
  ini: CONFIG_BADGE,
  cfg: CONFIG_BADGE,
  conf: CONFIG_BADGE,
  properties: CONFIG_BADGE,
  xaml: {
    label: "XAML",
    tone: FileTone.Xml,
    icon: "xml"
  },
  axaml: {
    label: "XAML",
    tone: FileTone.Xml,
    icon: "xml"
  },
  resx: XML_BADGE,
  config: XML_BADGE,
  nuspec: XML_BADGE,
  vbproj: XML_BADGE,
  fsproj: XML_BADGE,
  targets: XML_BADGE,
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
  lockb: LOCK_BADGE,
  glsl: SHADER_BADGE,
  vert: SHADER_BADGE,
  frag: SHADER_BADGE,
  geom: SHADER_BADGE,
  comp: SHADER_BADGE,
  tesc: SHADER_BADGE,
  tese: SHADER_BADGE,
  mesh: SHADER_BADGE,
  task: SHADER_BADGE,
  rgen: SHADER_BADGE,
  rchit: SHADER_BADGE,
  rahit: SHADER_BADGE,
  rmiss: SHADER_BADGE,
  rint: SHADER_BADGE,
  rcall: SHADER_BADGE,
  hlsl: SHADER_BADGE,
  hlsli: SHADER_BADGE,
  fx: SHADER_BADGE,
  wgsl: SHADER_BADGE,
  metal: SHADER_BADGE,
  cmake: BUILD_BADGE,
  mk: BUILD_BADGE,
  ninja: BUILD_BADGE,
  wrap: BUILD_BADGE,
  def: LINKER_BADGE,
  sym: LINKER_BADGE,
  rc: {
    label: "RC",
    tone: FileTone.Windows,
    icon: "window"
  }
};
