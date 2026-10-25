<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { layoutWithoutPane } from "@/lib/session/panes/pane-layout";
  import { DropSide } from "@/lib/session/panes/reorder";
  import { animatePaneIn, paneDropAt } from "@/lib/shell/screen/pane-dom";
  import SplitMenu from "@/lib/shell/screen/SplitMenu.svelte";
  import { dockBlockSize } from "@/lib/stores/tasks/runnerDock.svelte";
  import type { Agent, AgentSession } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag/drag-reorder";
  import { collapsePane } from "@/lib/ui/motion";
  import type { SvelteSet } from "svelte/reactivity";

  // The terminal area: every session's terminal stays mounted, the ones in the
  // current split are laid out side by side, and a tab dragged over the panes
  // previews where it will land. The shell owns the layout state; this binds it.
  let {
    sessions,
    project,
    agents,
    dragHint,
    collapsingSplitPanes,
    paneIds = $bindable(),
    activeId = $bindable(),
    isPaneDragOverTabs = $bindable(),
    elPanes = $bindable(),
    onselect,
    onexit,
    onlaunch
  }: {
    sessions: AgentSession[];
    /** The project in front, whose runner dock sits under the panes. */
    project: string;
    agents: Agent[];
    /** Live drag state from the tab strip; drives the "drop here" overlay + halves. */
    dragHint: DragHint | null;
    /** Split members collapsing out (out:collapsePane); dropped on `outroend`. */
    collapsingSplitPanes: SvelteSet<string>;
    paneIds: string[];
    activeId: string | null;
    /** True while a split pane's header is dragged up over the tab strip. */
    isPaneDragOverTabs: boolean;
    /** The panes container, so a drop's pointer can be hit-tested against the panes. */
    elPanes?: HTMLElement;
    onselect: (id: string) => void;
    onexit: (id: string) => void;
    onlaunch: (agent: Agent) => void;
  } = $props();

  // How long a split member takes to collapse out (out:collapsePane).
  const SPLIT_PANE_COLLAPSE_MS = 260;

  // A pane can be removed only while more than one is shown; sessions not
  // currently shown are offered in the "add to split" menu.
  const isSplit = $derived(paneIds.length > 1);
  const splitCandidates = $derived(sessions.filter(session => !paneIds.includes(session.id)));

  // The terminal slots render in this order: the shown panes first, in `paneIds`
  // order (so a pane-header drag that reorders `paneIds` reorders the view), then
  // the hidden sessions. Keyed by id, so a reorder moves DOM nodes rather than
  // remounting - every terminal keeps its scrollback.
  const bySessionId = $derived(new Map(sessions.map(session => [session.id, session] as const)));
  const orderedSessions = $derived.by(() => {
    const shown = paneIds
      .map(id => bySessionId.get(id))
      .filter((s): s is AgentSession => s !== undefined);
    const hidden = sessions.filter(session => !paneIds.includes(session.id));
    return [...shown, ...hidden];
  });

  const isDragOverPanes = $derived(dragHint?.isOutside === true);

  // Which pane + side the drag is currently over - the highlighted drop half.
  const splitTarget = $derived.by(() => {
    if (!isDragOverPanes || !dragHint) {
      return null;
    }

    return paneDropAt({
      elPanes,
      paneIds,
      x: dragHint.pointerX,
      y: dragHint.pointerY
    });
  });
  function dropSideFor(id: string) {
    return splitTarget?.id === id ? splitTarget.side : null;
  }
</script>

<section bind:this={elPanes} class="pane terminal-pane" data-panes>
  <!-- A tab dragged over the panes reads as "open in split" - a dashed
       primary frame invites the drop; each pane then shows the left/right
       half the pointer is over (below). -->
  {#if isDragOverPanes}
    <div class="drop-overlay">
      <span class="drop-badge"><Icon name="columns" /> Drop to open in split view</span>
    </div>
  {/if}
  {#if orderedSessions.length > 0}
    <!-- xterm is the heaviest frontend dependency. Load it only when
         the first live session needs a pane, not while the picker/onboarding
         screen is starting. The keyed loop still owns the mounted terminals
         after that, preserving their PTY attachment and scrollback. -->
    {#await import("@/panels/Terminal.svelte") then { default: Terminal }}
      {#each orderedSessions as session (session.id)}
        <div
          style:--dock-difference="{dockBlockSize(session.project) - dockBlockSize(project)}px"
          class="terminal-slot"
          class:shown={paneIds.includes(session.id)}
          data-pane-id={session.id}
          onfocusin={() => {
            // Focusing a split pane makes it the active one, so activeId
            // always names the last agent the user touched - the pane a
            // window-refocus (Ctrl+Alt+[ / ]) then hands the keyboard back to.
            if (paneIds.includes(session.id)) {
              activeId = session.id;
            }
          }}
          onoutroend={() => collapsingSplitPanes.delete(session.id)}
          out:collapsePane={{ duration: collapsingSplitPanes.has(session.id) ? SPLIT_PANE_COLLAPSE_MS : 0 }}
        >
          {#if dropSideFor(session.id) === DropSide.Left}
            <div class="drop-half left"></div>
          {:else if dropSideFor(session.id) === DropSide.Right}
            <div class="drop-half right"></div>
          {/if}
          <Terminal
            isActive={session.id === activeId && paneIds.includes(session.id)}
            isRemovable={isSplit && paneIds.includes(session.id)}
            isShown={paneIds.includes(session.id)}
            ondraghint={hint => (isPaneDragOverTabs = hint?.isOutside === true)}
            {onexit}
            onpopout={() => {
              // The pane header was dragged onto the tab strip: pop it out
              // of the split into its own tab, shown fullscreen and active
              // (the mirror of dragging a tab down onto the panes). Unlike
              // the pane's × it collapses the split to this session.
              if (isSplit) {
                onselect(session.id);
              }
            }}
            onremove={() => {
              // Drop a pane from the split - never the last visible one.
              if (!isSplit) {
                return;
              }

              ({ paneIds, activeId } = layoutWithoutPane({
                layout: {
                  sessions,
                  paneIds,
                  activeId
                },
                id: session.id
              }));
            }}
            onreorder={ids => (paneIds = ids)}
            {session}
          />
        </div>
      {/each}
    {/await}
  {/if}

  <SplitMenu
    {agents}
    candidates={splitCandidates}
    onadd={id => {
      // Show an already-running session alongside the current pane(s) - a
      // split-add, so the newcomer springs in.
      if (!paneIds.includes(id)) {
        paneIds = [...paneIds, id];
        animatePaneIn({
          elPanes,
          id
        });
      }

      activeId = id;
    }}
    {onlaunch}
  />
</section>

<style>
  .pane {
    overflow: hidden;
    min-block-size: 0;
    min-inline-size: 0;
  }

  /* All sessions stay mounted so their scrollback survives switching; only the
     sessions in the current split are laid out (side by side). A hidden slot is
     NOT `display:none` - that would zero its ResizeObserver and shrink the
     background PTY to a 2×1 grid, which some agents (Codex) exit on. Instead it
     is lifted out of flow over the whole pane and turned invisible, so it keeps
     a real, full-pane layout size: the background PTY stays sized to exactly what
     its tab will show, and switching tabs needs no refit at all. That holds
     across projects too: the runner dock under the panes is the FRONT
     project's, so a slot of another project ends by the difference between
     its project's dock and the one shown - the height it will have once its
     project, and its dock, come to the front. */
  .terminal-pane {
    position: relative;
    display: flex;
    align-items: stretch;
  }

  .terminal-slot {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-block-size: 0;
    min-inline-size: 0;
    border-inline-end: 1px solid var(--outline);

    /* A pane appearing is deliberately instant - opening, closing, or switching a
       tab must not animate the terminal (it would read as a needless wipe). Only a
       genuine split-ADD springs its newcomer in, fired imperatively on that one
      slot via animatePaneIn (pane-enter), never here on every `.shown`. */
    &:not(.shown) {
      position: absolute;
      inset: 0 0 var(--dock-difference);
      visibility: hidden;
      pointer-events: none;
    }
  }

  /* Dashed primary frame over the whole panes area while a tab is dragged onto
     it - the invitation to open the session as a split. */
  .drop-overlay {
    position: absolute;
    inset: 8px;
    z-index: 60;
    display: grid;
    place-items: center;
    border: 2px dashed var(--primary);
    border-radius: var(--radius-medium);
    background: color-mix(in oklab, var(--primary) 8%, transparent);
    pointer-events: none;
    animation: panel-swap 160ms var(--ease);

    .drop-badge {
      display: inline-flex;
      gap: 8px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 14px;
      border-radius: var(--radius-full);
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 700;
      font-size: 0.8125rem;
      box-shadow: 0 6px 20px var(--shadow-color);
    }
  }

  /* The half of a pane the pointer is over - where the dropped session will land
     (left = before this pane, right = after). A primary wash plus a solid edge. */
  .drop-half {
    position: absolute;
    inset-block: 0;
    z-index: 58;
    inline-size: 50%;
    background: color-mix(in oklab, var(--primary) 16%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--primary) 40%, transparent);
    pointer-events: none;

    &.left {
      inset-inline-start: 0;
      border-inline-start: 3px solid var(--primary);
    }

    &.right {
      inset-inline-end: 0;
      border-inline-end: 3px solid var(--primary);
    }
  }
</style>
