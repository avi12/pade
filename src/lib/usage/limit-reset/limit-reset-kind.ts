// What a free usage-limit reset frees up, named the way Claude Code names it.
// The backend decides each grant's kind (lib.rs `usage::limit_reset`); this
// gives each kind its words.

import type { LimitReset } from "@/lib/types";
import { formatCount } from "@/lib/util/format";

type ResetKind = LimitReset["kind"];

/** The weekly allowance of the one model the plan includes beyond its limits -
 *  Fable today, as Claude Code itself labels this window. */
const INCLUDED_MODEL_NAME = "Fable";

function kind({ name, effect }: {
  /** As it reads mid-sentence ("your one weekly reset"). */
  name: string;
  /** What spending it does, in plain words. */
  effect: string;
}) {
  return {
    name,
    title: name.charAt(0).toUpperCase() + name.slice(1),
    effect
  };
}

const RESET_KINDS = {
  session: kind({
    name: "5-hour reset",
    effect: "Resets your 5-hour limit"
  }),
  weekly: kind({
    name: "weekly reset",
    effect: "Resets your weekly limit"
  }),
  fable: kind({
    name: `${INCLUDED_MODEL_NAME} reset`,
    effect: `Resets your ${INCLUDED_MODEL_NAME} limit, and takes this week's ${INCLUDED_MODEL_NAME} use off your weekly limit`
  }),
  full: kind({
    name: "full reset",
    effect: "Resets your 5-hour and weekly limits"
  })
} as const satisfies Record<ResetKind, ReturnType<typeof kind>>;

/** Why a reset can't be used: everything it clears is unused. */
export const NOTHING_TO_RESET = "Nothing to reset yet - none of this limit is used";

/** Which of the account's resets a claim spends, mid-sentence ("your one
 *  Fable reset", "one of your 3 weekly resets"). */
export function spentResetPhrase({ name, resetsLeft }: {
  name: string;
  resetsLeft: number;
}) {
  if (resetsLeft === 1) {
    return `your one ${name}`;
  }

  return `one of your ${formatCount(resetsLeft)} ${name}s`;
}

/** The words for a kind of reset. */
export function limitResetKind(resetKind: ResetKind) {
  return RESET_KINDS[resetKind];
}

/** How many of a kind are left and until when, in the user's own locale and
 *  time ("3 left · use by Mar 9, 9:59 AM"). */
export function limitResetSummary(offer: LimitReset) {
  const left = `${formatCount(offer.resetsLeft)} left`;
  if (!offer.endsAt) {
    return left;
  }

  const date = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  }).format(Date.parse(offer.endsAt));
  return `${left} · use by ${date}`;
}

/** The meter chip's face for the offers: one kind is named outright ("Fable
 *  reset", its hint saying what it frees up); several are counted ("4
 *  resets", the hint listing each kind) and the click lets the user pick. */
export function limitResetChip(offers: readonly LimitReset[]) {
  const [only] = offers;
  if (offers.length === 1 && only) {
    const { title, effect } = limitResetKind(only.kind);
    return {
      label: title,
      hint: `${only.hasUsageToClear ? effect : NOTHING_TO_RESET} · ${limitResetSummary(only)}`
    };
  }

  let total = 0;
  for (const offer of offers) {
    total += offer.resetsLeft;
  }

  const lines = offers.map(offer => `${limitResetKind(offer.kind).title}: ${limitResetSummary(offer)}`);
  return {
    label: `${formatCount(total)} resets`,
    hint: [...lines, "Click to pick which one to use"].join("\n")
  };
}
