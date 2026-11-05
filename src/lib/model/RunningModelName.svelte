<script lang="ts">
  // A session's running model, as every place that shows one draws it: the
  // model's name tinted by how high its effort sits on its ladder (the level's
  // word is the caller's tooltip - it costs too much room), with a bolt while
  // Ultracode is on. Nothing renders until the agent has reported.

  import Icon from "@/lib/Icon.svelte";
  import EffortTint from "@/lib/model/EffortTint.svelte";
  import { sessionModelLook } from "@/lib/model/session-model-look";

  const { sessionId, command }: {
    sessionId: string;
    /** The agent's command, to rank the effort on the model's own ladder from
     *  its catalog - without it, the model family's usual ladder stands in. */
    command?: string;
  } = $props();

  const look = $derived(
    sessionModelLook({
      sessionId,
      command
    })
  );
</script>

{#if look !== null}
  <EffortTint tone={look.tone}>
    {look.running.name}
    {#if look.running.isUltracode}
      <Icon name="bolt" size={12} />
    {/if}
  </EffortTint>
{/if}
