<script lang="ts" module>
  /** The anchor the strip's one model menu hangs off - set on whichever tag opened it. */
  export const MODEL_MENU_ANCHOR = "--model-menu-anchor";
</script>

<script lang="ts">
  // A tab's running model and effort: the model's name, coloured by how high its
  // effort sits on that model's own ladder (from its agent's catalog), with the
  // exact words in the tooltip. A button: it opens the tab strip's model menu.
  // Nothing renders until the agent has reported.

  import { runningModelLook } from "@/lib/model/running-model";
  import RunningModelName from "@/lib/model/RunningModelName.svelte";
  import { sessionModel } from "@/lib/stores/agent/context.svelte";
  import type { AgentSession } from "@/lib/types";

  const { session, menuId, anchored, onopen }: {
    session: AgentSession;
    /** The model menu popover this tag opens. */
    menuId: string;
    /** Whether the open menu hangs off this tag (one tag at a time). */
    anchored: boolean;
    onopen: () => Promise<void>;
  } = $props();

  const running = $derived(sessionModel(session.id));
  const anchorName = $derived.by(() => {
    if (anchored) {
      return MODEL_MENU_ANCHOR;
    }

    return undefined;
  });
  const label = $derived.by(() => {
    if (running === null) {
      return "";
    }

    return runningModelLook({
      running
    }).label;
  });
</script>

{#if running !== null}
  <button
    style:anchor-name={anchorName}
    class="model"
    aria-label={`${label} - change model or effort`}
    data-noreorder
    data-tooltip={`${label} · click to change`}
    onclick={async () => await onopen()}
    popovertarget={menuId}
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
