// Per-agent usage grouping - the pure, DOM-free, rune-free core behind
// `UsageMeter.svelte` (mirrors the `format.ts` / `tab-fit.ts` / `colors.ts`
// pattern: plain functions + a colocated test).
//
// Honesty doctrine (see the comment atop `src-tauri/src/usage.rs`): ADE never
// fabricates a usage number. An agent whose vendor exposes real rate-limit
// windows through a local token (Claude Code, Codex) gets real limits + plan;
// an agent with no usable local usage signal surfaces as an `unknown` group with
// no limits rather than an invented figure. Each agent resolves its own windows
// through its per-agent account entry - no agent is special-cased here.

import { agentIconName } from "@/lib/agent-icon";
import { formatPercent } from "@/lib/format";
import type { IconName } from "@/lib/Icon.svelte";
import { CreditsState, SHELL_AGENT_ID, UsageWindowKind } from "@/lib/types";
import type {
  AccountUsage,
  Agent,
  AgentSession,
  CreditBalance,
  UsageWindow
} from "@/lib/types";

/** Consumption severity, applied as a CSS class: blue while there's room, amber
 *  past 75%, red past 90% - no green, per the design's usage semantics. */
export const Level = {
  Normal: "normal",
  Warning: "warn",
  Critical: "crit"
} as const;
export type Level = (typeof Level)[keyof typeof Level];

const WARNING_PERCENTAGE = 75;
const CRITICAL_PERCENTAGE = 90;
const MAXIMUM_PERCENTAGE = 100;

const MILLISECONDS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3_600;
const SECONDS_PER_DAY = 86_400;

/** The endpoint's sub-millisecond digits, trimmed so every engine parses the
 *  timestamp the same way. */
const SUB_MILLISECOND_DIGITS = /(\.\d{3})\d+/;

const NON_ALPHANUMERIC_RUN = /[^a-z0-9]+/i;
/** The vendor word a model label leads with - never the distinctive one. */
const VENDOR_WORD = "claude";
const TRAILING_CODE_OR_CLI = /\s+(code|cli)$/i;

const EXTRA_USAGE_LABEL = "Extra usage";

/** The key the account response files extra usage under (`usage.rs`'s
 *  `CREDITS_KEY`), given to the row an uncapped allowance has no window for. */
const UNCAPPED_CREDITS_KEY = "extra_usage";

export type Limit = {
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
};

export type AgentGroup = {
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
  limits: Limit[];
};

// Terminal-editor sessions (Neovim/Vim/Helix) run under an `editor-<id>` agent
// id (see `App.svelte`); they aren't coding agents and carry no usage.
const EDITOR_AGENT_ID_PREFIX = "editor-";

function limitLevel(percentage: number) {
  if (percentage >= CRITICAL_PERCENTAGE) {
    return Level.Critical;
  }

  return percentage >= WARNING_PERCENTAGE ? Level.Warning : Level.Normal;
}

function clampPercentage(value: number) {
  return Math.max(0, Math.min(MAXIMUM_PERCENTAGE, value));
}

// Normalize the endpoint's microsecond timestamps to ms so every engine parses
// them identically - otherwise the countdown can drift.
//
// Total by construction: a window's `resetsAt` is `nullish` (the vendor states a
// reset time only when it knows one), so this takes the absent cases too and
// answers NaN, which every caller already filters through `Number.isFinite`.
// It used to demand a `string`, and one caller's filter excluded `undefined` but
// not `null` - the null reached `.replace` and threw *inside an effect*, which
// takes the whole effect batch down with it: a terminal mounting in the same
// flush never got its xterm and sat at "Starting…" forever.
export function usageResetTime(iso: string | null | undefined) {
  if (!iso) {
    return Number.NaN;
  }

  return new Date(iso.replace(SUB_MILLISECOND_DIGITS, "$1")).getTime();
}

function hasResetPassed({ resetsAt, now }: {
  resetsAt: string | null | undefined;
  now: number;
}) {
  const resetMilliseconds = usageResetTime(resetsAt);
  return Number.isFinite(resetMilliseconds) && resetMilliseconds <= now;
}

// Absolute reset time for the hover tooltip - weekday + date + time so a window
// days out reads unambiguously. Local + locale-aware, like the number formatters.
const RESET_TIMESTAMP = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit"
});

// ISO reset time → its absolute local timestamp, or "" when unknown/unparseable.
function resetTimestamp(iso: string | null | undefined) {
  const resetMilliseconds = usageResetTime(iso);
  return Number.isFinite(resetMilliseconds) ? RESET_TIMESTAMP.format(resetMilliseconds) : "";
}

// ISO reset time → a live "in …" countdown (largest two units), or "". Consumers
// phrase it: the spotlight prefixes "resets", each limit row shows it as-is.
function resetCountdown({ iso, now }: {
  iso: string | null | undefined;
  now: number;
}) {
  const remaining = usageResetTime(iso) - now;
  const isResetAhead = Number.isFinite(remaining) && remaining > 0;
  if (!isResetAhead) {
    return "";
  }

  const totalSeconds = Math.floor(remaining / MILLISECONDS_PER_SECOND);
  const days = Math.floor(totalSeconds / SECONDS_PER_DAY);
  const hours = Math.floor((totalSeconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR);
  const minutes = Math.floor((totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = totalSeconds % SECONDS_PER_MINUTE;
  if (days > 0) {
    return `in ${days}d ${hours}h`;
  }

  if (hours > 0) {
    return `in ${hours}h ${minutes}m`;
  }

  if (minutes > 0) {
    return `in ${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }

  return `in ${seconds}s`;
}

// A single-letter mono code derived from a window's own label - the initial of
// its first distinctive word, e.g. "Claude Opus" → "O", "Claude Fable" → "F".
// Used for per-model and any opaque windows (session/weekly get fixed codes).
function labelShort(name: string) {
  const words = name.split(NON_ALPHANUMERIC_RUN).filter(Boolean);
  const word = words.find(candidate => candidate.toLowerCase() !== VENDOR_WORD) ?? words[0] ?? name;
  return word.slice(0, 1).toUpperCase();
}

// The display name minus a trailing " Code" / " CLI" ("Claude Code" → "Claude",
// "Grok CLI" → "Grok"), for the space-tight compact pills. Falls back to the
// full label when stripping would leave nothing.
function shortName(label: string) {
  return label.replace(TRAILING_CODE_OR_CLI, "").trim() || label;
}

/** A quota counts as spent only when the vendor says it is fully spent. Anything
 *  below that is still usable: a weekly cap at 96% has work left in it, and
 *  moving that work elsewhere costs far more than the few percent it saves. */
export const USAGE_SPENT_PERCENTAGE = 100;

// Where a limit sits in the "what actually stops work" order, which the meter
// ranks by before it ranks by percentage.
const LimitPriority = {
  /** Extra usage while every real window still has room: spending credits never
   *  stops an agent, so a part-spent spend cap is not what anyone is about to
   *  hit - 79% of the monthly spend beside a session window at 44% is headroom. */
  Headroom: 0,
  /** A session / weekly / per-model window: the real walls. */
  Window: 1,
  /** Extra usage once a real window IS spent and credits carry the work: the
   *  spend cap is then the last thing between the agent and a stop. */
  Carrying: 2
} as const;

type LimitPriorityValue = (typeof LimitPriority)[keyof typeof LimitPriority];

function limitPriority({ limit, limits }: {
  limit: Limit;
  limits: Limit[];
}) {
  const isCreditsCap = limit.kind === UsageWindowKind.enum.credits;
  if (!isCreditsCap) {
    return LimitPriority.Window;
  }

  if (limit.isUncapped) {
    return LimitPriority.Headroom;
  }

  const isSpentWindowRidingOnCredits = limit.isCarryingWork
    && limits.some(other => other.kind !== UsageWindowKind.enum.credits
      && other.percentage >= USAGE_SPENT_PERCENTAGE);
  return isSpentWindowRidingOnCredits ? LimitPriority.Carrying : LimitPriority.Headroom;
}

// A limit paired with its priority, so the ranking below is written once and
// reused across agents (the spotlight compares each agent's winner).
type RankedLimit = {
  limit: Limit;
  priority: LimitPriorityValue;
};

function rank({ limit, limits }: {
  limit: Limit;
  limits: Limit[];
}) {
  return {
    limit,
    priority: limitPriority({
      limit,
      limits
    })
  };
}

// The one comparison behind every "worst" in this module: priority first, then
// the more-consumed limit.
function outranks({ candidate, incumbent }: {
  candidate: RankedLimit;
  incumbent: RankedLimit;
}) {
  if (candidate.priority !== incumbent.priority) {
    return candidate.priority > incumbent.priority;
  }

  return candidate.limit.percentage > incumbent.limit.percentage;
}

function worstRanked(limits: Limit[]) {
  let worst: RankedLimit | null = null;
  for (const limit of limits) {
    const candidate = rank({
      limit,
      limits
    });
    const isWorstSoFar = !worst || outranks({
      candidate,
      incumbent: worst
    });
    if (isWorstSoFar) {
      worst = candidate;
    }
  }

  return worst;
}

/** The worst-consumed limit in a set - drives an agent's chip/pill color and the
 *  panel's "closest to a limit" signal. Ranked by `LimitPriority` before
 *  percentage, so extra usage never crowds out the windows that actually stop
 *  work. `null` for an agent with no limits. */
export function worstLimit(limits: Limit[]) {
  return worstRanked(limits)?.limit ?? null;
}

// A window's presentation for the meter: its display label, sub-caption, and
// single-letter mono code. The two named windows get product names; a per-model
// or opaque window derives its label + code from its own backend label.
function windowPresentation(window: UsageWindow) {
  if (window.kind === UsageWindowKind.enum.session) {
    return {
      label: "Session",
      sub: "5-hour window",
      kindShort: "S"
    };
  }

  if (window.kind === UsageWindowKind.enum.weekly) {
    return {
      label: "Weekly",
      sub: "all models",
      kindShort: "W"
    };
  }

  if (window.kind === UsageWindowKind.enum.credits) {
    return {
      label: EXTRA_USAGE_LABEL,
      sub: "monthly spend",
      kindShort: "E"
    };
  }

  const sub = window.kind === UsageWindowKind.enum.model ? "weekly" : "";
  return {
    label: window.label,
    sub,
    kindShort: labelShort(window.label)
  };
}

// A credits balance in its own currency, locale-formatted ("$46.89"), or "" when
// there is none to show.
function formatBalance(balance: CreditBalance | null | undefined) {
  if (!balance) {
    return "";
  }

  return new Intl.NumberFormat(undefined, {
    style: "currency",
    currency: balance.currency,
    // The bare symbol: a locale away from the currency's home spells it "US$124.72".
    currencyDisplay: "narrowSymbol"
  }).format(balance.amount);
}

// Extra usage with no monthly cap has no window - nothing to run out of - but
// its spend is still worth seeing, so it gets a row at 0% that reads as money.
function uncappedCreditsWindows(account: AccountUsage) {
  const hasCreditsWindow = account.windows.some(window => window.kind === UsageWindowKind.enum.credits);
  if (!account.uncappedSpend || hasCreditsWindow) {
    return [];
  }

  return [{
    key: UNCAPPED_CREDITS_KEY,
    kind: UsageWindowKind.enum.credits,
    label: EXTRA_USAGE_LABEL,
    utilization: 0
  }];
}

// An agent's real rate-limit windows off its account - every window the endpoint
// returned (session, weekly, per-model, and any others), in the order it sent
// them. A named window (session/weekly/model) is shown even at 0% - an unused
// session window is a real limit worth surfacing. An OPAQUE feature cap at 0%
// is hidden until it gains real usage, matching the vendor's own usage page
// (ChatGPT lists only the weekly cap while its endpoint also returns untouched
// per-feature limits like a Codex-Spark window).
function buildLimits({ account, now }: {
  account: AccountUsage;
  now: number;
}) {
  const limits: Limit[] = [];
  const windows: UsageWindow[] = [...account.windows, ...uncappedCreditsWindows(account)];
  for (const window of windows) {
    // A utilization belongs to the window that ended at `resetsAt`. Once that
    // boundary passes, retaining it is knowingly stale; render the new window's
    // zero baseline while UsageMeter forces a fresh vendor read.
    const percentage = hasResetPassed({
      resetsAt: window.resetsAt,
      now
    })
      ? 0
      : clampPercentage(window.utilization);
    const isUntouchedFeatureCap = window.kind === UsageWindowKind.enum.opaque && percentage === 0;
    if (isUntouchedFeatureCap) {
      continue;
    }

    const presentation = windowPresentation(window);
    const isCreditsCap = window.kind === UsageWindowKind.enum.credits;
    const isUncapped = isCreditsCap && Boolean(account.uncappedSpend);
    limits.push({
      label: presentation.label,
      sub: presentation.sub,
      kind: window.kind,
      kindShort: presentation.kindShort,
      balance: isCreditsCap ? formatBalance(account.creditBalance) : "",
      isCarryingWork: isCreditsCap && account.credits === CreditsState.enum.available,
      percentage,
      isUncapped,
      reading: isUncapped ? formatBalance(account.uncappedSpend) : formatPercent(percentage),
      level: limitLevel(percentage),
      reset: resetCountdown({
        iso: window.resetsAt,
        now
      }),
      resetAt: resetTimestamp(window.resetsAt)
    });
  }

  return limits;
}

// One group per agent: an agent with a resolved account gets its real limits +
// plan; an agent with none is `unknown` with no limits (ADE has no honest local
// usage signal for it). No agent is special-cased - the account decides.
function buildAgentGroup({ agent, account, now }: {
  agent: Agent;
  account: AccountUsage | null;
  now: number;
}) {
  const hasUsage = account !== null;
  return {
    id: agent.id,
    name: agent.label,
    shortName: shortName(agent.label),
    plan: hasUsage ? account.plan : "",
    icon: agentIconName(agent.id),
    isUnknown: !hasUsage,
    limits: hasUsage ? buildLimits({
      account,
      now
    }) : []
  };
}

// Worst-first: agents with limits ranked by their most-consumed limit (desc);
// agents with none (unknown, or Claude before its usage loads) sink to the end.
// `Array.prototype.sort` is stable, so ties keep their first-seen order.
function sortWorstFirst(groups: AgentGroup[]) {
  return [...groups].sort((first, second) => {
    const firstWorst = worstLimit(first.limits);
    const secondWorst = worstLimit(second.limits);
    if (!firstWorst && !secondWorst) {
      return 0;
    }

    if (!firstWorst) {
      return 1;
    }

    if (!secondWorst) {
      return -1;
    }

    return secondWorst.percentage - firstWorst.percentage;
  });
}

/** Build the agent groups from the running sessions + each agent's account usage.
 *  Distinct coding agents keyed by agent id (first occurrence wins); the shell
 *  fallback and terminal-editor (`editor-*`) sessions are excluded. Two agents
 *  whose accounts carry the same billing-account identity (Codex + opencode on
 *  one ChatGPT subscription) collapse into the first-seen agent's group - one
 *  account is never shown twice. Each agent reads its own windows from
 *  `accounts` (missing/`null` ⇒ an unknown group). Sorted worst-first. */
export function buildGroups({ accounts, sessions, now }: {
  accounts: ReadonlyMap<string, AccountUsage | null>;
  sessions: AgentSession[];
  now: number;
}) {
  const groups: AgentGroup[] = [];
  const seenAgentIds = new Set<string>();
  const seenAccountIds = new Set<string>();
  for (const session of sessions) {
    const agentId = session.agent.id;
    const account = accounts.get(agentId) ?? null;
    const accountId = account?.account ?? null;
    const isShellAgent = agentId === SHELL_AGENT_ID;
    const isEditorSession = agentId.startsWith(EDITOR_AGENT_ID_PREFIX);
    const isAlreadySeen = seenAgentIds.has(agentId);
    const isAccountAlreadyGrouped = accountId !== null && seenAccountIds.has(accountId);
    if (isShellAgent || isEditorSession || isAlreadySeen || isAccountAlreadyGrouped) {
      continue;
    }

    seenAgentIds.add(agentId);

    if (accountId !== null) {
      seenAccountIds.add(accountId);
    }

    groups.push(
      buildAgentGroup({
        agent: session.agent,
        account,
        now
      })
    );
  }

  return sortWorstFirst(groups);
}

// ── Panel view-model ────────────────────────────────────────────────────────
// Worst-first severity buckets: how many agents sit at crit / near / healthy, by
// each agent's most-consumed limit. Feeds both the header tallies and the
// distribution bar (one source, DRY).
const SEVERITY_ORDER = [
  {
    level: Level.Critical,
    label: "critical"
  },
  {
    level: Level.Warning,
    label: "near"
  },
  {
    level: Level.Normal,
    label: "healthy"
  }
] as const satisfies readonly {
  level: Level;
  label: string;
}[];

export type SeveritySlice = {
  level: Level;
  label: string;
  count: number;
};

// Groups with no limits (unknown agents, or Claude before its usage loads) have
// no severity, so they're skipped here - but still counted in the running total.
export function severityBreakdown(agents: AgentGroup[]) {
  return SEVERITY_ORDER.map(severity => ({
    level: severity.level,
    label: severity.label,
    count: agents.filter(agent => {
      const worst = worstLimit(agent.limits);
      return worst !== null && worst.level === severity.level;
    }).length
  }));
}

/** The single limit closest to its cap across every agent, tagged with its owner.
 *  Agents with no limits contribute nothing. */
type Spotlight = {
  agent: AgentGroup;
  limit: Limit;
};

/** How far a limit has to fall between two readings to count as having reset.
 *  Usage only climbs within a window, so a fall this size means it started over
 *  - its scheduled reset, or a free reset spent on it. */
const RESET_FALL_PERCENTAGE = 10;

/** A limit's identity across readings: its agent and its label. */
export function limitKey({ agentId, limit }: {
  agentId: string;
  limit: Limit;
}) {
  return `${agentId}:${limit.label}`;
}

/** Every capped limit's percentage, by {@link limitKey} - the reading the next
 *  one is compared with to spot a reset. */
export function limitPercentages(groups: AgentGroup[]) {
  const percentages = new Map<string, number>();
  for (const group of groups) {
    for (const limit of group.limits) {
      if (limit.isUncapped) {
        continue;
      }

      percentages.set(
        limitKey({
          agentId: group.id,
          limit
        }), limit.percentage
      );
    }
  }

  return percentages;
}

/** The limits that just reset: those that fell by {@link RESET_FALL_PERCENTAGE}
 *  or more since the previous reading. */
export function justResetLimits({ previous, current }: {
  previous: ReadonlyMap<string, number>;
  current: ReadonlyMap<string, number>;
}) {
  const reset = new Set<string>();
  for (const [key, percentage] of current) {
    const before = previous.get(key);
    const hasFallen = before !== undefined && before - percentage >= RESET_FALL_PERCENTAGE;
    if (hasFallen) {
      reset.add(key);
    }
  }

  return reset;
}

export function findSpotlight(agents: AgentGroup[]) {
  let closest: Spotlight | null = null;
  let closestRanked: RankedLimit | null = null;
  for (const agent of agents) {
    const candidate = worstRanked(agent.limits);
    if (!candidate) {
      continue;
    }

    const isClosestSoFar = !closestRanked || outranks({
      candidate,
      incumbent: closestRanked
    });
    if (isClosestSoFar) {
      closestRanked = candidate;
      closest = {
        agent,
        limit: candidate.limit
      };
    }
  }

  return closest;
}

// Distinct kind codes actually in play, each with the label it stands for - the
// legend that decodes the trigger's mono codes.
type KindLegendEntry = {
  short: string;
  name: string;
};

export function buildKindLegend(agents: AgentGroup[]) {
  // Plain-object dedupe (not a Map - this is a pure derivation, not reactive
  // state) keyed by the short code, first label wins.
  const seen: Record<string, true> = {};
  const entries: KindLegendEntry[] = [];
  for (const agent of agents) {
    for (const limit of agent.limits) {
      if (seen[limit.kindShort]) {
        continue;
      }

      seen[limit.kindShort] = true;
      entries.push({
        short: limit.kindShort,
        name: limit.label
      });
    }
  }

  return entries;
}
