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

    &:hover,
    &:focus-visible {
      background: color-mix(in sRGB, currentColor 14%, transparent);
    }
  }
</style>
