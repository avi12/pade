<script lang="ts">
  // Read docs/terminal-rendering.md BEFORE changing resize/replay behavior here.
  // A terminal has TWO screens and they invert every rule: the normal screen
  // (scrollback document; never send height, debounce width) vs the alternate
  // screen (fullscreen framebuffer; send both sizes, serialize refits, ask the
  // program to repaint). lib/terminal/terminalFit owns that policy.
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { appearance, effective } from "@/lib/settings/prefs.svelte";
  import { compactionHeadroom, dropContext, observeContextScreen } from "@/lib/stores/agent/context.svelte";
  import { dropMcpReload } from "@/lib/stores/agent/mcpReload.svelte";
  import { setSessionStatus } from "@/lib/stores/session/sessions.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { observeUsageLimitScreen } from "@/lib/stores/usage/usageResume.svelte";
  import { observeAgentOutput, observeReplayedOutput } from "@/lib/terminal/agent-output";
  import { createTerminal, currentXtermTheme, terminalFontSize } from "@/lib/terminal/create-terminal";
  import { listenForFileDrops } from "@/lib/terminal/file-drop";
  import { createInitialPromptDelivery } from "@/lib/terminal/initial-prompt-delivery";
  import { createPtyStream } from "@/lib/terminal/pty-stream";
  import { createPtyWriter } from "@/lib/terminal/pty-writer";
  import { createScreenSampler, viewportRows } from "@/lib/terminal/screen-sampler";
  import { createSessionReadings } from "@/lib/terminal/session-readings";
  import { createSessionActivity } from "@/lib/terminal/sessionActivity.svelte";
  import { colorSchemeReport, createColorSchemeRelay } from "@/lib/terminal/terminal-color-scheme";
  import { pathsAsPromptText } from "@/lib/terminal/terminal-input";
  import { attachKeyOverrides, attachWheelForwarding } from "@/lib/terminal/terminal-keys";
  import { createTerminalOutputScheduler } from "@/lib/terminal/terminal-output-scheduler";
  import { containsRtl } from "@/lib/terminal/terminal-rtl";
  import { ENTER_ALTERNATE_SCREEN, isFocusReport } from "@/lib/terminal/terminal-sequences";
  import TerminalFind from "@/lib/terminal/TerminalFind.svelte";
  import { createTerminalFit, Screen } from "@/lib/terminal/terminalFit.svelte";
  import TerminalRtl from "@/lib/terminal/TerminalRtl.svelte";
  import TerminalSessionBar from "@/lib/terminal/TerminalSessionBar.svelte";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession, PtyChunk } from "@/lib/types";
  import type { DragHint } from "@/lib/ui/drag-reorder";
  import { isEditingText } from "@/lib/ui/focus";
  import { errorMessage } from "@/lib/util/errors";
  import type { UnlistenFn } from "@tauri-apps/api/event";
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

  const CONTEXT_SCREEN_SAMPLE_MS = 1000;

  let elHost: HTMLDivElement;
  let elViewport: HTMLDivElement;
  let terminal: Terminal;
  const listeners: UnlistenFn[] = [];
  // A file dragged from Explorer or an IDE is hovering over this pane.
  let isFileDragOver = $state(false);
  // Guards the async onMount against a teardown that runs before its awaits
  // settle: onDestroy sets this, and each awaited step bails so no listener is
  // registered after unmount and no write hits a disposed terminal.
  let isDestroyed = false;
  // The terminal once it is built and attached - reactive, so the effects below
  // (and the right-to-left overlay) can wait for it, since it is created inside
  // an async onMount long after the first effects have run. Its presence IS
  // "attached": one value rather than a flag kept beside it.
  let attachedTerminal = $state<Terminal | undefined>();
  // The pane's find bar (lib/terminal/TerminalFind), bound so Ctrl+F can ask it to open.
  // Null until the component has mounted itself.
  let findBar = $state<{ reveal: () => Promise<void> } | null>(null);
  let isWindowFocused = $state(document.hasFocus());
  // This session has printed right-to-left text at least once, so its rows are
  // worth scanning (see lib/terminal/TerminalRtl). A latch, never unset: the text stays in
  // the scrollback. Until it flips, an English-only session does no scanning and
  // renders no overlay at all.
  let isRightToLeftSeen = $state(false);
  /** This pane holds the keyboard, so its cursor is the live one. */
  const isKeyboardHere = $derived(isActive && isWindowFocused);

  const activity = createSessionActivity({ onSettled: () => delivery.deliverIfReady() });
  const writeToPty = createPtyWriter({
    send: data => sendMessage(Command.ptyWrite, {
      id: session.id,
      data
    }),
    onTransportLost: () => activity.markExited()
  });
  // A new-project first prompt is NOT sent on mount: a fresh agent gates on a
  // "trust this folder?" prompt before its input is live, so it's delivered once
  // the agent has accepted that gate and settled at its REPL.
  const delivery = createInitialPromptDelivery({
    prompt: () => session.initialPrompt,
    write: writeToPty,
    isWorking: () => activity.status === SessionStatus.enum.working
  });
  const colorScheme = createColorSchemeRelay({
    agentId: () => session.agent.id,
    report: () => writeToPty(colorSchemeReport(appearance.terminalScheme))
  });
  const readings = createSessionReadings(() => session);
  const fit = createTerminalFit({
    terminal: () => terminal,
    viewport: () => elViewport,
    // Fire-and-forget resize: the session may have exited between measuring and
    // resizing, and a dropped resize on a dead PTY is harmless.
    resizeAgent: async ({ columns, rows }) => {
      try {
        await sendMessage(Command.ptyResize, {
          id: session.id,
          cols: columns,
          rows
        });
      } catch {
        // A failed IPC write means this WebView can no longer reach the session;
        // reflect that rather than pretending the grid is still live.
        activity.markExited();
      }
    },
    noteResize: () => activity.noteResize()
  });
  const screenSampler = createScreenSampler({
    intervalMs: CONTEXT_SCREEN_SAMPLE_MS,
    sample: () => {
      if (!terminal) {
        return;
      }

      const rows = viewportRows(terminal);
      delivery.answerTrustGate(rows);
      const screen = rows.join("\n");
      observeContextScreen({
        id: session.id,
        text: screen
      });
      observeUsageLimitScreen({
        id: session.id,
        text: screen
      });
    }
  });
  const output = createTerminalOutputScheduler({
    write: queued => {
      terminal.write(queued, () => screenSampler.request());
      fit.noteRepaintProgress();
    },
    flushContext: () => {
      const buffer = terminal?.buffer.active;
      const isReadingScrollback = buffer !== undefined && buffer.type !== Screen.Alternate
        && buffer.viewportY < buffer.baseY;
      return {
        isShown,
        isWindowFocused,
        isReadingScrollback
      };
    }
  });

  function watchForRightToLeft(text: string) {
    // An agent that reorders its own output has already done the browser's job,
    // and doing it twice reads backwards - so for those the latch never flips and
    // the overlay never exists (see `reordersBidi` in agents.rs).
    if (isRightToLeftSeen || session.agent.reordersBidi) {
      return;
    }

    if (containsRtl(text)) {
      isRightToLeftSeen = true;
    }
  }

  // Keep a listener that registered; one that lands after teardown is dropped at once.
  function keepListening(unlisteners: UnlistenFn[]) {
    if (isDestroyed) {
      for (const stopListening of unlisteners) {
        stopListening();
      }
      return false;
    }

    listeners.push(...unlisteners);
    return true;
  }

  // Publish status to the shared store so the top-bar tab shows a matching dot.
  $effect(() => {
    setSessionStatus({
      id: session.id,
      status: activity.status
    });
  });

  // Re-read the context fill off the session log (see lib/terminal/session-readings).
  // A session settling back to `ready` is the event to hang this off - no timer
  // needed: the agent has just finished a turn, which is both when the log has
  // grown and when the flows that gate on the fill (auto-handoff, usage-resume)
  // next look at it. Untracked so the effect follows the status alone: the
  // context store it writes is rewritten on every chunk of output, and
  // subscribing to that would re-run this per chunk.
  $effect(() => {
    if (activity.status !== SessionStatus.enum.ready) {
      return;
    }

    untrack(readings.readContextFromSessionLog);
    untrack(readings.readRunningModel);
  });

  // A turn can run for an hour, so turn ends alone leave the reading stale right
  // when it matters most. The agent's countdown to compaction ticking is the
  // mid-turn event: it only shows near the end, and each tick means the log grew.
  $effect(() => {
    if (compactionHeadroom(session.id) === null) {
      return;
    }

    untrack(readings.readContextFromSessionLog);
  });

  // Live-update the terminal font (family + zoom) when the preference changes.
  $effect(() => {
    const family = effective.monospaceFamily;
    const fontSize = terminalFontSize();
    if (!terminal) {
      return;
    }

    terminal.options.fontFamily = family;
    terminal.options.fontSize = fontSize;
    fit.fitToPane();
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
    // Read the palette BEFORE the guard: `terminal` is not reactive, so a guard
    // that short-circuits on it would run once at mount (when there is no
    // terminal yet), never reach this read, and leave the effect subscribed to
    // nothing at all - a palette change would then repaint no terminal, ever.
    const painted = paintedPalette;
    if (!terminal || !painted) {
      return;
    }

    terminal.options.theme = currentXtermTheme();
    colorScheme.relayChange();
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
    output.schedule();
  });

  // Stream this session's PTY output into the terminal; each chunk is a sign of
  // life that resets the idle → ready timer. Parked until the history replay is
  // in (see lib/terminal/pty-stream).
  function consume(chunk: PtyChunk) {
    // The terminal may already be disposed if a late chunk arrives during
    // teardown; skip the write rather than throw.
    if (isDestroyed || !terminal) {
      return;
    }

    output.queue(chunk.data);
    colorScheme.observe(chunk.data);
    watchForRightToLeft(chunk.data);
    activity.markActivity();
    observeAgentOutput({
      id: session.id,
      chunk: chunk.data,
      isOnNormalScreen: terminal.buffer.active.type !== Screen.Alternate
    });
    // Accept the first-run trust gate so the pending first prompt can land.
    delivery.watchOutput(chunk.data);
  }

  const stream = createPtyStream(consume);

  // Subscribe to this session's events. Events are filtered by id so sibling
  // sessions don't cross-write. False when the pane was torn down meanwhile.
  async function listenToSession() {
    const isDataListening = keepListening([
      await onMessage(EventType.ptyData, ({ data: chunk }) => {
        if (chunk.id === session.id) {
          stream.accept(chunk);
        }
      })
    ]);
    if (!isDataListening) {
      return false;
    }

    const isExitListening = keepListening([
      await onMessage(EventType.ptyExit, ({ data: { id } }) => {
        if (id !== session.id) {
          return;
        }

        delivery.cancel();
        activity.markExited();
        onexit?.(session.id);
      })
    ]);
    if (!isExitListening) {
      return false;
    }

    // Read the running model once now for the tag's first frame, then on every change.
    if (keepListening([await readings.listenForModelChanges()])) {
      await readings.readRunningModel();
    }

    keepListening(
      await listenForFileDrops({
        element: () => elViewport,
        onHoverChange: isOver => {
          isFileDragOver = isOver;
        },
        onDrop: paths => {
          terminal.paste(pathsAsPromptText(paths));
          terminal.focus();
        }
      })
    );
    return !isDestroyed;
  }

  // Wire the terminal's own events: keystrokes out to the PTY, the key and wheel
  // overrides, and every change of scroll, screen and size.
  function attachTerminalEvents() {
    terminal.onData(data => {
      if (isFocusReport(data)) {
        return;
      }

      activity.noteInput(data);
      writeToPty(data);
    });
    attachKeyOverrides({
      terminal,
      agentId: () => session.agent.id,
      write: writeToPty,
      openFind: () => findBar?.reveal()
    });
    attachWheelForwarding({
      terminal,
      write: writeToPty,
      onDocumentScroll: () => output.noteScroll()
    });

    // The document can outgrow the pane with no resize involved - the agent simply
    // prints past the last row - and that is the moment the grid must re-pin.
    terminal.onScroll(() => {
      fit.updateAnchor();
      output.schedule();
    });
    terminal.buffer.onBufferChange(() => {
      if (fit.syncScreen(terminal)) {
        colorScheme.reportToSubscribedAgent();
      }
    });
    terminal.onResize(({ cols: columns, rows }) => fit.handleGridResize({
      columns,
      rows
    }));
    fit.observePane(elViewport);
  }

  // Spawn the chosen agent in a real PTY. False when it failed to start, or the
  // pane was torn down meanwhile.
  async function spawnAgent() {
    fit.noteAgentColumns(terminal.cols);
    // A spawn that fails used to abandon the rest of this mount silently, and the
    // pane sat on "Starting…" for as long as the app ran - no process, no error,
    // no way to tell a wedged session from a slow one. Say what happened and mark
    // the session not-running, so the pane stops lying and a graceful leave never
    // waits on it.
    try {
      await sendMessage(Command.ptySpawn, {
        id: session.id,
        command: session.agent.command,
        cwd: session.cwd ?? null,
        cols: terminal.cols,
        rows: terminal.rows,
        args: session.args,
        scheme: appearance.terminalScheme,
        conversationId: session.conversationId,
        modelChoice: session.modelChoice
      });
    } catch (error) {
      activity.markExited();
      showToast(
        `Couldn't start ${session.agent.label} - ${errorMessage({
          error,
          fallback: "the terminal session failed to start"
        })}`
      );
      return false;
    }

    return !isDestroyed;
  }

  // Paint whatever the session has already said. A spawn for a session that is
  // still running is a no-op, so this terminal may be attaching to a conversation
  // already in flight - a remounted component, a reloaded window - and a PTY keeps
  // no scrollback of its own: with nothing replayed, the pane just sits blank while
  // the agent waits, quite happily, for input. For a fresh spawn the history is
  // empty and this costs nothing. Live chunks that arrived meanwhile were parked
  // and go in after it (see lib/terminal/pty-stream).
  async function replayHistory() {
    const history = await sendMessage(Command.ptyHistory, { id: session.id });
    if (isDestroyed) {
      return;
    }

    if (history.data) {
      await paintHistory(history);
    }

    stream.goLive(history.seq);

    if (history.alternate) {
      fit.repaintAgent();
    }
  }

  async function paintHistory({ data, alternate }: {
    data: string;
    alternate: boolean;
  }) {
    // A fullscreen program's history is not a document, it is a stream of edits to a
    // framebuffer - and once the buffer has been trimmed, the edits that built the
    // frame are half gone, so replaying it paints a torn one. Switch to the alternate
    // screen anyway (so the replay lands there, and the pane never flashes the wrong
    // buffer), then ask the program to repaint itself: it re-renders on a SIGWINCH,
    // and it is the only thing that can. Its own frame is the source of truth.
    if (alternate) {
      terminal.write(ENTER_ALTERNATE_SCREEN);
    }

    terminal.write(data);
    colorScheme.observe(data);
    watchForRightToLeft(data);
    observeReplayedOutput({
      id: session.id,
      data
    });

    // A session with history was already running before this terminal
    // attached - any handshake happened at its own startup, usually trimmed
    // out of the replay, so the watcher above never sees it. Report the
    // current scheme to the agents known to listen: their spawn-time
    // COLORFGBG (or a spawn that predates it) may disagree with the palette
    // this app is painting.
    await colorScheme.reportToSubscribedAgent();

    // It sits at its prompt, not "starting". A normal-screen session at rest
    // says nothing more after the replay (replayed bytes never pass through
    // markActivity), so left unseeded the status would read "starting" forever
    // and the graceful leave gating on idleness would treat a perfectly quiet
    // session as busy. Fresh output still flips it to "working" through the
    // live-chunk path.
    activity.markAttached();
  }

  onMount(async () => {
    terminal = createTerminal(elHost);
    attachedTerminal = terminal;
    fit.fitToPane();

    const isListening = await listenToSession();
    if (!isListening) {
      return;
    }

    attachTerminalEvents();
    const isSpawned = await spawnAgent();
    if (!isSpawned) {
      return;
    }

    await replayHistory();
  });

  onDestroy(() => {
    isDestroyed = true;
    for (const stopListening of listeners) {
      stopListening();
    }
    activity.dispose();
    delivery.cancel();
    fit.dispose();
    output.dispose();
    screenSampler.dispose();
    dropContext(session.id);
    dropMcpReload(session.id);
    terminal?.dispose();
  });
</script>

<!-- Returning to this window - the Ctrl+Alt+[ / ] window cycle, an alt-tab, a
     taskbar click - puts the OS focus on the window but not back on the terminal,
     so the agent would ignore your first keystrokes until you click the pane.
     Re-grab the keyboard for the front session on every window focus, so a cycled
     window is immediately promptable. Skipped while the user is mid-typing in a
     real field (a rename box, the picker) - that focus must be left alone; the
     xterm helper textarea deliberately doesn't count as editing (see lib/ui/focus). -->
<svelte:window
  onblur={() => {
    isWindowFocused = false;
  }}
  onfocus={() => {
    isWindowFocused = true;
    output.schedule();

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
    status={activity.status}
  />
  <div class="terminal-padding">
    <div
      bind:this={elViewport}
      class="terminal-viewport"
      class:anchor-bottom={fit.isBottomAnchored}
      class:file-drag-over={isFileDragOver}
    >
      <div bind:this={elHost} style:scale={`1 ${fit.squeeze}`} class="terminal-host">
        <TerminalRtl
          isEnabled={isRightToLeftSeen}
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
