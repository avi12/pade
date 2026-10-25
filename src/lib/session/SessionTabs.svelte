<script lang="ts" module>
  export { ADD_SESSION_MENU_ID } from "@/lib/session/tabs/LaunchMenu.svelte";
</script>

<script lang="ts">
  import { ADD_SLOT, packTabs } from "@/lib/session/tab-fit";
  import AddSessionMenu from "@/lib/session/tabs/AddSessionMenu.svelte";
  import OverflowSessionMenu from "@/lib/session/tabs/OverflowSessionMenu.svelte";
  import SessionDot from "@/lib/session/tabs/SessionDot.svelte";
  import SessionTab from "@/lib/session/tabs/SessionTab.svelte";
  import { collapseTab } from "@/lib/session/tabs/tab-collapse";
  import { beginTabDrag } from "@/lib/session/tabs/tab-drag";
  import TabModelMenu from "@/lib/session/tabs/TabModelMenu.svelte";
  import { awaitingChoice } from "@/lib/stores/session/sessionAttention.svelte";
  import type { Agent, AgentSession, ModelChoice } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag/drag-reorder";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  // The session tab strip: full pills for the sessions that fit, status dots
  // for the next few, a "+N" popover for the rest (packing in lib/session/tab-fit), the
  // off-layout mirror row that drives the measurements, and the add-agent menu.
  const {
    sessions,
    activeId,
    paneIds,
    agents,
    branches,
    onselect,
    onclose,
    onlaunch,
    onlaunchbranch,
    onreorder,
    onsplit,
    ondraghint,
    onmodelchoice,
    popPaneActive = false
  }: {
    sessions: AgentSession[];
    activeId: string | null;
    /** Sessions currently shown side by side - their pills read as "shown". */
    paneIds: string[];
    agents: Agent[];
    /** A split pane's header is being dragged over the strip - light it as a
     *  "drop to pop the pane out as a tab" target, mirroring the split overlay. */
    popPaneActive?: boolean;
    /** Local branches when the project is a git repo - offered as worktrees. */
    branches: string[];
    onselect: (id: string) => void;
    onclose: (session: AgentSession) => void;
    onlaunch: (agent: Agent) => void;
    onlaunchbranch: (branch: string) => Promise<void>;
    /** A drag reordered the visible pills - commit the new session order. */
    onreorder?: (orderedIds: string[]) => void;
    /** A pill was dropped over the terminal panes - open it as a split there. */
    onsplit?: (drop: {
      id: string;
      pointerX: number;
      pointerY: number;
    }) => void;
    /** Live drag state, so the shell's TerminalPanes can paint the "drop here" overlay. */
    ondraghint?: (hint: DragHint | null) => void;
    /** A model / effort was picked from a tab's model menu - apply it. */
    onmodelchoice: (pick: {
      session: AgentSession;
      choice: ModelChoice;
    }) => Promise<void>;
  } = $props();

  let modelMenuSessionId = $state<string | null>(null);
  const modelMenuSession = $derived(sessions.find(session => session.id === modelMenuSessionId));

  // ── Measurement ─────────────────────────────────────────────────────────────
  // The strip is bounded to the width the nav gives it. Pill widths come from
  // an off-layout mirror row (re-measured on session change / reflow) so
  // collapsing a tab never changes the numbers we packed against.
  let elStrip = $state<HTMLElement>();
  let elMeasure = $state<HTMLElement>();
  let stripWidth = $state(0);
  const tabWidths = new SvelteMap<string, number>();

  // Read each mirror pill's natural width into a fresh map (index-aligned with
  // `sessions`, since the mirror renders them in order).
  function measureTabs(sessionList: AgentSession[]) {
    const elMirror = elMeasure;
    if (!elMirror) {
      return;
    }

    tabWidths.clear();
    for (const [i, session] of sessionList.entries()) {
      const elTab = elMirror.children[i];
      if (elTab instanceof HTMLElement) {
        tabWidths.set(session.id, elTab.offsetWidth);
      }
    }
  }

  // Sync the strip's available width, then re-measure the pills.
  function remeasureTabStrip() {
    if (elStrip) {
      stripWidth = elStrip.clientWidth;
    }

    measureTabs(sessions);
  }

  // Re-measure after the mirror re-renders for a changed session set. Passing
  // `sessions` in is what subscribes this effect to that set: a bare read would
  // be an unused expression, and measureTabs bails early before touching it when
  // the mirror isn't mounted yet.
  $effect(() => {
    measureTabs(sessions);
  });

  // Track the strip's available width and re-measure on any reflow (font load,
  // window resize); both the strip and the mirror are observed.
  $effect(() => {
    const elObserved = elStrip;
    if (!elObserved) {
      return;
    }

    const observer = new ResizeObserver(remeasureTabStrip);
    observer.observe(elObserved);

    if (elMeasure) {
      observer.observe(elMeasure);
    }

    remeasureTabStrip();
    return () => observer.disconnect();
  });

  // Greedy three-tier packing: full pills → status dots → "+N" overflow.
  const tabPack = $derived(
    packTabs({
      ids: sessions.map(session => session.id),
      widthOf: id => tabWidths.get(id) ?? 0,
      // Reserve the trailing add button's slot so tabs never sit under it.
      stripWidth: Math.max(0, stripWidth - ADD_SLOT)
    })
  );

  const bySessionId = $derived(new Map(sessions.map(session => [session.id, session] as const)));
  function tabsFor(ids: string[]) {
    return ids
      .map(id => bySessionId.get(id))
      .filter((session): session is AgentSession => session !== undefined);
  }
  const visibleSessions = $derived(tabsFor(tabPack.visible));
  const dotSessions = $derived(tabsFor(tabPack.dots));
  const moreSessions = $derived(tabsFor(tabPack.more));
  const overflowHasActive = $derived(
    activeId !== null && (tabPack.dots.includes(activeId) || tabPack.more.includes(activeId))
  );

  // A tab flashes red while its agent waits on a multiple-choice answer - but not
  // while it's the tab in front, since looking at it is what stops the flashing
  // (the reconcile in the shell's sessionAutomation clears the flag; this also gates the very first frame).
  function isAwaitingChoice(id: string) {
    return awaitingChoice(id) && id !== activeId;
  }

  // The last tab's agent - a plain "+" click launches another of the same kind.
  const lastAgent = $derived(sessions.at(-1)?.agent ?? agents[0]);

  // Closing a tab removes the session synchronously; the pill's collapse is a
  // Svelte out-transition. `closingIds` marks which pills left via a real close
  // so the transition only animates those - a repack-driven exit snaps instantly.
  const closingIds = new SvelteSet<string>();

  function closeTab(session: AgentSession) {
    const hasVisiblePill = visibleSessions.some(candidateSession => candidateSession.id === session.id);
    if (hasVisiblePill) {
      closingIds.add(session.id);
    }

    onclose(session);
  }

  /** Route app-level close shortcuts through the same animated seam as pointer
   * and close-button activation. */
  export function closeSession(session: AgentSession) {
    closeTab(session);
  }

  // Collapse a closing pill; its size is read once as the outro begins. Tab
  // reordering is animated by the drag engine's spring-settle, not a framework
  // FLIP, so the pills carry no `animate:flip` (it would fight it).
  function collapse(elTab: HTMLElement, { id }: { id: string }) {
    if (!closingIds.has(id)) {
      return { duration: 0 };
    }

    return collapseTab({
      width: elTab.offsetWidth,
      height: elTab.offsetHeight
    });
  }

  // The session whose pill shows the inline rename field.
  let editingId = $state<string | null>(null);
</script>

<!-- `isMirror`: the off-layout measuring copy of the pill. -->
{#snippet tabInner(session: AgentSession, isMirror: boolean)}
  <SessionTab
    isActive={session.id === activeId}
    isAwaitingChoice={isAwaitingChoice(session.id)}
    isEditing={editingId === session.id}
    {isMirror}
    isModelMenuTarget={modelMenuSessionId === session.id}
    onclose={() => closeTab(session)}
    onmodelopen={() => (modelMenuSessionId = session.id)}
    onrename={() => (editingId = session.id)}
    onrenamedone={() => (editingId = null)}
    onselect={() => onselect(session.id)}
    {session}
  />
{/snippet}

<nav class="tabs" aria-label="Agent sessions">
  <div bind:this={elStrip} class="tab-strip" class:drop-target={popPaneActive} data-tab-strip>
    <span class="popout-hint" aria-hidden="true">Drop here - new tab</span>
    {#each visibleSessions as session (session.id)}
      {@const isActive = session.id === activeId}
      <!-- Pointer-only reorder handle; select/close/rename stay keyboard-reachable
           through the buttons inside, so the drag is a pure enhancement. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={isActive}
        class:shown={paneIds.includes(session.id)}
        data-selected-tab={isActive}
        data-session-tab={session.id}
        onoutroend={() => closingIds.delete(session.id)}
        onpointerdown={e => beginTabDrag({
          e,
          isRenaming: editingId !== null,
          onreorder,
          ondraghint,
          onsplit
        })}
        out:collapse={{ id: session.id }}
      >
        {@render tabInner(session, false)}
      </div>
    {/each}

    {#each dotSessions as session (session.id)}
      <button
        class="tab-dot"
        class:active={session.id === activeId}
        aria-label={session.agent.label}
        data-tooltip={session.agent.label}
        onclick={() => onselect(session.id)}
      ><SessionDot isAwaitingChoice={isAwaitingChoice(session.id)} sessionId={session.id} /></button>
    {/each}

    <OverflowSessionMenu
      {activeId}
      isActive={overflowHasActive}
      {isAwaitingChoice}
      {onclose}
      {onselect}
      sessions={moreSessions}
    />

    <AddSessionMenu {agents} {branches} {lastAgent} {onlaunch} {onlaunchbranch} />
  </div>

  <!-- Off-layout mirror: every tab at full width, purely for measuring. Keeps
       the active/shown classes so the measured width matches the rendered pill. -->
  <span bind:this={elMeasure} class="tab-measure" aria-hidden="true">
    {#each sessions as session (session.id)}
      {@const isActive = session.id === activeId}
      <div
        class="tab"
        class:active={isActive}
        class:shown={paneIds.includes(session.id)}
        data-selected-tab={isActive}
      >
        {@render tabInner(session, true)}
      </div>
    {/each}
  </span>

  <TabModelMenu onchoose={onmodelchoice} session={modelMenuSession} />
</nav>

<style>
  .tabs {
    position: relative;
    display: flex;
    flex: 1 1 0;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    /* The visible, bounded strip - pills/dots/+N clip here rather than wrap. */
    .tab-strip {
      display: flex;
      flex: 1;
      gap: 6px;
      align-items: center;
      min-inline-size: 0;
      border-radius: 8px;

      /* Transparent at rest so only the colour animates when a dragged pane is
         over the strip (the pop-out drop zone), never the box's geometry. */
      outline: 2px solid transparent;
      outline-offset: 2px;
      transition: outline-color 150ms var(--ease);

      &.drop-target {
        outline-color: var(--primary);
      }
    }

    /* Leading cue shown only while a pane is dragged over the strip: "drop here to
       pop it out as a tab", the mirror of the panes' "drop to open in split view". */
    .popout-hint {
      flex: none;
      padding-inline: 6px;
      color: var(--primary);
      font-weight: 700;
      font-size: 0.625rem;
      white-space: nowrap;

      .tab-strip:not(.drop-target) & {
        display: none;
      }
    }

    /* Off-layout copy of every full pill, measured to drive the packing. */
    .tab-measure {
      position: absolute;
      inset-block-start: 0;
      inset-inline-start: 0;
      display: flex;
      gap: 6px;
      visibility: hidden;
      pointer-events: none;
    }

    /* A session collapsed to just its status dot. */
    .tab-dot {
      display: inline-grid;
      flex: none;
      place-items: center;
      block-size: 22px;
      inline-size: 22px;
      border: none;
      border-radius: var(--radius-full);
      background: var(--surface-2);
      cursor: pointer;
      transition: background 150ms var(--ease);

      &.active {
        background: var(--primary-container);
      }

      &:hover {
        background: var(--surface-3);
      }
    }

    /* The pill itself; its contents (SessionTab) style off its hover / active. */
    .tab {
      display: inline-flex;
      align-items: center;
      border-radius: var(--radius-full);
      background: var(--surface-2);

      /* The pill body is a drag handle (reorder / split); a touch-drag must
         grab the pill, not scroll the strip. */
      cursor: grab;
      touch-action: none;
      animation: spring-in 320ms var(--ease);

      &:active {
        cursor: grabbing;
      }

      &.active {
        background: var(--primary-container);
      }
    }
  }

  /* Cyberpunk frames every control with an outline drawn one pixel inside.
     The pill is a DIV wrapping three buttons, so the frame goes on the pill and
     its parts carry none (below) - the console draws one chamfered shape per
     control, never boxes inside boxes. Hover lights the frame and a press snaps
     it one step inward, like any button's. `:where` keeps all three weaker than
     the selected-tab look (theme.css, `data-selected-tab`), which re-colours the
     active pill's frame. */
  :global(:root[data-palette="cyberpunk"]) :where(.tab):not(:focus-visible) {
    outline: 1px solid var(--control-edge);
    outline-offset: -1px;

    &:hover {
      outline-color: var(--hairline);
    }

    &:active {
      outline-width: 2px;
      outline-offset: -3px;
    }
  }

  /* On paper the frame is already the ink it would light to, so hover takes
     the card weight instead. */
  :global(:root[data-palette="cyberpunk"][data-theme="light"]) :where(.tab):hover:not(:focus-visible, :active) {
    outline-width: 2px;
    outline-offset: -2px;
  }

  /* The pill carries the frame, so the buttons inside it (SessionTab's, the
     model tag's, the pacing badge's - child components with no hook for it)
     carry none. Kept outside `.tabs` so it stays weaker than an open menu
     trigger's frame and a menu row's hover frame. */
  :global(:root[data-palette="cyberpunk"]) .tab :global(button:not(:focus-visible)) {
    outline: none;
  }

  /* Red attention ripple for a tab whose agent is waiting on a multiple-choice
     answer (see isAwaitingChoice) - a ring pulses out from the indicator. Global
     so the strip's parts (glyph, dot, "+N") share the one ripple. Held still (a
     steady red ring instead) under reduced motion, at each usage. */
  /* stylelint-disable-next-line keyframes-name-pattern -- Svelte's `-global-` prefix */
  @keyframes -global-choice-flash {
    0% {
      box-shadow: 0 0 0 0 color-mix(in oklab, var(--critical) 60%, transparent);
    }

    70% {
      box-shadow: 0 0 0 6px transparent;
    }

    100% {
      box-shadow: 0 0 0 0 transparent;
    }
  }
</style>
