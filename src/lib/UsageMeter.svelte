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
  } from "@/lib/usageGroups";
  import type { AgentGroup, Level } from "@/lib/usageGroups";
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
  const CLOCK_TICK_MS = 1_000;

  // Trigger compaction threshold: at or below this many running agents the trigger
  // shows a wide chip per agent; beyond it, compact per-agent pills + a trailing
  // "+N" overflow chip.
  const FEW_AGENTS_MAX = 2;

  // One group per distinct running agent, worst-first (see usageGroups.ts). Claude
  // carries its real limits; every other agent is "unknown" (no local signal).
  const groups = $derived<AgentGroup[]>(
    buildGroups({
      account,
      sessions,
      now
    })
  );

  const isFewAgents = $derived(groups.length <= FEW_AGENTS_MAX);
  const pillGroups = $derived(groups.slice(0, FEW_AGENTS_MAX));
  const overflowCount = $derived(Math.max(0, groups.length - FEW_AGENTS_MAX));
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

<span class="usage-wrap">
  <button
    style:anchor-name="--usage-anchor"
    class="pill"
    aria-label={ariaLabel}
    data-tooltip="Usage by agent"
    popovertarget="usage-menu"
  >
    {#if isFewAgents}
      <span class="tag">Usage</span>
    {/if}
    {#if groups.length === 0}
      <span class="none">—</span>
    {:else if isFewAgents}
      <span class="chips">
        {#each groups as group (group.id)}
          {@const groupWorst = worstLimit(group.limits)}
          {#if groupWorst}
            <span
              class="chip sev"
              class:crit={groupWorst.level === "crit"}
              class:warn={groupWorst.level === "warn"}
            >
              <span class="agent-icon"><Icon name={group.icon} /></span>
              <span class="bars">
                {#each group.limits as limit (limit.label)}
                  <span class="bar" data-tooltip="{limit.label} · {formatPercent(limit.pct)}">
                    <span
                      style:block-size="{limit.pct}%"
                      class="barfill sev"
                      class:crit={limit.level === "crit"}
                      class:warn={limit.level === "warn"}
                    ></span>
                  </span>
                {/each}
              </span>
              <span class="pct">{formatPercent(groupWorst.pct)}</span>
            </span>
          {:else}
            <span class="chip unknown">
              <span class="agent-icon"><Icon name={group.icon} /></span>
              <span class="none">—</span>
            </span>
          {/if}
        {/each}
      </span>
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
            <span class="agent-pill-sep"></span>
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
          <span class="spotlight-reset">{spotlight.limit.reset || "—"}</span>
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
            class="agent-row sev"
            class:crit={groupWorst?.level === "crit"}
            class:unknown={group.unknown}
            class:warn={groupWorst?.level === "warn"}
          >
            <span class="rail"></span>
            <span class="agent-icon"><Icon name={group.icon} /></span>
            <span class="agent-id">
              <span class="agent-name">{group.name}</span>
              <span class="agent-plan">{group.plan || "usage not available locally"}</span>
            </span>
            <span class="agent-limits">
              {#if group.limits.length > 0}
                {#each group.limits as limit (limit.label)}
                  <span
                    class="limit-chip sev"
                    class:crit={limit.level === "crit"}
                    class:warn={limit.level === "warn"}
                  >
                    <span class="chip-kind">{limit.kindShort}</span>
                    <span class="chip-track">
                      <span style:inline-size="{limit.pct}%" class="chip-fill"></span>
                    </span>
                    <output class="chip-pct">{formatPercent(limit.pct)}</output>
                  </span>
                {/each}
              {:else}
                <span class="agent-limits-none">—</span>
              {/if}
            </span>
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
    gap: 9px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 11px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .tag {
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 11px;
    }

    .none {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 12px;
    }

    .chips {
      display: inline-flex;
      gap: 6px;
      align-items: center;
    }

    /* One chip per agent: icon + a row of tiny severity bar-stacks + worst %. */
    .chip {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding-block: 3px;
      padding-inline: 7px;
      border-radius: var(--radius-small);
      background: var(--surface-1);
    }

    /* An agent with no local usage signal — icon + a muted em-dash, no numbers. */
    .chip.unknown {
      color: var(--on-surface-variant);

      .agent-icon {
        color: var(--on-surface-variant);
      }
    }

    .agent-icon {
      display: inline-flex;
      flex: none;
      color: var(--sev);
      font-size: 13px;
    }

    /* A mini bar-chart — one bottom-aligned track per active limit. */
    .bars {
      display: inline-flex;
      gap: 2px;
      align-items: flex-end;
      block-size: 15px;
    }

    .bar {
      display: flex;
      flex: none;
      align-items: flex-end;
      block-size: 100%;
      inline-size: 4.5px;
      border-radius: 2px;
      background: var(--surface-3);

      /* Each bar is tiny and sits right under the pointer, so widen the gap to
         drop its tooltip clear of the cursor and the whole pill (base gap is 6px). */
      &[data-tooltip]::after {
        margin-block-start: 16px;
      }
    }

    .barfill {
      min-block-size: 2px;
      inline-size: 100%;
      border-radius: 2px;
      background: var(--sev);
      transition: block-size 300ms var(--ease), background 300ms var(--ease);
    }

    .pct {
      color: var(--sev);
      font-weight: 700;
      font-size: 12px;
      font-variant-numeric: tabular-nums;
    }

    /* ── Many-agents mode: compact per-agent pills + a "+N" overflow chip ── */
    .pills {
      display: inline-flex;
      gap: 5px;
      align-items: center;
    }

    /* One pill per agent: icon + short name · a divider · per-limit kind + pct. */
    .agent-pill {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 3px;
      padding-inline: 8px;
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
      gap: 5px;
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

    /* A hairline splitting the agent's identity from its readouts. */
    .agent-pill-sep {
      flex: none;
      align-self: stretch;
      inline-size: 1px;
      margin-block: 1px;
      background: var(--outline);
    }

    .agent-pill-limits {
      display: inline-flex;
      gap: 6px;
      align-items: center;
    }

    .agent-pill-limit {
      display: inline-flex;
      gap: 3px;
      align-items: center;
      font-weight: 700;
      font-size: 10px;
      font-variant-numeric: tabular-nums;

      .agent-pill-kind {
        color: var(--on-surface-variant);
        font-family: var(--font-monospace);
        font-weight: 800;
      }

      .agent-pill-pct {
        color: var(--sev);
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
      gap: 10px;
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
      gap: 5px 10px;
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
      gap: 10px;
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

    /* ── Agent rows ── */
    .agents {
      display: flex;
      flex-direction: column;
      gap: 5px;
      overflow-y: auto;
      max-block-size: min(46vh, 22rem);
      margin: 0;
      padding: 0;
      list-style: none;
    }

    .agent-row {
      display: flex;
      gap: 9px;
      align-items: center;
      padding-block: 8px;
      padding-inline: 6px 10px;
      border: 1px solid var(--outline);
      border-radius: 10px;
      background: transparent;

      /* Rows at near/crit pick up the faint severity wash. */
      &.warn,
      &.crit {
        background: var(--sev-wash);
      }

      /* An unknown agent has no severity — mute its rail + icon so the primary
         blue never reads as a (fabricated) "healthy" score. */
      &.unknown {
        --sev: var(--on-surface-variant);
      }

      .rail {
        flex: none;
        align-self: stretch;
        inline-size: 3px;
        border-radius: 999px;
        background: var(--sev);
      }

      .agent-icon {
        font-size: 14px;
      }
    }

    .agent-id {
      display: flex;
      flex: 0 0 auto;
      flex-direction: column;
      min-inline-size: 4.5rem;
      max-inline-size: 8rem;
      line-height: 1.2;
    }

    .agent-name {
      overflow: hidden;
      font-weight: 700;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .agent-plan {
      overflow: hidden;
      color: var(--on-surface-variant);
      font-size: 9px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .agent-limits {
      display: flex;
      flex: 1;
      flex-wrap: wrap;
      gap: 5px 8px;
      justify-content: flex-end;
      min-inline-size: 0;
    }

    /* Stands in for the readouts on an unknown agent's row. */
    .agent-limits-none {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 12px;
    }

    /* One per-limit chip: mono kind code + mini track + tabular percent. */
    .limit-chip {
      display: inline-flex;
      gap: 5px;
      align-items: center;

      .chip-kind {
        color: var(--on-surface-variant);
        font-family: var(--font-monospace);
        font-weight: 800;
        font-size: 9px;
      }

      .chip-track {
        display: block;
        flex: none;
        overflow: clip;
        block-size: 6px;
        inline-size: 32px;
        border-radius: 999px;
        background: var(--surface-3);
      }

      .chip-fill {
        display: block;
        block-size: 100%;
        border-radius: 999px;
        background: var(--sev);
        transition: inline-size 300ms var(--ease), background 300ms var(--ease);
      }

      .chip-pct {
        min-inline-size: 26px;
        color: var(--sev);
        font-weight: 700;
        font-size: 11px;
        font-variant-numeric: tabular-nums;
        text-align: end;
      }
    }

    .empty {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 12px;
      line-height: 1.5;
    }
  }
</style>
