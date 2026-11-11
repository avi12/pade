<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { limitResetOffers, refreshLimitResetOffers } from "@/lib/stores/usage/limitReset.svelte";
  import type { AccountUsage, AgentSession, NoReading } from "@/lib/types";
  import { limitResetTotal } from "@/lib/usage/limit-reset/limit-reset-kind";
  import LimitResetConfirm from "@/lib/usage/limit-reset/LimitResetConfirm.svelte";
  import { noReadingsOf, pendingResetBoundaries, withLastGoodReadings } from "@/lib/usage/meter/usage-meter";
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
  const { agent, sessions }: {
    /** The active session's agent id, whose account the panel's pacing reads. */
    agent: string;
    sessions: AgentSession[];
  } = $props();

  // Live account usage per agent (mirrors claude.ai / `claude /usage` and Codex's
  // own limits): each agent's rate-limit windows - the session, weekly, and any
  // per-model caps - read via that vendor's usage endpoint from the local token,
  // keyed by agent id. The backend caches each (~3 min), so this polls slowly.
  let accounts = $state<ReadonlyMap<string, AccountUsage | null>>(new Map());
  // Why each agent's latest ask brought no reading (signed out, sign-in
  // expired, …), so a pill without numbers can say so instead of a bare dash.
  let noReadings = $state<ReadonlyMap<string, NoReading>>(new Map());
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
      noReadings,
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
    noReadings = noReadingsOf(entries);
    const read = withLastGoodReadings({
      fresh: new Map(entries.map(([id, reading]) => [id, reading?.account ?? null])),
      kept: keptReadings,
      now: Date.now()
    });
    keptReadings = read.kept;
    return read.accounts;
  }

  const resetCount = $derived(limitResetTotal(limitResetOffers()));

  // Each new Claude usage reading re-checks the free reset, so the offer follows
  // the meter's own refresh instead of keeping a timer of its own.
  const claudeAccount = $derived(accounts.get(AgentId.Claude));
  $effect(() => {
    if (claudeAccount === undefined) {
      return;
    }

    untrack(refreshLimitResetOffers);
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
  <UsageTrigger {groups} {justReset} {resetCount} />

  <UsagePanel {agent} {groups} {justReset} />

  <LimitResetConfirm
    onreset={async () => {
      accounts = await loadAccounts({ forceRefresh: true });
    }}
  />
</span>

<style>
  .usage-wrap {
    position: relative;
    flex-shrink: 0;
  }

  /* The meter's shared visual language, reaching into the trigger and the panel
     at zero specificity (`:where`) so a part's own rule still wins.

     Severity color: the level class sets --severity-color (and a faint
     --severity-wash tint for warn/crit) once, and every bar / number / row just
     reads it. A normal level reads in the context gauge's "fine" colour, not
     the primary: a palette's primary can be its alarm red (Cyberpunk by day),
     and a reading with room to spare must never look like danger.
     --severity-wash stays unset at "normal" - there is no wash for fine. */
  :where(.usage-wrap) :global(.severity) {
    --severity-color: var(--context-ok);
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
</style>
