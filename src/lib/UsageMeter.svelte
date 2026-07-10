<script lang="ts">
  import { usage as usageApi } from "@/lib/bridge";
  import type { SessionUsage, Usage } from "@/lib/types";
  import { onDestroy } from "svelte";

  // Two usage signals, both sourced WITHOUT spending quota (local data only):
  //  · session = how full the active agent's context window is — exact, from the
  //    session transcript. This is the real "%" we can show today.
  //  · weekly  = the plan's weekly limit %, which lives only on the vendor site;
  //    we surface the plan/tier honestly and leave the % blank until a signed-in
  //    webview can read it.
  const { agent, cwd }: {
    agent: string;
    cwd: string;
  } = $props();

  let session = $state<SessionUsage | null>(null);
  let weekly = $state<Usage | null>(null);

  // Session context grows as you work, so re-read it fairly often; the weekly
  // figure barely moves, so a slow poll keeps it fresh without churn.
  const SESSION_REFRESH_MS = 15_000;
  const WEEKLY_REFRESH_MS = 60_000;

  function clamp(value: number): number {
    return Math.max(0, Math.min(100, value));
  }

  const sessPct = $derived(session ? clamp(session.pct) : null);
  const weekPct = $derived(
    typeof weekly?.usedPct === "number" ? clamp(weekly.usedPct) : null
  );
  const weekHigh = $derived(weekPct !== null && weekPct >= 80);
  const resetIn = $derived(weekly?.resetsAt ? untilLabel(weekly.resetsAt) : "");
  const planLabel = $derived(weekly?.label ?? "");
  const modelLabel = $derived(session ? prettyModel(session.model) : "");

  // "claude-sonnet-4-6-20250101" → "sonnet 4 6" — a friendlier model name.
  function prettyModel(model: string): string {
    return model.replace(/^claude-/, "").replace(/-\d{6,}.*$/, "").replaceAll("-", " ");
  }

  function untilLabel(iso: string): string {
    const remaining = new Date(iso).getTime() - Date.now();
    if (!Number.isFinite(remaining) || remaining <= 0) {
      return "";
    }

    const hours = Math.round(remaining / 3_600_000);
    if (hours >= 24) {
      return `${Math.round(hours / 24)}d`;
    }

    return `${Math.max(1, hours)}h`;
  }

  // Re-read the session context when the project changes; a cancel guard drops a
  // stale response so a slow read can't overwrite a newer one.
  $effect(() => {
    const dir = cwd;
    let cancelled = false;
    void (async () => {
      const next = await usageApi.session(dir).catch(() => null);
      if (!cancelled) {
        session = next;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const active = agent;
    let cancelled = false;
    void (async () => {
      const next = await usageApi.get(active).catch(() => null);
      if (!cancelled) {
        weekly = next;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  const sessionTimer = setInterval(async () => {
    session = await usageApi.session(cwd).catch(() => null);
  }, SESSION_REFRESH_MS);
  const weeklyTimer = setInterval(async () => {
    weekly = await usageApi.get(agent).catch(() => null);
  }, WEEKLY_REFRESH_MS);
  onDestroy(() => {
    clearInterval(sessionTimer);
    clearInterval(weeklyTimer);
  });
</script>

<span class="usage-wrap">
  <button
    style:anchor-name="--usage-anchor"
    class="pill"
    class:high={weekHigh}
    aria-label="Show usage details"
    data-tooltip="Usage — session &amp; weekly"
    popovertarget="usage-menu"
  >
    <span class="leg">
      <span class="cap">SESS</span>
      <span class="track"><span style:inline-size="{sessPct ?? 0}%" class="fill"></span></span>
    </span>
    <span class="sep"></span>
    <span class="leg">
      <span class="cap">WEEK</span>
      <span class="track"><span style:inline-size="{weekPct ?? 0}%" class="fill week"></span></span>
      {#if weekPct !== null}
        <span class="num">{Math.round(weekPct)}%</span>
      {/if}
    </span>
    <span class="caret">▾</span>
  </button>

  <div id="usage-menu" style:position-anchor="--usage-anchor" class="panel" aria-label="Usage details" popover role="dialog">
    <div class="head">
      <span class="eyebrow">This session</span>
      <span class="who"><span class="dot ok"></span>{agent}{#if modelLabel}
        · {modelLabel}
      {/if}</span>
    </div>
    <div class="line">
      <span class="lbl">Context used this session</span>
      <span class="big">{#if sessPct !== null}
        {Math.round(sessPct)}%{:else}—{/if}</span>
    </div>
    <span class="bar"><span style:inline-size="{sessPct ?? 0}%" class="bfill"></span></span>

    <div class="rule"></div>

    <div class="head">
      <span class="eyebrow">This week</span>
      {#if resetIn}
        <span class="reset" data-tooltip={weekly?.resetsAt}>resets in {resetIn}</span>
      {/if}
    </div>
    <div class="line">
      <span class="lbl">Weekly limit</span>
      <span class="big" class:warn={weekHigh}>{#if weekPct !== null}
        {Math.round(weekPct)}%{:else}—{/if}</span>
    </div>
    <span class="bar"><span style:inline-size="{weekPct ?? 0}%" class="bfill week"></span></span>
    <div class="foot">
      <span class="dot ok"></span>
      {#if planLabel}
        {planLabel} —
      {/if} weekly % needs the sign-in webview; session is exact from local logs.
    </div>
  </div>
</span>

<style>
  .usage-wrap {
    flex-shrink: 0;
  }

  .pill {
    display: inline-flex;
    gap: 10px;
    align-items: center;
    padding-block: 6px;
    padding-inline: 12px 11px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .leg {
      display: inline-flex;
      gap: 6px;
      align-items: center;
    }

    .cap {
      color: var(--on-surface-var);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.06em;
    }

    .track {
      overflow: hidden;
      block-size: 6px;
      inline-size: 34px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--primary);
      transition: inline-size 300ms var(--ease), background 300ms var(--ease);
    }

    &.high .fill.week {
      background: var(--warn);
    }

    .num {
      font-weight: 600;
      font-size: 12px;
      font-variant-numeric: tabular-nums;
    }

    .sep {
      block-size: 15px;
      inline-size: 1px;
      background: var(--outline);
    }

    .caret {
      font-size: 10px;
      opacity: 60%;
    }
  }

  .panel {
    position: absolute;
    inset: auto;
    inline-size: 322px;
    padding: 15px;
    border: 1px solid var(--outline);
    border-radius: 16px;
    background: var(--surface-2);
    box-shadow: 0 16px 40px color-mix(in sRGB, var(--on-surface) 28%, transparent);
    position-area: bottom span-left;

    .head {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-block-end: 10px;
    }

    .eyebrow {
      color: var(--on-surface-var);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .who {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      color: var(--on-surface-var);
      font-family: var(--font-mono);
      font-size: 11px;
    }

    .reset {
      color: var(--on-surface-var);
      font-size: 11px;
    }

    .line {
      display: flex;
      justify-content: space-between;
      align-items: baseline;
      margin-block-end: 6px;
    }

    .lbl {
      font-weight: 600;
      font-size: 13px;
    }

    .big {
      color: var(--primary);
      font-weight: 800;
      font-size: 18px;
      font-variant-numeric: tabular-nums;

      &.warn {
        color: var(--warn);
      }
    }

    .bar {
      display: block;
      overflow: hidden;
      block-size: 8px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .bfill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--primary);
      transition: inline-size 300ms var(--ease);

      &.week {
        background: color-mix(in sRGB, var(--primary) 80%, var(--warn));
      }
    }

    .rule {
      block-size: 1px;
      margin-block: 13px;
      margin-inline: -15px;
      background: var(--outline);
    }

    .dot {
      flex: none;
      block-size: 6px;
      inline-size: 6px;
      border-radius: 999px;

      &.ok {
        background: var(--tertiary);
      }
    }

    .foot {
      display: flex;
      gap: 7px;
      align-items: baseline;
      margin-block-start: 12px;
      color: var(--on-surface-var);
      font-size: 11px;
      line-height: 1.45;
    }
  }
</style>
