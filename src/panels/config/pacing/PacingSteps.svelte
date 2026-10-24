<script lang="ts">
  import { PacingStep, PacingStopThen } from "@/lib/types";
  import type { PacingSettings } from "@/lib/types";
  import Checkbox from "@/lib/ui/controls/Checkbox.svelte";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";
  import { PACING_STEP_ORDER, PACING_STEP_TEXT, withStep } from "@/lib/usage/pacing-policy";

  // Usage pacing's steps, numbered in the order they are climbed, each one
  // allowed or not. What follows a stop lives inside the Stop row, the one place
  // it applies.
  const { settings, onsave }: {
    settings: PacingSettings;
    onsave: (next: PacingSettings) => Promise<void>;
  } = $props();

  const stopThenOptions = [
    {
      value: PacingStopThen.enum.ask,
      label: "Ask me"
    },
    {
      value: PacingStopThen.enum.continue,
      label: "Continue"
    }
  ] as const;
  const isStopAllowed = $derived(settings.steps.includes(PacingStep.enum.stop));
</script>

<ol class="steps">
  {#each PACING_STEP_ORDER as step, i (step)}
    <li>
      <label class="step">
        <Checkbox
          checked={settings.steps.includes(step)}
          onchange={async checked => await onsave(
            withStep({
              settings,
              step,
              isAllowed: checked
            })
          )}
        />
        <span class="number">{i + 1}</span>
        <span class="label">{PACING_STEP_TEXT[step].label}</span>
        <span class="hint">{PACING_STEP_TEXT[step].hint}</span>
      </label>
    </li>
  {/each}
</ol>

{#if isStopAllowed}
  <div class="stop-then">
    <span class="hint">Once a stopped agent has room</span>
    <Segmented
      chosen={settings.stopThen}
      label="Once a stopped agent has room"
      onchoose={async value => await onsave({
        ...settings,
        stopThen: value
      })}
      options={stopThenOptions}
    />
  </div>
{/if}

<style>
  .steps {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 2px 10px;
    align-items: center;
    cursor: pointer;
  }

  .number {
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.6875rem;
  }

  .label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 0.75rem;
  }

  .hint {
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.625rem;
  }

  .step .hint {
    grid-column: 3;
  }

  .stop-then {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-inline-start: 30px;
  }
</style>
