<script lang="ts">
  import { agentIconName } from "@/lib/agent-icon";
  import { ContextLevel, contextLevel } from "@/lib/context-level";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import type { DragHint } from "@/lib/drag-reorder";
  import { formatCount, formatPercent } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { effective } from "@/lib/prefs.svelte";
  import { rovingMenu } from "@/lib/roving-menu";
  import { contextPercentage, measuredContextPercentage } from "@/lib/stores/context.svelte";
  import { awaitingChoice } from "@/lib/stores/sessionAttention.svelte";
  import { sessionLabel, setSessionLabel } from "@/lib/stores/sessionLabels.svelte";
  import { isNaming, toggleNaming } from "@/lib/stores/sessionNaming.svelte";
  import { sessionStatus } from "@/lib/stores/sessions.svelte";
  import { ADD_SLOT, packTabs } from "@/lib/tab-fit";
  import { TAB_SHORTCUTS, TabAction } from "@/lib/tab-shortcuts";
  import type { Agent, AgentSession } from "@/lib/types";
  import { parseInput, SessionName } from "@/lib/validate";
  import { cubicOut } from "svelte/easing";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import type { TransitionConfig } from "svelte/transition";

  // The session tab strip: full pills for the sessions that fit, status dots
  // for the next few, a "+N" popover for the rest (packing in lib/tab-fit), the
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
    popPaneActive = false
  }: {
    sessions: AgentSession[];
    activeId: string | null;
    /** Sessions currently shown side by side — their pills read as "shown". */
    paneIds: string[];
    agents: Agent[];
    /** A split pane's header is being dragged over the strip — light it as a
     *  "drop to pop the pane out as a tab" target, mirroring the split overlay. */
    popPaneActive?: boolean;
    /** Local branches when the project is a git repo — offered as worktrees. */
    branches: string[];
    onselect: (id: string) => void;
    onclose: (session: AgentSession) => void;
    onlaunch: (agent: Agent) => void;
    onlaunchbranch: (branch: string) => Promise<void>;
    /** A drag reordered the visible pills — commit the new session order. */
    onreorder?: (orderedIds: string[]) => void;
    /** A pill was dropped over the terminal panes — open it as a split there. */
    onsplit?: (drop: {
      id: string;
      pointerX: number;
      pointerY: number;
    }) => void;
    /** Live drag state, so App can paint the panes' "drop here" overlay. */
    ondraghint?: (hint: DragHint | null) => void;
  } = $props();

  const newTabShortcut = TAB_SHORTCUTS[TabAction.New];
  const launchMenuShortcut = TAB_SHORTCUTS[TabAction.LaunchMenu];

  // ── Measurement ─────────────────────────────────────────────────────────────
  // The strip is bounded to the width the nav gives it. Pill widths come from
  // an off-layout mirror row (re-measured on session change / reflow) so
  // collapsing a tab never changes the numbers we packed against.
  let stripElement = $state<HTMLElement>();
  let measureElement = $state<HTMLElement>();
  let stripWidth = $state(0);
  const tabWidths = new SvelteMap<string, number>();

  // Read each mirror pill's natural width into a fresh map (index-aligned with
  // `sessions`, since the mirror renders them in order).
  function measureTabs(sessionList: AgentSession[]) {
    const mirror = measureElement;
    if (!mirror) {
      return;
    }

    tabWidths.clear();
    sessionList.forEach((session, index) => {
      const element = mirror.children[index];
      if (element instanceof HTMLElement) {
        tabWidths.set(session.id, element.offsetWidth);
      }
    });
  }

  // Sync the strip's available width, then re-measure the pills.
  function remeasureTabStrip() {
    const strip = stripElement;
    if (strip) {
      stripWidth = strip.clientWidth;
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
    const strip = stripElement;
    if (!strip) {
      return;
    }

    const observer = new ResizeObserver(remeasureTabStrip);
    observer.observe(strip);

    if (measureElement) {
      observer.observe(measureElement);
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
  function tabsFor(ids: string[]): AgentSession[] {
    return ids
      .map(id => bySessionId.get(id))
      .filter((session): session is AgentSession => session !== undefined);
  }
  const visibleSessions = $derived(tabsFor(tabPack.visible));
  const dotSessions = $derived(tabsFor(tabPack.dots));
  const moreSessions = $derived(tabsFor(tabPack.more));
  const hasMoreSessions = $derived(moreSessions.length > 0);
  const overflowHasActive = $derived(
    activeId !== null && (tabPack.dots.includes(activeId) || tabPack.more.includes(activeId))
  );

  // A tab flashes red while its agent waits on a multiple-choice answer — but not
  // while it's the tab in front, since looking at it is what stops the flashing
  // (the reconcile in App clears the flag; this also gates the very first frame).
  function isAwaitingChoice(id: string): boolean {
    return awaitingChoice(id) && id !== activeId;
  }
  // Any collapsed-into-"+N" session waiting on a choice — so a pending prompt
  // hidden in the overflow still surfaces on the trigger.
  const overflowAwaiting = $derived(moreSessions.some(session => isAwaitingChoice(session.id)));

  // Whether motion is suppressed — gates the tab-close out-transition below. Tab
  // reordering is animated by the drag engine's spring-settle (drag-reorder), not
  // a framework FLIP, so the pills carry no `animate:flip` (it would fight the
  // engine's own settle).
  const prefersReducedMotion =
    typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  // The last tab's agent — a plain "+" click launches another of the same kind;
  // Ctrl/Cmd-click opens the full launch menu instead.
  const lastAgent = $derived(sessions.at(-1)?.agent ?? agents[0]);

  function openAddMenu() {
    const menu = document.getElementById("add-session-menu");
    if (menu instanceof HTMLElement && !menu.matches(":popover-open")) {
      menu.showPopover();
    }
  }

  // Closing a tab removes the session synchronously; the pill's collapse is a
  // Svelte out-transition. `closingIds` marks which pills left via a real close
  // so the transition only animates those — a repack-driven exit snaps instantly.
  const closingIds = new SvelteSet<string>();
  // Middle-click anywhere on a pill closes it (preventDefault stops the browser's
  // middle-click autoscroll). onmousedown suppresses the same on press.
  function onTabPointer(e: MouseEvent, session: AgentSession) {
    if (e.button !== 1) {
      return;
    }

    e.preventDefault();

    if (e.type === "auxclick") {
      closeTab(session);
    }
  }

  function closeTab(session: AgentSession) {
    const hasVisiblePill = visibleSessions.some(candidateSession => candidateSession.id === session.id);
    if (hasVisiblePill) {
      closingIds.add(session.id);
    }

    onclose(session);
  }

  /** Route app-level close shortcuts through the same animated seam as pointer
   * and close-button activation. */
  export function closeSession(session: AgentSession): void {
    closeTab(session);
  }

  // Collapse a closing pill (width + fade), pinning its height so the label
  // reflow can't grow the row. Height/width are read once as the outro begins.
  function collapse(node: HTMLElement, { id }: { id: string }): TransitionConfig {
    if (prefersReducedMotion || !closingIds.has(id)) {
      return { duration: 0 };
    }

    const width = node.offsetWidth;
    const height = node.offsetHeight;
    return {
      duration: 240,
      easing: cubicOut,
      css: t =>
        `overflow: hidden; block-size: ${height}px; inline-size: ${width * t}px;` +
          `opacity: ${t}; margin-inline-start: ${(t - 1) * 6}px;`
    };
  }

  // ── Inline manual rename ────────────────────────────────────────────────────
  let editingId = $state<string | null>(null);
  let renameDraft = $state("");

  // Enter inline rename for a session, seeding the field with its current label.
  function startRename(id: string) {
    editingId = id;
    renameDraft = sessionLabel(id) ?? bySessionId.get(id)?.agent.label ?? "";
  }

  function commitRename() {
    if (editingId === null) {
      return;
    }

    const label = parseInput({
      schema: SessionName,
      raw: renameDraft
    });
    if (label !== null) {
      setSessionLabel({
        id: editingId,
        label
      });
    }

    editingId = null;
  }

  // Focus + select the rename field the moment it mounts.
  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // Press-and-drag a pill by its body to reorder the strip (past a 5px threshold),
  // or drag it down over the terminal panes to open it as a split. The close / AI
  // buttons and the rename field carry `data-noreorder`, so a press on them is
  // theirs; a plain press-and-release still selects. Never while renaming.
  function startTabDrag(e: PointerEvent) {
    if (editingId !== null) {
      return;
    }

    beginReorder({
      e,
      itemSelector: "[data-session-tab]",
      idAttribute: "data-session-tab",
      axis: Axis.Horizontal,
      threshold: 5,
      ignoreSelector: "[data-noreorder]",
      onCommit: ids => onreorder?.(ids),
      onHint: hint => ondraghint?.(hint),
      outsideSelector: "[data-panes]",
      onDropOutside: drop => onsplit?.(drop)
    });
  }

  // The glyph tooltip states only what it can vouch for: the agent's own
  // reported percent reads as fact; the byte estimate over-counts a fullscreen
  // agent's repaints badly (see lib/stores/context), so it is labeled the
  // rough estimate it is instead of masquerading as a measurement.
  function contextTooltip(id: string): string {
    const measured = measuredContextPercentage(id);
    if (measured !== null) {
      return `${formatPercent(measured)} of context window used`;
    }

    const estimate = contextPercentage(id);
    if (estimate === null) {
      return "Context window — measuring…";
    }

    return `≈${formatPercent(estimate)} of context window used — rough estimate`;
  }
</script>

<!-- The tab's leading glyph: the agent's brand mark, tinted by how full its
     context window is (green→amber→red toward the auto-handoff threshold) and
     carrying status — a working agent breathes, a ready one gets a soft halo. -->
{#snippet statusGlyph(session: AgentSession)}
  {@const percentage = contextPercentage(session.id)}
  {@const level = percentage === null ? null : contextLevel({
    percentage,
    threshold: effective.handoffPercentage
  })}
  <span
    class="agent-icon {sessionStatus(session.id)}"
    class:awaiting-choice={isAwaitingChoice(session.id)}
    class:critical={level === ContextLevel.critical}
    class:unknown={percentage === null}
    class:warning={level === ContextLevel.warning}
    data-tooltip={contextTooltip(session.id)}
  ><Icon name={agentIconName(session.agent.id)} size={14} /></span>
{/snippet}

{#snippet tabInner(session: AgentSession)}
  {#if editingId === session.id}
    <span class="rename">
      {@render statusGlyph(session)}
      <input
        class="rename-input"
        aria-label="Rename session"
        data-noreorder
        onblur={commitRename}
        oninput={e => (renameDraft = e.currentTarget.value)}
        onkeydown={e => {
          if (e.key === "Enter") {
            e.preventDefault();
            commitRename();
          } else if (e.key === "Escape") {
            e.preventDefault();
            editingId = null;
          }
        }}
        value={renameDraft}
        use:focusOnMount
      />
    </span>
  {:else}
    <button
      class="pick"
      onauxclick={e => onTabPointer(e, session)}
      onclick={() => {
        // Finder-style: a click selects an inactive tab; clicking the already-active
        // tab renames it (its label reads with a text caret). The reorder engine
        // swallows the post-drag click, so dragging the active pill never renames.
        if (session.id === activeId) {
          startRename(session.id);
        } else {
          onselect(session.id);
        }
      }}
      onmousedown={e => onTabPointer(e, session)}
    >
      {@render statusGlyph(session)}
      <span class="label">{sessionLabel(session.id) ?? session.agent.label}</span>
    </button>
    <span
      class="ai-wrap"
      class:on={isNaming(session.id)}
      data-tooltip={isNaming(session.id) ? "Auto-naming on — click to turn off" : "Auto-name this session with AI"}
    >
      <button
        class="ai"
        aria-label="Auto-name this session with AI"
        data-noreorder
        onclick={() => toggleNaming({
          id: session.id,
          agent: session.agent.command
        })}
      ><Icon name="sparkles" size={13} /></button>
    </span>
  {/if}
  <button
    class="close-button"
    aria-label="Close session"
    data-noreorder
    data-tooltip="Close session"
    onclick={() => closeTab(session)}
  ><Icon name="close" size={13} /></button>
{/snippet}

<nav class="tabs" aria-label="Agent sessions">
  <div bind:this={stripElement} class="tab-strip" class:drop-target={popPaneActive} data-tab-strip>
    {#if popPaneActive}
      <span class="popout-hint" aria-hidden="true">Drop here — new tab</span>
    {/if}
    {#each visibleSessions as session (session.id)}
      <!-- Pointer-only reorder handle; select/close/rename stay keyboard-reachable
           through the buttons inside, so the drag is a pure enhancement. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="tab"
        class:active={session.id === activeId}
        class:shown={paneIds.includes(session.id)}
        data-session-tab={session.id}
        onoutroend={() => closingIds.delete(session.id)}
        onpointerdown={startTabDrag}
        out:collapse={{ id: session.id }}
      >
        {@render tabInner(session)}
      </div>
    {/each}

    {#each dotSessions as session (session.id)}
      <button
        class="tab-dot"
        class:active={session.id === activeId}
        aria-label={session.agent.label}
        data-tooltip={session.agent.label}
        onclick={() => onselect(session.id)}
      ><span
        class="dot {sessionStatus(session.id)}"
        class:awaiting-choice={isAwaitingChoice(session.id)}
      ></span></button>
    {/each}

    {#if hasMoreSessions}
      <span class="more-wrap menu-host">
        <button
          style:anchor-name="--overflow-session-anchor"
          class="more-button menu-trigger"
          class:active={overflowHasActive}
          class:awaiting-choice={overflowAwaiting}
          aria-label="Show remaining sessions"
          popovertarget="overflow-session-menu"
        >+{formatCount(moreSessions.length)}</button>
        <ul id="overflow-session-menu" style:position-anchor="--overflow-session-anchor" class="menu more-menu popover-menu" popover>
          {#each moreSessions as session (session.id)}
            <li class="more-item" class:active={session.id === activeId}>
              <button
                class="more-pick"
                onclick={() => onselect(session.id)}
                popovertarget="overflow-session-menu"
                popovertargetaction="hide"
              >
                <span
                  class="dot {sessionStatus(session.id)}"
                  class:awaiting-choice={isAwaitingChoice(session.id)}
                ></span>
                <span class="more-label">{sessionLabel(session.id) ?? session.agent.label}</span>
              </button>
              <button
                class="more-close-button"
                aria-label="Close session"
                data-tooltip="Close session"
                onclick={() => onclose(session)}
              ><Icon name="close" size={13} /></button>
            </li>
          {/each}
        </ul>
      </span>
    {/if}

    <span class="add-wrap menu-host">
      <button
        style:anchor-name="--add-session-anchor"
        class="add-button menu-trigger"
        aria-label={`New ${lastAgent?.label ?? "agent"} session — ${newTabShortcut.description}: ${newTabShortcut.label}; ${launchMenuShortcut.description}: ${launchMenuShortcut.label}`}
        data-tooltip={`New ${lastAgent?.label ?? "agent"} session · ${newTabShortcut.label} · Ctrl-click, right-click, or ${launchMenuShortcut.label} for options`}
        onclick={e => {
          if (e.ctrlKey || e.metaKey) {
            openAddMenu();
            return;
          }

          if (lastAgent) {
            onlaunch(lastAgent);
          }
        }}
        oncontextmenu={e => {
          e.preventDefault();
          openAddMenu();
        }}
      >+</button>
      <ul id="add-session-menu" style:position-anchor="--add-session-anchor" class="menu popover-menu" {@attach rovingMenu} popover>
        <li class="menu-separator">Launch an agent</li>
        {#each agents as agent (agent.id)}
          <li>
            <button
              onclick={() => onlaunch(agent)}
              popovertarget="add-session-menu"
              popovertargetaction="hide"
            ><span class="launch-icon"><Icon name={agentIconName(agent.id)} /></span>{agent.label}</button>
          </li>
        {/each}
        {#if branches.length > 0}
          <li class="menu-divider" role="separator"></li>
          <li class="menu-separator">On a branch — new worktree</li>
          {#each branches as branch (branch)}
            <li>
              <button
                class="branch-item"
                onclick={async () => await onlaunchbranch(branch)}
                popovertarget="add-session-menu"
                popovertargetaction="hide"
              ><span class="branch-icon"><Icon name="git" /></span>{branch}</button>
            </li>
          {/each}
        {/if}
      </ul>
    </span>
  </div>

  <!-- Off-layout mirror: every tab at full width, purely for measuring. Keeps
       the active/shown classes so the measured width matches the rendered pill. -->
  <span bind:this={measureElement} class="tab-measure" aria-hidden="true">
    {#each sessions as session (session.id)}
      <div class="tab" class:active={session.id === activeId} class:shown={paneIds.includes(session.id)}>
        {@render tabInner(session)}
      </div>
    {/each}
  </span>
</nav>

<style>
  .tabs {
    position: relative;
    display: flex;
    flex: 1 1 0;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    /* The visible, bounded strip — pills/dots/+N clip here rather than wrap. */
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
      font-size: 10px;
      white-space: nowrap;
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
      border-radius: 999px;
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

    .more-wrap {
      flex: none;
    }

    /* The "+N" overflow trigger. */
    .more-button {
      display: inline-flex;
      flex: none;
      align-items: center;
      block-size: 22px;
      padding-inline: 9px;
      border: none;
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      font-variant-numeric: tabular-nums;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease);

      &.active {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      /* A choice pending on a session hidden in the overflow flashes the +N. */
      &.awaiting-choice {
        color: var(--critical);
        animation: choice-flash 1100ms var(--ease) infinite;

        @media (prefers-reduced-motion: reduce) {
          box-shadow: 0 0 0 2px var(--critical-wash);
          animation: none;
        }
      }

      &:hover {
        background: var(--surface-3);
      }
    }

    .tab {
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
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

      &.active .pick {
        color: var(--on-primary-container);
        font-weight: 600;
      }

      /* The active pill's label reads with a text caret — a single click there
         renames it (Finder-style); inactive labels inherit the pill's pointer. */
      &.active .label {
        cursor: text;
      }

      /* On the active pill the close × rides the container's on-color too. */
      &.active .close-button {
        color: var(--on-primary-container);
      }
    }

    .pick {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 12px 4px;
      border: none;
      background: transparent;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 12px;
      white-space: nowrap;
      cursor: pointer;
    }

    /* The ✦ AI-name toggle — hidden until the tab is hovered or active, and
       pinned visible (primary) while auto-naming is on for the session. */
    .ai-wrap {
      display: inline-flex;
      flex: none;
      align-items: center;
      block-size: 26px;
      inline-size: 0;
      transition: inline-size 140ms var(--ease);

      .tab:hover &,
      .tab.active &,
      &.on {
        inline-size: 24px;
      }
    }

    .ai {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      overflow: hidden;
      block-size: 26px;
      inline-size: 24px;
      border: none;
      background: transparent;
      color: var(--on-surface-variant);
      opacity: 0%;
      cursor: pointer;
      transition:
        opacity 140ms var(--ease),
        color 140ms var(--ease);

      /* Revealed when the tab is hovered or active. */
      .tab:hover .ai-wrap &,
      .tab.active .ai-wrap & {
        opacity: 85%;
      }

      /* The tooltip belongs to `.ai-wrap`, outside this faded icon button, so
         the indicator can stay subdued without dimming or clipping its bubble. */
      .tab:hover .ai-wrap &:hover,
      .tab.active .ai-wrap &:hover,
      &:hover {
        color: var(--primary);
        opacity: 100%;
      }

      /* Pinned visible while auto-naming is on for this session. */
      .ai-wrap.on & {
        color: var(--primary);
        opacity: 100%;
      }
    }

    /* Inline rename field, sized like the label it replaces. */
    .rename {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 12px 4px;

      .rename-input {
        inline-size: 7.5rem;
        min-inline-size: 0;
        border: none;
        background: transparent;
        color: var(--on-surface);
        outline: none;
        font-family: var(--font-monospace);
        font-weight: 700;
        font-size: 12px;
      }
    }

    /* Leading glyph on a full tab: the agent's brand mark, coloured by how full
       its context window is (the --context-* gauge) and carrying status. Stands
       in for the plain status dot, which now marks only the collapsed tiers. */
    .agent-icon {
      display: inline-flex;
      flex: none;
      border-radius: 999px;
      color: var(--context-ok);
      transition: color 300ms var(--ease);

      &.warning {
        color: var(--context-warning);
      }

      &.critical {
        color: var(--context-critical);
      }

      /* No signal yet (a just-launched agent, or a non-agent terminal) — stay
         neutral so the blue never reads as a real "plenty of room" measurement. */
      &.unknown {
        color: var(--on-surface-variant);
      }

      /* Working breathes; ready (idle, awaiting you) keeps the dot's soft halo.
         The pulse dims the inner mark, never this host span: the tooltip bubble
         is this span's ::after, and an opacity animation on the host would drag
         the visible bubble through every breath with it. */
      &.working > :global(svg) {
        animation: pulse 1100ms var(--ease) infinite;
      }

      &.ready {
        box-shadow: 0 0 0 3px var(--tertiary-wash);
      }

      /* Waiting on a multiple-choice answer — a red ring pulses out to grab the
         eye. Reduced motion swaps the pulse for a steady red ring. */
      &.awaiting-choice {
        color: var(--critical);
        animation: choice-flash 1100ms var(--ease) infinite;

        @media (prefers-reduced-motion: reduce) {
          box-shadow: 0 0 0 2px var(--critical-wash);
          animation: none;
        }
      }
    }

    /* Per-session status dot — mirrors the SessionBadge states. Used now only by
       the collapsed overflow dots and the "+N" more-menu rows. */
    .dot {
      flex: none;
      block-size: 8px;
      inline-size: 8px;
      border-radius: 999px;
      background: var(--on-surface-variant);

      &.working {
        background: var(--primary);
        animation: pulse 1100ms var(--ease) infinite;
      }

      &.ready {
        background: var(--tertiary);
        box-shadow: 0 0 0 4px var(--tertiary-wash);
      }

      /* Same red attention pulse as the full pill's glyph, on the collapsed dot. */
      &.awaiting-choice {
        background: var(--critical);
        animation: choice-flash 1100ms var(--ease) infinite;

        @media (prefers-reduced-motion: reduce) {
          box-shadow: 0 0 0 2px var(--critical-wash);
          animation: none;
        }
      }
    }

    .close-button {
      display: inline-flex;
      justify-content: center;
      align-items: center;
      block-size: 26px;
      inline-size: 24px;
      border: none;
      border-end-start-radius: 0;
      border-end-end-radius: 999px;
      border-start-end-radius: 999px;
      border-start-start-radius: 0;
      background: transparent;
      color: var(--on-surface-variant);
      font-size: 15px;
      line-height: 1;
      opacity: 60%;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease), opacity 150ms var(--ease);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
        opacity: 100%;
      }
    }
  }

  /* Groups the add trigger with its popover as one menu-host, adding no box of
     its own so the button lays out in the strip exactly as before. */
  .add-wrap {
    display: contents;
  }

  .add-button {
    display: grid;
    flex: none;
    place-items: center;
    block-size: 30px;
    inline-size: 30px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 18px;
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  /* Shell comes from the shared .popover-menu; only width and anchor side
     live here. */
  .menu {
    min-inline-size: 220px;
    position-area: bottom span-right;

    li button {
      display: flex;
      gap: 9px;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    .menu-separator {
      margin-block: 6px 2px;
      padding-block: 2px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* Hairline between the agent list and the worktree-branch group. */
    .menu-divider {
      block-size: 1px;
      margin-block: 6px;
      margin-inline: 8px;
      background: var(--outline);
    }

    /* Leading glyph tints: agents read primary, branches read tertiary (git). */
    .launch-icon {
      display: inline-flex;
      color: var(--primary);
    }

    .branch-icon {
      display: inline-flex;
      color: var(--tertiary);
    }

    /* Branch rows spell the branch name in the mono face. */
    .branch-item {
      font-family: var(--font-monospace);
    }
  }

  /* Overflow-session popover: a compact two-column grid of the collapsed tabs. */
  .more-menu {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    overflow-y: auto;
    max-block-size: min(60vh, 420px);
    inline-size: min(360px, 80vw);
    min-inline-size: 0;
    padding: 8px;

    .more-item {
      display: flex;
      align-items: center;
      border-radius: var(--radius-small);

      /* The whole row washes neutral surface-3 on hover (canon); the inner pick
         button carries no fill of its own — so it cancels the shared menu-item
         primary hover below. */
      &:not(.active):hover {
        background: var(--surface-3);
      }

      &.active {
        background: var(--primary-container);
      }

      &.active .more-pick {
        color: var(--on-primary-container);
      }
    }

    .more-pick {
      display: flex;
      flex: 1;
      gap: 8px;
      align-items: center;
      inline-size: auto;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-size: 12px;

      &:hover {
        background: transparent;
        color: inherit;
      }
    }

    .more-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .more-close-button {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      block-size: 26px;
      inline-size: 26px;
      padding: 0;
      color: var(--on-surface-variant);
      font-size: 0.9375rem;

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }

  /* Red attention ripple for a tab whose agent is waiting on a multiple-choice
     answer (see isAwaitingChoice) — a ring pulses out from the indicator. Held
     still (a steady red ring instead) under reduced motion, at each usage. */
  @keyframes choice-flash {
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
