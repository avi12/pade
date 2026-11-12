// Cross-agent views of the groups: the severity breakdown, the spotlighted limit,
// just-reset detection and the kind legend.

import { type AgentGroup, Level, type Limit } from "@/lib/usage/groups/usage-model";
import { outranks, worstLimit, worstRanked } from "@/lib/usage/groups/limit-ranking";
import type { RankedLimit } from "@/lib/usage/groups/limit-ranking";

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
export type Spotlight = {
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
