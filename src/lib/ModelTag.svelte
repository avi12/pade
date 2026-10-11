<script lang="ts">
  // A tab's running model and effort: the model's name, then its effort as
  // rising bars on that model's own ladder (from its agent's catalog), with the
  // exact words in the tooltip. Nothing renders until the agent has reported.
  import EffortBars from "@/lib/EffortBars.svelte";
  import { effortStep, modelLabel } from "@/lib/model-label";
  import { sessionModel } from "@/lib/stores/context.svelte";
  import { modelOption } from "@/lib/stores/modelOptions.svelte";
  import type { AgentSession } from "@/lib/types";

  const { session }: { session: AgentSession } = $props();

  const running = $derived(sessionModel(session.id));
  const effort = $derived.by(() => {
    if (running === null || running.effort === null) {
      return null;
    }

    const ladder = modelOption({
      command: session.agent.command,
      model: running.id
    })?.efforts.map(option => option.id);
    return effortStep({
      model: running.id,
      effort: running.effort,
      ladder
    });
  });
</script>

{#if running !== null}
  <span class="model" aria-label={modelLabel(running)} data-tooltip={modelLabel(running)}>
    {running.name}
    {#if effort !== null}
      <EffortBars step={effort.step} steps={effort.steps} />
    {:else if running.effort !== null}
      · {running.effort}
    {/if}
  </span>
{/if}

<style>
  /* The effort bars light in the accent — and on the active pill, whose fill
     IS the accent, in the pill's on-colour (set by the tab via --effort-lit). */
  .model {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    font-size: 10px;
    opacity: 80%;
  }
</style>
