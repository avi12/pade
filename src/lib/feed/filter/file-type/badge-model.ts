// The shape of a file-type badge, the closed set of colour tones, and the badges
// reached from both the extension table and the name rules - one home each.

import type { IconName } from "@/lib/icon-name";

/** The closed set of colour tones a badge can carry; each maps to a `.tone-*`
 *  class the card styles. One authoritative home for the tone names. */
export const FileTone = {
  TYPE_SCRIPT: "typescript",
  JAVA_SCRIPT: "javascript",
  SVELTE: "svelte",
  VUE: "vue",
  REACT: "react",
  ANGULAR: "angular",
  ASTRO: "astro",
  LARAVEL: "laravel",
  RAILS: "rails",
  RUST: "rust",
  CPP: "cpp",
  C: "c",
  STYLE: "style",
  MARKUP: "markup",
  PYTHON: "python",
  GO: "go",
  PHP: "php",
  JAVA: "java",
  C_SHARP: "csharp",
  RUBY: "ruby",
  KOTLIN: "kotlin",
  SWIFT: "swift",
  DART: "dart",
  LUA: "lua",
  SCALA: "scala",
  ELIXIR: "elixir",
  HASKELL: "haskell",
  R: "r",
  ZIG: "zig",
  PERL: "perl",
  POWER_SHELL: "powershell",
  XML: "xml",
  SQL: "sql",
  CSV: "csv",
  ENV: "env",
  LOCK: "lock",
  BATCH: "batch",
  DOCKER: "docker",
  GRAPH_QL: "graphql",
  PRISMA: "prisma",
  GRADLE: "gradle",
  NPM: "npm",
  GIT: "git",
  SHADER: "shader",
  BUILD: "build",
  LLVM: "llvm",
  LICENSE: "license",
  WINDOWS: "windows",
  DATA: "data",
  DOC: "doc",
  SHELL: "shell",
  IMAGE: "image",
  NEUTRAL: "neutral"
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
  tone: FileTone.GRADLE,
  icon: "gradle"
};
export const DOCKER_BADGE: FileTypeBadge = {
  label: "DOCK",
  tone: FileTone.DOCKER,
  icon: "docker"
};
export const LOCK_BADGE: FileTypeBadge = {
  label: "LOCK",
  tone: FileTone.LOCK,
  icon: "lock"
};
export const CONFIG_BADGE: FileTypeBadge = {
  label: "CFG",
  tone: FileTone.DATA,
  icon: "sliders"
};
// Build-system files: Meson, CMake, Make, Ninja.
export const BUILD_BADGE: FileTypeBadge = {
  label: "BLD",
  tone: FileTone.BUILD,
  icon: "build"
};
