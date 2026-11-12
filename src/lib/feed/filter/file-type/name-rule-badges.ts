// Name-rule badge data for the file-type lookup in file-type.ts.

import { BUILD_BADGE, CONFIG_BADGE, DOCKER_BADGE, FileTone, GRADLE_BADGE, LOCK_BADGE } from "@/lib/feed/filter/file-type/badge-model";
import type { FileTypeBadge } from "@/lib/feed/filter/file-type/badge-model";

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
const GIT_BADGE: FileTypeBadge = {
  label: "GIT",
  tone: FileTone.Git,
  icon: "git"
};
const LLVM_BADGE: FileTypeBadge = {
  label: "LLVM",
  tone: FileTone.Llvm,
  icon: "llvm"
};
const LICENSE_BADGE: FileTypeBadge = {
  label: "LIC",
  tone: FileTone.License,
  icon: "license"
};

/** How a name rule compares its `pattern` against a lower-cased file name. */
export const NameMatch = {
  Exact: "exact",
  Prefix: "prefix",
  Suffix: "suffix"
} as const;
type NameMatch = (typeof NameMatch)[keyof typeof NameMatch];

export interface NameRule {
  match: NameMatch;
  pattern: string;
  badge: FileTypeBadge;
}

// Files identified by their whole name rather than their extension: tool files
// (`Dockerfile`, `.env.local`, lockfiles named `.yaml`/`.json`) and framework
// files that share a plain extension (`.blade.php`, `.component.ts`). Checked
// before the extension table, so `pnpm-lock.yaml` is a lockfile, not YAML.
export const NAME_RULES: readonly NameRule[] = [
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
  },
  {
    match: NameMatch.Exact,
    pattern: ".gitignore",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".gitattributes",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".gitmodules",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".editorconfig",
    badge: CONFIG_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "meson.build",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "meson_options.txt",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "meson.options",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".wraplock",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "cmakelists.txt",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "makefile",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: "gnumakefile",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: ".clang-",
    badge: LLVM_BADGE
  },
  {
    match: NameMatch.Exact,
    pattern: ".clangd",
    badge: LLVM_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: "license",
    badge: LICENSE_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: "licence",
    badge: LICENSE_BADGE
  },
  {
    match: NameMatch.Prefix,
    pattern: "copying",
    badge: LICENSE_BADGE
  }
];
