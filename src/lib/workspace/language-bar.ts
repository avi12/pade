// The rules and words behind a project row's GitHub-style language bar.

import type { LanguageShare } from "@/lib/types";
import { formatPercent } from "@/lib/util/format";

const PER_MILLE_PER_PERCENT = 10;
/** A language this big a share of the source is one of the project's major ones. */
const MAJOR_LANGUAGE_PER_MILLE = 100;
const MINIMUM_MAJOR_LANGUAGES = 2;

/** A bar is only drawn for a project with two or more major languages - a
 *  project that is essentially one language is said by its icon alone. Once
 *  drawn, the bar shows the smaller languages too, as GitHub's does. */
export function isLanguageMix(shares: readonly LanguageShare[]) {
  return shares.filter(share => share.perMille >= MAJOR_LANGUAGE_PER_MILLE).length >= MINIMUM_MAJOR_LANGUAGES;
}

/** One segment's hover text: its language and share. */
export function languageShareText(share: LanguageShare) {
  return `${share.label} ${formatPercent(share.perMille / PER_MILLE_PER_PERCENT)}`;
}
