<script lang="ts">
  import { usage as usageApi } from "@/lib/bridge";
  import { formatCount, formatPercent } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import type { AccountUsage, AgentSession } from "@/lib/types";
  import {
    buildGroups,
    buildKindLegend,
    findSpotlight,
    severityBreakdown,
    worstLimit
  } from "@/lib/usage-groups";
  import type { AgentGroup, Level } from "@/lib/usage-groups";
  import { onDestroy } from "svelte";

  // The running agent sessions — one usage group per distinct coding agent among
  // them is derived below (App.svelte owns the list; the shell fallback and
  // terminal-editor sessions are filtered out in `buildGroups`).
  const { sessions }: { sessions: AgentSession[] } = $props();

  // Live account usage, grouped by agent (mirrors claude.ai / `claude /usage`):
  // each agent's rate-limit windows — the 5-hour session, the weekly all-models
  // cap, and any per-model weekly caps in use — read via the OAuth endpoint from
  // the local token. The backend caches it (~3 min), so this polls slowly.
  let account = $state<AccountUsage | null>(null);
  // A ticking clock so the "resets in …" countdowns stay live between polls.
  let now = $state(Date.now());

  const ACCOUNT_REFRESH_MS = 180_000;
  // The countdowns it feeds ("resets in 3h 30m") have minute granularity, so a
  // half-minute tick keeps them honest at a thirtieth of the repaints a
  // per-second clock would force (idle frames are what make the app heavy).
  const CLOCK_TICK_MS = 30_000;

  // How many per-agent pills the trigger shows before collapsing the rest into a
  // trailing "+N" overflow chip. Every agent renders as a pill — a single agent
  // still shows its full per-window breakdown, never a compacted chip.
  const MAX_TRIGGER_PILLS = 2;

  // One group per distinct running agent, worst-first (see usage-groups.ts). Claude
  // carries its real limits; every other agent is "unknown" (no local signal).
  const groups = $derived<AgentGroup[]>(
    buildGroups({
      account,
      sessions,
      now
    })
  );

  const pillGroups = $derived(groups.slice(0, MAX_TRIGGER_PILLS));
  const overflowCount = $derived(Math.max(0, groups.length - MAX_TRIGGER_PILLS));
  // The "+N" chip's severity dot: red if any agent is critical, amber if any is
  // near its cap, else no dot.
  const hasCriticalAgent = $derived(groups.some(group => worstLimit(group.limits)?.level === "crit"));
  const hasNearAgent = $derived(groups.some(group => worstLimit(group.limits)?.level === "warn"));
  const overflowLevel = $derived.by((): Level | null => {
    if (hasCriticalAgent) {
      return "crit";
    }

    if (hasNearAgent) {
      return "warn";
    }

    return null;
  });
  const overflowTooltip = $derived(
    `${formatCount(overflowCount)} more ${overflowCount === 1 ? "agent" : "agents"} — open for the full list`
  );

  const severitySlices = $derived(severityBreakdown(groups));
  // Agents that actually carry a severity — the distribution bar's denominator, so
  // its segments fill it exactly (unknown agents have no severity to plot).
  const measuredAgents = $derived(severitySlices.reduce((sum, slice) => sum + slice.count, 0));
  const spotlight = $derived(findSpotlight(groups));
  // The spotlight's countdown, phrased ("resets in 3h 30m"); an em-dash when its
  // reset time is unknown or already past.
  const spotlightReset = $derived(spotlight?.limit.reset ? `resets ${spotlight.limit.reset}` : "—");
  const kindLegend = $derived(buildKindLegend(groups));
  const runningLabel = $derived(`${formatCount(groups.length)} ${groups.length === 1 ? "agent" : "agents"} running`);

  const ariaLabel = $derived(
    groups.length > 0
      ? `Usage — ${groups
        .map(group => `${group.name} ${group.limits.map(limit => `${limit.label} ${formatPercent(limit.pct)}`).join(", ")}`)
        .join("; ")}`
      : "Usage details"
  );

  $effect(() => {
    let cancelled = false;
    void (async () => {
      const next = await usageApi.account().catch(() => null);
      if (!cancelled) {
        account = next;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  const accountTimer = setInterval(async () => {
    account = await usageApi.account().catch(() => null);
  }, ACCOUNT_REFRESH_MS);
  const clockTimer = setInterval(() => {
    now = Date.now();
  }, CLOCK_TICK_MS);
  onDestroy(() => {
    clearInterval(accountTimer);
    clearInterval(clockTimer);
  });
</script>

<span class="usage-wrap menu-host">
  <button
    style:anchor-name="--usage-anchor"
    class="pill menu-trigger"
    aria-label={ariaLabel}
    data-tooltip="Usage by agent"
    popovertarget="usage-menu"
  >
    {#if groups.length === 0}
      <span class="none">—</span>
    {:else}
      <span class="pills">
        {#each pillGroups as group (group.id)}
          {@const groupWorst = worstLimit(group.limits)}
          <span
            class="agent-pill sev"
            class:crit={groupWorst?.level === "crit"}
            class:unknown={group.unknown}
            class:warn={groupWorst?.level === "warn"}
          >
            <span class="agent-pill-id">
              <span class="agent-icon"><Icon name={group.icon} /></span>
              <span class="agent-pill-name">{group.shortName}</span>
            </span>
            {#if group.limits.length > 0}
              <span class="agent-pill-limits">
                {#each group.limits as limit (limit.label)}
                  <span
                    class="agent-pill-limit sev"
                    class:crit={limit.level === "crit"}
                    class:warn={limit.level === "warn"}
                  >
                    <span class="agent-pill-kind">{limit.kindShort}</span>
                    <span class="agent-pill-pct">{formatPercent(limit.pct)}</span>
                  </span>
                {/each}
              </span>
            {:else}
              <span class="agent-pill-dash">—</span>
            {/if}
          </span>
        {/each}
        {#if overflowCount > 0}
          <span
            class="overflow-chip sev"
            class:crit={overflowLevel === "crit"}
            class:warn={overflowLevel === "warn"}
            data-tooltip={overflowTooltip}
          >
            {#if overflowLevel}
              <span class="overflow-dot"></span>
            {/if}
            +{formatCount(overflowCount)}
          </span>
        {/if}
      </span>
    {/if}
    <span class="caret">▾</span>
  </button>

  <div id="usage-menu" style:position-anchor="--usage-anchor" class="panel" aria-label="Usage details" popover role="dialog">
    {#if groups.length > 0}
      <header class="summary">
        <div class="titles">
          <h2 class="title">Usage</h2>
          <p class="running">{runningLabel}</p>
        </div>
        <div class="severity-counts">
          {#each severitySlices as slice (slice.level)}
            {#if slice.count > 0}
              <span class="severity-count sev" class:crit={slice.level === "crit"} class:warn={slice.level === "warn"}>
                <span class="severity-dot"></span>
                <span class="severity-value">{formatCount(slice.count)}</span> {slice.label}
              </span>
            {/if}
          {/each}
        </div>
      </header>

      <div class="distribution">
        {#each severitySlices as slice (slice.level)}
          {#if slice.count > 0}
            <span
              style:inline-size="{(slice.count / measuredAgents) * 100}%"
              class="distribution-segment sev"
              class:crit={slice.level === "crit"}
              class:warn={slice.level === "warn"}
            ></span>
          {/if}
        {/each}
      </div>

      {#if spotlight}
        <section
          class="spotlight sev"
          class:crit={spotlight.limit.level === "crit"}
          class:warn={spotlight.limit.level === "warn"}
        >
          <div class="spotlight-head">
            <span class="spotlight-id">
              <span class="agent-icon"><Icon name={spotlight.agent.icon} /></span>
              <span class="spotlight-text">
                <span class="eyebrow">Closest to a limit</span>
                <span class="spotlight-title">{spotlight.agent.name} · {spotlight.limit.label}</span>
              </span>
            </span>
            <output class="spotlight-pct">{formatPercent(spotlight.limit.pct)}</output>
          </div>
          <span class="track">
            <span style:inline-size="{spotlight.limit.pct}%" class="track-fill"></span>
          </span>
          <span class="spotlight-reset">{spotlightReset}</span>
        </section>
      {/if}

      <div class="agents-head">
        <span class="eyebrow">All agents</span>
        {#if kindLegend.length > 0}
          <span class="kind-legend">
            {#each kindLegend as entry (entry.short)}
              <span class="kind-legend-item">
                <span class="kind-code">{entry.short}</span>{entry.name}
              </span>
            {/each}
          </span>
        {/if}
      </div>

      <ul class="agents">
        {#each groups as group (group.id)}
          {@const groupWorst = worstLimit(group.limits)}
          <li
            class="agent-card sev"
            class:crit={groupWorst?.level === "crit"}
            class:unknown={group.unknown}
            class:warn={groupWorst?.level === "warn"}
          >
            <div class="agent-head">
              <span class="agent-icon"><Icon name={group.icon} /></span>
              <span class="agent-name">{group.name}</span>
              <span class="agent-plan">{group.plan || "usage not available locally"}</span>
            </div>
            {#if group.limits.length > 0}
              <div class="limits">
                {#each group.limits as limit (limit.label)}
                  <div
                    class="limit sev"
                    class:crit={limit.level === "crit"}
                    class:warn={limit.level === "warn"}
                  >
                    <span class="limit-kind">{limit.kindShort}</span>
                    <span class="limit-track">
                      <span style:inline-size="{limit.pct}%" class="limit-fill"></span>
                    </span>
                    <output class="limit-pct">{formatPercent(limit.pct)}</output>
                    <span class="limit-reset" data-tooltip={limit.resetAt || undefined}>
                      {#if limit.reset}
                        <Icon name="clock" />{limit.reset}
                      {/if}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">No usage to show yet — sign in to Claude Code to load your limits.</p>
    {/if}
  </div>
</span>

<style>
  .usage-wrap {
    flex-shrink: 0;
  }

  /* Severity color, shared: the level class sets --sev (and a faint --sev-wash
     tint for warn/crit) once, and every bar / number / row below just reads it
     (DRY). --sev-wash stays unset at "normal" — there is no primary wash. */
  .sev {
    --sev: var(--primary);

    &.warn {
      --sev: var(--warning);
      --sev-wash: var(--warning-wash);
    }

    &.crit {
      --sev: var(--critical);
      --sev-wash: var(--critical-wash);
    }
  }

  .pill {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 11px 9px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .none {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 12px;
    }

    .agent-icon {
      display: inline-flex;
      flex: none;
      color: var(--sev);
      font-size: 13px;
    }

    /* ── Per-agent pills + a "+N" overflow chip ── */
    .pills {
      display: inline-flex;
      gap: 4px;
      align-items: center;
    }

    /* One pill per agent: icon + short name, then a stack per limit (kind over pct). */
    .agent-pill {
      display: inline-flex;
      gap: 8px;
      align-items: center;
      padding-block: 4px;
      padding-inline: 7px 8px;
      border-radius: var(--radius-small);
      background: var(--surface-1);

      /* No trustworthy usage → mute the whole pill; the dash carries the meaning. */
      &.unknown {
        --sev: var(--on-surface-variant);
      }
    }

    .agent-pill-id {
      display: inline-flex;
      flex: none;
      gap: 6px;
      align-items: center;
    }

    .agent-pill-name {
      overflow: hidden;
      max-inline-size: 4rem;
      color: var(--on-surface);
      font-weight: 600;
      font-size: 11px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .agent-pill-limits {
      display: inline-flex;
      gap: 8px;
      align-items: stretch;
    }

    /* Each limit is a tight vertical stack: the mono kind letter over its percent. */
    .agent-pill-limit {
      display: inline-flex;
      flex-direction: column;
      gap: 1px;
      align-items: center;
      line-height: 1;

      .agent-pill-kind {
        color: var(--on-surface-variant);
        font-family: var(--font-monospace);
        font-weight: 800;
        font-size: 8px;
        letter-spacing: 0.02em;
      }

      .agent-pill-pct {
        color: var(--sev);
        font-weight: 700;
        font-size: 12px;
        font-variant-numeric: tabular-nums;
      }
    }

    .agent-pill-dash {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
    }

    /* The "+N" overflow: a muted count with a severity dot when any agent is hot. */
    .overflow-chip {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      padding-block: 3px;
      padding-inline: 7px;
      border-radius: var(--radius-small);
      background: var(--surface-1);
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      font-variant-numeric: tabular-nums;

      .overflow-dot {
        flex: none;
        block-size: 6px;
        inline-size: 6px;
        border-radius: 999px;
        background: var(--sev);
      }
    }

    .caret {
      font-size: 10px;
      opacity: 60%;
    }
  }

  .panel {
    position: absolute;
    inset: auto;

    /* A native [popover] defaults to overflow:auto — clip so no stray scrollbar
       shows; the inner .agents list owns vertical scrolling. */
    overflow: clip;
    inline-size: min(29rem, 92vw);
    margin-block-start: 6px;
    padding: 15px;
    border: 1px solid var(--outline);
    border-radius: 16px;
    background: var(--surface-2);
    color: var(--on-surface);
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-left;
    position-try-fallbacks: flip-block, flip-inline, flip-block flip-inline;

    /* Small uppercase section label, shared by the spotlight + "all agents". */
    .eyebrow {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* An agent's icon, tinted by whichever severity its context set on --sev. */
    .agent-icon {
      display: inline-flex;
      flex: none;
      color: var(--sev);
      font-size: 15px;
    }

    /* ── Header: title + running count, plus per-severity agent tallies ── */
    .summary {
      display: flex;
      gap: 12px;
      justify-content: space-between;
      align-items: flex-start;
      margin-block-end: 11px;
    }

    .titles {
      display: flex;
      flex-direction: column;
      line-height: 1.15;
    }

    .title {
      margin: 0;
      font-weight: 700;
      font-size: 15px;
      letter-spacing: -0.01em;
    }

    .running {
      margin: 0;
      margin-block-start: 1px;
      color: var(--on-surface-variant);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
    }

    .severity-counts {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      justify-content: flex-end;
      padding-block-start: 2px;
    }

    .severity-count {
      display: inline-flex;
      gap: 5px;
      align-items: center;
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 11px;

      .severity-dot {
        flex: none;
        block-size: 8px;
        inline-size: 8px;
        border-radius: 999px;
        background: var(--sev);
      }

      .severity-value {
        font-variant-numeric: tabular-nums;
      }
    }

    /* ── Distribution: one pill-track split by how agents bucket on severity ── */
    .distribution {
      display: flex;
      gap: 2px;
      overflow: clip;
      block-size: 6px;
      margin-block-end: 15px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .distribution-segment {
      min-inline-size: 2px;
      background: var(--sev);
      transition: inline-size 300ms var(--ease), background 300ms var(--ease);
    }

    /* ── Spotlight: the single limit closest to its cap across all agents ── */
    .spotlight {
      margin-block-end: 14px;
      padding: 12px 13px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-1);

      .spotlight-head {
        display: flex;
        gap: 10px;
        justify-content: space-between;
        align-items: center;
        margin-block-end: 9px;
      }

      .spotlight-id {
        display: inline-flex;
        gap: 8px;
        align-items: center;
        min-inline-size: 0;
      }

      .spotlight-text {
        display: flex;
        flex-direction: column;
        min-inline-size: 0;
        line-height: 1.2;
      }

      .eyebrow {
        font-size: 9px;
        letter-spacing: 0.09em;
      }

      .spotlight-title {
        overflow: hidden;
        font-weight: 700;
        font-size: 13px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .spotlight-pct {
        flex: none;
        color: var(--sev);
        font-weight: 800;
        font-size: 22px;
        line-height: 1;
        font-variant-numeric: tabular-nums;
      }

      .spotlight-reset {
        display: block;
        margin-block-start: 7px;
        color: var(--on-surface-variant);
        font-size: 11px;
        font-variant-numeric: tabular-nums;
      }
    }

    /* The spotlight's full-width progress track + its severity-colored fill. */
    .track {
      display: block;
      overflow: clip;
      block-size: 8px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .track-fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--sev);
      transition: inline-size 300ms var(--ease), background 300ms var(--ease);
    }

    /* ── "All agents" heading + kind-code legend ── */
    .agents-head {
      display: flex;
      gap: 8px;
      justify-content: space-between;
      align-items: center;
      margin-block-end: 8px;
    }

    .kind-legend {
      display: flex;
      flex-wrap: wrap;
      gap: 5px 10px;
      justify-content: flex-end;
    }

    .kind-legend-item {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      color: var(--on-surface-variant);
      font-size: 9px;
    }

    /* The legend's mono kind code (a small filled square). */
    .kind-code {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      block-size: 14px;
      inline-size: 14px;
      border-radius: 4px;
      background: var(--surface-3);
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 8px;
    }

    /* ── Agent cards: a header, then one full-width bar row per active limit ── */
    .agents {
      display: flex;
      flex-direction: column;
      gap: 7px;
      overflow-y: auto;
      max-block-size: min(46vh, 22rem);
      margin: 0;
      padding: 0;
      list-style: none;
    }

    .agent-card {
      display: flex;
      flex-direction: column;
      gap: 9px;
      padding: 11px 12px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-1);

      /* Near/crit cards lift onto the severity wash with a matching hairline. */
      &.warn,
      &.crit {
        border-color: color-mix(in sRGB, var(--sev) 35%, var(--outline));
        background: var(--sev-wash);
      }

      /* An unknown agent has no severity — mute its icon so the primary blue
         never reads as a (fabricated) "healthy" score. */
      &.unknown {
        --sev: var(--on-surface-variant);
      }
    }

    /* Header: agent icon + name, with the plan trailing it, muted. */
    .agent-head {
      display: flex;
      gap: 7px;
      align-items: center;
      min-inline-size: 0;
    }

    .agent-name {
      overflow: hidden;
      font-weight: 700;
      font-size: 12.5px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .agent-plan {
      overflow: hidden;
      color: var(--on-surface-variant);
      font-size: 10px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* One shared grid across every limit row so the badge / bar / percent /
       reset columns line up down the card (each row is display: contents). */
    .limits {
      display: grid;
      grid-template-columns: auto 1fr auto auto;
      gap: 8px 9px;
      align-items: center;
    }

    .limit {
      display: contents;
    }

    /* A limit's single-letter kind badge (S / W / O), tinted neutral. */
    .limit-kind {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      block-size: 17px;
      min-inline-size: 17px;
      padding-inline: 3px;
      border-radius: 5px;
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 9px;
    }

    .limit-track {
      display: block;
      overflow: clip;
      block-size: 7px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .limit-fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--sev);
      transition: inline-size 300ms var(--ease), background 300ms var(--ease);
    }

    .limit-pct {
      min-inline-size: 2.5rem;
      color: var(--sev);
      font-weight: 700;
      font-size: 11.5px;
      font-variant-numeric: tabular-nums;
      text-align: end;
    }

    /* Clock + live "in …" countdown, right-aligned to the card edge. */
    .limit-reset {
      display: inline-flex;
      gap: 3px;
      align-items: center;
      justify-self: end;
      color: var(--on-surface-variant);
      font-size: 10px;
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }

    .empty {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 12px;
      line-height: 1.5;
    }
  }
</style>
