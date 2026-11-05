<script lang="ts">
  // A tab's running model and effort, the design's model badge: the model in
  // one word ("Opus") and its effort as signal bars on the model's own ladder
  // (from its agent's catalog), with the full words in the tooltip. A button:
  // it opens the window's model menu. Nothing renders until the agent has
  // reported.

  import Icon from "@/lib/Icon.svelte";
  import EffortBars from "@/lib/model/EffortBars.svelte";
  import { sessionModelLook } from "@/lib/model/session-model-look";
  import { MODEL_MENU_ID, modelMenuAnchor, ModelMenuOpener, openModelMenu } from "@/lib/stores/model/modelMenu.svelte";
  import type { AgentSession } from "@/lib/types";

  const { session, isMeasuring }: {
    session: AgentSession;
    /** The strip's off-layout measuring copy, which must never claim the
     *  menu's anchor (two anchors of one name pin the menu to the wrong pill). */
    isMeasuring: boolean;
  } = $props();

  const look = $derived(
    sessionModelLook({
      sessionId: session.id,
      command: session.agent.command
    })
  );
  const anchorName = $derived.by(() => {
    if (isMeasuring) {
      return undefined;
    }

    return modelMenuAnchor({
      sessionId: session.id,
      opener: ModelMenuOpener.Tab
    });
  });
</script>

{#if look !== null}
  <button
    style:anchor-name={anchorName}
    class="model"
    aria-label={`${look.label} - change model or effort`}
    data-noreorder
    data-tooltip={`${look.label} · click to change`}
    onclick={async () => await openModelMenu({
      session,
      opener: ModelMenuOpener.Tab
    })}
    popovertarget={MODEL_MENU_ID}
  >
    <span class="name">
      {look.shortName}
      {#if look.running.isUltracode}
        <Icon name="bolt" size={10} />
      {/if}
    </span>
    <EffortBars --effort-bar-width="2px" --effort-bars-height="10px" bars={look.bars} />
  </button>
{/if}

<style>
  /* A sunken chip of the ground colour inside the pill, in the pill's ink. */
  .model {
    display: inline-flex;
    flex-shrink: 0;
    gap: 4px;
    align-items: center;
    block-size: 20px;
    margin-block: 0;
    margin-inline: 2px;
    padding-block: 0;
    padding-inline: 6px;
    border: none;
    border-radius: 6px;
    background: var(--surface-1);
    color: inherit;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.59375rem;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color 120ms var(--ease);

    /* The contents jitter, not the tag: a translated tag would hold its own
       tooltip's fixed bubble and draw it over the tag. */
    @media (prefers-reduced-motion: no-preference) {
      :global(:root[data-palette="cyberpunk"]) &:hover > :global(*) {
        animation: var(--cyber-jitter-animation);
      }
    }

    &:hover,
    &:focus-visible {
      background: var(--surface-3);
    }

    /* Cyberpunk cuts the chip's top-left and bottom-right corners and spaces
       its mono readout; in the dark the readout is lit in the primary. */
    :global(:root[data-palette="cyberpunk"]) & {
      border-radius: var(--radius-small);
      clip-path: polygon(6px 0, 100% 0, 100% calc(100% - 6px), calc(100% - 6px) 100%, 0 100%, 0 6px);
      letter-spacing: 0.02em;
    }

    :global(:root[data-palette="cyberpunk"][data-theme="dark"]) & {
      color: var(--primary);
    }
  }

  .name {
    display: inline-flex;
    gap: 2px;
    align-items: center;
    opacity: 85%;
  }
</style>
