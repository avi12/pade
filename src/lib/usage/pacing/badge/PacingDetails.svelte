<script lang="ts">
  // The inside of a tab's pacing popover: why pacing is holding the session
  // back, which steps are active, what it runs now, when it all reverts, the
  // overrides, and what pacing did - in that order.

  import { adaptedChoice } from "@/lib/stores/usage/usageAdapt.svelte";
  import type { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import { PACING_STEP_TEXT, pacingFacts, pacingWhen, spokenTime } from "@/lib/usage/pacing/engine/pacing-policy";
  import type { pacingResume } from "@/lib/usage/pacing/engine/pacing-policy";
  import PacingActions from "@/lib/usage/pacing/badge/PacingActions.svelte";

  const { sessionId, status, resume }: {
    sessionId: string;
    status: NonNullable<ReturnType<typeof pacingStatus>>;
    /** When it is back to normal - the badge's hint says the same. */
    resume: ReturnType<typeof pacingResume>;
  } = $props();

  const COUNTDOWN_TICK_MS = 1000;

  const lines = $derived(
    pacingFacts({
      verdict: status.verdict,
      choice: adaptedChoice(sessionId),
      resume
    })
  );
  // One switch for the popover: clicking any time reads them all as a
  // countdown, and back (CSS swaps the two spans).
  const countdownToggleId = $derived(`pacing-countdown-${sessionId}`);
  let isCountdown = $state(false);
  let now = $state(Date.now());
  const history = $derived(
    status.history.map(entry => `${spokenTime({
      time: entry.at,
      now
    })} ${entry.text}`)
  );

  // A countdown counts down: while it shows, the clock ticks each second. A
  // clock time needs no tick.
  $effect(() => {
    if (!isCountdown) {
      return;
    }

    now = Date.now();
    const timer = setInterval(() => {
      now = Date.now();
    }, COUNTDOWN_TICK_MS);
    return () => clearInterval(timer);
  });
</script>

<h4 class="section-label">Usage pacing</h4>
<input
  id={countdownToggleId}
  class="countdown-toggle"
  aria-label="Show times as a countdown"
  type="checkbox"
  bind:checked={isCountdown}
/>
{#each lines as line (line.text)}
  <p>
    {line.text}
    {#if line.time !== null}
      <label class="when" for={countdownToggleId}>
        <span class="at">{pacingWhen({
          time: line.time,
          now,
          isCountdown: false
        })}</span>
        <span class="in">{pacingWhen({
          time: line.time,
          now,
          isCountdown: true
        })}</span>
      </label>
    {/if}
  </p>
{/each}
<ul class="active">
  {#each status.steps as step (step)}
    <li>{PACING_STEP_TEXT[step]}</li>
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
    font-size: 0.625rem;
  }

  p {
    margin: 0;
    color: var(--on-surface);
  }

  /* Hidden from sight but still in the tab order, so the keyboard can flip it. */
  .countdown-toggle {
    position: absolute;
    block-size: 1px;
    inline-size: 1px;
    opacity: 0%;
    pointer-events: none;
  }

  .when {
    text-decoration: underline dotted;
    text-underline-offset: 2px;
    cursor: pointer;

    .in {
      display: none;
    }

    .countdown-toggle:checked ~ p & {
      .at {
        display: none;
      }

      .in {
        display: inline;
      }
    }

    .countdown-toggle:focus-visible ~ p & {
      outline: 2px solid var(--primary);
    }
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
