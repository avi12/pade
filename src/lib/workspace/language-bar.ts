// The rules, colours and words behind a project row's GitHub-style language bar.

import type { LanguageShare } from "@/lib/types";
import { formatPercent } from "@/lib/util/format";

const PER_MILLE_PER_PERCENT = 10;
/** A language this big a share of the source is one of the project's major ones. */
const MAJOR_LANGUAGE_PER_MILLE = 100;
const MINIMUM_MAJOR_LANGUAGES = 2;
/** Two segment colours closer than this (straight-line distance in RGB, out of
 *  about 441) read as the same colour at 4px. */
const MINIMUM_COLOR_DISTANCE = 90;
const HEX_RADIX = 16;
const CHANNEL_DIGITS = 2;
const RED_START = 1;
const GREEN_START = 3;
const BLUE_START = 5;

/** Each language's colour: GitHub Linguist's where it shows on both themes
 *  (C's near-black and CSS's dark purple are lightened), plus an alternate - its
 *  logo colour - for when the first is too close to one already in the bar. */
const LANGUAGE_COLORS: Record<string, readonly string[]> = {
  TypeScript: ["#3178c6", "#7fb8ff"],
  JavaScript: ["#f1e05a", "#c9a800"],
  Svelte: ["#ff3e00", "#ff9a70"],
  Vue: ["#41b883", "#35495e"],
  HTML: ["#e34c26", "#f6a08c"],
  CSS: ["#8a63d2", "#c4b0f0"],
  SCSS: ["#c6538c", "#f0a6c8"],
  Python: ["#3572a5", "#ffd43b"],
  PHP: ["#4f5d95", "#8892bf"],
  Ruby: ["#cc342d", "#f08a84"],
  Go: ["#00add8", "#007d9c"],
  Rust: ["#dea584", "#b7410e"],
  Java: ["#b07219", "#e76f00"],
  Kotlin: ["#a97bff", "#7f52ff"],
  C: ["#8a9bb0", "#5c6b80"],
  "C++": ["#f34b7d", "#9c033a"],
  "C#": ["#178600", "#9b4f96"]
};
const FALLBACK_COLOR = "#8a9bb0";

/** A bar is only drawn for a project with two or more major languages - a
 *  project that is essentially one language is said by its icon alone. Once
 *  drawn, the bar shows the smaller languages too, as GitHub's does. */
export function isLanguageMix(shares: readonly LanguageShare[]) {
  return shares.filter(share => share.perMille >= MAJOR_LANGUAGE_PER_MILLE).length >= MINIMUM_MAJOR_LANGUAGES;
}

/** A project's counted shares: `undefined` before the count is made, `null`
 *  for a project with no source. */
type CountedShares = readonly LanguageShare[] | null | undefined;

/** Whether a recount added a language the previous count didn't have. */
function isGainingLanguage({ previous, current }: {
  previous: CountedShares;
  current: CountedShares;
}) {
  if (previous === undefined || !current) {
    return false;
  }

  const known = new Set((previous ?? []).map(share => share.language));
  return current.some(share => !known.has(share.language));
}

/** Whether a bar's segments should grow in after a count lands: a recount that
 *  adds a language starts the entrance, a count with no bar to draw ends it,
 *  and otherwise the entrance carries on (the first count of a row on screen
 *  as the list loads keeps the entrance it was mounted with). */
export function isEnteringAfterCount({ previous, current, isEntering }: {
  previous: CountedShares;
  current: CountedShares;
  isEntering: boolean;
}) {
  if (!isLanguageMix(current ?? [])) {
    return current === undefined && isEntering;
  }

  return isEntering || isGainingLanguage({
    previous,
    current
  });
}

/** One segment's hover text: its language and share. */
export function languageShareText(share: LanguageShare) {
  return `${share.language} ${formatPercent(share.perMille / PER_MILLE_PER_PERCENT)}`;
}

function channels(hex: string) {
  return [RED_START, GREEN_START, BLUE_START].map(start =>
    parseInt(hex.slice(start, start + CHANNEL_DIGITS), HEX_RADIX));
}

function colorDistance({ from, to }: {
  from: string;
  to: string;
}) {
  const [a, b] = [channels(from), channels(to)];
  return Math.hypot(...a.map((channel, i) => channel - b[i]));
}

/** How far a candidate sits from the nearest colour already in the bar. */
function clearance({ candidate, taken }: {
  candidate: string;
  taken: readonly string[];
}) {
  let nearest = Number.POSITIVE_INFINITY;
  for (const color of taken) {
    nearest = Math.min(
      nearest, colorDistance({
        from: candidate,
        to: color
      })
    );
  }

  return nearest;
}

/** Every segment's colour, biggest language first: its own colour unless that
 *  is too close to one already in the bar, then its alternate - whichever
 *  stands furthest apart when neither clears. No two segments of one bar can
 *  then be mistaken for each other. */
export function segmentColors(shares: readonly LanguageShare[]) {
  const taken: string[] = [];
  for (const share of shares) {
    const candidates = LANGUAGE_COLORS[share.language] ?? [FALLBACK_COLOR];
    const clear = candidates.find(candidate => clearance({
      candidate,
      taken
    }) >= MINIMUM_COLOR_DISTANCE);
    const furthest = candidates.toSorted((a, b) =>
      clearance({
        candidate: b,
        taken
      }) - clearance({
        candidate: a,
        taken
      }))[0];
    taken.push(clear ?? furthest);
  }

  return taken;
}
