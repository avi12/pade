<script lang="ts">
  // A tab's running model and effort: the model's name, then its effort as
  // rising bars on that model's own ladder (from its agent's catalog), with the
  // exact words in the tooltip. A button: it opens the tab strip's model menu.
  // Nothing renders until the agent has reported.

  import EffortBars from "@/lib/EffortBars.svelte";
  import { effortStep, modelLabel } from "@/lib/model-label";
  import { sessionModel } from "@/lib/stores/context.svelte";
  import { modelOption } from "@/lib/stores/modelOptions.svelte";
  import type { AgentSession } from "@/lib/types";

  const { session, menuId, anchored, onopen }: {
    session: AgentSession;
    /** The model menu popover this tag opens. */
    menuId: string;
    /** Whether the open menu hangs off this tag (one tag at a time). */
    anchored: boolean;
    onopen: () => Promise<void>;
  } = $props();

  const MODEL_MENU_ANCHOR = "--model-menu-anchor";

  const running = $derived(sessionModel(session.id));
  const label = $derived.by(() => {
    if (running === null) {
      return "";
    }

    return modelLabel(running);
  });
  const effortStepShown = $derived.by(() => {
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
  const unladderedEffortWord = $derived.by(() => {
    const isDrawn = effortStepShown !== null;
    if (isDrawn || running === null || running.effort === null) {
      return "";
    }

    return `· ${running.effort}`;
  });
</script>

{#if running !== null}
  <button
    style:anchor-name={anchored ? MODEL_MENU_ANCHOR : undefined}
    class="model"
    aria-label={`${label} - change model or effort`}
    data-noreorder
    data-tooltip={`${label} · click to change`}
    onclick={async () => await onopen()}
    popovertarget={menuId}
  >
    {running.name}
    {#if effortStepShown === null}
      {unladderedEffortWord}
    {:else}
      <EffortBars step={effortStepShown.step} steps={effortStepShown.steps} />
    {/if}
  </button>
{/if}

<style>
  /* Reads as part of the pill's label, not as a separate control - until it is
     hovered. Its effort bars light in --effort-lit, which the pill sets. */
  .model {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    padding: 2px 6px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: inherit;
    font-family: var(--font-monospace);
    font-size: 10px;
    opacity: 80%;
    cursor: pointer;
    transition:
      opacity 120ms var(--ease),
      background-color 120ms var(--ease);

    &:hover,
    &:focus-visible {
      background: color-mix(in sRGB, currentColor 14%, transparent);
      opacity: 100%;
    }
  }
</style>
