<script lang="ts">
  import { usage as usageApi } from "@/lib/bridge";
  import { formatPercent } from "@/lib/format";
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
  // The pill shows at most this many limit bars before collapsing to "+N".
  const CELL_CAP = 4;

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

  // A one-word type for the pill, from the limit's label.
  function typeOf(label: string): string {
    const lower = label.toLowerCase();
    if (lower.includes("session")) {
      return "session";
    }

    if (lower.includes("week")) {
      return "weekly";
    }

    return lower.split(" ")[0];
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

  // A 4..16px bar height for the pill's mini-chart cells.
  function cellHeight(pct: number): number {
    return Math.max(4, Math.round((pct / 100) * 16));
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
      limits
    }];
  }

  const allLimits = $derived(groups.flatMap(group => group.limits));
  const worst = $derived(
    allLimits.length > 0 ? allLimits.reduce((max, limit) => (limit.pct > max.pct ? limit : max)) : null
  );
  const cells = $derived(allLimits.slice(0, CELL_CAP));
  const cellsMore = $derived(Math.max(0, allLimits.length - CELL_CAP));

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
    {#if worst}
      <span class="worst">
        <span class="dot sev" class:crit={worst.level === "crit"} class:warn={worst.level === "warn"}></span>
        <span class="pct sev" class:crit={worst.level === "crit"} class:warn={worst.level === "warn"}>{formatPercent(worst.pct)}</span>
        <span class="type">{typeOf(worst.label)}</span>
      </span>
      <span class="sep"></span>
      <span class="cells">
        {#each cells as cell (cell.label)}
          <span class="cell" data-tooltip="{cell.label} · {formatPercent(cell.pct)}">
            <span style:block-size="{cellHeight(cell.pct)}px" class="cfill sev" class:crit={cell.level === "crit"} class:warn={cell.level === "warn"}></span>
          </span>
        {/each}
        {#if cellsMore > 0}
          <span class="more">+{cellsMore}</span>
        {/if}
      </span>
    {:else}
      <span class="none">—</span>
    {/if}
    <span class="caret">▾</span>
  </button>

  <div id="usage-menu" style:position-anchor="--usage-anchor" class="panel" aria-label="Usage details" popover role="dialog">
    <div class="eyebrow">Usage by agent</div>

    {#if groups.length > 0}
      <div class="scroll">
        {#each groups as group (group.name)}
          <section class="group">
            <div class="ghead">
              <span class="gname">{group.name}</span>
              <span class="gplan">{group.plan}</span>
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

    .worst {
      display: inline-flex;
      gap: 5px;
      align-items: center;
    }

    .dot {
      flex: none;
      block-size: 6px;
      inline-size: 6px;
      border-radius: 999px;
      background: var(--sev);
    }

    .pct {
      color: var(--sev);
      font-weight: 700;
      font-size: 12px;
      font-variant-numeric: tabular-nums;
    }

    .type {
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 10px;
    }

    .none {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 12px;
    }

    .sep {
      block-size: 14px;
      inline-size: 1px;
      background: var(--outline);
    }

    /* A mini bar-chart — one bottom-aligned cell per active limit. */
    .cells {
      display: inline-flex;
      gap: 3px;
      align-items: flex-end;
      block-size: 16px;
    }

    .cell {
      display: flex;
      flex: none;
      align-items: flex-end;
      block-size: 16px;
      inline-size: 6px;
      border-radius: 3px;
      background: var(--surface-3);
    }

    .cfill {
      inline-size: 100%;
      border-radius: 3px;
      background: var(--sev);
      transition: block-size 300ms var(--ease), background 300ms var(--ease);
    }

    .more {
      margin-inline-start: 2px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
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
    inline-size: 326px;
    padding: 15px;
    border: 1px solid var(--outline);
    border-radius: 16px;
    background: var(--surface-2);
    color: var(--on-surface);
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-left;

    .eyebrow {
      margin-block-end: 13px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .scroll {
      display: flex;
      flex-direction: column;
      gap: 16px;
      overflow-y: auto;
      max-block-size: min(60vh, 440px);
    }

    .group {
      .ghead {
        display: flex;
        gap: 10px;
        justify-content: space-between;
        align-items: center;
        margin-block-end: 10px;
      }

      .gname {
        font-weight: 700;
        font-size: 13px;
      }

      .gplan {
        color: var(--on-surface-variant);
        font-size: 11px;
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
