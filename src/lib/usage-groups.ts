// Per-agent usage grouping — the pure, DOM-free, rune-free core behind
// `UsageMeter.svelte` (mirrors the `format.ts` / `tab-fit.ts` / `colors.ts`
// pattern: plain functions + a colocated test).
//
// Honesty doctrine (see the comment atop `src-tauri/src/usage.rs`): ADE never
// fabricates a usage number. An agent whose vendor exposes real rate-limit
// windows through a local token (Claude Code, Codex) gets real limits + plan;
// an agent with no usable local usage signal surfaces as an `unknown` group with
// no limits rather than an invented figure. Each agent resolves its own windows
// through its per-agent account entry — no agent is special-cased here.

import { agentIconName } from "@/lib/agent-icon";
import type { IconName } from "@/lib/Icon.svelte";
import { SHELL_AGENT_ID, UsageWindowKind } from "@/lib/types";
import type { AccountUsage, Agent, AgentSession, UsageWindow } from "@/lib/types";

/** Consumption severity, applied as a CSS class: blue while there's room, amber
 *  past 75%, red past 90% — no green, per the design's usage semantics. */
export type Level = "normal" | "warn" | "crit";

export type Limit = {
  label: string;
  sub: string;
  reset: string;
  /** The same reset moment as an absolute local timestamp ("Thu, Jul 24, 2:15 PM"),
   *  shown on hover over the countdown; empty when the reset time is unknown. */
  resetAt: string;
  pct: number;
  level: Level;
  /** The window's semantic kind (session / weekly / model / opaque), straight
   *  from the account payload's classification. */
  kind: UsageWindowKind;
  /** A single-letter mono code shown in the trigger pills, legend + rows
   *  ("S", "W", "O"). */
  kindShort: string;
};

export type AgentGroup = {
  /** The agent registry id (`claude`, `codex`, …) — the dedupe + sort key. */
  id: string;
  name: string;
  /** `name` minus a trailing " Code" / " CLI", for the compact trigger pills. */
  shortName: string;
  plan: string;
  icon: IconName;
  /** True when ADE has no trustworthy local usage signal for this agent, so it
   *  shows no numbers (only Claude Code exposes its limits locally today). */
  unknown: boolean;
  limits: Limit[];
};

// Terminal-editor sessions (Neovim/Vim/Helix) run under an `editor-<id>` agent
// id (see `App.svelte`); they aren't coding agents and carry no usage.
const EDITOR_AGENT_ID_PREFIX = "editor-";

// Severity by consumption. Applied as a CSS class downstream.
export function limitLevel(pct: number): Level {
  if (pct >= 90) {
    return "crit";
  }

  if (pct >= 75) {
    return "warn";
  }

  return "normal";
}

function clamp(value: number): number {
  return Math.max(0, Math.min(100, value));
}

// Normalize the endpoint's microsecond timestamps to ms so every engine parses
// them identically — otherwise the countdown can drift.
function parseIso(iso: string): number {
  return new Date(iso.replace(/(\.\d{3})\d+/, "$1")).getTime();
}

// Absolute reset time for the hover tooltip — weekday + date + time so a window
// days out reads unambiguously. Local + locale-aware, like the number formatters.
const RESET_TIMESTAMP = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit"
});

// ISO reset time → its absolute local timestamp, or "" when unknown/unparseable.
function resetTimestamp(iso: string | null | undefined): string {
  if (!iso) {
    return "";
  }

  const resetMs = parseIso(iso);
  if (!Number.isFinite(resetMs)) {
    return "";
  }

  return RESET_TIMESTAMP.format(resetMs);
}

// ISO reset time → a live "in …" countdown (largest two units), or "". Consumers
// phrase it: the spotlight prefixes "resets", each limit row shows it as-is.
function resetCountdown({ iso, now }: {
  iso: string | null | undefined;
  now: number;
}): string {
  if (!iso) {
    return "";
  }

  const remaining = parseIso(iso) - now;
  if (!Number.isFinite(remaining) || remaining <= 0) {
    return "";
  }

  const totalSeconds = Math.floor(remaining / 1000);
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
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

// A single-letter mono code derived from a window's own label — the initial of
// its first distinctive word, e.g. "Claude Opus" → "O", "Claude Fable" → "F".
// Used for per-model and any opaque windows (session/weekly get fixed codes).
function labelShort(name: string): string {
  const tokens = name.split(/[^a-z0-9]+/i).filter(Boolean);
  const word = tokens.find(token => token.toLowerCase() !== "claude") ?? tokens[0] ?? name;
  return word.slice(0, 1).toUpperCase();
}

// The display name minus a trailing " Code" / " CLI" ("Claude Code" → "Claude",
// "Grok CLI" → "Grok"), for the space-tight compact pills. Falls back to the
// full label when stripping would leave nothing.
function shortName(label: string): string {
  return label.replace(/\s+(code|cli)$/i, "").trim() || label;
}

/** The worst-consumed limit in a set — drives an agent's chip/pill color and the
 *  panel's "closest to a limit" signal. `null` for an agent with no limits. */
export function worstLimit(limits: Limit[]): Limit | null {
  return limits.length > 0 ? limits.reduce((max, limit) => (limit.pct > max.pct ? limit : max)) : null;
}

// A window's presentation for the meter: its display label, sub-caption, and
// single-letter mono code. The two named windows get product names; a per-model
// or opaque window derives its label + code from its own backend label.
function windowPresentation(window: UsageWindow): {
  label: string;
  sub: string;
  kindShort: string;
} {
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

  const sub = window.kind === UsageWindowKind.enum.model ? "weekly" : "";
  return {
    label: window.label,
    sub,
    kindShort: labelShort(window.label)
  };
}

// An agent's real rate-limit windows off its account — every window the endpoint
// returned (session, weekly, per-model, and any others), in the order it sent
// them. Every returned window is shown, including one still at 0%: an unused
// session window is a real limit worth surfacing, not noise to hide.
function buildLimits({ account, now }: {
  account: AccountUsage;
  now: number;
}): Limit[] {
  const limits: Limit[] = [];
  for (const window of account.windows) {
    const value = clamp(window.utilization);
    const presentation = windowPresentation(window);
    limits.push({
      label: presentation.label,
      sub: presentation.sub,
      kind: window.kind,
      kindShort: presentation.kindShort,
      pct: value,
      level: limitLevel(value),
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
// usage signal for it). No agent is special-cased — the account decides.
function buildAgentGroup({ agent, account, now }: {
  agent: Agent;
  account: AccountUsage | null;
  now: number;
}): AgentGroup {
  const hasUsage = account !== null;
  return {
    id: agent.id,
    name: agent.label,
    shortName: shortName(agent.label),
    plan: hasUsage ? account.plan : "",
    icon: agentIconName(agent.id),
    unknown: !hasUsage,
    limits: hasUsage ? buildLimits({
      account,
      now
    }) : []
  };
}

// Worst-first: agents with limits ranked by their most-consumed limit (desc);
// agents with none (unknown, or Claude before its usage loads) sink to the end.
// `Array.prototype.sort` is stable, so ties keep their first-seen order.
function sortWorstFirst(groups: AgentGroup[]): AgentGroup[] {
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

    return secondWorst.pct - firstWorst.pct;
  });
}

/** Build the agent groups from the running sessions + each agent's account usage.
 *  Distinct coding agents keyed by agent id (first occurrence wins); the shell
 *  fallback and terminal-editor (`editor-*`) sessions are excluded. Two agents
 *  whose accounts carry the same billing-account identity (Codex + opencode on
 *  one ChatGPT subscription) collapse into the first-seen agent's group — one
 *  account is never shown twice. Each agent reads its own windows from
 *  `accounts` (missing/`null` ⇒ an unknown group). Sorted worst-first. */
export function buildGroups({ accounts, sessions, now }: {
  accounts: ReadonlyMap<string, AccountUsage | null>;
  sessions: AgentSession[];
  now: number;
}): AgentGroup[] {
  const groups: AgentGroup[] = [];
  const seenAgentIds = new Set<string>();
  const seenAccountIds = new Set<string>();
  for (const session of sessions) {
    const agentId = session.agent.id;
    const account = accounts.get(agentId) ?? null;
    const accountId = account?.account ?? null;
    const isShellAgent = agentId === SHELL_AGENT_ID;
    const isEditorSession = agentId.startsWith(EDITOR_AGENT_ID_PREFIX);
    const alreadySeen = seenAgentIds.has(agentId);
    const accountAlreadyGrouped = accountId !== null && seenAccountIds.has(accountId);
    if (isShellAgent || isEditorSession || alreadySeen || accountAlreadyGrouped) {
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
    level: "crit",
    label: "critical"
  },
  {
    level: "warn",
    label: "near"
  },
  {
    level: "normal",
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
// no severity, so they're skipped here — but still counted in the running total.
export function severityBreakdown(agents: AgentGroup[]): SeveritySlice[] {
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
export type Spotlight = {
  agent: AgentGroup;
  limit: Limit;
};

export function findSpotlight(agents: AgentGroup[]): Spotlight | null {
  let closest: Spotlight | null = null;
  for (const agent of agents) {
    if (agent.limits.length === 0) {
      continue;
    }

    for (const limit of agent.limits) {
      if (!closest || limit.pct > closest.limit.pct) {
        closest = {
          agent,
          limit
        };
      }
    }
  }

  return closest;
}

// Distinct kind codes actually in play, each with the label it stands for — the
// legend that decodes the trigger's mono codes.
export type KindLegendEntry = {
  short: string;
  name: string;
};

export function buildKindLegend(agents: AgentGroup[]): KindLegendEntry[] {
  // Plain-object dedupe (not a Map — this is a pure derivation, not reactive
  // state) keyed by the short code, first label wins.
  const seen: Record<string, true> = {};
  const entries: KindLegendEntry[] = [];
  for (const agent of agents) {
    if (agent.limits.length === 0) {
      continue;
    }

    for (const limit of agent.limits) {
      if (!seen[limit.kindShort]) {
        seen[limit.kindShort] = true;
        entries.push({
          short: limit.kindShort,
          name: limit.label
        });
      }
    }
  }

  return entries;
}
