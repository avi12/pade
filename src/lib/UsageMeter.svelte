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
  type Limit = {
    label: string;
    sub: string;
    reset: string;
    pct: number;
    level: Level;
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

  // Build the agent groups from the account. Claude is the only agent that
  // exposes usage locally, so this is one group today — but the shape supports
  // more. Only limits actually in use (> 0%) are kept.
  const groups = $derived<AgentGroup[]>(buildGroups(account, now));

  function buildGroups(usage: AccountUsage | null, nowMs: number): AgentGroup[] {
    if (!usage) {
      return [];
    }

    const limits: Limit[] = [];
    function add(
      label: string,
      sub: string,
      pct: number | null | undefined,
      resetsAt: string | null | undefined
    ): void {
      if (typeof pct !== "number" || pct <= 0) {
        return;
      }

      const value = clamp(pct);
      limits.push({
        label,
        sub,
        pct: value,
        level: limitLevel(value),
        reset: resetLabel(resetsAt, nowMs)
      });
    }

    add("Session", "5-hour window", usage.fiveHour?.utilization, usage.fiveHour?.resetsAt);
    add("Weekly", "all models", usage.sevenDay?.utilization, usage.sevenDay?.resetsAt);
    for (const model of usage.models) {
      add(model.name, "weekly", model.utilization, model.resetsAt);
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

  const allLimits = $derived(groups.flatMap(group => group.limits));
  const worst = $derived(worstLimit(allLimits));
  const isNearLimit = $derived(worst !== null && worst.level !== "normal");
  const nearLabel = $derived(worst?.level === "crit" ? "at limit" : "near limit");
  const agentCountLabel = $derived(`${formatCount(groups.length)} ${groups.length === 1 ? "agent" : "agents"}`);

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
    <div class="phead">
      <div class="eyebrow">Usage by agent</div>
      {#if groups.length > 0}
        <div class="pmeta">
          <span class="pcount">{agentCountLabel}</span>
          {#if worst && isNearLimit}
            <span class="near sev" class:crit={worst.level === "crit"} class:warn={worst.level === "warn"}>
              <span class="near-dot"></span>{nearLabel}
            </span>
          {/if}
        </div>
      {/if}
    </div>

    {#if groups.length > 0}
      <div class="scroll">
        {#each groups as group (group.name)}
          {@const groupWorst = worstLimit(group.limits)}
          <section
            class="group sev"
            class:crit={groupWorst?.level === "crit"}
            class:warn={groupWorst?.level === "warn"}
          >
            <div class="ghead">
              <span class="gid">
                <span class="agent-icon"><Icon name={group.icon} /></span>
                <span class="gtext">
                  <span class="gname">{group.name}</span>
                  <span class="gplan">{group.plan}</span>
                </span>
              </span>
              <span class="gcount">{formatCount(group.limits.length)} {#if group.limits.length === 1}
                limit{:else}limits{/if}</span>
            </div>
            <ul class="limits">
              {#each group.limits as limit (limit.label)}
                <li class="limit">
                  <div class="lrow">
                    <span class="ltext">
                      <span class="llabel">{limit.label}</span>
                      <span class="lsub">{limit.sub}{#if limit.reset}
                        · {limit.reset}
                      {/if}</span>
                    </span>
                    <output class="lpct sev" class:crit={limit.level === "crit"} class:warn={limit.level === "warn"}>{formatPercent(limit.pct)}</output>
                  </div>
                  <span class="lbar">
                    <span style:inline-size="{limit.pct}%" class="lfill sev" class:crit={limit.level === "crit"} class:warn={limit.level === "warn"}></span>
                  </span>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {:else}
      <p class="empty">No usage to show yet — sign in to Claude Code to load your limits.</p>
    {/if}
  </div>
</span>

<style>
  .usage-wrap {
    flex-shrink: 0;
  }

  /* Severity color, shared: the level class sets --sev once and every bar /
     number below just reads it (DRY). */
  .sev {
    --sev: var(--primary);

    &.warn {
      --sev: var(--warning);
    }

    &.crit {
      --sev: var(--critical);
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

      /* Each bar is tiny and sits right under the pointer, so drop its tooltip
         below the whole pill — clear of the cursor (base offset is 6px). */
      &[data-tooltip]::after {
        inset-block-start: calc(100% + 16px);
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
       shows; the inner .scroll owns vertical scrolling. */
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

    .phead {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
      margin-block-end: 14px;
    }

    .eyebrow {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .pmeta {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 11px;
    }

    .pcount {
      font-variant-numeric: tabular-nums;
    }

    /* Near-limit chip: worst-severity dot + a short "near/at limit" label. */
    .near {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      padding-block: 1px;
      padding-inline: 7px;
      border-radius: 999px;
      background: var(--surface-3);
      color: var(--sev);

      .near-dot {
        flex: none;
        block-size: 6px;
        inline-size: 6px;
        border-radius: 999px;
        background: var(--sev);
      }
    }

    .scroll {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(12.5rem, 1fr));
      gap: 8px;
      overflow-y: auto;
      max-block-size: min(62vh, 30rem);
    }

    .group {
      padding: 10px 11px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-1);

      .ghead {
        display: flex;
        gap: 8px;
        justify-content: space-between;
        align-items: flex-start;
        margin-block-end: 9px;
      }

      .gid {
        display: inline-flex;
        gap: 7px;
        align-items: center;
        min-inline-size: 0;
      }

      .agent-icon {
        display: inline-flex;
        flex: none;
        color: var(--sev);
        font-size: 15px;
      }

      .gtext {
        display: flex;
        flex-direction: column;
        min-inline-size: 0;
        line-height: 1.15;
      }

      .gname {
        overflow: hidden;
        font-weight: 700;
        font-size: 13px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .gplan {
        overflow: hidden;
        color: var(--on-surface-variant);
        font-size: 11px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .gcount {
        flex: none;
        padding-block: 2px;
        padding-inline: 7px;
        border-radius: 999px;
        background: var(--surface-3);
        color: var(--on-surface-variant);
        font-weight: 700;
        font-size: 9px;
        font-variant-numeric: tabular-nums;
        letter-spacing: 0.03em;
        text-transform: uppercase;
        white-space: nowrap;
      }
    }

    .limits {
      display: flex;
      flex-direction: column;
      gap: 12px;
      margin: 0;
      padding: 0;
      list-style: none;
    }

    .limit {
      .lrow {
        display: flex;
        gap: 10px;
        justify-content: space-between;
        align-items: baseline;
        margin-block-end: 6px;
      }

      .ltext {
        min-inline-size: 0;
      }

      .llabel {
        display: block;
        overflow: hidden;
        font-weight: 600;
        font-size: 13px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .lsub {
        display: block;
        overflow: hidden;
        color: var(--on-surface-variant);
        font-size: 11px;
        font-variant-numeric: tabular-nums;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .lpct {
        flex: none;
        color: var(--sev);
        font-weight: 800;
        font-size: 17px;
        font-variant-numeric: tabular-nums;
      }

      .lbar {
        display: block;
        overflow: hidden;
        block-size: 8px;
        border-radius: 999px;
        background: var(--surface-3);
      }

      .lfill {
        display: block;
        block-size: 100%;
        border-radius: 999px;
        background: var(--sev);
        transition: inline-size 300ms var(--ease), background 300ms var(--ease);
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
