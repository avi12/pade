<script lang="ts">
  // Read docs/terminal-rendering.md BEFORE changing resize/replay behavior here.
  // A terminal has TWO screens and they invert every rule: the normal screen
  // (scrollback document; never send height, debounce width) vs the alternate
  // screen (fullscreen framebuffer; send both sizes, serialize refits, ask the
  // program to repaint). lib/terminal/fit/terminalFit owns that policy, and
  // lib/terminal/session/terminalSession the live session behind this view.
  import { appearance, effective } from "@/lib/settings/prefs.svelte";
  import { compactionHeadroom } from "@/lib/stores/agent/context.svelte";
  import { setSessionStatus } from "@/lib/stores/session/sessions.svelte";
  import TerminalFind from "@/lib/terminal/find/TerminalFind.svelte";
  import TerminalRtl from "@/lib/terminal/rtl/TerminalRtl.svelte";
  import { currentXtermTheme, terminalFontSize } from "@/lib/terminal/session/create-terminal";
  import { createTerminalSession } from "@/lib/terminal/session/terminalSession.svelte";
  import TerminalSessionBar from "@/lib/terminal/session/TerminalSessionBar.svelte";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag/drag-reorder";
  import { isEditingText } from "@/lib/ui/keyboard/focus";
  import type { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onDestroy, onMount, untrack } from "svelte";

  const {
    session, isActive = false, isShown = true, isRemovable = false,
    onremove, onpopout, onreorder, onexit, ondraghint
  }: {
    session: AgentSession;
    /** The session the keyboard belongs to - the one tab (or split pane) in front. */
    isActive?: boolean;
    /** This pane is in the current split (laid out and visible). A background
        tab's pane stays mounted at full size but `visibility: hidden`, so only
        `shown` - never geometry - can tell the two apart for cursor blinking. */
    isShown?: boolean;
    /** Show a trailing remove-from-split button in the session bar. */
    isRemovable?: boolean;
    /** Remove this pane from the split - the trailing × button. The other pane(s)
        stay shown; the removed session lives on as a background tab. */
    onremove?: () => void;
    /** Pop this pane out of the split into its own tab - its header was dragged up
        onto the tab strip. Collapses the split to this session (shown fullscreen,
        active); the mirror of dragging a tab down onto the panes to split it. */
    onpopout?: () => void;
    /** A drag of this pane's header reordered the split - commit the new order. */
    onreorder?: (orderedIds: string[]) => void;
    /** Live pane-drag state, so App can light the tab strip's "drop → new tab"
        zone while this header is dragged over it (`hint.isOutside`). */
    ondraghint?: (hint: DragHint | null) => void;
    /** The PTY exited on its own (the agent quit, e.g. via Ctrl-C) - so App can
        auto-close this tab (and respawn the agent if it was the last one). */
    onexit?: (id: string) => void;
  } = $props();

  let elHost: HTMLDivElement;
  let elViewport: HTMLDivElement;
  // The terminal once it is built and attached - reactive, so the effects below
  // (and the right-to-left overlay) can wait for it, since it is created inside
  // onMount long after the first effects have run. Its presence IS "attached":
  // one value rather than a flag kept beside it.
  let attachedTerminal = $state<Terminal | undefined>();
  // The pane's find bar (lib/terminal/find/TerminalFind), bound so Ctrl+F can ask it to open.
  // Null until the component has mounted itself.
  let findBar = $state<{ reveal: () => Promise<void> } | null>(null);
  let isWindowFocused = $state(document.hasFocus());
  /** This pane holds the keyboard, so its cursor is the live one. */
  const isKeyboardHere = $derived(isActive && isWindowFocused);

  const pane = createTerminalSession({
    session: () => session,
    view: () => ({
      isShown,
      isWindowFocused
    }),
    openFind: () => findBar?.reveal(),
    onexit: id => onexit?.(id)
  });

  // Publish status to the shared store so the top-bar tab shows a matching dot.
  $effect(() => {
    setSessionStatus({
      id: session.id,
      status: pane.activity.status
    });
  });

  // Re-read the context fill off the session log (see lib/terminal/session/session-readings).
  // A session settling back to `ready` is the event to hang this off - no timer
  // needed: the agent has just finished a turn, which is both when the log has
  // grown and when the flows that gate on the fill (auto-handoff, usage-resume)
  // next look at it. Untracked so the effect follows the status alone: the
  // context store it writes is rewritten on every chunk of output, and
  // subscribing to that would re-run this per chunk.
  $effect(() => {
    if (pane.activity.status !== SessionStatus.enum.ready) {
      return;
    }

    untrack(pane.readings.readContextFromSessionLog);
    untrack(pane.readings.readRunningModel);
  });

  // A turn can run for an hour, so turn ends alone leave the reading stale right
  // when it matters most. The agent's countdown to compaction ticking is the
  // mid-turn event: it only shows near the end, and each tick means the log grew.
  $effect(() => {
    if (compactionHeadroom(session.id) === null) {
      return;
    }

    untrack(pane.readings.readContextFromSessionLog);
  });

  // Live-update the terminal font (family + zoom) when the preference changes.
  $effect(() => {
    const family = effective.monospaceFamily;
    const fontSize = terminalFontSize();
    const terminal = pane.terminal;
    if (!terminal) {
      return;
    }

    terminal.options.fontFamily = family;
    terminal.options.fontSize = fontSize;
    pane.fit.fitToPane();
  });

  // Everything that decides the terminal's colours, as one value: the app scheme
  // (the tokens theme.css installs) and the chosen Windows Terminal scheme (the
  // tokens lib/settings/prefs installs over them). Reading it is what subscribes the
  // effect below, so a light/dark flip and a change of scheme both re-theme.
  const paintedPalette = $derived(`${appearance.scheme}/${appearance.terminalPalette ?? ""}`);

  // Re-theme xterm whenever that changes. The palette changes in place, so it is
  // safe for every live session. An agent that subscribed to DECSET 2031
  // color-scheme reports (Claude's `auto` theme, opencode's `system` theme) also
  // gets the standard report relayed through its PTY after repainting: xterm
  // does not emit one merely because `options.theme` changed. It reaches the
  // already-running process and redraws its own TUI without a restart or
  // context loss.
  $effect(() => {
    // Read the palette BEFORE the guard: `pane.terminal` is not reactive, so a
    // guard that short-circuits on it would run once at mount (when there is no
    // terminal yet), never reach this read, and leave the effect subscribed to
    // nothing at all - a palette change would then repaint no terminal, ever.
    const painted = paintedPalette;
    const terminal = pane.terminal;
    if (!terminal || !painted) {
      return;
    }

    terminal.options.theme = currentXtermTheme();
    pane.colorScheme.relayChange();
  });

  // Hand the keyboard to the session in front - the moment it launches, and again
  // whenever the user switches to it. Nothing else claims focus for a terminal, so
  // without this the keystrokes go to whatever the user last clicked (the tab, the
  // agent button in onboarding) and the agent looks like it is ignoring you: you
  // have to click into the pane before it will hear a single key.
  $effect(() => {
    if (isActive && attachedTerminal) {
      attachedTerminal.focus();
    }
  });

  // A hidden or unfocused terminal keeps consuming every PTY chunk, but it does
  // not need a cursor-blink paint loop or foreground output cadence until the
  // user can see it again.
  $effect(() => {
    if (!attachedTerminal) {
      return;
    }

    attachedTerminal.options.cursorBlink = isShown && isWindowFocused;
    pane.output.schedule();
  });

  onMount(async () => {
    const grid = pane.open({
      elHost,
      elPane: elViewport
    });
    attachedTerminal = grid;
    await pane.start({
      grid,
      elPane: elViewport
    });
  });

  onDestroy(() => pane.dispose());
</script>

<!-- Returning to this window - the Ctrl+Alt+[ / ] window cycle, an alt-tab, a
     taskbar click - puts the OS focus on the window but not back on the terminal,
     so the agent would ignore your first keystrokes until you click the pane.
     Re-grab the keyboard for the front session on every window focus, so a cycled
     window is immediately promptable. Skipped while the user is mid-typing in a
     real field (a rename box, the picker) - that focus must be left alone; the
     xterm helper textarea deliberately doesn't count as editing (see lib/ui/keyboard/focus). -->
<svelte:window
  onblur={() => {
    isWindowFocused = false;
  }}
  onfocus={() => {
    isWindowFocused = true;
    pane.output.schedule();

    if (isActive && attachedTerminal && !isEditingText(document.activeElement)) {
      attachedTerminal.focus();
    }
  }}
/>

<div class="terminal-wrapper">
  <TerminalSessionBar
    {isRemovable}
    {ondraghint}
    {onpopout}
    {onremove}
    {onreorder}
    {session}
    status={pane.activity.status}
  />
  <div class="terminal-padding">
    <div
      bind:this={elViewport}
      class="terminal-viewport"
      class:anchor-bottom={pane.fit.isBottomAnchored}
      class:file-drag-over={pane.isFileDragOver}
    >
      <div bind:this={elHost} style:scale={`1 ${pane.fit.squeeze}`} class="terminal-host">
        <TerminalRtl
          isEnabled={pane.isRightToLeftSeen}
          isFocused={isKeyboardHere}
          terminal={attachedTerminal}
        />
      </div>
    </div>
    <TerminalFind
      bind:this={findBar}
      onclose={() => attachedTerminal?.focus()}
      palette={paintedPalette}
      terminal={attachedTerminal}
    />
  </div>
</div>

<style>
  .terminal-wrapper {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  /* Visual insets live on this pad, off the measured viewport, so they never
     count toward the fit - it lifts the output off every pane edge (canon:
     12px top, 8px right, 8px bottom, 14px left). */
  .terminal-padding {
    position: relative; /* the containing block for the find bar */
    flex: 1;
    min-block-size: 0;
    padding-block: 12px 8px;
    padding-inline: 14px 8px;
    background: var(--code-background);
  }

  /* Full-size measuring frame: fitToPane reads its client size for the cols/rows and
     pins the grid to the end of the content the terminal is actually showing (see
     `isBottomAnchored`) - a fullscreen agent's frame and an unscrolled conversation pin the
     top, a scrolled one pins the bottom. The grid is whole cells and never quite fills
     the frame, so the leftover sits as a sliver of background at the unpinned edge. */
  .terminal-viewport {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    overflow: hidden;
    block-size: 100%;
    inline-size: 100%;

    &.anchor-bottom {
      justify-content: flex-end;
    }

    /* The pane a dragged file will land in, so a split shows which agent gets it.
       An overlay rather than an outline: xterm's positioned host would paint
       over an outline drawn on the viewport itself. */
    &.file-drag-over {
      position: relative;

      &::after {
        content: "";
        position: absolute;
        inset: 0;
        z-index: 1;
        border: 2px dashed var(--primary);
        background: color-mix(in sRGB, var(--primary) 8%, transparent);
        pointer-events: none;
      }
    }

    /* xterm mounts here at its natural whole-cell size, scaled only while a grid that is
       momentarily too tall for the pane is being squeezed to fit (see `squeeze`) - from
       the top, so the squeeze pulls the overflowing bottom up into view rather than
       moving the text you are reading. At rest the scale is exactly 1: text stays crisp
       and clicks map at native cell size. */
    .terminal-host {
      position: relative; /* the containing block for the right-to-left overlay */
      flex: none;
      transform-origin: top left;
    }
  }

  /* A row of xterm's DOM renderer is a CELL GRID, not a paragraph: column N holds
     the Nth cell whatever was printed into it. Left alone the browser runs the
     bidi algorithm over the row's text and moves those cells, which is wrong
     twice over. An agent that ordered its own right-to-left text (agents.rs
     `reorders_bidi`) has it undone, and the reader gets the sentence backwards -
     measured: the buffer's `ךותמ` was painted `מתוך`. And for every other agent it
     competes with the overlay, which anchors each run to the columns xterm was
     supposed to have painted. Overriding it makes the grid honest, and leaves
     exactly one place where right-to-left text is ever reordered: the overlay, or
     the agent itself.

     MDN tells authors not to set `unicode-bidi` themselves, and for documents it
     is right: in prose, directionality is the content's own semantics and belongs
     in `dir`/`<bdo>` markup, which the browser then implements in CSS. This is not
     prose - and the markup that says what is meant here IS `<bdo>`: "the characters
     inside are already in the order I want them drawn". The HTML spec's rendering
     rules define that element as `bdo, bdo[dir] { unicode-bidi: isolate-override }`,
     which is the declaration below. xterm generates these rows itself, so wrapping
     every span in a real `<bdo>` would mean patching its row factory - on the
     hottest path in the app - to arrive at the same computed style. (The same
     rendering rules are why a row is an isolate already: `div { unicode-bidi:
     isolate }`. And `.xterm-rows` carries `aria-hidden`, with xterm keeping its own
     accessibility buffer in logical order, so no semantics are being papered over.)

     `isolate-override` rather than `plaintext`, which only changes how the base
     direction is *detected* and still reorders, or `isolate`, which only stops the
     element influencing its siblings. `direction` is named rather than inherited
     because the override's whole meaning is "in sequence according to `direction`"
     - the guarantee has to be local to be a guarantee.

     Both the row and its spans need it. A block container's override reaches its
     inline-level descendants but NOT those inside another block container, and
     xterm gives every styled span `display: inline-block` - so each one is its own
     bidi paragraph that the row's override cannot reach into. Measured: with the
     row alone overridden, `ךותמ` still painted as `מתוך`. */
  :global(.xterm-rows > div),
  :global(.xterm-rows span) {
    unicode-bidi: isolate-override;
    direction: ltr;
  }
</style>
