<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import {
    askToResetLimits,
    limitResetOffer,
    limitResetSummary,
    refreshLimitResetOffer
  } from "@/lib/stores/usage/limitReset.svelte";
  import type { AccountUsage, AgentSession } from "@/lib/types";
  import { growInline } from "@/lib/ui/motion";
  import LimitResetConfirm from "@/lib/usage/LimitResetConfirm.svelte";
  import { hasClaudePill, pendingResetBoundaries, withLastGoodReadings } from "@/lib/usage/meter/usage-meter";
  import type { KeptReading } from "@/lib/usage/meter/usage-meter";
  import UsagePanel from "@/lib/usage/meter/UsagePanel.svelte";
  import UsageTrigger from "@/lib/usage/meter/UsageTrigger.svelte";
  import { buildGroups, justResetLimits, limitPercentages } from "@/lib/usage/usage-groups";
  import { settledValue } from "@/lib/util/async/settled";
  import { onDestroy, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  // The running agent sessions - one usage group per distinct coding agent among
  // them is derived below (the shell's windowSessions owns the list; the shell fallback and
  // terminal-editor sessions are filtered out in `buildGroups`).
  const { sessions }: { sessions: AgentSession[] } = $props();

  // Live account usage per agent (mirrors claude.ai / `claude /usage` and Codex's
  // own limits): each agent's rate-limit windows - the session, weekly, and any
  // per-model caps - read via that vendor's usage endpoint from the local token,
  // keyed by agent id. The backend caches each (~3 min), so this polls slowly.
  let accounts = $state<ReadonlyMap<string, AccountUsage | null>>(new Map());
  // A ticking clock so the "resets in …" countdowns stay live between polls.
  let now = $state(Date.now());

  const ACCOUNT_REFRESH_MS = 180_000;
  // The countdowns it feeds ("resets in 3h 30m") have minute granularity, so a
  // half-minute tick keeps them honest at a thirtieth of the repaints a
  // per-second clock would force (idle frames are what make the app heavy).
  const CLOCK_TICK_MS = 30_000;
  const RESET_SETTLE_MS = 1_000;
  const refreshedResetBoundaries = new SvelteSet<string>();

  // One group per distinct running agent, worst-first (see usage-groups.ts). Each
  // agent carries its real limits when its account resolved; an agent with no
  // local usage signal is "unknown".
  const groups = $derived(
    buildGroups({
      accounts,
      sessions,
      now
    })
  );

  // Each agent's last real reading, so one refused request (Claude's usage
  // endpoint rate-limits) doesn't blank its pill - see withLastGoodReadings.
  let keptReadings: ReadonlyMap<string, KeptReading> = new Map();

  // Fetch each distinct running agent's account (Claude, Codex, …) in parallel,
  // keyed by agent id. An agent with no local usage signal resolves to null and
  // renders as an "unknown" group - never fabricated - unless it had a real
  // reading within the last hour, which stands in while the endpoint refuses.
  async function loadAccounts({ forceRefresh = false }: { forceRefresh?: boolean } = {}) {
    const agentIds = [...new Set(sessions.map(session => session.agent.id))];
    const entries = await Promise.all(
      agentIds.map(async id => [
        id,
        await settledValue({
          promise: sendMessage(Command.usageAccountAgent, {
            agent: id,
            forceRefresh
          }),
          fallback: null
        })
      ] as const)
    );
    const read = withLastGoodReadings({
      fresh: new Map(entries),
      kept: keptReadings,
      now: Date.now()
    });
    keptReadings = read.kept;
    return read.accounts;
  }

  const resetOffer = $derived(limitResetOffer());
  // The chip sits in Claude's pill, so it shows only while that pill does (not
  // folded into the "+N" overflow).
  const showsResetInPill = $derived(resetOffer !== null && hasClaudePill(groups));

  // Each new Claude usage reading re-checks the free reset, so the offer follows
  // the meter's own refresh instead of keeping a timer of its own.
  const claudeAccount = $derived(accounts.get(AgentId.Claude));
  $effect(() => {
    if (claudeAccount === undefined) {
      return;
    }

    untrack(refreshLimitResetOffer);
  });

  // Limits that just started over - on schedule or by a spent free reset - so
  // their numbers can flash. Compared reading to reading; the set is replaced on
  // every new reading, which also clears it for the next reset of the same limit.
  let justReset = $state<ReadonlySet<string>>(new Set());
  let previousPercentages = new Map<string, number>();
  $effect(() => {
    const current = limitPercentages(groups);
    justReset = justResetLimits({
      previous: previousPercentages,
      current
    });
    previousPercentages = current;
  });

  async function refreshAtResetBoundary() {
    now = Date.now();
    accounts = await loadAccounts({ forceRefresh: true });
  }

  $effect(() => {
    let isCancelled = false;
    async function refreshAccounts() {
      const next = await loadAccounts();
      if (!isCancelled) {
        accounts = next;
      }
    }
    refreshAccounts();
    return () => {
      isCancelled = true;
    };
  });

  const accountTimer = setInterval(async () => {
    accounts = await loadAccounts();
  }, ACCOUNT_REFRESH_MS);
  const clockTimer = setInterval(() => {
    now = Date.now();
  }, CLOCK_TICK_MS);
  // Refresh the moment a window resets: at once for one that already passed,
  // otherwise on a timer set for the soonest one ahead.
  $effect(() => {
    const currentTime = Date.now();
    const { expired, next } = pendingResetBoundaries({
      accounts,
      refreshed: refreshedResetBoundaries,
      now: currentTime
    });
    if (expired) {
      refreshedResetBoundaries.add(expired.value);
      refreshAtResetBoundary();
      return;
    }

    if (!next) {
      return;
    }

    const timer = setTimeout(async () => {
      refreshedResetBoundaries.add(next.value);
      await refreshAtResetBoundary();
    }, next.time - currentTime + RESET_SETTLE_MS);
    return () => clearTimeout(timer);
  });
  onDestroy(() => {
    clearInterval(accountTimer);
    clearInterval(clockTimer);
  });
</script>

<span class="usage-wrap menu-host">
  <UsageTrigger {groups} {justReset} {resetLabel} {showsResetInPill} />

  <UsagePanel {groups} {justReset} />

  {#if showsResetInPill}
    <button
      class="reset-face reset-chip"
      aria-label="Free limit reset available - {limitResetSummary()}"
      data-tooltip="Free limit reset available · {limitResetSummary()}"
      onclick={askToResetLimits}
      type="button"
      transition:growInline
    >{@render resetLabel()}</button>
  {/if}

  <LimitResetConfirm
    onreset={async () => {
      accounts = await loadAccounts({ forceRefresh: true });
    }}
  />
</span>

{#snippet resetLabel()}
  <Icon name="refresh" size={11} />Reset
{/snippet}

<style>
  .usage-wrap {
    position: relative;
    flex-shrink: 0;
  }

  /* The meter's shared visual language, reaching into the trigger and the panel
     at zero specificity (`:where`) so a part's own rule still wins.

     Severity color: the level class sets --severity-color (and a faint
     --severity-wash tint for warn/crit) once, and every bar / number / row just
     reads it. --severity-wash stays unset at "normal" - there is no primary wash. */
  :where(.usage-wrap) :global(.severity) {
    --severity-color: var(--primary);
  }

  :where(.usage-wrap) :global(.severity.warning) {
    --severity-color: var(--warning);
    --severity-wash: var(--warning-wash);
  }

  :where(.usage-wrap) :global(.severity.critical) {
    --severity-color: var(--critical);
    --severity-wash: var(--critical-wash);
  }

  /* A limit that just started over - its scheduled reset, or a free reset spent
     on it - flashes once: the number pops while a primary wash fades behind it,
     so a sudden drop to 0% reads as a refill rather than a glitch. */
  :where(.usage-wrap) :global(.just-reset) {
    animation: limit-refill 900ms var(--ease) both;
  }

  @keyframes limit-refill {
    0% {
      background-color: transparent;
      scale: 1;
    }

    25% {
      background-color: color-mix(in oklch, var(--primary) 38%, transparent);
      scale: 1.2;
    }

    100% {
      background-color: transparent;
      scale: 1;
    }
  }

  /* A free reset waiting to be spent, shown in Claude's pill because it is
     Claude's. The trigger keeps an invisible slot of the chip's exact size and
     the real chip - a sibling of the trigger - is anchored over it, so both
     faces share this look. */
  :where(.usage-wrap) :global(.reset-face) {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    padding-block: 1px;
    padding-inline: 6px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-full);
    font-weight: 700;
    font-size: 0.6563rem;
    line-height: 1.4;
    white-space: nowrap;
  }

  .reset-chip {
    position: absolute;
    inset: anchor(top) anchor(right) anchor(bottom) anchor(left);
    z-index: 1;
    justify-content: center;
    background: color-mix(in oklch, var(--primary) 16%, transparent);
    color: var(--primary);
    cursor: pointer;
    transition: background 150ms var(--ease);

    /* A reset is worth knowing about the moment it lands: as the chip appears
       it pops, then a ring pulses out from it a few times. A CSS animation runs
       on insertion, so this plays exactly when the offer shows up. */
    animation: reset-arrive 1200ms var(--ease) 3;
    position-anchor: --reset-slot;

    /* The slot is invisible by design; the default (anchors-visible) would hide
       the chip along with it. */
    position-visibility: always;

    &:hover {
      background: color-mix(in oklch, var(--primary) 30%, transparent);
    }
  }

  @keyframes reset-arrive {
    0% {
      box-shadow: 0 0 0 0 color-mix(in oklch, var(--primary) 70%, transparent);
      scale: 1;
    }

    20% {
      scale: 1.15;
    }

    45% {
      scale: 1;
    }

    100% {
      box-shadow: 0 0 0 10px transparent;
      scale: 1;
    }
  }
</style>
