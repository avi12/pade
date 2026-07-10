<script lang="ts">
  import { usage as usageApi } from "./bridge";
  import type { Usage } from "./types";
  import { onDestroy } from "svelte";

  // Remaining usage / quota for the selected agent, sourced WITHOUT spending
  // quota (local data only — never the CLI, never the network). When no reliable
  // local signal exists we render an honest "usage —"; we never fabricate a bar.
  const { agent }: { agent: string } = $props();

  // Shown when no local quota data exists — honest about why the number is
  // missing and where the real one will come from.
  const UNAVAILABLE =
    "Usage unavailable for this agent — no local quota data. The live figure needs the vendor site (coming with the sign-in webview).";

  let usage = $state<Usage | null>(null);

  // How often to re-read the local figure. Slow — the numbers barely move and
  // the read is cheap, so a minute keeps it fresh without churn.
  const REFRESH_MS = 60_000;

  // The bar only renders when a precise percent is known; clamp to 0..100 so a
  // stale/odd value can't overflow the track.
  const pct = $derived(
    typeof usage?.usedPct === "number" ? Math.max(0, Math.min(100, usage.usedPct)) : null
  );

  // Human "resets in 3d / 5h" from an ISO reset time; empty when unknown/past.
  const resetIn = $derived(usage?.resetsAt ? untilLabel(usage.resetsAt) : "");

  function untilLabel(iso: string): string {
    const remaining = new Date(iso).getTime() - Date.now();
    if (!Number.isFinite(remaining) || remaining <= 0) {
      return "";
    }

    const hours = Math.round(remaining / 3_600_000);
    if (hours >= 24) {
      return `resets ${Math.round(hours / 24)}d`;
    }

    return `resets ${Math.max(1, hours)}h`;
  }

  // Fetch when the agent changes; a cancel guard drops a stale response so a
  // slow read for the previous agent can't overwrite the current one.
  $effect(() => {
    const active = agent;
    let cancelled = false;
    void (async () => {
      const next = await usageApi.get(active);
      if (!cancelled) {
        usage = next;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  // Slow background refresh for the active agent, independent of the change
  // effect above. Cleaned up on unmount.
  const timer = setInterval(async () => {
    usage = await usageApi.get(agent);
  }, REFRESH_MS);
  onDestroy(() => clearInterval(timer));
</script>

{#if usage}
  <!-- <output> carries role="status" natively (rule 7). -->
  <output class="usage" class:high={pct !== null && pct >= 80} data-tooltip={usage.source}>
    {#if pct !== null}
      <span class="track">
        <span style:inline-size="{pct}%" class="fill"></span>
      </span>
      <span class="text">
        <span class="num">{Math.round(pct)}%</span>
        {#if resetIn}
          <span class="reset"> · {resetIn}</span>
        {/if}
      </span>
    {:else}
      <span class="text">{usage.label}</span>
    {/if}
  </output>
{:else}
  <output class="usage unknown" data-tooltip={UNAVAILABLE}>
    usage —
  </output>
{/if}

<style>
  /* Nested CSS (rule 8): the meter's rules live together, mirroring its markup. */
  .usage {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 6px;
    padding-inline: 12px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-weight: 600;
    font-size: 13px;

    .track {
      overflow: hidden;
      block-size: 6px;
      inline-size: 46px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--primary);
      transition: inline-size 300ms var(--ease);
    }

    /* High consumption (≥80%) reads as a warning — a glanceable signal. */
    &.high .fill {
      background: var(--warn);
    }

    .text {
      color: var(--on-surface);
    }

    .num {
      font-variant-numeric: tabular-nums;
    }

    .reset {
      color: var(--on-surface-var);
      font-weight: 400;
    }

    &.unknown {
      color: var(--on-surface-var);
      font-weight: 400;
    }
  }
</style>
