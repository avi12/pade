<script lang="ts">
  // The window's one model menu, hung off whichever model chip opened it - a
  // tab's model tag or a pane header's chip (stores/model/modelMenu).

  import ModelMenu from "@/lib/model/ModelMenu.svelte";
  import { MODEL_MENU_ANCHOR, MODEL_MENU_ID, modelMenuSessionId } from "@/lib/stores/model/modelMenu.svelte";
  import type { AgentSession, ModelChoice } from "@/lib/types";
  import { rovingMenu } from "@/lib/ui/keyboard/roving-menu";

  const { sessions, onchoose }: {
    /** Every session a chip can open the menu for. */
    sessions: AgentSession[];
    onchoose: (pick: {
      session: AgentSession;
      choice: ModelChoice;
    }) => Promise<void>;
  } = $props();

  const session = $derived(sessions.find(candidate => candidate.id === modelMenuSessionId()));
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
     width and anchor side live here. It opens rightward from a tab's tag and
     flips to open leftward from a chip near the window's end, a pane header's. */
  .menu {
    min-inline-size: 220px;
    position-area: bottom span-right;
    position-try-fallbacks: flip-inline;
  }
</style>
