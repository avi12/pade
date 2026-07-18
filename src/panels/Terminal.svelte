<script lang="ts">
  // Read docs/terminal-rendering.md BEFORE changing resize/replay behavior here.
  // A terminal has TWO screens and they invert every rule: the normal screen
  // (scrollback document; never send height, debounce width) vs the alternate
  // screen (fullscreen framebuffer; send both sizes, serialize refits, ask the
  // program to repaint). `onAlternateScreen` is the flag; the doc is the policy.
  import { clipboard, os, pty } from "@/lib/bridge";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import type { DragHint } from "@/lib/drag-reorder";
  import Icon from "@/lib/Icon.svelte";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import SessionBadge from "@/lib/SessionBadge.svelte";
  import { observeApiError } from "@/lib/stores/apiErrorRetry.svelte";
  import { dropContext, observeContext } from "@/lib/stores/context.svelte";
  import { setSessionStatus } from "@/lib/stores/sessions.svelte";
  import { observeUsageLimit } from "@/lib/stores/usageResume.svelte";
  import { registerWrappedLinkProvider } from "@/lib/terminal-links";
  import { accumulateWheelNotches } from "@/lib/terminal-scroll";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession, PtyChunk } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal } from "@xterm/xterm";
  import { onDestroy, onMount } from "svelte";

  const { session, active = false, removable = false, onremove, onpopout, onreorder, onexit, ondraghint }: {
    session: AgentSession;
    /** The session the keyboard belongs to — the one tab (or split pane) in front. */
    active?: boolean;
    /** Show a trailing remove-from-split button in the session bar. */
    removable?: boolean;
    /** Remove this pane from the split — the trailing × button. The other pane(s)
        stay shown; the removed session lives on as a background tab. */
    onremove?: () => void;
    /** Pop this pane out of the split into its own tab — its header was dragged up
        onto the tab strip. Collapses the split to this session (shown fullscreen,
        active); the mirror of dragging a tab down onto the panes to split it. */
    onpopout?: () => void;
    /** A drag of this pane's header reordered the split — commit the new order. */
    onreorder?: (orderedIds: string[]) => void;
    /** Live pane-drag state, so App can light the tab strip's "drop → new tab"
        zone while this header is dragged over it (`hint.outside`). */
    ondraghint?: (hint: DragHint | null) => void;
    /** The PTY exited on its own (the agent quit, e.g. via Ctrl-C) — so App can
        auto-close this tab (and respawn the agent if it was the last one). */
    onexit?: (id: string) => void;
  } = $props();

  // Drag the session bar to reorder the visible split panes (past a 4px
  // threshold), or drag it up onto the tab strip to pop the pane out of the split
  // and back to a plain tab (the mirror of dragging a tab down to split it). The
  // `[data-pane-id]` slot the engine reorders lives in App, one level up from this
  // header; `closest` reaches it across the component boundary. The remove button
  // carries `data-noreorder` so pressing it stays a click. Only a pane in a live
  // split reorders — `removable` is true exactly then, so a lone pane's header
  // never lifts with nothing to sort.
  function startPaneDrag(e: PointerEvent) {
    if (!removable) {
      return;
    }

    beginReorder({
      e,
      itemSelector: "[data-pane-id]",
      idAttribute: "data-pane-id",
      axis: Axis.Horizontal,
      threshold: 4,
      ignoreSelector: "[data-noreorder]",
      onCommit: ids => onreorder?.(ids),
      onHint: hint => ondraghint?.(hint),
      outsideSelector: "[data-tab-strip]",
      onDropOutside: () => onpopout?.()
    });
  }

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
  // The terminal exists and is attached — reactive, so the focus effect below can
  // wait for it (the terminal is built inside an async onMount, well after the
  // first effects have already run).
  let attached = $state(false);

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let fitFrame: number | undefined;
  let sigwinchTimer: ReturnType<typeof setTimeout> | undefined;
  // Flow control for the alternate screen (see altFit): the agent is repainting the size
  // we last gave it, the size the pane has reached since, and the timers that decide when
  // that repaint is done — plus whether we ever gave up waiting, which means the frame on
  // screen may be torn and owes a full repaint when the drag stops.
  let awaitingRepaint = false;
  let altFitTimer: ReturnType<typeof setTimeout> | undefined;
  let lastAltFitAt = 0;
  let pendingFit:
    | {
      cols: number;
      rows: number;
    }
    | undefined;
  let repaintQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let repaintWatchdog: ReturnType<typeof setTimeout> | undefined;
  let missedRepaint = false;
  // A repaint nudge resizes the grid itself, so it comes back through the resize path —
  // this is what stops it queueing another repaint off the back of its own.
  let repainting = false;
  const IDLE_MS = 700;
  // How long the grid must hold still before the agent is told its new width. Long
  // enough that one drag gesture is one SIGWINCH, short enough to feel immediate.
  const SIGWINCH_SETTLE_MS = 150;
  // How long the nudged size is held before it is put back, when asking a fullscreen
  // agent to repaint (see repaintAgent). It must outlast the agent's own coalescing of
  // resize events, or it processes the two as one and paints for the wrong size —
  // measured at 40ms, that left its frame a row short (the hint under its prompt).
  const REPAINT_NUDGE_MS = 180;
  // A frame the agent has gone quiet for this long is a frame it has finished painting.
  const ALT_REPAINT_QUIET_MS = 40;
  // …and it is never disturbed more often than this, however fast the pane is moving.
  // Waiting for its repaint alone is not enough: it goes quiet *between* the bursts of
  // one repaint, so the credit comes back early and the resizes still pile up. Measured,
  // that pile-up eventually stops it painting for good.
  const ALT_FIT_MIN_INTERVAL_MS = 250;
  // …and if it says nothing at all for this long, stop waiting. Something is wrong (or
  // the resize genuinely changed nothing) and the drag must not stall on it.
  const ALT_REPAINT_TIMEOUT_MS = 400;
  // The width the agent is currently wrapping its output to — the PTY's spawn width,
  // then whatever we last sent it. A resize that leaves this alone is a resize the
  // agent never needs to hear about (see term.onResize).
  let agentCols = 0;
  // A resize makes the agent repaint; output within this window after one is
  // treated as that repaint's echo, not fresh activity — so revealing a hidden
  // pane (which refits it) can't flash the badge from "ready" to "working".
  const RESIZE_SETTLE_MS = 400;
  let lastResizeAt = 0;

  // Terminal control sequences, composed from named parts.
  const CONTROL_SEQUENCE_INTRODUCER = "\x1b[";
  const ALTERNATE_SCREEN_PRIVATE_MODE = "?1049";
  const SET_MODE = "h";

  // Written into xterm, not to the agent, when re-attaching to a session that is
  // already painting the alternate screen. Wire constant shared with pty.rs, which
  // detects this exact sequence to set `history.alternate` — change them together.
  const ENTER_ALTERNATE_SCREEN = `${CONTROL_SEQUENCE_INTRODUCER}${ALTERNATE_SCREEN_PRIVATE_MODE}${SET_MODE}`;

  // Shift+Enter should add a newline to the agent's prompt, not submit it.
  // Terminals send plain `\r` (0x0D) for both Enter and Shift+Enter, so the
  // wrapped CLI can't tell them apart and submits on either. Emit the CSI u
  // (fixterms) encoding for Shift+Enter instead, which Claude Code decodes as
  // "insert newline". This mirrors what `claude`'s own /terminal-setup makes
  // terminals like VS Code emit. https://code.claude.com/docs/en/terminal-config
  const ENTER_KEY_CODE = 13;
  const SHIFT_MODIFIER = 2;
  const FIXTERMS_KEY_SUFFIX = "u";
  const SHIFT_ENTER = `${CONTROL_SEQUENCE_INTRODUCER}${ENTER_KEY_CODE};${SHIFT_MODIFIER}${FIXTERMS_KEY_SUFFIX}`;

  // Wheel-scroll a fullscreen agent's own transcript. Claude Code's renderer
  // repaints its UI in place (cursor addressing) rather than appending lines, so
  // the earlier conversation lives inside the agent, not in xterm's buffer — there
  // is nothing in xterm's scrollback for a wheel tick to reveal. The agent scrolls
  // its transcript a half-page per PageUp/PageDown and says so on screen ("use
  // PgUp/PgDn to scroll"); the arrow keys the terminal would otherwise emit for a
  // wheel tick instead walk the prompt's input history — the well-known hijack
  // (claude-code#65833). So a wheel tick with nothing to reveal is forwarded as
  // PageUp/PageDown. CSI 5 ~ / CSI 6 ~.
  const PAGE_UP_PARAMETER = "5";
  const PAGE_DOWN_PARAMETER = "6";
  const TILDE_FINAL_BYTE = "~";
  const PAGE_UP = `${CONTROL_SEQUENCE_INTRODUCER}${PAGE_UP_PARAMETER}${TILDE_FINAL_BYTE}`;
  const PAGE_DOWN = `${CONTROL_SEQUENCE_INTRODUCER}${PAGE_DOWN_PARAMETER}${TILDE_FINAL_BYTE}`;
  // Mouse-tracking mode xterm reports when no program has grabbed the mouse.
  const NO_MOUSE_TRACKING = "none";

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

  // xterm paints on a canvas in px, so the font-size pref scales it here directly
  // (the CSS --ui-scale reaches rem/em UI but not the canvas). Base size × zoom.
  const TERMINAL_FONT_SIZE = 13;

  // WCAG AA for body text — the floor xterm holds every foreground to against
  // the themed background (see the Terminal options).
  const MINIMUM_CONTRAST_RATIO = 4.5;

  // Live-update the terminal font (family + zoom) when the preference changes.
  $effect(() => {
    const family = effective.monoFamily;
    const fontSize = Math.round(TERMINAL_FONT_SIZE * effective.uiScale);
    if (term) {
      term.options.fontFamily = family;
      term.options.fontSize = fontSize;
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

  // Hand the keyboard to the session in front — the moment it launches, and again
  // whenever the user switches to it. Nothing else claims focus for a terminal, so
  // without this the keystrokes go to whatever the user last clicked (the tab, the
  // agent button in onboarding) and the agent looks like it is ignoring you: you
  // have to click into the pane before it will hear a single key.
  $effect(() => {
    if (active && attached) {
      term.focus();
    }
  });

  // xterm needs the scrollbar's own width reserved out of the usable columns, or
  // the last column hides behind it. Default track width when scrollback is on.
  const SCROLLBAR_WIDTH = 14;

  // The two screens a terminal has, and the only thing that decides how a resize must
  // behave. ADE runs Claude Code fullscreen, on the ALTERNATE one (see agents.rs), but
  // nothing here may assume either: a plain shell, an agent with no fullscreen mode, and
  // Claude Code itself put back with `/tui default` all live on the NORMAL screen — and
  // every rule below is the opposite there.
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
  // decides what the eye sees move:
  //
  //   Alternate screen: the agent's frame is RIGID — conversation nailed to its first
  //   row, prompt to its last — so whichever edge is not pinned is the one that steps a
  //   whole row when the row count changes. Pin the TOP: the conversation, which is what
  //   you are reading and most of what is on screen, then never moves at all, and the
  //   remainder collects at the BOTTOM as a strip of terminal background — the same
  //   colour as the terminal, so it is not visible as anything. Pinning the bottom
  //   instead welds the prompt to the pane's edge but makes the whole conversation
  //   sawtooth by a row on every boundary, which is exactly the "mid-step".
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

  // Vertical scale of the grid, and 1 in every settled state (see `fit`).
  let squeeze = $state(1);

  // Scrollback existing at all is the signal that the document has outgrown the grid —
  // xterm only pushes a line into scrollback then. (The alternate screen never has any,
  // and pins its top: see above.)
  function updateAnchor() {
    anchorBottom = !onAlternateScreen && (term?.buffer.active.baseY ?? 0) > 0;
  }

  // How much a grid that is momentarily too tall for its pane has to be scaled by to fit
  // inside it — 1 whenever it fits, which is every settled state.
  //
  // It has to be recomputed both when the PANE moves (the drag) and when the GRID does
  // (the agent catching up, which lands later, on its own schedule): either one closes
  // the gap, and only redoing it on both is what brings the scale back to exactly 1 when
  // they meet.
  function updateSqueeze() {
    const cell = term?.dimensions?.css.cell;
    if (!term || !viewport || !cell || !(cell.height > 0)) {
      return;
    }

    const gridHeight = term.rows * cell.height;
    const paneHeight = viewport.clientHeight;
    squeeze = gridHeight > paneHeight && gridHeight > 0 ? paneHeight / gridHeight : 1;
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

  // Make a fullscreen program redraw its whole frame, by resizing the terminal a row
  // and back. It owns the alternate screen and only re-lays-out when the size changes,
  // so this is the one lever a terminal has to ask for a fresh frame — needed when
  // attaching to a session already in flight, whose framebuffer cannot be faithfully
  // replayed (a trimmed history is a torn frame, and the program's own model of the
  // screen is the only complete copy).
  //
  // The GRID has to move, not just the PTY: a size sent to the program alone leaves
  // xterm's grid saying one thing and the program's model another, and it paints its
  // frame a row short. Resizing the grid drives `term.onResize`, which sends the
  // SIGWINCH — terminal and program move together, exactly as in a real resize.
  function repaintAgent() {
    if (!term || repainting) {
      return;
    }

    const grid = term;
    const { cols, rows } = grid;
    // Both halves of the nudge drive term.onResize, which would otherwise queue another
    // repaint off the back of this one, forever.
    repainting = true;
    grid.resize(cols, Math.max(1, rows - 1));
    setTimeout(() => {
      grid.resize(cols, rows);
      repainting = false;
    }, REPAINT_NUDGE_MS);
  }

  // Resize the grid on the ALTERNATE screen — at the pace the agent can actually paint.
  //
  // Only the agent can paint a row there, and it paints by diffing against its own model
  // of the screen. Move the grid faster than it can process the SIGWINCH and that model
  // starts describing a screen which no longer exists; from then on it writes only the
  // cells it *believes* changed, so the torn frame never repairs itself. Measured with a
  // fast drag: resizing every frame stopped it painting altogether — the pane went blank
  // and stayed blank, with the process still alive and still not drawing. A fixed
  // throttle only moves the cliff (100ms survived one drag, then wedged on the third).
  //
  // But freezing the grid for the whole gesture is what makes a TUI "only update when you
  // let go". So neither: **one resize in flight at a time.** Give the agent a size, wait
  // until it has finished painting it (its output goes quiet), and only then give it the
  // size the pane has reached in the meantime. The drag is paced by the agent itself — as
  // fast as it can actually follow, never faster.
  function altFit({
    cols,
    rows
  }: {
    cols: number;
    rows: number;
  }) {
    if (!term) {
      return;
    }

    const sinceLastFit = Date.now() - lastAltFitAt;
    if (awaitingRepaint || sinceLastFit < ALT_FIT_MIN_INTERVAL_MS) {
      pendingFit = {
        cols,
        rows
      };
      // Nothing else will come back to collect it: a drag that ends inside the interval
      // has to have its last size land anyway.
      clearTimeout(altFitTimer);
      altFitTimer = setTimeout(
        () => {
          const next = pendingFit;
          pendingFit = undefined;

          if (next && !awaitingRepaint) {
            altFit(next);
          }
        },
        Math.max(0, ALT_FIT_MIN_INTERVAL_MS - sinceLastFit)
      );
      return;
    }

    if (cols === term.cols && rows === term.rows) {
      return;
    }

    lastAltFitAt = Date.now();
    awaitingRepaint = true;
    term.resize(cols, rows);
    clearTimeout(repaintWatchdog);
    repaintWatchdog = setTimeout(() => {
      // It never answered. Whatever is on screen may be torn, so the gesture owes a full
      // repaint once it stops (see term.onResize).
      missedRepaint = true;
      finishRepaint();
    }, ALT_REPAINT_TIMEOUT_MS);
  }

  // The agent has stopped talking, so the frame it was painting is done: let the next
  // size through, if the pane has moved on since.
  function finishRepaint() {
    clearTimeout(repaintQuietTimer);
    clearTimeout(repaintWatchdog);
    awaitingRepaint = false;

    const next = pendingFit;
    pendingFit = undefined;

    if (next) {
      // altFit re-parks it if the minimum interval has not elapsed yet.
      altFit(next);
    }
  }

  // Every chunk of output while a resize is in flight is the agent painting that resize.
  // A gap in it means the frame is finished — that is the credit the next resize waits on.
  function noteRepaintProgress() {
    if (!awaitingRepaint) {
      return;
    }

    clearTimeout(repaintQuietTimer);
    repaintQuietTimer = setTimeout(finishRepaint, ALT_REPAINT_QUIET_MS);
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
    const grid = term;
    // The normal screen reflows every frame: xterm owns the document there, so it can
    // rewrap the text itself as fast as the drag moves.
    if (!onAlternateScreen) {
      if (cols !== grid.cols || rows !== grid.rows) {
        grid.resize(cols, rows);
      }

      updateAnchor();
      return;
    }

    // The agent's frame only reaches the new size at the pace the agent can paint it, so
    // between here and there the grid can still be TALLER than the pane — and the grid is
    // pinned at the top, which would put the overflow past the bottom edge and cut the
    // agent's status line off. Nothing may ever cut that line. So for as long as the grid
    // is too big, it is squeezed to fit; the squeeze is never more than the lag, and goes
    // back to exactly 1 the moment the agent catches up — so a settled terminal is never
    // scaled, its text is crisp, and its clicks map true.
    updateSqueeze();
    altFit({
      cols,
      rows
    });
    updateAnchor();
  }

  onMount(async () => {
    term = new Terminal({
      fontFamily: effective.monoFamily,
      fontSize: Math.round(TERMINAL_FONT_SIZE * effective.uiScale),
      cursorBlink: true,
      allowProposedApi: true,
      theme: readXtermTheme(),
      // Safety net for colors the palette can't remap: an agent that paints
      // truecolor picked for the opposite scheme (a pale blue on the light
      // background) is nudged to WCAG AA against ours. Render-time only — the
      // buffer keeps the agent's true colors.
      minimumContrastRatio: MINIMUM_CONTRAST_RATIO,
      // OSC 8 hyperlinks — the terminal's <a>: an escape-wrapped label with a
      // hidden URL (Claude's "Security guide", "MCP documentation"). xterm
      // detects these itself but only activates them through this handler.
      // Routed through the same bridge as plain-text URLs; the Rust side still
      // refuses anything that isn't http(s), so a file:// or custom-scheme
      // link an agent emits goes nowhere.
      linkHandler: {
        activate: (_event, uri) => void os.openUrl(uri)
      }
    });
    term.open(host);
    attached = true;

    // Make URLs in the output clickable — the agent's OAuth sign-in links, docs
    // pointers. xterm's stock web-links addon only rejoins soft-wrapped rows, so
    // a URL a fullscreen agent hard-wraps at the edge would open truncated; this
    // provider stitches those rows too (see terminal-links). The default handler
    // is window.open, which a Tauri WebView won't turn into a browser tab, so
    // route the whole URL through the bridge to the system browser instead.
    registerWrappedLinkProvider({
      terminal: term,
      openUrl: uri => void os.openUrl(uri)
    });

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
      noteRepaintProgress();
      markActivity();
      // Track how full this agent's context window is (drives auto-handoff).
      observeContext({
        id: session.id,
        chunk: chunk.data
      });
      // Spot the CLI's "limit reached" stop message (drives auto-resume).
      observeUsageLimit({
        id: session.id,
        chunk: chunk.data
      });
      // Spot a transient API-error stop (drives API-error auto-retry).
      observeApiError({
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
      onexit?.(session.id);
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

    async function pasteClipboard() {
      const text = await clipboard.readText();
      if (text) {
        // paste (not write) so xterm wraps it in bracketed-paste markers when the
        // agent has that mode on — it then treats it as pasted text, not typing.
        term.paste(text);
      }
    }

    // Keyboard overrides layered on xterm's own handling; returning false stops
    // xterm from also sending the key's control code.
    //  • Shift+Enter → a prompt newline (CSI u) instead of a submitting `\r`.
    //  • Ctrl+C → copy the selection; with nothing selected it falls through so
    //    xterm still sends ^C (SIGINT) to interrupt the agent.
    //  • Ctrl+V → paste the clipboard (xterm would otherwise send a raw ^V, and
    //    only the WebView's right-click menu pasted).
    term.attachCustomKeyEventHandler(event => {
      if (event.type !== "keydown") {
        return true;
      }

      const isShiftEnter =
        event.key === "Enter" && event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
      if (isShiftEnter) {
        // preventDefault stops the browser inserting a newline into xterm's hidden
        // textarea, which xterm would forward to the PTY as a submit.
        event.preventDefault();
        void pty.write({
          id: session.id,
          data: SHIFT_ENTER
        });
        return false;
      }

      const isPlainCtrl = event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey;

      const isCopyChord = isPlainCtrl && (event.key === "c" || event.key === "C");
      if (isCopyChord && term.hasSelection()) {
        event.preventDefault();
        void clipboard.writeText(term.getSelection());
        return false;
      }

      const isPasteChord = isPlainCtrl && (event.key === "v" || event.key === "V");
      if (isPasteChord) {
        event.preventDefault();
        void pasteClipboard();
        return false;
      }

      return true;
    });

    // Wheel-scroll the agent's own transcript when xterm has nothing to scroll
    // itself (see PAGE_UP). Defer to xterm — return true, behave as if unhooked —
    // in the two cases it can handle: a program that grabbed the mouse (Neovim, a
    // pager, an agent doing its own wheel handling) wants the wheel as a mouse
    // report, and a plain shell with real scrollback (baseY > 0) wants its own
    // document scrolled. Otherwise the visible frame is all xterm holds — a
    // fullscreen agent repainting in place — so forward the scroll it understands.
    let wheelCarry = 0;
    term.attachCustomWheelEventHandler(e => {
      const agentOwnsMouse = term.modes.mouseTrackingMode !== NO_MOUSE_TRACKING;
      const hasNativeScrollback = term.buffer.active.baseY > 0;
      if (agentOwnsMouse || hasNativeScrollback) {
        return true;
      }

      const { notches, carry } = accumulateWheelNotches({
        deltaY: e.deltaY,
        deltaMode: e.deltaMode,
        carry: wheelCarry
      });
      wheelCarry = carry;

      if (notches !== 0) {
        const scrollKey = notches < 0 ? PAGE_UP : PAGE_DOWN;
        void pty.write({
          id: session.id,
          data: scrollKey.repeat(Math.abs(notches))
        });
      }

      e.preventDefault();
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
      // The grid just changed, which is the other half of what the squeeze measures.
      updateSqueeze();

      // Alternate screen: tell the agent at once — it owns every row, and a size it has
      // not heard is a row nobody paints. The grid only reaches it at a pace the agent
      // can keep up with in the first place (see altFit).
      //
      // If we ever gave up waiting for one of its repaints, the frame on screen may be
      // torn, so the gesture owes it a full repaint once the pane stops moving. Otherwise
      // it kept up, and forcing one would only make the drag end with a needless blink.
      if (onAlternateScreen) {
        clearTimeout(sigwinchTimer);
        sizeAgent({
          cols,
          rows
        });

        if (missedRepaint && !repainting) {
          sigwinchTimer = setTimeout(() => {
            missedRepaint = false;
            repaintAgent();
          }, SIGWINCH_SETTLE_MS);
        }

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

    // Refit once per animation frame. On the normal screen that reflows the document
    // live as you drag, the way a web page does — xterm holds the text there, so it
    // can rewrap it itself. On the alternate screen fitToPane holds the grid still
    // until the gesture ends instead (the agent owns the pixels; see there). rAF
    // coalesces a burst of resize events into one fit per frame; xterm 6.1 renders the
    // reflow synchronously (issue #4922 / PR #5529) so it stays crisp.
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
      // A fullscreen program's history is not a document, it is a stream of edits to a
      // framebuffer — and once the buffer has been trimmed, the edits that built the
      // frame are half gone, so replaying it paints a torn one. Switch to the alternate
      // screen anyway (so the replay lands there, and the pane never flashes the wrong
      // buffer), then ask the program to repaint itself: it re-renders on a SIGWINCH,
      // and it is the only thing that can. Its own frame is the source of truth.
      if (history.alternate) {
        term.write(ENTER_ALTERNATE_SCREEN);
      }

      term.write(history.data);
    }

    for (const chunk of pendingChunks) {
      if (chunk.seq > history.seq) {
        consume(chunk);
      }
    }

    pendingChunks.length = 0;
    replayed = true;

    if (history.alternate) {
      repaintAgent();
    }

    // Seed a new-project first prompt and submit it: the prompt's own newlines
    // are `\n` (which the agent keeps as soft newlines in the input), and the
    // trailing `\r` is the Enter that sends it — same submit convention as the
    // handoff/successor prompts.
    if (session.initialPrompt) {
      await pty.write({
        id: session.id,
        data: `${session.initialPrompt}\r`
      });
    }
  });

  onDestroy(() => {
    destroyed = true;
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);
    clearTimeout(sigwinchTimer);
    clearTimeout(repaintQuietTimer);
    clearTimeout(repaintWatchdog);
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
    function token(name: string) {
      return style.getPropertyValue(name).trim();
    }
    // The full ANSI palette comes from the theme (see the --terminal-* tokens):
    // agent CLIs paint with these 16 slots, and xterm's own defaults only suit
    // a dark screen — the light scheme re-picks every one dark enough to read.
    return {
      background: token("--code-background"),
      foreground: token("--code-foreground"),
      cursor: token("--primary"),
      selectionBackground: token("--terminal-selection"),
      black: token("--terminal-black"),
      red: token("--terminal-red"),
      green: token("--terminal-green"),
      yellow: token("--terminal-yellow"),
      blue: token("--terminal-blue"),
      magenta: token("--terminal-magenta"),
      cyan: token("--terminal-cyan"),
      white: token("--terminal-white"),
      brightBlack: token("--terminal-bright-black"),
      brightRed: token("--terminal-bright-red"),
      brightGreen: token("--terminal-bright-green"),
      brightYellow: token("--terminal-bright-yellow"),
      brightBlue: token("--terminal-bright-blue"),
      brightMagenta: token("--terminal-bright-magenta"),
      brightCyan: token("--terminal-bright-cyan"),
      brightWhite: token("--terminal-bright-white")
    };
  }
</script>

<div class="term-wrap">
  <!-- Pointer-only reorder handle for the split; the remove button stays
       keyboard-reachable, so the drag is a pure enhancement. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class="session-bar" class:reorderable={removable} onpointerdown={startPaneDrag}>
    <SessionBadge label={session.branch ? `${session.agent.label} · ${session.branch}` : session.agent.label} {status} />
    {#if removable}
      <button
        class="remove-pane"
        aria-label="Remove from split"
        data-noreorder
        data-tooltip="Remove from split"
        onclick={() => onremove?.()}
      >
        <Icon name="close" size={16} />
      </button>
    {/if}
  </header>
  <div class="term-pad">
    <div bind:this={viewport} class="term-viewport" class:anchor-bottom={anchorBottom}>
      <div bind:this={host} style:scale={`1 ${squeeze}`} class="term-host"></div>
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

    /* In a split, the bar is a drag handle for reordering the panes; a touch-drag
       must grab it, not scroll. A lone pane has nothing to sort, so no affordance. */
    &.reorderable {
      cursor: grab;
      touch-action: none;

      &:active {
        cursor: grabbing;
      }
    }
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
     count toward the fit — it lifts the output off every pane edge (canon:
     12px top, 8px right, 8px bottom, 14px left). */
  .term-pad {
    flex: 1;
    min-block-size: 0;
    padding-block: 12px 8px;
    padding-inline: 14px 8px;
    background: var(--code-background);
  }

  /* Full-size measuring frame: fitToPane reads its client size for the cols/rows and
     pins the grid to the end of the content the terminal is actually showing (see
     `anchorBottom`) — a fullscreen agent's frame and an unscrolled conversation pin the
     top, a scrolled one pins the bottom. The grid is whole cells and never quite fills
     the frame, so the leftover sits as a sliver of background at the unpinned edge. */
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

    /* xterm mounts here at its natural whole-cell size, scaled only while a grid that is
       momentarily too tall for the pane is being squeezed to fit (see `squeeze`) — from
       the top, so the squeeze pulls the overflowing bottom up into view rather than
       moving the text you are reading. At rest the scale is exactly 1: text stays crisp
       and clicks map at native cell size. */
    .term-host {
      flex: none;
      transform-origin: top left;
    }
  }
</style>
