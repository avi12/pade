<script lang="ts">
  import { usage as usageApi } from "@/lib/bridge";
  import { formatCount, formatPercent } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import type { AccountUsage } from "@/lib/types";
  import { onDestroy } from "svelte";

  // Live account usage, grouped by agent (mirrors claude.ai / `claude /usage`):
  // each agent's rate-limit windows — the 5-hour session, the weekly all-models
  // cap, and any per-model weekly caps in use — read via the OAuth endpoint from
  // the local token. The backend caches it (~3 min), so this polls slowly.
  let account = $state<AccountUsage | null>(null);
  // A ticking clock so the "resets in …" countdowns stay live between polls.
  let now = $state(Date.now());

  const ACCOUNT_REFRESH_MS = 180_000;
  const CLOCK_TICK_MS = 1_000;

  type Level = "normal" | "warn" | "crit";

  // The three limit kinds; the short mono code in the "all agents" chips maps off
  // this closed set (one authoritative definition, no bare string literals).
  const LimitKind = {
    Session: "session",
    Weekly: "weekly",
    Model: "model"
  } as const;
  type LimitKind = (typeof LimitKind)[keyof typeof LimitKind];

  type Limit = {
    label: string;
    sub: string;
    reset: string;
    pct: number;
    level: Level;
    kind: LimitKind;
    /** A 2-char mono code shown in the "all agents" chips + legend ("5h", "wk"). */
    kindShort: string;
  };
  type AgentGroup = {
    name: string;
    plan: string;
    icon: IconName;
    limits: Limit[];
  };

  // Severity by consumption: blue while there's room, amber past 75%, red past
  // 90% — no green, per the design's usage semantics. Applied as a CSS class.
  function limitLevel(pct: number): Level {
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

  // ISO reset time → a live "resets in …" countdown (largest two units), or "".
  function resetLabel(iso: string | null | undefined, nowMs: number): string {
    if (!iso) {
      return "";
    }

    const remaining = parseIso(iso) - nowMs;
    if (!Number.isFinite(remaining) || remaining <= 0) {
      return "";
    }

    const totalSeconds = Math.floor(remaining / 1000);
    const days = Math.floor(totalSeconds / 86_400);
    const hours = Math.floor((totalSeconds % 86_400) / 3_600);
    const minutes = Math.floor((totalSeconds % 3_600) / 60);
    const seconds = totalSeconds % 60;
    if (days > 0) {
      return `resets in ${days}d ${hours}h`;
    }

    if (hours > 0) {
      return `resets in ${hours}h ${minutes}m`;
    }

    if (minutes > 0) {
      return `resets in ${minutes}m ${String(seconds).padStart(2, "0")}s`;
    }

    return `resets in ${seconds}s`;
  }

  // The worst-consumed limit in a set — drives the agent chip's readout color and
  // the panel's "near limit" signal.
  function worstLimit(limits: Limit[]): Limit | null {
    return limits.length > 0 ? limits.reduce((max, limit) => (limit.pct > max.pct ? limit : max)) : null;
  }

  // A 2-letter mono code for a per-model weekly limit (the backend sends none) —
  // the model's first distinctive word, e.g. "Claude Opus" → "OP".
  function modelShort(name: string): string {
    const tokens = name.split(/[^a-z0-9]+/i).filter(Boolean);
    const word = tokens.find(token => token.toLowerCase() !== "claude") ?? tokens[0] ?? name;
    return word.slice(0, 2).toUpperCase();
  }

  // Build the agent groups from the account. Claude is the only agent that
  // exposes usage locally, so this is one group today — but the shape supports
  // more. Only limits actually in use (> 0%) are kept.
  const groups = $derived<AgentGroup[]>(buildGroups(account, now));

  function buildGroups(usage: AccountUsage | null, nowMs: number): AgentGroup[] {
    if (!usage) {
      return [];
    }

    const limits: Limit[] = [];
    function add({ label, sub, kind, kindShort, pct, resetsAt }: {
      label: string;
      sub: string;
      kind: LimitKind;
      kindShort: string;
      pct: number | null | undefined;
      resetsAt: string | null | undefined;
    }): void {
      if (typeof pct !== "number" || pct <= 0) {
        return;
      }

      const value = clamp(pct);
      limits.push({
        label,
        sub,
        kind,
        kindShort,
        pct: value,
        level: limitLevel(value),
        reset: resetLabel(resetsAt, nowMs)
      });
    }

    add({
      label: "Session",
      sub: "5-hour window",
      kind: LimitKind.Session,
      kindShort: "5h",
      pct: usage.fiveHour?.utilization,
      resetsAt: usage.fiveHour?.resetsAt
    });
    add({
      label: "Weekly",
      sub: "all models",
      kind: LimitKind.Weekly,
      kindShort: "wk",
      pct: usage.sevenDay?.utilization,
      resetsAt: usage.sevenDay?.resetsAt
    });
    for (const model of usage.models) {
      add({
        label: model.name,
        sub: "weekly",
        kind: LimitKind.Model,
        kindShort: modelShort(model.name),
        pct: model.utilization,
        resetsAt: model.resetsAt
      });
    }

    if (limits.length === 0) {
      return [];
    }

    return [{
      name: "Claude Code",
      plan: usage.plan,
      icon: "sparkles",
      limits
    }];
  }

  // ── Panel view-model ──────────────────────────────────────────────────────
  // Worst-first severity buckets: how many agents sit at crit / near / healthy,
  // by each agent's most-consumed limit. Feeds both the header tallies and the
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

  type SeveritySlice = {
    level: Level;
    label: string;
    count: number;
  };

  function severityBreakdown(agents: AgentGroup[]): SeveritySlice[] {
    return SEVERITY_ORDER.map(severity => ({
      level: severity.level,
      label: severity.label,
      count: agents.filter(agent => (worstLimit(agent.limits)?.level ?? "normal") === severity.level).length
    }));
  }

  // The single limit closest to its cap across every agent, tagged with its owner.
  type Spotlight = {
    agent: AgentGroup;
    limit: Limit;
  };

  function findSpotlight(agents: AgentGroup[]): Spotlight | null {
    let closest: Spotlight | null = null;
    for (const agent of agents) {
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
  // legend that decodes the "all agents" chips.
  type KindLegendEntry = {
    short: string;
    name: string;
  };

  function buildKindLegend(agents: AgentGroup[]): KindLegendEntry[] {
    // Plain-object dedupe (not a Map — this is a pure derivation, not reactive
    // state) keyed by the short code, first label wins.
    const seen: Record<string, true> = {};
    const entries: KindLegendEntry[] = [];
    for (const agent of agents) {
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

  const severitySlices = $derived(severityBreakdown(groups));
  const spotlight = $derived(findSpotlight(groups));
  const kindLegend = $derived(buildKindLegend(groups));
  const runningLabel = $derived(`${formatCount(groups.length)} running`);

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
    <span class="tag">Usage</span>
    {#if groups.length > 0}
      <span class="chips">
        {#each groups as group (group.name)}
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
          {/if}
        {/each}
      </span>
    {:else}
      <span class="none">—</span>
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
              style:inline-size="{(slice.count / groups.length) * 100}%"
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
        {#each groups as group (group.name)}
          {@const groupWorst = worstLimit(group.limits)}
          <li
            class="agent-row sev"
            class:crit={groupWorst?.level === "crit"}
            class:warn={groupWorst?.level === "warn"}
          >
            <span class="rail"></span>
            <span class="agent-icon"><Icon name={group.icon} /></span>
            <span class="agent-id">
              <span class="agent-name">{group.name}</span>
              <span class="agent-plan">{group.plan}</span>
            </span>
            <span class="agent-limits">
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
