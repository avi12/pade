import type { IconName } from "@/lib/icon-name";

// Mirrors the project-kind ids in the Rust kind registry (src-tauri/src/ide.rs) -
// the closed set the logo map keys off, so no bare kind string literals leak out.
export const ProjectKind = {
  WEB: "web",
  PYTHON: "python",
  JAVA: "java",
  GO: "go",
  RUST: "rust",
  ANDROID: "android",
  CPP: "cpp",
  DOT_NET: "dotnet",
  PHP: "php",
  RUBY: "ruby"
} as const;
type ProjectKind = (typeof ProjectKind)[keyof typeof ProjectKind];

// Each kind's language logo, keyed by `ProjectKind` so adding a kind forces adding
// its icon. Umbrella kinds take their most recognisable member's mark: web → the
// JavaScript badge, .NET → the C# badge.
const PROJECT_KIND_ICONS: Record<ProjectKind, IconName> = {
  [ProjectKind.WEB]: "javascript",
  [ProjectKind.PYTHON]: "python",
  [ProjectKind.JAVA]: "java",
  [ProjectKind.GO]: "go",
  [ProjectKind.RUST]: "rust",
  [ProjectKind.ANDROID]: "android",
  [ProjectKind.CPP]: "cplusplus",
  [ProjectKind.DOT_NET]: "csharp",
  [ProjectKind.PHP]: "php",
  [ProjectKind.RUBY]: "ruby"
};

const FALLBACK_ICON: IconName = "code";

// A web project the backend detects as TypeScript reports this key instead of its
// `web` kind (see `ide_project_kinds`), so its per-project icon is the TS badge,
// not JavaScript. It's a language narrowing, not a project KIND, so it maps here
// rather than polluting the `ProjectKind`-keyed map above.
const TYPESCRIPT_KEY = "typescript";
const TYPESCRIPT_ICON: IconName = "typescript";

function isProjectKind(value: string): value is ProjectKind {
  return Object.hasOwn(PROJECT_KIND_ICONS, value);
}

/** A known project kind's language logo (or the TypeScript narrowing a web project
 *  can report), else the generic code glyph - so a backend-only kind added later
 *  still renders a row without frontend changes. */
export function languageIcon(kind: string) {
  if (isProjectKind(kind)) {
    return PROJECT_KIND_ICONS[kind];
  }

  return kind === TYPESCRIPT_KEY ? TYPESCRIPT_ICON : FALLBACK_ICON;
}

/** Each Linguist language's logo, keyed by the names `ide_project_languages`
 *  reports. SCSS wears the CSS mark. */
const LANGUAGE_ICONS: Record<string, IconName> = {
  TypeScript: "typescript",
  JavaScript: "javascript",
  Svelte: "svelte",
  Vue: "vue",
  HTML: "html",
  CSS: "css",
  SCSS: "css",
  Python: "python",
  PHP: "php",
  Ruby: "ruby",
  Go: "go",
  Rust: "rust",
  Java: "java",
  Kotlin: "kotlin",
  C: "c",
  "C++": "cplusplus",
  "C#": "csharp"
};

/** A language's own logo, else the generic code glyph. */
export function languageIconOf(language: string) {
  return LANGUAGE_ICONS[language] ?? FALLBACK_ICON;
}
