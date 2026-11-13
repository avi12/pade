// Name-rule badge data for the file-type lookup in file-type.ts.

import {
  BUILD_BADGE,
  CONFIG_BADGE,
  DOCKER_BADGE,
  FileTone,
  GRADLE_BADGE,
  LOCK_BADGE
} from "@/lib/feed/filter/file-type/badge-model";
import type { FileTypeBadge } from "@/lib/feed/filter/file-type/badge-model";

const NPM_BADGE: FileTypeBadge = {
  label: "NPM",
  tone: FileTone.NPM,
  icon: "npm"
};
const ANGULAR_BADGE: FileTypeBadge = {
  label: "NG",
  tone: FileTone.ANGULAR,
  icon: "angular"
};
const GIT_BADGE: FileTypeBadge = {
  label: "GIT",
  tone: FileTone.GIT,
  icon: "git"
};
const LLVM_BADGE: FileTypeBadge = {
  label: "LLVM",
  tone: FileTone.LLVM,
  icon: "llvm"
};
const LICENSE_BADGE: FileTypeBadge = {
  label: "LIC",
  tone: FileTone.LICENSE,
  icon: "license"
};

/** How a name rule compares its `pattern` against a lower-cased file name. */
export const NameMatch = {
  EXACT: "exact",
  PREFIX: "prefix",
  SUFFIX: "suffix"
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
    match: NameMatch.SUFFIX,
    pattern: ".blade.php",
    badge: {
      label: "BLADE",
      tone: FileTone.LARAVEL,
      icon: "laravel"
    }
  },
  {
    match: NameMatch.SUFFIX,
    pattern: ".component.ts",
    badge: ANGULAR_BADGE
  },
  {
    match: NameMatch.SUFFIX,
    pattern: ".component.html",
    badge: ANGULAR_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "dockerfile",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: "dockerfile.",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".dockerignore",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "docker-compose.yml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "docker-compose.yaml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "compose.yml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "compose.yaml",
    badge: DOCKER_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: ".env",
    badge: {
      label: "ENV",
      tone: FileTone.ENV,
      icon: "env"
    }
  },
  {
    match: NameMatch.EXACT,
    pattern: "package-lock.json",
    badge: LOCK_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "pnpm-lock.yaml",
    badge: LOCK_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "package.json",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".npmrc",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".npmignore",
    badge: NPM_BADGE
  },
  {
    match: NameMatch.SUFFIX,
    pattern: ".gradle.kts",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "gradle.properties",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "gradlew",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "gradlew.bat",
    badge: GRADLE_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".gitignore",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".gitattributes",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".gitmodules",
    badge: GIT_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".editorconfig",
    badge: CONFIG_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "meson.build",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "meson_options.txt",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "meson.options",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".wraplock",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "cmakelists.txt",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "makefile",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: "gnumakefile",
    badge: BUILD_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: ".clang-",
    badge: LLVM_BADGE
  },
  {
    match: NameMatch.EXACT,
    pattern: ".clangd",
    badge: LLVM_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: "license",
    badge: LICENSE_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: "licence",
    badge: LICENSE_BADGE
  },
  {
    match: NameMatch.PREFIX,
    pattern: "copying",
    badge: LICENSE_BADGE
  }
];
