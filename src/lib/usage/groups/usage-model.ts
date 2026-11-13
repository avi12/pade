// The shapes the usage grouping produces: the severity level, one limit row, and
// one agent's group of limits.

import type { IconName } from "@/lib/Icon.svelte";
import type { UsageWindowKind } from "@/lib/types/usage";
import type { NoReading } from "@/lib/types/usage";

/** Consumption severity, applied as a CSS class: blue while there's room, amber
 *  past 75%, red past 90% - no green, per the design's usage semantics. */
export const Level = {
  Normal: "normal",
  Warning: "warn",
  Critical: "crit"
} as const;

export type Level = (typeof Level)[keyof typeof Level];

export interface Limit {
  label: string;
  sub: string;
  reset: string;
  /** The same reset moment as an absolute local timestamp ("Thu, Jul 24, 2:15 PM"),
   *  shown on hover over the countdown; empty when the reset time is unknown. */
  resetAt: string;
  percentage: number;
  /** What the row reads as: its percentage ("57%"), or - on an extra-usage row
   *  with no monthly cap, so no percentage - the month's spend ("$259.18"). */
  reading: string;
  level: Level;
  /** The window's semantic kind (session / weekly / model / opaque), straight
   *  from the account payload's classification. */
  kind: UsageWindowKind;
  /** A single-letter mono code shown in the trigger pills, legend + rows
   *  ("S", "W", "O"). */
  kindShort: string;
  /** The prepaid usage-credits balance ("$46.89") on the credits row; empty on
   *  every other row, and whenever extra usage is off. */
  balance: string;
  /** Whether this limit is the extra usage that can carry work past a spent
   *  window - the credits row while credits are on and usable. False on every
   *  other row, and on a credits row that is off or held back, which carries
   *  nothing and so never outranks the window that stopped. */
  isCarryingWork: boolean;
  /** Extra usage with no monthly cap: nothing to run out of, so it reads as the
   *  month's spend and is never the limit anything is close to. */
  isUncapped: boolean;
}

export interface AgentGroup {
  /** The agent registry id (`claude`, `codex`, …) - the dedupe + sort key. */
  id: string;
  name: string;
  /** `name` minus a trailing " Code" / " CLI", for the compact trigger pills. */
  shortName: string;
  plan: string;
  icon: IconName;
  /** True when ADE has no trustworthy local usage signal for this agent, so it
   *  shows no numbers (only Claude Code exposes its limits locally today). */
  isUnknown: boolean;
  /** Why an unknown agent has no reading (signed out, sign-in expired, …), when
   *  the backend said; `null` for an agent with a reading. */
  noReading: NoReading | null;
  limits: Limit[];
}
