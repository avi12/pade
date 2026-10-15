<script lang="ts" module>
  /** The strip's one model menu - every tab's model tag opens it by this id. */
  export const MODEL_MENU_ID = "model-menu";
</script>

<script lang="ts">
  // One model menu for the whole strip, hung off whichever tab's model tag
  // opened it.

  import ModelMenu from "@/lib/model/ModelMenu.svelte";
  import { MODEL_MENU_ANCHOR } from "@/lib/model/ModelTag.svelte";
  import type { AgentSession, ModelChoice } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/roving-menu";

  const { session, onchoose }: {
    /** The session whose model tag last opened the menu. */
    session: AgentSession | undefined;
    onchoose: (pick: {
      session: AgentSession;
      choice: ModelChoice;
    }) => Promise<void>;
  } = $props();
</script>

<div
  id={MODEL_MENU_ID}
  style:position-anchor={MODEL_MENU_ANCHOR}
  class="menu popover-menu model-menu"
  {@attach rovingMenu}
  popover
>
  {#if session !== undefined}
    <ModelMenu
      onchoose={async choice => await onchoose({
        session,
        choice
      })}
      {session}
    />
  {/if}
</div>

<style>
  /* Shell and rows come from the shared .popover-menu and .menu-item; only
     width and anchor side live here. */
  .menu {
    min-inline-size: 220px;
    position-area: bottom span-right;
  }
</style>
