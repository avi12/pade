<script lang="ts">
  // A tab's running model and effort: the model's name, coloured by how high its
  // effort sits on that model's own ladder (from its agent's catalog), with the
  // exact words in the tooltip. A button: it opens the window's model menu.
  // Nothing renders until the agent has reported.

  import RunningModelName from "@/lib/model/RunningModelName.svelte";
  import { sessionModelLook } from "@/lib/model/session-model-look";
  import { MODEL_MENU_ID, modelMenuAnchor, ModelMenuOpener, openModelMenu } from "@/lib/stores/model/modelMenu.svelte";
  import type { AgentSession } from "@/lib/types";

  const { session, isMeasuring }: {
    session: AgentSession;
    /** The strip's off-layout measuring copy, which must never claim the
     *  menu's anchor (two anchors of one name pin the menu to the wrong pill). */
    isMeasuring: boolean;
  } = $props();

  const look = $derived(sessionModelLook({ sessionId: session.id }));
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
    <RunningModelName command={session.agent.command} sessionId={session.id} />
  </button>
{/if}

<style>
  /* Reads as part of the pill's label, not as a separate control - until it is
     hovered. */
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
    font-size: 0.625rem;
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
      background: color-mix(in sRGB, currentColor 14%, transparent);
    }

    /* Cyberpunk sets the tag on a sunken chip of the ground colour, the
       design's model badge, and cuts its top-left and bottom-right corners. */
    :global(:root[data-palette="cyberpunk"]) & {
      block-size: 20px;
      padding-block: 0;
      background: var(--surface-1);
      clip-path: polygon(4px 0, 100% 0, 100% calc(100% - 4px), calc(100% - 4px) 100%, 0 100%, 0 4px);
      font-weight: 700;
      font-size: 0.59375rem;

      &:hover,
      &:focus-visible {
        background: var(--surface-3);
      }
    }
  }
</style>
