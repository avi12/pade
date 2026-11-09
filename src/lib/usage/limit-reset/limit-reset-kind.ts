// What a free usage-limit reset actually clears, named the way Claude Code
// names it: a grant lists the limit windows it clears, and that set decides
// whether it is a 5-hour, weekly, Fable or full reset.

import { formatCount } from "@/lib/util/format";

/** The limit windows a grant can list in its `clears`. */
const ResetWindow = {
  Session: "five_hour",
  Weekly: "seven_day",
  /** The weekly allowance of the one model the plan includes beyond its
   *  limits - Fable today, as Claude Code itself labels this window. */
  IncludedModel: "seven_day_overage_included"
} as const;

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

const SESSION_RESET = kind({
  name: "5-hour reset",
  effect: "Resets your 5-hour limit"
});

const WEEKLY_RESET = kind({
  name: "weekly reset",
  effect: "Resets your weekly limit"
});

const INCLUDED_MODEL_RESET = kind({
  name: `${INCLUDED_MODEL_NAME} reset`,
  effect: `Resets your ${INCLUDED_MODEL_NAME} limit, and takes this week's ${INCLUDED_MODEL_NAME} use off your weekly limit`
});

const FULL_RESET = kind({
  name: "full reset",
  effect: "Resets your 5-hour and weekly limits"
});

const UNKNOWN_RESET = kind({
  name: "limit reset",
  effect: "Resets your usage limits"
});

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

/** Name a reset by the windows it clears. A set this doesn't recognise (a new
 *  window, or none listed) reads as a plain usage-limit reset. */
export function limitResetKind(clears: readonly string[]) {
  const clearsSession = clears.includes(ResetWindow.Session);
  const clearsWeekly = clears.includes(ResetWindow.Weekly);
  if (clearsSession && clearsWeekly) {
    return FULL_RESET;
  }

  if (clearsWeekly) {
    return WEEKLY_RESET;
  }

  if (clearsSession) {
    return SESSION_RESET;
  }

  if (clears.includes(ResetWindow.IncludedModel)) {
    return INCLUDED_MODEL_RESET;
  }

  return UNKNOWN_RESET;
}
