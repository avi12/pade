<script lang="ts">
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import { fetchAccountReading, latestAccount } from "@/lib/stores/usage/accountReadings.svelte";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import PacedSubagents from "@/lib/usage/pacing/badge/PacedSubagents.svelte";
  import UnstoppedSubagents from "@/lib/usage/pacing/badge/UnstoppedSubagents.svelte";
  import { isPacingOn } from "@/lib/usage/pacing/engine/pacing-policy";
  import { reportedPacedKinds } from "@/lib/usage/pacing/engine/usage-pace";
  import PacingForecast from "@/lib/usage/pacing/settings/PacingForecast.svelte";
  import { settledValue } from "@/lib/util/async/settled";
  import { untrack } from "svelte";

  // Usage pacing in the usage panel: one switch, and a live line per limit the
  // agent's plan has - how much is spent, when it runs out at this pace, and
  // what pacing is doing about it (lib/usage/pacing/engine/pacing-policy).
  const { agent }: {
    /** The active agent, whose account the live lines read. */
    agent: string;
  } = $props();

  const SWITCH_OPTIONS = [
    {
      value: true,
      label: "On"
    },
    {
      value: false,
      label: "Off"
    }
  ] as const;

  // Only the limits the agent's plan has: Claude Ultra reports a weekly limit
  // and no 5-hour one. The backend caches the reading for a few minutes.
  const account = $derived(latestAccount(agent));
  $effect(() => {
    const agentId = agent;
    untrack(() => loadAccount(agentId));
  });

  async function loadAccount(agentId: string) {
    await settledValue({
      promise: fetchAccountReading({ agent: agentId }),
      fallback: null
    });
  }

  const shownKinds = $derived(reportedPacedKinds(account));
  const isOn = $derived(isPacingOn(prefs));
</script>

<section class="pacing" aria-label="Usage pacing">
  <header class="pacing-head">
    <span class="head-text">
      <span class="eyebrow section-label">Usage pacing</span>
      <span class="hint">Keeps your usage from running out before it resets</span>
    </span>
    <Segmented
      chosen={isOn}
      label="Usage pacing"
      onchoose={async value => await updatePrefs({ paceUsage: value })}
      options={SWITCH_OPTIONS}
    />
  </header>

  {#if isOn}
    {#each shownKinds as kind (kind)}
      <PacingForecast {agent} {kind} />
    {/each}

    <PacedSubagents />
    <UnstoppedSubagents />
  {/if}
</section>

<style>
  /* A section of the usage panel, framed like its spotlight card. */
  .pacing {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-block-end: 14px;
    padding-block: 12px;
    padding-inline: 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .pacing-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .head-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .hint {
    color: var(--on-surface-variant);
    font-size: 0.6563rem;
  }

  /* The field vocabulary the subagent lists write in (`.field`, `.field-row`,
     `.field-label`), at the usage panel's smaller scale, at zero specificity
     (`:where`) so a part's own rule still wins. */
  :where(.pacing) :global(.field) {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  :where(.pacing) :global(.field-row) {
    display: flex;
    gap: 10px;
    justify-content: space-between;
    align-items: center;
  }

  :where(.pacing) :global(.field-label) {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.6875rem;
  }

  /* A notch tighter than a Config switch, at the usage panel's scale. */
  :where(.pacing) :global(.segmented .option) {
    padding-block: 6px;
    padding-inline: 10px;
    font-size: 0.6875rem;
    white-space: nowrap;
  }
</style>
