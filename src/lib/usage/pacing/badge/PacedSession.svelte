<script lang="ts">
  // One session in the usage panel's list of paced subagents: its name, whether
  // it may start new subagents, and each running subagent's step.

  import { SubagentStepKind } from "@/lib/types/pacing";
  import type { SubagentStep } from "@/lib/types/pacing";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import type { ThrottledSession } from "@/lib/usage/pacing/engine/account-round";

  const { session }: {
    session: ThrottledSession;
  } = $props();

  const SECONDS = new Intl.NumberFormat(undefined, {
    style: "unit",
    unit: "second",
    unitDisplay: "narrow"
  });
  /** A subagent named by its id alone shows this much of it. */
  const SHORT_ID_LENGTH = 7;

  function stepText(step: SubagentStep) {
    if (step.step === SubagentStepKind.enum.wrapUp) {
      return "wrapping up";
    }

    return `held ${SECONDS.format(step.holdSeconds)}`;
  }

  const rows = $derived(
    session.subagents.map(subagent => ({
      id: subagent.id,
      label: subagent.description ?? subagent.id.slice(0, SHORT_ID_LENGTH),
      fullLabel: subagent.description ?? subagent.id,
      step: stepText(subagent.step)
    }))
  );
</script>

<li>
  <div class="field-row">
    <span class="name" {@attach truncationTooltip({ tooltip: session.name })}>{session.name}</span>
    {#if session.isBlockingSpawns}
      <span class="step">spawns blocked</span>
    {/if}
  </div>
  <ul class="subagents">
    {#each rows as row (row.id)}
      <li class="field-row">
        <span class="field-hint description" {@attach truncationTooltip({ tooltip: row.fullLabel })}>{row.label}</span>
        <span class="step">{row.step}</span>
      </li>
    {/each}
  </ul>
</li>

<style>
  .subagents {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding-block: 0;
    padding-inline: 10px 0;
    list-style: none;
  }

  .name {
    overflow: hidden;
    color: var(--on-surface);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .description {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .step {
    flex-shrink: 0;
    padding-block: 1px;
    padding-inline: 6px;
    border-radius: var(--radius-full);
    background: var(--warning-wash);
    color: var(--warning);
    font-family: var(--font-monospace);
    font-size: 0.6563rem;
  }
</style>
