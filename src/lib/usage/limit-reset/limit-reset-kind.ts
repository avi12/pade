// What a free usage-limit reset frees up, named the way Claude Code names it.
// The backend decides each grant's kind (lib.rs `usage::limit_reset`); this
// gives each kind its words.

import { UsageWindowKind } from "@/lib/types/usage";
import type { LimitReset } from "@/lib/types/usage";
import type { Limit } from "@/lib/usage/groups/usage-model";
import { resetCountdown } from "@/lib/usage/groups/reset-time";
import { formatCount } from "@/lib/util/format";

type ResetKind = LimitReset["kind"];

/** The weekly allowance of the one model the plan includes beyond its limits -
 *  Fable today, as Claude Code itself labels this window. */
const INCLUDED_MODEL_NAME = "Fable";

function kind({ name, code, scope, effect }: {
  /** As it reads mid-sentence ("your one weekly reset"). */
  name: string;
  /** The limit's mono letter in the meter pills, so a reset reads against the
   *  limit it clears. */
  code: string;
  /** Which limits it works on, as a card's badge says it. */
  scope: string;
  /** What spending it does, in plain words. */
  effect: string;
}) {
  return {
    name,
    title: name.charAt(0).toUpperCase() + name.slice(1),
    code,
    scope,
    effect
  };
}

const RESET_KINDS = {
  session: kind({
    name: "5-hour reset",
    code: "S",
    scope: "5-hour",
    effect: "Resets your 5-hour limit"
  }),
  weekly: kind({
    name: "weekly reset",
    code: "W",
    scope: "All models",
    effect: "Resets your weekly limit"
  }),
  fable: kind({
    name: `${INCLUDED_MODEL_NAME} reset`,
    code: INCLUDED_MODEL_NAME.charAt(0),
    scope: INCLUDED_MODEL_NAME,
    effect: `Resets your ${INCLUDED_MODEL_NAME} limit, and takes this week's ${INCLUDED_MODEL_NAME} use off your weekly limit`
  }),
  full: kind({
    name: "full reset",
    code: "SW",
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

function clearsLimit({ offerKind, limit }: {
  offerKind: ResetKind;
  limit: Limit;
}) {
  const isSessionLimit = limit.kind === UsageWindowKind.enum.session;
  const isWeeklyLimit = limit.kind === UsageWindowKind.enum.weekly;
  const isIncludedModelLimit = limit.kind === UsageWindowKind.enum.model
    && limit.kindShort === limitResetKind("fable").code;
  switch (offerKind) {
    case "session": return isSessionLimit;
    case "weekly": return isWeeklyLimit;
    case "fable": return isIncludedModelLimit;
    case "full": return isSessionLimit || isWeeklyLimit;
  }
}

/** The free resets that clear one limit, for the mark beside it in the meter:
 *  how many the account holds, and a phrase naming them for the tooltip. */
export function limitResetsFor({ offers, limit }: {
  offers: readonly LimitReset[];
  limit: Limit;
}) {
  const matching = offers.filter(
    (offer) => offer.resetsLeft > 0 && clearsLimit({
      offerKind: offer.kind,
      limit
    })
  );
  let count = 0;
  for (const offer of matching) {
    count += offer.resetsLeft;
  }

  return {
    count,
    description: matching
      .map((offer) => {
        const { name } = limitResetKind(offer.kind);
        return `${formatCount(offer.resetsLeft)} ${name}${offer.resetsLeft === 1 ? "" : "s"}`;
      })
      .join(", ")
  };
}

/** Every reset the account holds, across its kinds. */
export function limitResetTotal(offers: readonly LimitReset[]) {
  let total = 0;
  for (const offer of offers) {
    total += offer.resetsLeft;
  }

  return total;
}
