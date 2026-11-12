// The shape of a file-type badge, the closed set of colour tones, and the badges
// reached from both the extension table and the name rules - one home each.

import type { IconName } from "@/lib/Icon.svelte";

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
  Git: "git",
  Shader: "shader",
  Build: "build",
  Llvm: "llvm",
  License: "license",
  Windows: "windows",
  Data: "data",
  Doc: "doc",
  Shell: "shell",
  Image: "image",
  Neutral: "neutral"
} as const;
type FileTone = (typeof FileTone)[keyof typeof FileTone];

export interface FileTypeBadge {
  /** 2–4 char label shown in the chip (e.g. `TS`, `CSS`) - the fallback for a
   *  file type without a brand logo, and always present so every row is tagged. */
  label: string;
  tone: FileTone;
  /** Brand-logo icon (an `Icon.svelte` name) when the language has one; the card
   *  renders this in place of the text `label`. Absent for logoless types. */
  icon?: IconName;
}

export const GRADLE_BADGE: FileTypeBadge = {
  label: "GRDL",
  tone: FileTone.Gradle,
  icon: "gradle"
};
export const DOCKER_BADGE: FileTypeBadge = {
  label: "DOCK",
  tone: FileTone.Docker,
  icon: "docker"
};
export const LOCK_BADGE: FileTypeBadge = {
  label: "LOCK",
  tone: FileTone.Lock,
  icon: "lock"
};
export const CONFIG_BADGE: FileTypeBadge = {
  label: "CFG",
  tone: FileTone.Data,
  icon: "sliders"
};
// Build-system files: Meson, CMake, Make, Ninja.
export const BUILD_BADGE: FileTypeBadge = {
  label: "BLD",
  tone: FileTone.Build,
  icon: "build"
};
