import type { IconName } from "@/lib/Icon.svelte";

// Mirrors the project-kind ids in the Rust kind registry (src-tauri/src/ide.rs) —
// the closed set the logo map keys off, so no bare kind string literals leak out.
export const ProjectKind = {
  Web: "web",
  Python: "python",
  Java: "java",
  Go: "go",
  Rust: "rust",
  Android: "android",
  Cpp: "cpp",
  DotNet: "dotnet",
  Php: "php",
  Ruby: "ruby"
} as const;
export type ProjectKind = (typeof ProjectKind)[keyof typeof ProjectKind];

// Each kind's language logo, keyed by `ProjectKind` so adding a kind forces adding
// its icon. Umbrella kinds take their most recognisable member's mark: web → the
// JavaScript badge, .NET → the C# badge.
export const PROJECT_KIND_ICONS: Record<ProjectKind, IconName> = {
  [ProjectKind.Web]: "javascript",
  [ProjectKind.Python]: "python",
  [ProjectKind.Java]: "java",
  [ProjectKind.Go]: "go",
  [ProjectKind.Rust]: "rust",
  [ProjectKind.Android]: "android",
  [ProjectKind.Cpp]: "cplusplus",
  [ProjectKind.DotNet]: "csharp",
  [ProjectKind.Php]: "php",
  [ProjectKind.Ruby]: "ruby"
};

const FALLBACK_ICON: IconName = "code";

// A web project the backend detects as TypeScript reports this key instead of its
// `web` kind (see `ide_project_kinds`), so its per-project icon is the TS badge,
// not JavaScript. It's a language narrowing, not a project KIND, so it maps here
// rather than polluting the `ProjectKind`-keyed map above.
const TYPESCRIPT_KEY = "typescript";

function isProjectKind(value: string): value is ProjectKind {
  return Object.hasOwn(PROJECT_KIND_ICONS, value);
}

/** A known project kind's language logo (or the TypeScript narrowing a web project
 *  can report), else the generic code glyph — so a backend-only kind added later
 *  still renders a row without frontend changes. */
export function languageIcon(kind: string): IconName {
  if (isProjectKind(kind)) {
    return PROJECT_KIND_ICONS[kind];
  }

  return kind === TYPESCRIPT_KEY ? "typescript" : FALLBACK_ICON;
}
