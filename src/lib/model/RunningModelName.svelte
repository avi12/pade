<script lang="ts">
  // A session's running model, as every place that shows one draws it: the
  // model's name tinted by how high its effort sits on its ladder (the level's
  // word is the caller's tooltip - it costs too much room), with a bolt while
  // Ultracode is on. Nothing renders until the agent has reported.

  import Icon from "@/lib/Icon.svelte";
  import EffortTint from "@/lib/model/EffortTint.svelte";
  import { runningModelLook } from "@/lib/model/running-model";
  import { sessionModel } from "@/lib/stores/agent/context.svelte";
  import { modelOption } from "@/lib/stores/model/modelOptions.svelte";

  const { sessionId, command }: {
    sessionId: string;
    /** The agent's command, to rank the effort on the model's own ladder from
     *  its catalog - without it, the model family's usual ladder stands in. */
    command?: string;
  } = $props();

  const running = $derived(sessionModel(sessionId));
  const look = $derived.by(() => {
    if (running === null) {
      return null;
    }

    const ladder = command === undefined ? undefined : modelOption({
      command,
      model: running.id
    })?.efforts.map(option => option.id);
    return runningModelLook({
      running,
      ladder
    });
  });
</script>

{#if running !== null && look !== null}
  <EffortTint tone={look.tone}>
    {running.name}
    {#if running.isUltracode}
      <Icon name="bolt" size={12} />
    {/if}
  </EffortTint>
{/if}
