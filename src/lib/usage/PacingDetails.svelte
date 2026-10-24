<script lang="ts">
  // The inside of a tab's pacing popover: why pacing is holding the session
  // back, which steps are active, what it runs now, when it all reverts, the
  // overrides, and what pacing did - in that order.

  import { adaptedChoice } from "@/lib/stores/usage/usageAdapt.svelte";
  import type { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import { PACING_STEP_TEXT, pacingFacts, spokenTime } from "@/lib/usage/pacing-policy";
  import PacingActions from "@/lib/usage/PacingActions.svelte";

  const { sessionId, status }: {
    sessionId: string;
    status: NonNullable<ReturnType<typeof pacingStatus>>;
  } = $props();

  const lines = $derived.by(() => {
    const choice = adaptedChoice(sessionId);
    return pacingFacts({
      verdict: status.verdict,
      choice,
      restUntil: status.restUntil,
      revertsAt: status.revertsAt ?? choice?.restoreAt ?? null,
      now: Date.now()
    });
  });
  const history = $derived(
    status.history.map(entry => `${spokenTime({
      time: entry.at,
      now: Date.now()
    })} ${entry.text}`)
  );
</script>

<h4>Usage pacing</h4>
{#each lines as line (line)}
  <p>{line}</p>
{/each}
<ul class="active">
  {#each status.steps as step (step)}
    <li>{PACING_STEP_TEXT[step].label}</li>
  {/each}
</ul>

<PacingActions isStopped={status.isStopped} {sessionId} />

<ol class="history">
  {#each history as entry, i (i)}
    <li>{entry}</li>
  {/each}
</ol>

<style>
  h4 {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.625rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  p {
    margin: 0;
    color: var(--on-surface);
  }

  .active {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      padding-block: 1px;
      padding-inline: 6px;
      border-radius: var(--radius-full);
      background: var(--warning-wash);
      color: var(--warning);
      font-size: 0.6875rem;
    }
  }

  .history {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding-block-start: 6px;
    padding-inline: 0;
    border-block-start: 1px solid var(--outline);
    color: var(--on-surface-variant);
    list-style: none;
    font-family: var(--font-monospace);
    font-size: 0.6875rem;

    &:empty {
      display: none;
    }
  }
</style>
