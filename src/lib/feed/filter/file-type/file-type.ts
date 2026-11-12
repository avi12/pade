// Pure file-type badge for a Change Feed card: a file's extension → a short label,
// a colour tone, and (where the language has one) a brand-logo icon name.
// Language-agnostic - an unrecognised extension still yields a chip (its own
// uppercased extension), so every row is tagged even without a logo.

import { FileTone } from "@/lib/feed/filter/file-type/badge-model";
import { EXTENSION_BADGES } from "@/lib/feed/filter/file-type/extension-badges";
import { NAME_RULES, NameMatch } from "@/lib/feed/filter/file-type/name-rule-badges";
import type { NameRule } from "@/lib/feed/filter/file-type/name-rule-badges";
import { baseName } from "@/lib/util/paths";

// Template suffixes: `version.h.in` is a C header waiting to be configured, so it
// takes the badge of the file it generates (`version.h`).
const TEMPLATE_EXTENSIONS: ReadonlySet<string> = new Set(["in", "template", "tmpl", "tpl", "dist", "sample", "example"]);

/** How many characters of an unknown name / extension a neutral chip shows. */
const NAME_LABEL_LENGTH = 3;
const EXTENSION_LABEL_LENGTH = 4;
const FALLBACK_LABEL = "FILE";
const LEADING_DOT = /^\./;

function nameRuleMatches({ rule, lowerName }: {
  rule: NameRule;
  lowerName: string;
}) {
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
export function pathExtension(path: string) {
  const name = baseName(path);
  const iDot = name.lastIndexOf(".");
  const hasExtension = iDot > 0 && iDot < name.length - 1;
  return hasExtension ? name.slice(iDot + 1).toLowerCase() : null;
}

/** A file path's type key for grouping/counting: its extension with the leading
 *  dot, lowercased (e.g. `.ts`, `.css`). A dotfile (`.gitignore`) or
 *  extensionless file keys on its own base name, so every path yields one key.
 *  The single source both the file-type filter's counts and its filtering read. */
export function fileExtension(path: string) {
  const extension = pathExtension(path);
  return extension ? `.${extension}` : baseName(path);
}

/** The badge for a file path - read from its extension. A dotfile (`.gitignore`)
 *  or extensionless file gets a neutral chip from its own name. */
export function fileTypeBadge(path: string) {
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
    const stem = name.replace(LEADING_DOT, "").slice(0, NAME_LABEL_LENGTH).toUpperCase();
    return {
      label: stem.length > 0 ? stem : FALLBACK_LABEL,
      tone: FileTone.Neutral
    };
  }

  const generatedName = name.slice(0, -(extension.length + 1));
  const isTemplateOfTypedFile = TEMPLATE_EXTENSIONS.has(extension) && pathExtension(generatedName) !== null;
  if (isTemplateOfTypedFile) {
    return fileTypeBadge(generatedName);
  }

  return EXTENSION_BADGES[extension] ?? {
    label: extension.slice(0, EXTENSION_LABEL_LENGTH).toUpperCase(),
    tone: FileTone.Neutral
  };
}
