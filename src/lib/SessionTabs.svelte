<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { sessionLabel, setSessionLabel } from "@/lib/stores/sessionLabels.svelte";
  import { isNaming, toggleNaming } from "@/lib/stores/sessionNaming.svelte";
  import { sessionStatus } from "@/lib/stores/sessions.svelte";
  import { ADD_SLOT, packTabs } from "@/lib/tabFit";
  import type { Agent, AgentSession } from "@/lib/types";
  import { parseInput, SessionName } from "@/lib/validate";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import type { TransitionConfig } from "svelte/transition";

  // The session tab strip: full pills for the sessions that fit, status dots
  // for the next few, a "+N" popover for the rest (packing in lib/tabFit), the
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
    onlaunchbranch
  }: {
    sessions: AgentSession[];
    activeId: string | null;
    /** Sessions currently shown side by side — their pills read as "shown". */
    paneIds: string[];
    agents: Agent[];
    /** Local branches when the project is a git repo — offered as worktrees. */
    branches: string[];
    onselect: (id: string) => void;
    onclose: (session: AgentSession) => void;
    onlaunch: (agent: Agent) => void;
    onlaunchbranch: (branch: string) => Promise<void>;
  } = $props();

  // ── Measurement ─────────────────────────────────────────────────────────────
  // The strip is bounded to the width the nav gives it. Pill widths come from
  // an off-layout mirror row (re-measured on session change / reflow) so
  // collapsing a tab never changes the numbers we packed against.
  let stripEl = $state<HTMLElement>();
  let measureEl = $state<HTMLElement>();
  let stripWidth = $state(0);
  const tabWidths = new SvelteMap<string, number>();

  // Read each mirror pill's natural width into a fresh map (index-aligned with
  // `sessions`, since the mirror renders them in order).
  function measureTabs() {
    const mirror = measureEl;
    if (!mirror) {
      return;
    }

    tabWidths.clear();
    sessions.forEach((session, index) => {
      const element = mirror.children[index];
      if (element instanceof HTMLElement) {
        tabWidths.set(session.id, element.offsetWidth);
      }
    });
  }

  // Sync the strip's available width, then re-measure the pills.
  function remeasureTabStrip() {
    const strip = stripEl;
    if (strip) {
      stripWidth = strip.clientWidth;
    }

    measureTabs();
  }

  // Re-measure after the mirror re-renders for a changed session set.
  $effect(() => {
    void sessions.length;
    measureTabs();
  });

  // Track the strip's available width and re-measure on any reflow (font load,
  // window resize); both the strip and the mirror are observed.
  $effect(() => {
    const strip = stripEl;
    if (!strip) {
      return;
    }

    const observer = new ResizeObserver(remeasureTabStrip);
    observer.observe(strip);

    if (measureEl) {
      observer.observe(measureEl);
    }

    remeasureTabStrip();
    return () => observer.disconnect();
  });

  // Greedy three-tier packing: full pills → status dots → "+N" overflow.
  const tabPack = $derived(
    packTabs({
      ids: sessions.map(s => s.id),
      widthOf: id => tabWidths.get(id) ?? 0,
      // Reserve the trailing add button's slot so tabs never sit under it.
      stripWidth: Math.max(0, stripWidth - ADD_SLOT)
    })
  );

  const bySessionId = $derived(new Map(sessions.map(s => [s.id, s] as const)));
  function tabsFor(ids: string[]): AgentSession[] {
    return ids
      .map(id => bySessionId.get(id))
      .filter((s): s is AgentSession => s !== undefined);
  }
  const visibleSessions = $derived(tabsFor(tabPack.visible));
  const dotSessions = $derived(tabsFor(tabPack.dots));
  const moreSessions = $derived(tabsFor(tabPack.more));
  const hasMoreSessions = $derived(moreSessions.length > 0);
  const overflowHasActive = $derived(
    activeId !== null && (tabPack.dots.includes(activeId) || tabPack.more.includes(activeId))
  );

  // Survivors slide to their new spots when a tab is added, closed, or the strip
  // repacks (Svelte's built-in FLIP). Disabled under reduced-motion so it snaps.
  const prefersReducedMotion =
    typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const flipParams = { duration: prefersReducedMotion ? 0 : 280 };

  // The last tab's agent — a plain "+" click launches another of the same kind;
  // Ctrl/Cmd-click opens the full launch menu instead.
  const lastAgent = $derived(sessions.at(-1)?.agent ?? agents[0]);

  function onAddClick(event: MouseEvent) {
    if (event.ctrlKey || event.metaKey) {
      document.getElementById("add-menu")?.togglePopover();
      return;
    }

    if (lastAgent) {
      onlaunch(lastAgent);
    }
  }

  // Closing a tab removes the session synchronously; the pill's collapse is a
  // Svelte out-transition. `closingIds` marks which pills left via a real close
  // so the transition only animates those — a repack-driven exit snaps instantly.
  const closingIds = new SvelteSet<string>();
  // Middle-click anywhere on a pill closes it (preventDefault stops the browser's
  // middle-click autoscroll). onmousedown suppresses the same on press.
  function onTabPointer(event: MouseEvent, session: AgentSession) {
    if (event.button === 1) {
      event.preventDefault();

      if (event.type === "auxclick") {
        closeTab(session);
      }
    }
  }

  function closeTab(session: AgentSession) {
    closingIds.add(session.id);
    onclose(session);
    // Prune the marker after the outro; purely housekeeping, not a close delay.
    setTimeout(() => {
      closingIds.delete(session.id);
    }, 260);
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

  function startRename(session: AgentSession) {
    editingId = session.id;
    renameDraft = sessionLabel(session.id) ?? session.agent.label;
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

  function onRenameKey(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitRename();
    } else if (event.key === "Escape") {
      event.preventDefault();
      editingId = null;
    }
  }

  // Focus + select the rename field the moment it mounts.
  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

{#snippet tabInner(s: AgentSession)}
  {#if editingId === s.id}
    <span class="rename">
      <span class="dot {sessionStatus(s.id)}"></span>
      <input
        class="rename-input"
        aria-label="Rename session"
        onblur={commitRename}
        oninput={event => (renameDraft = event.currentTarget.value)}
        onkeydown={onRenameKey}
        value={renameDraft}
        use:focusOnMount
      />
    </span>
  {:else}
    <button
      class="pick"
      onauxclick={event => onTabPointer(event, s)}
      onclick={() => onselect(s.id)}
      ondblclick={() => startRename(s)}
      onmousedown={event => onTabPointer(event, s)}
    >
      <span class="dot {sessionStatus(s.id)}"></span>
      {sessionLabel(s.id) ?? s.agent.label}
    </button>
    <button
      class="ai"
      class:on={isNaming(s.id)}
      aria-label="Auto-name this session with AI"
      data-tooltip="AI name"
      onclick={() => toggleNaming({
        id: s.id,
        agent: s.agent.command
      })}
    ><Icon name="sparkles" size={13} /></button>
  {/if}
  <button
    class="x"
    aria-label="Close session"
    data-tooltip="Close session"
    onclick={() => closeTab(s)}
  ><Icon name="close" size={13} /></button>
{/snippet}

<nav class="tabs" aria-label="Agent sessions">
  <div bind:this={stripEl} class="tab-strip">
    {#each visibleSessions as s (s.id)}
      <div
        class="tab"
        class:active={s.id === activeId}
        class:shown={paneIds.includes(s.id)}
        out:collapse={{ id: s.id }}
        animate:flip={flipParams}
      >
        {@render tabInner(s)}
      </div>
    {/each}

    {#each dotSessions as s (s.id)}
      <button
        class="tab-dot"
        class:active={s.id === activeId}
        aria-label={s.agent.label}
        data-tooltip={s.agent.label}
        onclick={() => onselect(s.id)}
      ><span class="dot {sessionStatus(s.id)}"></span></button>
    {/each}

    {#if hasMoreSessions}
      <span class="more-wrap">
        <button
          style:anchor-name="--more-anchor"
          class="more-btn"
          class:active={overflowHasActive}
          aria-label="Show remaining sessions"
          popovertarget="more-menu"
        >+{moreSessions.length}</button>
        <ul id="more-menu" style:position-anchor="--more-anchor" class="menu more-menu" popover>
          {#each moreSessions as s (s.id)}
            <li class="more-item" class:active={s.id === activeId}>
              <button
                class="more-pick"
                onclick={() => onselect(s.id)}
                popovertarget="more-menu"
                popovertargetaction="hide"
              >
                <span class="dot {sessionStatus(s.id)}"></span>
                <span class="more-label">{sessionLabel(s.id) ?? s.agent.label}</span>
              </button>
              <button
                class="more-x"
                aria-label="Close session"
                data-tooltip="Close session"
                onclick={() => onclose(s)}
              ><Icon name="close" size={13} /></button>
            </li>
          {/each}
        </ul>
      </span>
    {/if}

    <button
      style:anchor-name="--add-anchor"
      class="add-btn"
      aria-label={`New ${lastAgent?.label ?? "agent"} session — Ctrl-click for launch options`}
      data-tooltip={`New ${lastAgent?.label ?? "agent"} session · Ctrl-click for options`}
      onclick={onAddClick}
    >+</button>
    <ul id="add-menu" style:position-anchor="--add-anchor" class="menu" popover>
      <li class="menu-sep">Launch an agent</li>
      {#each agents as a (a.id)}
        <li>
          <button onclick={() => onlaunch(a)} popovertarget="add-menu" popovertargetaction="hide">
            {a.label}
          </button>
        </li>
      {/each}
      {#if branches.length > 0}
        <li class="menu-sep">On a branch — new worktree</li>
        {#each branches as b (b)}
          <li>
            <button
              onclick={async () => await onlaunchbranch(b)}
              popovertarget="add-menu"
              popovertargetaction="hide"
            ><Icon name="git" /> {b}</button>
          </li>
        {/each}
      {/if}
    </ul>
  </div>

  <!-- Off-layout mirror: every tab at full width, purely for measuring. Keeps
       the active/shown classes so the measured width matches the rendered pill. -->
  <span bind:this={measureEl} class="tab-measure" aria-hidden="true">
    {#each sessions as s (s.id)}
      <div class="tab" class:active={s.id === activeId} class:shown={paneIds.includes(s.id)}>
        {@render tabInner(s)}
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
      overflow: hidden;
      min-inline-size: 0;
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
    .more-btn {
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

      &:hover {
        background: var(--surface-3);
      }
    }

    .tab {
      display: inline-flex;
      align-items: center;
      overflow: hidden;
      border-radius: 999px;
      background: var(--surface-2);
      animation: spring-in 320ms var(--ease);

      &.active {
        background: var(--primary-container);
      }

      &.active .pick {
        color: var(--on-primary-container);
        font-weight: 600;
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
    .ai {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      overflow: hidden;
      block-size: 26px;
      inline-size: 0;
      border: none;
      background: transparent;
      color: var(--on-surface-variant);
      opacity: 0%;
      cursor: pointer;
      transition:
        inline-size 140ms var(--ease),
        opacity 140ms var(--ease),
        color 140ms var(--ease);

      /* Revealed when the tab is hovered or active. */
      .tab:hover &,
      .tab.active & {
        inline-size: 22px;
        opacity: 85%;
      }

      &:hover {
        color: var(--primary);
        opacity: 100%;
      }

      /* Pinned visible while auto-naming is on for this session. */
      &.on {
        inline-size: 22px;
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

    /* Per-session status dot — mirrors the SessionBadge states. */
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
    }

    .x {
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

  .add-btn {
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

  /* Native popover (light-dismiss on outside click) anchored to its button.
     Scoped copy of the shared menu chrome — same as AppMenu carries its own. */
  .menu {
    position: absolute;
    inset: auto;
    min-inline-size: 220px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 16px 40px var(--shadow-color);
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

    .menu-sep {
      margin-block: 6px 2px;
      padding-block: 2px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }
  }

  /* Overflow-session popover: a compact two-column grid of the collapsed tabs. */
  .more-menu {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    inline-size: min(360px, 80vw);
    min-inline-size: 0;

    .more-item {
      display: flex;
      align-items: center;
      border-radius: var(--radius-small);

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
    }

    .more-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .more-x {
      flex: none;
      justify-content: center;
      inline-size: 26px;
      padding: 0;
      color: var(--on-surface-variant);
      font-size: 15px;

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }
</style>
