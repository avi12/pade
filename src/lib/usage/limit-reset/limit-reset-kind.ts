// What a free usage-limit reset frees up, named the way Claude Code names it.
// The backend decides each grant's kind (lib.rs `usage::limit_reset`); this
// gives each kind its words.

import type { LimitReset } from "@/lib/types/usage";
import { resetCountdown } from "@/lib/usage/groups/reset-time";
import { formatCount } from "@/lib/util/format";

type ResetKind = LimitReset["kind"];

/** The weekly allowance of the one model the plan includes beyond its limits -
 *  Fable today, as Claude Code itself labels this window. */
const INCLUDED_MODEL_NAME = "Fable";

function kind({ name, scope, effect }: {
  /** As it reads mid-sentence ("your one weekly reset"). */
  name: string;
  /** Which limits it works on, as a card's badge says it. */
  scope: string;
  /** What spending it does, in plain words. */
  effect: string;
}) {
  return {
    name,
    title: name.charAt(0).toUpperCase() + name.slice(1),
    scope,
    effect
  };
}

const RESET_KINDS = {
  session: kind({
    name: "5-hour reset",
    scope: "5-hour",
    effect: "Resets your 5-hour limit"
  }),
  weekly: kind({
    name: "weekly reset",
    scope: "All models",
    effect: "Resets your weekly limit"
  }),
  fable: kind({
    name: `${INCLUDED_MODEL_NAME} reset`,
    scope: INCLUDED_MODEL_NAME,
    effect: `Resets your ${INCLUDED_MODEL_NAME} limit, and takes this week's ${INCLUDED_MODEL_NAME} use off your weekly limit`
  }),
  full: kind({
    name: "full reset",
    scope: "All limits",
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

const EXPIRY_DATE = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit"
});

/** How long a reset stays "expiring soon" before it lapses, so its card warns. */
const EXPIRING_SOON_MS = 24 * 60 * 60 * 1000;

/** When a kind's first reset lapses, as its card says it ("Expires in 2d 6h"
 *  beside "Mar 9, 9:59 AM"), flagged once it is under a day away. Null for a
 *  reset that never expires. */
export function limitResetExpiry({ offer, now }: {
  offer: LimitReset;
  now: number;
}) {
  if (!offer.endsAt) {
    return null;
  }

  const endsAt = Date.parse(offer.endsAt);
  return {
    countdown: `Expires ${resetCountdown({
      iso: offer.endsAt,
      now
    })}`,
    date: EXPIRY_DATE.format(endsAt),
    isSoon: endsAt - now < EXPIRING_SOON_MS
  };
}

/** Every reset the account holds, across its kinds - the meter's count. */
export function limitResetTotal(offers: readonly LimitReset[]) {
  let total = 0;
  for (const offer of offers) {
    total += offer.resetsLeft;
  }

  return total;
}
