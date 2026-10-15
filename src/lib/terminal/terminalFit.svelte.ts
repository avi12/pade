// Fitting xterm's grid to its pane, and telling the agent about it.
//
// Read docs/terminal-rendering.md BEFORE changing resize behavior here. A
// terminal has TWO screens and they invert every rule: the normal screen
// (scrollback document; never send height, debounce width) vs the alternate
// screen (fullscreen framebuffer; send both sizes, serialize refits, ask the
// program to repaint). `isOnAlternateScreen` is the flag; the doc is the policy.

import { gridForPane, squeezeToFit } from "@/lib/terminal/pane-grid";
import type { GridSize } from "@/lib/terminal/pane-grid";
import type { Terminal } from "@xterm/xterm";

// The two screens a terminal has, and the only thing that decides how a resize must
// behave. ADE runs Claude Code fullscreen, on the ALTERNATE one (see agents.rs), but
// nothing here may assume either: a plain shell, an agent with no fullscreen mode, and
// Claude Code itself put back with `/tui default` all live on the NORMAL screen - and
// every rule below is the opposite there.
export const Screen = {
  Normal: "normal",
  Alternate: "alternate"
} as const;

// How long the grid must hold still before the agent is told its new width. Long
// enough that one drag gesture is one SIGWINCH, short enough to feel immediate.
const SIGWINCH_SETTLE_MS = 150;
// How long the nudged size is held before it is put back, when asking a fullscreen
// agent to repaint (see repaintAgent). It must outlast the agent's own coalescing of
// resize events, or it processes the two as one and paints for the wrong size -
// measured at 40ms, that left its frame a row short (the hint under its prompt).
const REPAINT_NUDGE_MS = 180;
// A frame the agent has gone quiet for this long is a frame it has finished painting.
const ALT_REPAINT_QUIET_MS = 40;
// …and it is never disturbed more often than this, however fast the pane is moving.
// Waiting for its repaint alone is not enough: it goes quiet *between* the bursts of
// one repaint, so the credit comes back early and the resizes still pile up. Measured,
// that pile-up eventually stops it painting for good.
const ALT_FIT_MINIMUM_INTERVAL_MS = 250;
// …and if it says nothing at all for this long, stop waiting. Something is wrong (or
// the resize genuinely changed nothing) and the drag must not stall on it.
const ALT_REPAINT_TIMEOUT_MS = 400;

export function createTerminalFit({ terminal, viewport, resizeAgent, noteResize }: {
  terminal: () => Terminal | undefined;
  /** The full-size measuring frame the grid is fitted into. */
  viewport: () => HTMLElement | undefined;
  /** Tell the PTY its size. */
  resizeAgent: (size: GridSize) => void;
  /** The grid or the agent's size just changed - its repaint is not activity. */
  noteResize: () => void;
}) {
  // On the alternate screen there is no document and no scrollback: it is a
  // framebuffer the agent owns and repaints, and the terminal holds nothing it could
  // reflow. So the agent must be told the size - height included - immediately and
  // every frame, or the rows it hasn't painted come out blank and the ones it lost are
  // truncated. Everything below keys off this.
  let isOnAlternateScreen = $state(false);

  // Which edge the grid is pinned to. The grid is whole cells and the pane is not, so
  // there is always a sub-cell remainder to put somewhere, and which end it belongs at
  // decides what the eye sees move:
  //
  //   Alternate screen: the agent's frame is RIGID - conversation nailed to its first
  //   row, prompt to its last - so whichever edge is not pinned is the one that steps a
  //   whole row when the row count changes. Pin the TOP: the conversation, which is what
  //   you are reading and most of what is on screen, then never moves at all, and the
  //   remainder collects at the BOTTOM as a strip of terminal background - the same
  //   colour as the terminal, so it is not visible as anything. Pinning the bottom
  //   instead welds the prompt to the pane's edge but makes the whole conversation
  //   sawtooth by a row on every boundary, which is exactly the "mid-step".
  //
  //   Normal screen, no scrollback yet (the conversation still fits): output starts at
  //   row 0, so pinning the TOP keeps every line at a fixed y. Resizing then moves
  //   nothing at all - the pane just reveals or hides empty rows at the bottom.
  //
  //   Normal screen with scrollback (the conversation overflows, terminal parked at
  //   the newest line): xterm scrolls the document in whole rows, so a top-pinned grid
  //   would step the text by a row each time the row count changes. Pinning the BOTTOM
  //   instead puts every visible line at a fixed distance from the pane's bottom edge -
  //   `y = paneBottom - (linesFromEnd + 1) * cellHeight`, which has no `rows` term in
  //   it. The row xterm scrolls away and the remainder the grid gains cancel out, so
  //   the text is continuous through a row boundary.
  //
  // Either way the remainder ends up at the unpinned edge, as a sliver of background.
  let isBottomAnchored = $state(false);

  // Vertical scale of the grid, and 1 in every settled state (see `fitToPane`).
  let squeeze = $state(1);

  // The width the agent is currently wrapping its output to - the PTY's spawn width,
  // then whatever we last sent it. A resize that leaves this alone is a resize the
  // agent never needs to hear about (see handleGridResize).
  let agentColumns = 0;
  let sigwinchTimer: ReturnType<typeof setTimeout> | undefined;
  let fitFrame: number | undefined;
  let resizeObserver: ResizeObserver | undefined;

  // Flow control for the alternate screen (see altFit): the agent is repainting the size
  // we last gave it, the size the pane has reached since, and the timers that decide when
  // that repaint is done. Whether it kept up or not, the settled gesture ends with a
  // full repaint (see handleGridResize) - that is what makes a torn frame heal.
  let isAwaitingRepaint = false;
  let altFitTimer: ReturnType<typeof setTimeout> | undefined;
  let lastAltFitAt = 0;
  let pendingFit: GridSize | undefined;
  let repaintQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let repaintWatchdog: ReturnType<typeof setTimeout> | undefined;
  // A repaint nudge resizes the grid itself, so it comes back through the resize path -
  // this is what stops it queueing another repaint off the back of its own.
  let isRepainting = false;

  // Scrollback existing at all is the signal that the document has outgrown the grid -
  // xterm only pushes a line into scrollback then. (The alternate screen never has any,
  // and pins its top: see above.)
  function updateAnchor() {
    isBottomAnchored = !isOnAlternateScreen && (terminal()?.buffer.active.baseY ?? 0) > 0;
  }

  // It has to be recomputed both when the PANE moves (the drag) and when the GRID does
  // (the agent catching up, which lands later, on its own schedule): either one closes
  // the gap, and only redoing it on both is what brings the scale back to exactly 1 when
  // they meet.
  function updateSqueeze() {
    const pane = measuredPane();
    if (!pane) {
      return;
    }

    const fitted = squeezeToFit({
      rows: pane.grid.rows,
      cellHeight: pane.grid.dimensions?.css.cell.height,
      paneHeight: pane.elViewport.clientHeight
    });
    if (fitted !== undefined) {
      squeeze = fitted;
    }
  }

  // The grid and the frame it is fitted into, once both exist.
  function measuredPane() {
    const grid = terminal();
    const elViewport = viewport();
    return grid && elViewport
      ? {
        grid,
        elViewport
      }
      : undefined;
  }

  // The one place that tells the PTY how big it is (DRY - both resize paths and the
  // screen switch go through here). Remembering the width we sent is what lets a
  // height-only change on the normal screen be dropped entirely. Stamp the time so the
  // repaint the agent sends back isn't counted as activity.
  function sizeAgent(size: GridSize) {
    noteResize();
    agentColumns = size.columns;
    resizeAgent(size);
  }

  // The grid this pane implies right now - the one measurement behind both the fit
  // and the repaint nudge's restore, so the two can never disagree about the size the
  // pane is actually asking for.
  function paneGrid() {
    const pane = measuredPane();
    if (!pane) {
      return undefined;
    }

    return gridForPane({
      width: pane.elViewport.clientWidth,
      height: pane.elViewport.clientHeight,
      cell: pane.grid.dimensions?.css.cell
    });
  }

  function isGridSized(grid: Terminal, size: GridSize) {
    return size.columns === grid.cols && size.rows === grid.rows;
  }

  // Make a fullscreen program redraw its whole frame, by resizing the terminal a row
  // and back. It owns the alternate screen and only re-lays-out when the size changes,
  // so this is the one lever a terminal has to ask for a fresh frame - needed when
  // attaching to a session already in flight, whose framebuffer cannot be faithfully
  // replayed (a trimmed history is a torn frame, and the program's own model of the
  // screen is the only complete copy).
  //
  // The GRID has to move, not just the PTY: a size sent to the program alone leaves
  // xterm's grid saying one thing and the program's model another, and it paints its
  // frame a row short. Resizing the grid drives `terminal.onResize`, which sends the
  // SIGWINCH - terminal and program move together, exactly as in a real resize.
  function repaintAgent() {
    const grid = terminal();
    if (!grid || isRepainting) {
      return;
    }

    const { cols: columns, rows } = grid;
    // Both halves of the nudge drive terminal.onResize, which would otherwise queue another
    // repaint off the back of this one, forever.
    isRepainting = true;
    grid.resize(columns, Math.max(1, rows - 1));
    setTimeout(() => {
      // Come back to the size the pane implies NOW, not the one captured when the
      // nudge started. A fit that lands inside these 180ms - the flow control
      // parks a fit while the agent is repainting and applies it the moment the
      // agent goes quiet - would otherwise be overwritten by a stale row count,
      // and nothing re-fits afterwards because the pane's own size never changed
      // again. Measured on a freshly opened window: a grid left at 107 rows in a
      // 55-row pane, legible only because the squeeze scaled it to half height.
      const fitted = paneGrid() ?? {
        columns,
        rows
      };
      grid.resize(fitted.columns, fitted.rows);
      isRepainting = false;
    }, REPAINT_NUDGE_MS);
  }

  // Resize the grid on the ALTERNATE screen - at the pace the agent can actually paint.
  //
  // Only the agent can paint a row there, and it paints by diffing against its own model
  // of the screen. Move the grid faster than it can process the SIGWINCH and that model
  // starts describing a screen which no longer exists; from then on it writes only the
  // cells it *believes* changed, so the torn frame never repairs itself. Measured with a
  // fast drag: resizing every frame stopped it painting altogether - the pane went blank
  // and stayed blank, with the process still alive and still not drawing. A fixed
  // throttle only moves the cliff (100ms survived one drag, then wedged on the third).
  //
  // But freezing the grid for the whole gesture is what makes a TUI "only update when you
  // let go". So neither: **one resize in flight at a time.** Give the agent a size, wait
  // until it has finished painting it (its output goes quiet), and only then give it the
  // size the pane has reached in the meantime. The drag is paced by the agent itself - as
  // fast as it can actually follow, never faster.
  function altFit(size: GridSize) {
    const grid = terminal();
    if (!grid) {
      return;
    }

    const sinceLastFit = Date.now() - lastAltFitAt;
    if (!isAgentReadyForFit(sinceLastFit)) {
      parkFit({
        size,
        sinceLastFit
      });
      return;
    }

    if (isGridSized(grid, size)) {
      return;
    }

    lastAltFitAt = Date.now();
    isAwaitingRepaint = true;
    grid.resize(size.columns, size.rows);
    clearTimeout(repaintWatchdog);
    // It never answered. Let the next size through anyway; the settled gesture ends
    // with a forced repaint regardless (see handleGridResize).
    repaintWatchdog = setTimeout(finishRepaint, ALT_REPAINT_TIMEOUT_MS);
  }

  // The agent has finished painting the last size, and was not disturbed too recently.
  function isAgentReadyForFit(sinceLastFit: number) {
    return !isAwaitingRepaint && sinceLastFit >= ALT_FIT_MINIMUM_INTERVAL_MS;
  }

  // Hold a size the agent is not ready for. Nothing else will come back to collect it:
  // a drag that ends inside the interval has to have its last size land anyway.
  function parkFit({ size, sinceLastFit }: {
    size: GridSize;
    sinceLastFit: number;
  }) {
    pendingFit = size;
    clearTimeout(altFitTimer);
    altFitTimer = setTimeout(
      () => {
        const next = pendingFit;
        pendingFit = undefined;

        if (next && !isAwaitingRepaint) {
          altFit(next);
        }
      },
      Math.max(0, ALT_FIT_MINIMUM_INTERVAL_MS - sinceLastFit)
    );
  }

  // The agent has stopped talking, so the frame it was painting is done: let the next
  // size through, if the pane has moved on since.
  function finishRepaint() {
    clearTimeout(repaintQuietTimer);
    clearTimeout(repaintWatchdog);
    isAwaitingRepaint = false;

    const next = pendingFit;
    pendingFit = undefined;

    if (next) {
      // altFit re-parks it if the minimum interval has not elapsed yet.
      altFit(next);
    }
  }

  // Fit the grid to the pane, and re-pin it (above). No transform anywhere, so text
  // stays crisp and clicks map at native cell size. `terminal.dimensions.css.cell` is
  // the font metric, independent of the current grid, so there's no circular
  // measurement.
  function fitToPane() {
    const fitted = paneGrid();
    const grid = terminal();
    if (!grid || !fitted) {
      return;
    }

    if (isOnAlternateScreen) {
      fitAlternateScreen(fitted);
    } else {
      fitNormalScreen({
        grid,
        fitted
      });
    }
  }

  // The normal screen reflows every frame: xterm owns the document there, so it can
  // rewrap the text itself as fast as the drag moves.
  function fitNormalScreen({ grid, fitted }: {
    grid: Terminal;
    fitted: GridSize;
  }) {
    if (!isGridSized(grid, fitted)) {
      grid.resize(fitted.columns, fitted.rows);
    }

    updateAnchor();
  }

  function fitAlternateScreen(fitted: GridSize) {
    // The agent's frame only reaches the new size at the pace the agent can paint it, so
    // between here and there the grid can still be TALLER than the pane - and the grid is
    // pinned at the top, which would put the overflow past the bottom edge and cut the
    // agent's status line off. Nothing may ever cut that line. So for as long as the grid
    // is too big, it is squeezed to fit; the squeeze is never more than the lag, and goes
    // back to exactly 1 the moment the agent catches up - so a settled terminal is never
    // scaled, its text is crisp, and its clicks map true.
    updateSqueeze();
    altFit(fitted);
    updateAnchor();
  }

  return {
    get isOnAlternateScreen() {
      return isOnAlternateScreen;
    },
    get isBottomAnchored() {
      return isBottomAnchored;
    },
    get squeeze() {
      return squeeze;
    },
    fitToPane,
    repaintAgent,
    updateAnchor,
    /** The PTY was spawned this wide. */
    noteAgentColumns(columns: number) {
      agentColumns = columns;
    },
    /** Every chunk of output while a resize is in flight is the agent painting that
     *  resize. A gap in it means the frame is finished - that is the credit the next
     *  resize waits on. */
    noteRepaintProgress() {
      if (!isAwaitingRepaint) {
        return;
      }

      clearTimeout(repaintQuietTimer);
      repaintQuietTimer = setTimeout(finishRepaint, ALT_REPAINT_QUIET_MS);
    },
    /** xterm switched buffers. A program that takes over the alternate screen has to
     *  be told the moment the grid changes, and told the height too - it is painting
     *  the whole framebuffer, and nobody else can. Switching screens also makes the
     *  grid re-pin, and squares the agent's idea of the size with ours, since on the
     *  normal screen we deliberately let its height go stale. Returns whether the
     *  alternate screen is now up. */
    syncScreen(grid: Terminal) {
      isOnAlternateScreen = grid.buffer.active.type === Screen.Alternate;
      updateAnchor();

      if (isOnAlternateScreen) {
        sizeAgent({
          columns: grid.cols,
          rows: grid.rows
        });
      }

      return isOnAlternateScreen;
    },
    // Which of a resize's two numbers the agent hears depends on the screen it is on.
    //
    // ALTERNATE: both, immediately, every frame. There is no document behind that
    // screen and no scrollback - only the agent can paint a row - so a size it hasn't
    // been told is a row nobody paints (blank at the bottom, truncated at the top).
    // Never debounce this one.
    //
    // NORMAL: the width, once the drag settles, and NEVER the height. A CLI printing an
    // inline document needs the width - that is what its text wraps to. It does not
    // need the height: how much of a document you can see is the terminal's business,
    // and xterm already knows. Send the height anyway and every SIGWINCH makes it
    // re-render - which is what kept a step on screen:
    //
    //   - it re-lays-out its frame for the new row count and drops or adds a line, so
    //     the conversation above that line sits a full row off from the text below it.
    //     Not geometry: the document itself changing under us; and
    //   - Ink reprints its whole static history on a resize, stranding the previous
    //     copy in the scrollback (one per SIGWINCH - a fast drag once left 52).
    //
    // So a vertical drag there sends nothing at all: the agent's output is untouched
    // and xterm simply reveals more or less of it, exactly like scrolling a web page.
    handleGridResize(size: GridSize) {
      noteResize();
      // The grid just changed, which is the other half of what the squeeze measures.
      updateSqueeze();

      // Alternate screen: tell the agent at once - it owns every row, and a size it has
      // not heard is a row nobody paints. The grid only reaches it at a pace the agent
      // can keep up with in the first place (see altFit).
      //
      // Then, once the pane stops moving, ask for a full repaint - every time, not
      // only when we gave up waiting for one. Answering the SIGWINCH is not the same
      // as redrawing the whole screen: the agent erases the rows its own model says
      // it last wrote, and a size change is exactly what makes that model wrong, so
      // rows it no longer accounts for keep whatever they held. Measured on a live
      // Claude after the task dock opened: a tool result from an earlier frame
      // ("host … / packaged: no / this build: 1.1.0") sat under the live composer for
      // fifteen minutes of repaints, with the composer's own line ending in the tail
      // of the row underneath it. It never healed on its own; one resize did. The
      // nudge costs a double repaint at the end of a gesture - cheap next to a frame
      // that stays torn until the user happens to resize the pane again.
      if (isOnAlternateScreen) {
        clearTimeout(sigwinchTimer);
        sizeAgent(size);

        if (!isRepainting) {
          sigwinchTimer = setTimeout(repaintAgent, SIGWINCH_SETTLE_MS);
        }

        return;
      }

      if (size.columns === agentColumns) {
        return;
      }

      clearTimeout(sigwinchTimer);
      sigwinchTimer = setTimeout(() => sizeAgent(size), SIGWINCH_SETTLE_MS);
    },
    // Refit once per animation frame. On the normal screen that reflows the document
    // live as you drag, the way a web page does - xterm holds the text there, so it
    // can rewrap it itself. On the alternate screen fitToPane paces the grid to the
    // agent instead (the agent owns the pixels; see altFit). rAF coalesces a burst of
    // resize events into one fit per frame; xterm 6.1 renders the reflow
    // synchronously (issue #4922 / PR #5529) so it stays crisp.
    observePane(elViewport: HTMLElement) {
      resizeObserver = new ResizeObserver(() => {
        if (fitFrame !== undefined) {
          return;
        }

        fitFrame = requestAnimationFrame(() => {
          fitFrame = undefined;
          fitToPane();
        });
      });
      resizeObserver.observe(elViewport);
    },
    dispose() {
      clearTimeout(sigwinchTimer);
      clearTimeout(repaintQuietTimer);
      clearTimeout(repaintWatchdog);
      clearTimeout(altFitTimer);

      if (fitFrame !== undefined) {
        cancelAnimationFrame(fitFrame);
      }

      resizeObserver?.disconnect();
    }
  };
}
