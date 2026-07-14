<script lang="ts">
  import { pty } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import SessionBadge from "@/lib/SessionBadge.svelte";
  import { dropContext, observeContext } from "@/lib/stores/context.svelte";
  import { setSessionStatus } from "@/lib/stores/sessions.svelte";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession, PtyChunk } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal } from "@xterm/xterm";
  import { onDestroy, onMount } from "svelte";

  const { session, removable = false, onremove }: {
    session: AgentSession;
    /** Show a trailing remove-from-split button in the session bar. */
    removable?: boolean;
    onremove?: () => void;
  } = $props();

  let host: HTMLDivElement;
  let viewport: HTMLDivElement;
  let term: Terminal;
  let unlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;
  let resizeObs: ResizeObserver | undefined;
  // Guards the async onMount against a teardown that runs before its awaits
  // settle: onDestroy sets this, and each awaited step bails so no listener is
  // registered after unmount and no write hits a disposed terminal.
  let destroyed = false;

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let fitFrame: number | undefined;
  let sigwinchTimer: ReturnType<typeof setTimeout> | undefined;
  let altFitTimer: ReturnType<typeof setTimeout> | undefined;
  const IDLE_MS = 700;
  // How long the grid must hold still before the agent is told its new width. Long
  // enough that one drag gesture is one SIGWINCH, short enough to feel immediate.
  const SIGWINCH_SETTLE_MS = 150;
  // The width the agent is currently wrapping its output to — the PTY's spawn width,
  // then whatever we last sent it. A resize that leaves this alone is a resize the
  // agent never needs to hear about (see term.onResize).
  let agentCols = 0;
  // A resize makes the agent repaint; output within this window after one is
  // treated as that repaint's echo, not fresh activity — so revealing a hidden
  // pane (which refits it) can't flash the badge from "ready" to "working".
  const RESIZE_SETTLE_MS = 400;
  let lastResizeAt = 0;

  // Shift+Enter should add a newline to the agent's prompt, not submit it.
  // Terminals send plain `\r` (0x0D) for both Enter and Shift+Enter, so the
  // wrapped CLI can't tell them apart and submits on either. Emit the CSI u
  // (fixterms) encoding for Shift+Enter instead — key 13 (Enter) with modifier
  // 2 (Shift) — which Claude Code decodes as "insert newline". This mirrors what
  // `claude`'s own /terminal-setup makes terminals like VS Code emit.
  // https://code.claude.com/docs/en/terminal-config
  const SHIFT_ENTER = "\x1b[13;2u";

  function markActivity() {
    if (status === SessionStatus.enum.exited) {
      return;
    }

    // Ignore the agent's own resize-repaint: it isn't the agent working, so a
    // settled "ready" session shouldn't blink to "working" when its pane is
    // revealed and refitted. Real work arrives outside the settle window.
    const isResizeEcho =
      status === SessionStatus.enum.ready && Date.now() - lastResizeAt < RESIZE_SETTLE_MS;
    if (isResizeEcho) {
      return;
    }

    status = SessionStatus.enum.working;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      if (status === SessionStatus.enum.working) {
        status = SessionStatus.enum.ready;
      }
    }, IDLE_MS);
  }

  // Publish status to the shared store so the top-bar tab shows a matching dot.
  $effect(() => {
    setSessionStatus({
      id: session.id,
      status
    });
  });

  // Live-update the terminal font when the preference changes.
  $effect(() => {
    const family = effective.monoFamily;
    if (term) {
      term.options.fontFamily = family;
      fitToPane();
    }
  });

  // Re-theme the terminal when the app scheme flips, so Claude Code's output
  // sits on a background that matches the light/dark theme.
  $effect(() => {
    const { scheme } = appearance;
    if (term) {
      void scheme;
      term.options.theme = readXtermTheme();
    }
  });

  // xterm needs the scrollbar's own width reserved out of the usable columns, or
  // the last column hides behind it. Default track width when scrollback is on.
  const SCROLLBAR_WIDTH = 14;

  // The two screens a terminal has, and the only thing that decides how a resize must
  // behave. ADE runs Claude Code on the normal one (see agents.rs), but nothing here
  // may assume that: a fullscreen agent (Codex, aider), a pager or editor the agent
  // shells out to, or Claude Code itself switched back with `/tui fullscreen`, all
  // take over the ALTERNATE screen — and the rules there are the exact opposite.
  const Screen = {
    Normal: "normal",
    Alternate: "alternate"
  } as const;

  // On the alternate screen there is no document and no scrollback: it is a
  // framebuffer the agent owns and repaints, and the terminal holds nothing it could
  // reflow. So the agent must be told the size — height included — immediately and
  // every frame, or the rows it hasn't painted come out blank and the ones it lost are
  // truncated. Everything below keys off this.
  let onAlternateScreen = $state(false);

  // Which edge the grid is pinned to. The grid is whole cells and the pane is not, so
  // there is always a sub-cell remainder to put somewhere, and which end it belongs at
  // depends on which end of its content the terminal is showing:
  //
  //   Alternate screen: the agent paints all `rows` rows, with its prompt on the last
  //   one. Pin the BOTTOM so the prompt stays welded to the pane's edge.
  //
  //   Normal screen, no scrollback yet (the conversation still fits): output starts at
  //   row 0, so pinning the TOP keeps every line at a fixed y. Resizing then moves
  //   nothing at all — the pane just reveals or hides empty rows at the bottom.
  //
  //   Normal screen with scrollback (the conversation overflows, terminal parked at
  //   the newest line): xterm scrolls the document in whole rows, so a top-pinned grid
  //   would step the text by a row each time the row count changes. Pinning the BOTTOM
  //   instead puts every visible line at a fixed distance from the pane's bottom edge —
  //   `y = paneBottom - (linesFromEnd + 1) * cellHeight`, which has no `rows` term in
  //   it. The row xterm scrolls away and the remainder the grid gains cancel out, so
  //   the text is continuous through a row boundary.
  //
  // Either way the remainder ends up at the unpinned edge, as a sliver of background.
  let anchorBottom = $state(false);

  // Scrollback existing at all is the signal that the document has outgrown the grid —
  // xterm only pushes a line into scrollback then. (The alternate screen never has any.)
  function updateAnchor() {
    anchorBottom = onAlternateScreen || (term?.buffer.active.baseY ?? 0) > 0;
  }

  // The one place that tells the PTY how big it is (DRY — both resize paths and the
  // screen switch go through here). Remembering the width we sent is what lets a
  // height-only change on the normal screen be dropped entirely. Stamp the time so the
  // repaint the agent sends back isn't counted as activity (see markActivity).
  function sizeAgent({
    cols,
    rows
  }: {
    cols: number;
    rows: number;
  }) {
    lastResizeAt = Date.now();
    agentCols = cols;
    void pty.resize({
      id: session.id,
      cols,
      rows
    });
  }

  // Fit the grid to the pane, and re-pin it (above). Whole cells only, rounded down,
  // so the grid always fits inside the pane. Never round up — the overflowing row
  // would have to be clipped, and on the normal screen buffer every row is content.
  //
  // No transform anywhere, so text stays crisp and clicks map at native cell size.
  // `term.dimensions.css.cell` is the font metric, independent of the current grid,
  // so there's no circular measurement.
  function fitToPane() {
    if (!term || !viewport) {
      return;
    }

    const cell = term.dimensions?.css.cell;
    if (!cell || !(cell.width > 0) || !(cell.height > 0)) {
      return;
    }

    const cols = Math.max(2, Math.floor((viewport.clientWidth - SCROLLBAR_WIDTH) / cell.width));
    const rows = Math.max(1, Math.floor(viewport.clientHeight / cell.height));
    // On the alternate screen the grid may not move under the agent mid-drag. A
    // fullscreen TUI paints by DIFFING against its own model of the screen (that is what
    // makes it flicker-free), so a grid that resizes faster than it can process the
    // SIGWINCH leaves its model describing a screen that no longer exists — and because
    // it then writes only the cells it believes changed, the half-drawn frame never
    // repairs itself, not even on the next resize (measured: one fast drag left the
    // layout stuck at a stale width for good). So there the grid waits for the drag to
    // settle, and then moves in lockstep with the agent (see term.onResize).
    //
    // The normal screen has no such contract — xterm owns the document and reflows it
    // itself — so it refits every frame, which is what makes a drag flow like a page.
    const grid = term;
    const isGridStale = cols !== grid.cols || rows !== grid.rows;
    if (!onAlternateScreen) {
      if (isGridStale) {
        grid.resize(cols, rows);
      }

      updateAnchor();
      return;
    }

    // Always re-arm with the latest size, even when it matches the grid: the grid does
    // not move during the drag, so a gesture that ends back where it started still has
    // to cancel the stale resize a mid-drag frame left pending.
    if (isGridStale || altFitTimer !== undefined) {
      clearTimeout(altFitTimer);
      altFitTimer = setTimeout(() => {
        altFitTimer = undefined;

        if (cols !== grid.cols || rows !== grid.rows) {
          grid.resize(cols, rows);
        }
      }, SIGWINCH_SETTLE_MS);
    }

    updateAnchor();
  }

  onMount(async () => {
    term = new Terminal({
      fontFamily: effective.monoFamily,
      fontSize: 13,
      cursorBlink: true,
      allowProposedApi: true,
      theme: readXtermTheme()
    });
    term.open(host);

    // GPU-accelerated rendering; fall back silently if WebGL is unavailable.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      term.loadAddon(webgl);
    } catch {
    /* CPU renderer is fine as a fallback */
    }

    fitToPane();

    // Stream this session's PTY output into the terminal; each chunk is a sign
    // of life that resets the idle → ready timer. Events are filtered by id so
    // sibling sessions don't cross-write.
    //
    // Until the session's history has been replayed (below) the live chunks are only
    // parked, not written: the PTY may already be running and this terminal is empty,
    // so writing them now would paint the tail of a conversation whose beginning is
    // missing. Each chunk carries its position in the stream, so once the history is
    // in, the ones it already contains can be dropped and the rest written in order.
    const pendingChunks: PtyChunk[] = [];
    let replayed = false;

    function consume(chunk: PtyChunk) {
      // The terminal may already be disposed if a late chunk arrives during
      // teardown; skip the write rather than throw.
      if (destroyed || !term) {
        return;
      }

      term.write(chunk.data);
      markActivity();
      // Track how full this agent's context window is (drives auto-handoff).
      observeContext({
        id: session.id,
        chunk: chunk.data
      });
    }

    const dataUnlisten = await pty.onData(chunk => {
      if (chunk.id !== session.id) {
        return;
      }

      if (!replayed) {
        pendingChunks.push(chunk);
        return;
      }

      consume(chunk);
    });
    // If we were destroyed while awaiting, this listener registered too late
    // for onDestroy to see — tear it down now and stop.
    if (destroyed) {
      dataUnlisten();
      return;
    }

    unlisten = dataUnlisten;

    const exitListener = await pty.onExit(id => {
      if (id !== session.id) {
        return;
      }

      clearTimeout(idleTimer);
      status = SessionStatus.enum.exited;
    });
    if (destroyed) {
      exitListener();
      return;
    }

    exitUnlisten = exitListener;

    // Send keystrokes to this session's PTY.
    term.onData(data => void pty.write({
      id: session.id,
      data
    }));

    // Translate Shift+Enter into a prompt newline before xterm encodes it as a
    // plain Enter (see SHIFT_ENTER). Returning false stops xterm from also
    // sending its own `\r`, which would submit.
    term.attachCustomKeyEventHandler(event => {
      const isShiftEnter =
        event.type === "keydown" &&
          event.key === "Enter" &&
          event.shiftKey &&
          !event.altKey &&
          !event.ctrlKey &&
          !event.metaKey;
      if (!isShiftEnter) {
        return true;
      }

      // preventDefault stops the browser inserting a newline into xterm's hidden
      // textarea, which xterm's input handler would otherwise forward to the PTY
      // as a submit. Returning false additionally stops xterm sending its own
      // `\r`, so the CSI u sequence we write is the only thing the agent sees.
      event.preventDefault();
      void pty.write({
        id: session.id,
        data: SHIFT_ENTER
      });
      return false;
    });

    // The document can outgrow the pane with no resize involved — the agent simply
    // prints past the last row — and that is the moment the grid must re-pin.
    term.onScroll(updateAnchor);

    // A program that takes over the alternate screen has to be told the moment the
    // grid changes, and told the height too — it is painting the whole framebuffer,
    // and nobody else can. Switching screens also makes the grid re-pin, and squares
    // the agent's idea of the size with ours, since on the normal screen we deliberately
    // let its height go stale (below).
    term.buffer.onBufferChange(() => {
      onAlternateScreen = term.buffer.active.type === Screen.Alternate;
      updateAnchor();

      if (onAlternateScreen) {
        sizeAgent({
          cols: term.cols,
          rows: term.rows
        });
      }
    });

    // Which of a resize's two numbers the agent hears depends on the screen it is on.
    //
    // ALTERNATE: both, immediately, every frame. There is no document behind that
    // screen and no scrollback — only the agent can paint a row — so a size it hasn't
    // been told is a row nobody paints (blank at the bottom, truncated at the top).
    // Never debounce this one.
    //
    // NORMAL: the width, once the drag settles, and NEVER the height. A CLI printing an
    // inline document needs the width — that is what its text wraps to. It does not
    // need the height: how much of a document you can see is the terminal's business,
    // and xterm already knows. Send the height anyway and every SIGWINCH makes it
    // re-render — which is what kept a step on screen:
    //
    //   - it re-lays-out its frame for the new row count and drops or adds a line, so
    //     the conversation above that line sits a full row off from the text below it.
    //     Not geometry: the document itself changing under us; and
    //   - Ink reprints its whole static history on a resize, stranding the previous
    //     copy in the scrollback (one per SIGWINCH — a fast drag once left 52).
    //
    // So a vertical drag there sends nothing at all: the agent's output is untouched
    // and xterm simply reveals more or less of it, exactly like scrolling a web page.
    term.onResize(({ cols, rows }) => {
      lastResizeAt = Date.now();

      // The grid only reaches this size on the alternate screen once the drag has
      // already settled (see fitToPane), so the agent is told at once — it owns every
      // row there and nothing else can paint the ones it hasn't seen.
      if (onAlternateScreen) {
        clearTimeout(sigwinchTimer);
        sizeAgent({
          cols,
          rows
        });
        return;
      }

      if (cols === agentCols) {
        return;
      }

      clearTimeout(sigwinchTimer);
      sigwinchTimer = setTimeout(
        () =>
          sizeAgent({
            cols,
            rows
          }),
        SIGWINCH_SETTLE_MS
      );
    });

    // Fit the grid once per animation frame so the conversation rewraps live as you
    // drag, the way a web page reflows — which only works because the agent runs on
    // the normal screen buffer (see agents.rs: ADE forces the classic renderer), so
    // xterm holds a real document and real scrollback to reflow. rAF coalesces a
    // burst of resize events into one fit per frame; xterm 6.1 renders the reflow
    // synchronously (issue #4922 / PR #5529) so it stays crisp.
    resizeObs = new ResizeObserver(() => {
      if (fitFrame !== undefined) {
        return;
      }

      fitFrame = requestAnimationFrame(() => {
        fitFrame = undefined;
        fitToPane();
      });
    });
    resizeObs.observe(viewport);

    // Spawn the chosen agent in a real PTY.
    if (destroyed) {
      return;
    }

    agentCols = term.cols;
    await pty.spawn({
      id: session.id,
      command: session.agent.command,
      cwd: session.cwd ?? null,
      cols: agentCols,
      rows: term.rows,
      args: session.args
    });

    // Paint whatever the session has already said. A spawn for a session that is
    // still running is a no-op, so this terminal may be attaching to a conversation
    // already in flight — a remounted component, a reloaded window — and a PTY keeps
    // no scrollback of its own: with nothing replayed, the pane just sits blank while
    // the agent waits, quite happily, for input. For a fresh spawn the history is
    // empty and this costs nothing.
    //
    // Chunks that arrived while this was in flight were parked, not written. The
    // history is authoritative up to its `seq`, so anything at or below it is already
    // painted and only the newer ones still need to go in — in order, and before any
    // further live chunk is written.
    if (destroyed) {
      return;
    }

    const history = await pty.history(session.id);
    if (destroyed || !term) {
      return;
    }

    if (history.data) {
      term.write(history.data);
    }

    for (const chunk of pendingChunks) {
      if (chunk.seq > history.seq) {
        consume(chunk);
      }
    }

    pendingChunks.length = 0;
    replayed = true;

    // Seed a new-project first prompt into the input (typed, not submitted —
    // the user reviews and presses Enter).
    if (session.initialPrompt) {
      await pty.write({
        id: session.id,
        data: session.initialPrompt
      });
    }
  });

  onDestroy(() => {
    destroyed = true;
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);
    clearTimeout(sigwinchTimer);
    clearTimeout(altFitTimer);

    if (fitFrame !== undefined) {
      cancelAnimationFrame(fitFrame);
    }

    resizeObs?.disconnect();
    dropContext(session.id);
    term?.dispose();
  });

  function readXtermTheme() {
    const style = getComputedStyle(document.documentElement);
    return {
      background: style.getPropertyValue("--code-background").trim(),
      foreground: style.getPropertyValue("--code-foreground").trim(),
      cursor: style.getPropertyValue("--primary").trim()
    };
  }
</script>

<div class="term-wrap">
  <header class="session-bar">
    <SessionBadge label={session.branch ? `${session.agent.label} · ${session.branch}` : session.agent.label} {status} />
    {#if removable}
      <button
        class="remove-pane"
        aria-label="Remove from split"
        data-tooltip="Remove from split"
        onclick={() => onremove?.()}
      >
        <Icon name="close" size={16} />
      </button>
    {/if}
  </header>
  <div class="term-pad">
    <div bind:this={viewport} class="term-viewport" class:anchor-bottom={anchorBottom}>
      <div bind:this={host} class="term-host"></div>
    </div>
  </div>
</div>

<style>
  .term-wrap {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  /* Thin session bar on surface-1 with a hairline divider; the SessionBadge
     (dot + mono label + state phrase) sits flush at the start. */
  .session-bar {
    display: flex;
    flex-shrink: 0;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  /* Inline remove-from-split action at the end of the bar — transparent until
     hovered, then a soft crit wash (canvas line 276). */
  .remove-pane {
    display: inline-flex;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: color 150ms var(--ease), background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }

  /* Visual insets live on this pad, off the measured viewport, so they never
     count toward the fit — it lifts the output off every pane edge (canvas line
     264: 10px top, 8px right, 8px bottom, 14px left). */
  .term-pad {
    flex: 1;
    min-block-size: 0;
    padding-block: 10px 8px;
    padding-inline: 14px 8px;
    background: var(--code-background);
  }

  /* Full-size measuring frame: fitToPane reads its client size for the cols/rows and
     pins the grid to the end of the document the terminal is actually showing (see
     `anchorBottom`) — the top while the conversation still fits, the bottom once it
     scrolls. Either way the grid is whole cells and never quite fills the frame, so
     the leftover sits as a sliver of background at the unpinned edge. */
  .term-viewport {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    overflow: hidden;
    block-size: 100%;
    inline-size: 100%;

    &.anchor-bottom {
      justify-content: flex-end;
    }

    /* xterm mounts here at its natural whole-cell size. No transform — text stays
       crisp and clicks map at native cell size. */
    .term-host {
      flex: none;
    }
  }
</style>
