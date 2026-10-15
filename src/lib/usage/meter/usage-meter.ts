// The usage meter's view logic, kept pure so the trigger, the details panel and
// the meter itself all read it from one place (usage-groups.ts builds the data).

import { AgentId } from "@/lib/agent/agent-icon";
import type { AccountUsage } from "@/lib/types";
import {
  type AgentGroup,
  Level,
  type Limit,
  limitKey,
  type SeveritySlice,
  usageResetTime,
  worstLimit
} from "@/lib/usage/usage-groups";

/** How many per-agent pills the trigger shows before collapsing the rest into a
 *  trailing "+N" overflow chip. Every agent renders as a pill - a single agent
 *  still shows its full per-window breakdown, never a compacted chip. */
const MAX_TRIGGER_PILLS = 2;

export const USAGE_MENU_ID = "usage-menu";
export const USAGE_MENU_ANCHOR = "--usage-anchor";
export const USAGE_PANEL_LABEL = "Usage details";

/** What a reading shows when there is nothing to read: no agents, an agent
 *  without limits, a spotlight whose reset time is unknown or already past. */
export const NO_READING = "-";

/** The `.severity` modifier each level paints with; a normal level takes none. */
const SEVERITY_CLASS: Record<Level, string | undefined> = {
  [Level.Normal]: undefined,
  [Level.Warning]: "warning",
  [Level.Critical]: "critical"
};

export function severityClass(level: Level | null | undefined) {
  if (!level) {
    return undefined;
  }

  return SEVERITY_CLASS[level];
}

/** An agent paints with its worst limit's severity. */
export function groupSeverityClass(group: AgentGroup) {
  return severityClass(worstLimit(group.limits)?.level);
}

/** The agents that get a pill of their own in the trigger. */
export function triggerPillGroups(groups: readonly AgentGroup[]) {
  return groups.slice(0, MAX_TRIGGER_PILLS);
}

/** Whether Claude has a pill of its own - not folded into the "+N" chip. */
export function hasClaudePill(groups: readonly AgentGroup[]) {
  return triggerPillGroups(groups).some(group => group.id === AgentId.Claude);
}

/** How many agents fold into the trigger's "+N" chip. */
export function overflowCount(groups: readonly AgentGroup[]) {
  return Math.max(0, groups.length - MAX_TRIGGER_PILLS);
}

/** The "+N" chip's severity dot: red if any agent is critical, amber if any is
 *  near its cap, else no dot. */
export function overflowLevel(groups: readonly AgentGroup[]) {
  const levels = new Set(groups.map(group => worstLimit(group.limits)?.level));
  if (levels.has(Level.Critical)) {
    return Level.Critical;
  }

  if (levels.has(Level.Warning)) {
    return Level.Warning;
  }

  return null;
}

/** Agents that actually carry a severity - the distribution bar's denominator,
 *  so its segments fill it exactly (unknown agents have no severity to plot). */
export function measuredAgentCount(slices: readonly SeveritySlice[]) {
  let total = 0;
  for (const slice of slices) {
    total += slice.count;
  }

  return total;
}

export function usageAriaLabel(groups: readonly AgentGroup[]) {
  if (groups.length === 0) {
    return USAGE_PANEL_LABEL;
  }

  const readings = groups.map(group => `${group.name} ${group.limits.map(limit => `${limit.label} ${limit.reading}`).join(", ")}`);
  return `Usage - ${readings.join("; ")}`;
}

/** Whether a limit is among those that just started over, so its number flashes. */
export function isJustReset({ justReset, agentId, limit }: {
  justReset: ReadonlySet<string>;
  agentId: string;
  limit: Limit;
}) {
  return justReset.has(
    limitKey({
      agentId,
      limit
    })
  );
}

type ResetBoundary = {
  /** The raw `resetsAt` value - the boundary's identity, so it refreshes once. */
  value: string;
  time: number;
};

/** The reset boundaries the accounts' windows cross: one already passed that has
 *  not been refreshed for yet, and the soonest still ahead. */
export function pendingResetBoundaries({ accounts, refreshed, now }: {
  accounts: ReadonlyMap<string, AccountUsage | null>;
  refreshed: ReadonlySet<string>;
  now: number;
}) {
  const boundaries: ResetBoundary[] = [];
  for (const account of accounts.values()) {
    for (const window of account?.windows ?? []) {
      const time = usageResetTime(window.resetsAt);
      if (window.resetsAt && Number.isFinite(time)) {
        boundaries.push({
          value: window.resetsAt,
          time
        });
      }
    }
  }

  const expired = boundaries.find(boundary => boundary.time <= now && !refreshed.has(boundary.value));
  const next = boundaries
    .filter(boundary => boundary.time > now)
    .sort((first, second) => first.time - second.time)[0];
  return {
    expired,
    next
  };
}

/** How long a meter keeps showing an agent's last real reading while its usage
 *  endpoint refuses to answer (a rate limit, a network blip) - long enough to
 *  ride out a refusal, short enough that a reading never passes for current. */
export const KEEP_LAST_READING_MS = 60 * 60_000;

/** One agent's last real reading and when it arrived. */
export interface KeptReading {
  account: AccountUsage;
  at: number;
}

/** The accounts to show for a fresh round of readings: a real reading replaces
 *  the kept one; a failed one (null) falls back to the last real reading while
 *  it is younger than KEEP_LAST_READING_MS, so a refused request no longer
 *  blanks the meter. Returns the accounts and the updated kept readings. */
export function withLastGoodReadings({ fresh, kept, now }: {
  fresh: ReadonlyMap<string, AccountUsage | null>;
  kept: ReadonlyMap<string, KeptReading>;
  now: number;
}) {
  const accounts = new Map<string, AccountUsage | null>();
  const nextKept = new Map(kept);
  for (const [agentId, account] of fresh) {
    if (account !== null) {
      nextKept.set(agentId, {
        account,
        at: now
      });
      accounts.set(agentId, account);
      continue;
    }

    const last = kept.get(agentId);
    const isLastFresh = last !== undefined && now - last.at <= KEEP_LAST_READING_MS;
    accounts.set(agentId, isLastFresh ? last.account : null);
  }

  return {
    accounts,
    kept: nextKept
  };
}
