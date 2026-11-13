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

import { agentIconName } from "@/lib/agent/agent-icon";
import { SHELL_AGENT_ID } from "@/lib/types/agent";
import type { Agent, AgentSession } from "@/lib/types/agent";
import { CreditsState, UsageWindowKind } from "@/lib/types/usage";
import type { AccountUsage, CreditBalance, NoReading, UsageWindow } from "@/lib/types/usage";
import { worstLimit } from "@/lib/usage/groups/limit-ranking";
import { hasResetPassed, resetCountdown, resetTimestamp } from "@/lib/usage/groups/reset-time";
import { type AgentGroup, Level, type Limit } from "@/lib/usage/groups/usage-model";
import { formatPercent } from "@/lib/util/format";

const WARNING_PERCENTAGE = 75;

const CRITICAL_PERCENTAGE = 90;

const MAXIMUM_PERCENTAGE = 100;

const NON_ALPHANUMERIC_RUN = /[^a-z0-9]+/i;

const DIGITS_ONLY = /^\d+$/;

/** The vendor word a model label leads with - never the distinctive one. */
const VENDOR_WORD = "claude";

const TRAILING_CODE_OR_CLI = /\s+(code|cli)$/i;

const EXTRA_USAGE_LABEL = "Extra usage";

/** The key the account response files extra usage under (`usage.rs`'s
 *  `CREDITS_KEY`), given to the row an uncapped allowance has no window for. */
const UNCAPPED_CREDITS_KEY = "extra_usage";

// Terminal-editor sessions (Neovim/Vim/Helix) run under an `editor-<id>` agent
// id (see `WorkspaceScreen.svelte`); they aren't coding agents and carry no usage.
const EDITOR_AGENT_ID_PREFIX = "editor-";

function limitLevel(percentage: number) {
  if (percentage >= CRITICAL_PERCENTAGE) {
    return Level.CRITICAL;
  }

  return percentage >= WARNING_PERCENTAGE ? Level.WARNING : Level.NORMAL;
}

function clampPercentage(value: number) {
  return Math.max(0, Math.min(MAXIMUM_PERCENTAGE, value));
}

// A single-letter mono code derived from a window's own label - the initial of
// its first distinctive word, e.g. "Claude Opus" → "O", "Claude Fable" → "F".
// Used for per-model and any opaque windows (session/weekly get fixed codes).
function labelShort(name: string) {
  const words = name.split(NON_ALPHANUMERIC_RUN).filter(Boolean);
  const iWord = words.findIndex(candidate => candidate.toLowerCase() !== VENDOR_WORD);
  const word = words[iWord] ?? words[0] ?? name;
  const unit = words[iWord + 1];
  // A first letter of "30-day" would be a lone "3": a span keeps its number.
  const isSpan = DIGITS_ONLY.test(word) && unit !== undefined;
  if (isSpan) {
    return `${word}${unit.slice(0, 1).toUpperCase()}`;
  }

  return word.slice(0, 1).toUpperCase();
}

// The display name minus a trailing " Code" / " CLI" ("Claude Code" → "Claude",
// "Grok CLI" → "Grok"), for the space-tight compact pills. Falls back to the
// full label when stripping would leave nothing.
function shortName(label: string) {
  return label.replace(TRAILING_CODE_OR_CLI, "").trim() || label;
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

  return [
    {
      key: UNCAPPED_CREDITS_KEY,
      kind: UsageWindowKind.enum.credits,
      label: EXTRA_USAGE_LABEL,
      utilization: 0
    }
  ];
}

// A window's utilization as shown. It belongs to the window that ended at
// `resetsAt`; once that boundary passes, retaining it is knowingly stale, so
// the new window's zero baseline shows while UsageMeter forces a fresh read.
function windowPercentage({ window, now }: { window: UsageWindow; now: number }) {
  const hasReset = hasResetPassed({
    resetsAt: window.resetsAt,
    now
  });
  return hasReset ? 0 : clampPercentage(window.utilization);
}

// One window as the meter's limit row: its label and kind letter, its reading
// (money for uncapped extra usage, a percentage otherwise) and its reset.
function windowLimit({
  window,
  account,
  percentage,
  now
}: {
  window: UsageWindow;
  account: AccountUsage;
  percentage: number;
  now: number;
}) {
  const presentation = windowPresentation(window);
  const isCreditsCap = window.kind === UsageWindowKind.enum.credits;
  const isUncapped = isCreditsCap && Boolean(account.uncappedSpend);
  return {
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
  };
}

// An agent's real rate-limit windows off its account - every window the endpoint
// returned (session, weekly, per-model, and any others), in the order it sent
// them. A named window (session/weekly/model) is shown even at 0% - an unused
// session window is a real limit worth surfacing. An OPAQUE feature cap at 0%
// is hidden until it gains real usage, matching the vendor's own usage page
// (ChatGPT lists only the weekly cap while its endpoint also returns untouched
// per-feature limits like a Codex-Spark window) - unless it is the account's
// only limit (Copilot's premium interactions), which then reads as the 0% it
// is rather than as no reading at all.
function buildLimits({ account, now }: { account: AccountUsage; now: number }) {
  const limits: Limit[] = [];
  const windows: UsageWindow[] = [...account.windows, ...uncappedCreditsWindows(account)];
  const hasOtherLimits = windows.length > 1;
  for (const window of windows) {
    const percentage = windowPercentage({
      window,
      now
    });
    const isUntouchedFeatureCap = window.kind === UsageWindowKind.enum.opaque && percentage === 0;
    if (isUntouchedFeatureCap && hasOtherLimits) {
      continue;
    }

    limits.push(
      windowLimit({
        window,
        account,
        percentage,
        now
      })
    );
  }

  return limits;
}

// One group per agent: an agent with a resolved account gets its real limits +
// plan; an agent with none is `unknown` with no limits (ADE has no honest local
// usage signal for it), carrying why when the backend said. No agent is
// special-cased - the account decides. The composer's LLM menu builds one per
// installed agent, running or not.
export function buildAgentGroup({
  agent,
  account,
  noReading = null,
  now
}: {
  agent: Agent;
  account: AccountUsage | null;
  noReading?: NoReading | null;
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
    noReading: hasUsage ? null : noReading,
    limits: hasUsage
      ? buildLimits({
          account,
          now
        })
      : []
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
export function buildGroups({
  accounts,
  noReadings = new Map(),
  sessions,
  now
}: {
  accounts: ReadonlyMap<string, AccountUsage | null>;
  /** Why each agent without an account has no reading, by agent id. */
  noReadings?: ReadonlyMap<string, NoReading>;
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
        noReading: noReadings.get(agentId) ?? null,
        now
      })
    );
  }

  return sortWorstFirst(groups);
}
