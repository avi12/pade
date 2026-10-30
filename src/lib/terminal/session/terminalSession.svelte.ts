// One pane's live terminal session: the xterm instance, the agent's PTY behind
// it, and everything that runs between the two - spawning or re-attaching,
// replaying history, streaming output, input, resizing, and status. The pane
// component (panels/Terminal) owns only the view and its reactive glue.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { appearance } from "@/lib/settings/prefs.svelte";
import { observeTurnOnScreen } from "@/lib/stores/agent/agentActivity.svelte";
import { observeContextScreen } from "@/lib/stores/agent/context.svelte";
import { clearPendingPrompt, pendingPrompt } from "@/lib/stores/session/pendingPrompts.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { observeUsageLimitScreen } from "@/lib/stores/usage/usageResume.svelte";
import { createSessionActivity } from "@/lib/terminal/activity/sessionActivity.svelte";
import { createTerminalFit, Screen } from "@/lib/terminal/fit/terminalFit.svelte";
import { listenForFileDrops } from "@/lib/terminal/input/file-drop";
import { createInitialPromptDelivery } from "@/lib/terminal/input/initial-prompt-delivery";
import { pathsAsPromptText } from "@/lib/terminal/input/terminal-input";
import { attachKeyOverrides, attachWheelForwarding } from "@/lib/terminal/input/terminal-keys";
import { ENTER_ALTERNATE_SCREEN, isFocusReport } from "@/lib/terminal/input/terminal-sequences";
import { observeAgentOutput, observeReplayedOutput } from "@/lib/terminal/output/agent-output";
import { createScreenSampler, viewportRows } from "@/lib/terminal/output/screen-sampler";
import { createTerminalOutputScheduler } from "@/lib/terminal/output/terminal-output-scheduler";
import { createPtyStream } from "@/lib/terminal/pty/pty-stream";
import { createPtyWriter } from "@/lib/terminal/pty/pty-writer";
import { containsRtl } from "@/lib/terminal/rtl/terminal-rtl";
import { createTerminal } from "@/lib/terminal/session/create-terminal";
import { createSessionReadings } from "@/lib/terminal/session/session-readings";
import { colorSchemeReport, createColorSchemeRelay } from "@/lib/terminal/theme/terminal-color-scheme";
import { SessionStatus } from "@/lib/types";
import type { AgentSession, PtyChunk } from "@/lib/types";
import { errorMessage } from "@/lib/util/errors/errors";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Terminal } from "@xterm/xterm";

const CONTEXT_SCREEN_SAMPLE_MS = 1000;

export function createTerminalSession({ session, view, openFind, onexit }: {
  session: () => AgentSession;
  /** Whether the pane is laid out and visible, and its window focused. */
  view: () => {
    isShown: boolean;
    isWindowFocused: boolean;
  };
  /** Open the pane's find bar (Ctrl+F). */
  openFind: () => unknown;
  /** The PTY exited on its own. */
  onexit: (id: string) => void;
}) {
  let terminal: Terminal | undefined;
  let elViewport: HTMLElement | undefined;
  const listeners: UnlistenFn[] = [];
  // Guards the async start against a teardown that runs before its awaits
  // settle: dispose sets this, and each awaited step bails so no listener is
  // registered after unmount and no write hits a disposed terminal.
  let isDestroyed = false;
  // A file dragged from Explorer or an IDE is hovering over this pane.
  let isFileDragOver = $state(false);
  // This session has printed right-to-left text at least once, so its rows are
  // worth scanning (see lib/terminal/rtl/TerminalRtl). A latch, never unset: the text
  // stays in the scrollback. Until it flips, an English-only session does no
  // scanning and renders no overlay at all.
  let isRightToLeftSeen = $state(false);

  const activity = createSessionActivity({ onSettled: () => delivery.deliverIfReady() });
  const writeToPty = createPtyWriter({
    send: data => sendMessage(Command.ptyWrite, {
      id: session().id,
      data
    }),
    onTransportLost: () => activity.markExited()
  });
  // A new-project first prompt is NOT sent on mount: a fresh agent gates on a
  // "trust this folder?" prompt before its input is live, so it's delivered once
  // the agent has accepted that gate and settled at its REPL.
  const delivery = createInitialPromptDelivery({
    prompt: () => pendingPrompt(session().id),
    write: writeToPty,
    isWorking: () => activity.status === SessionStatus.enum.working,
    settle: () => clearPendingPrompt(session().id)
  });
  const colorScheme = createColorSchemeRelay({
    agentId: () => session().agent.id,
    report: () => writeToPty(colorSchemeReport(appearance.terminalScheme))
  });
  const readings = createSessionReadings(session);
  const fit = createTerminalFit({
    terminal: () => terminal,
    viewport: () => elViewport,
    // Fire-and-forget resize: the session may have exited between measuring and
    // resizing, and a dropped resize on a dead PTY is harmless.
    async resizeAgent({ columns, rows }) {
      try {
        await sendMessage(Command.ptyResize, {
          id: session().id,
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
    sample() {
      if (!terminal) {
        return;
      }

      const rows = viewportRows(terminal);
      delivery.answerTrustGate(rows);
      const screen = rows.join("\n");
      observeContextScreen({
        id: session().id,
        text: screen
      });
      observeUsageLimitScreen({
        id: session().id,
        text: screen
      });
      observeTurnOnScreen({
        id: session().id,
        text: screen
      });
    }
  });
  const output = createTerminalOutputScheduler({
    write(queued) {
      terminal?.write(queued, () => screenSampler.request());
      fit.noteRepaintProgress();
    },
    flushContext() {
      const buffer = terminal?.buffer.active;
      const isReadingScrollback = buffer !== undefined && buffer.type !== Screen.Alternate
        && buffer.viewportY < buffer.baseY;
      return {
        ...view(),
        isReadingScrollback
      };
    }
  });

  function watchForRightToLeft(text: string) {
    // An agent that reorders its own output has already done the browser's job,
    // and doing it twice reads backwards - so for those the latch never flips and
    // the overlay never exists (see `reordersBidi` in agents.rs).
    if (isRightToLeftSeen || session().agent.reordersBidi) {
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

  // Stream this session's PTY output into the terminal; each chunk is a sign of
  // life that resets the idle → ready timer. Parked until the history replay is
  // in (see lib/terminal/pty/pty-stream).
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
      id: session().id,
      chunk: chunk.data,
      isOnNormalScreen: terminal.buffer.active.type !== Screen.Alternate
    });
    // Accept the first-run trust gate so the pending first prompt can land.
    delivery.watchOutput(chunk.data);
  }

  const stream = createPtyStream(consume);

  // Subscribe to this session's events. Events are filtered by id so sibling
  // sessions don't cross-write. False when the pane was torn down meanwhile.
  async function listenToSession({ grid, elPane }: {
    grid: Terminal;
    elPane: HTMLElement;
  }) {
    const isDataListening = keepListening([
      await onMessage(EventType.ptyData, ({ data: chunk }) => {
        if (chunk.id === session().id) {
          stream.accept(chunk);
        }
      })
    ]);
    if (!isDataListening) {
      return false;
    }

    const isExitListening = keepListening([
      await onMessage(EventType.ptyExit, ({ data: { id } }) => {
        if (id !== session().id) {
          return;
        }

        delivery.cancel();
        activity.markExited();
        onexit(id);
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
        element: () => elPane,
        onHoverChange(isOver) {
          isFileDragOver = isOver;
        },
        onDrop(paths) {
          grid.paste(pathsAsPromptText(paths));
          grid.focus();
        }
      })
    );
    return !isDestroyed;
  }

  // Wire the terminal's own events: keystrokes out to the PTY, the key and wheel
  // overrides, and every change of scroll, screen and size.
  function attachTerminalEvents({ grid, elPane }: {
    grid: Terminal;
    elPane: HTMLElement;
  }) {
    grid.onData(data => {
      if (isFocusReport(data)) {
        return;
      }

      activity.noteInput(data);
      writeToPty(data);
    });
    attachKeyOverrides({
      terminal: grid,
      agentId: () => session().agent.id,
      write: writeToPty,
      openFind
    });
    attachWheelForwarding({
      terminal: grid,
      write: writeToPty,
      onDocumentScroll: () => output.noteScroll()
    });

    // The document can outgrow the pane with no resize involved - the agent simply
    // prints past the last row - and that is the moment the grid must re-pin.
    grid.onScroll(() => {
      fit.updateAnchor();
      output.schedule();
    });
    grid.buffer.onBufferChange(() => {
      if (fit.syncScreen(grid)) {
        colorScheme.reportToSubscribedAgent();
      }
    });
    grid.onResize(({ cols: columns, rows }) => fit.handleGridResize({
      columns,
      rows
    }));
    fit.observePane(elPane);
  }

  // Spawn the chosen agent in a real PTY. False when it failed to start, or the
  // pane was torn down meanwhile.
  async function spawnAgent(grid: Terminal) {
    const { id, agent, cwd, args, conversationId, modelChoice } = session();
    fit.noteAgentColumns(grid.cols);
    // A spawn that fails used to abandon the rest of this mount silently, and the
    // pane sat on "Starting…" for as long as the app ran - no process, no error,
    // no way to tell a wedged session from a slow one. Say what happened and mark
    // the session not-running, so the pane stops lying and a graceful leave never
    // waits on it.
    try {
      await sendMessage(Command.ptySpawn, {
        id,
        command: agent.command,
        cwd: cwd ?? null,
        cols: grid.cols,
        rows: grid.rows,
        args,
        scheme: appearance.terminalScheme,
        conversationId,
        modelChoice
      });
    } catch (error) {
      activity.markExited();
      showToast(
        `Couldn't start ${agent.label} - ${errorMessage({
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
  // and go in after it (see lib/terminal/pty/pty-stream).
  async function replayHistory(grid: Terminal) {
    const history = await sendMessage(Command.ptyHistory, { id: session().id });
    if (isDestroyed) {
      return;
    }

    if (history.data) {
      await paintHistory({
        grid,
        data: history.data,
        alternate: history.alternate
      });
    }

    stream.goLive(history.seq);

    if (history.alternate) {
      fit.repaintAgent();
    }
  }

  async function paintHistory({ grid, data, alternate }: {
    grid: Terminal;
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
      grid.write(ENTER_ALTERNATE_SCREEN);
    }

    grid.write(data);
    colorScheme.observe(data);
    watchForRightToLeft(data);
    observeReplayedOutput({
      id: session().id,
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

  /** Listen, spawn (or re-attach to) the agent, and replay what it already said. */
  async function start({ grid, elPane }: {
    grid: Terminal;
    elPane: HTMLElement;
  }) {
    const isListening = await listenToSession({
      grid,
      elPane
    });
    if (!isListening) {
      return;
    }

    attachTerminalEvents({
      grid,
      elPane
    });
    const isSpawned = await spawnAgent(grid);
    if (!isSpawned) {
      return;
    }

    await replayHistory(grid);
  }

  return {
    activity,
    colorScheme,
    fit,
    output,
    readings,
    /** The xterm instance once opened - deliberately not reactive. */
    get terminal() {
      return terminal;
    },
    get isFileDragOver() {
      return isFileDragOver;
    },
    get isRightToLeftSeen() {
      return isRightToLeftSeen;
    },
    /** Build the terminal into `elHost`, fitted to the measuring frame `elPane`. */
    open({ elHost, elPane }: {
      elHost: HTMLElement;
      elPane: HTMLElement;
    }) {
      const grid = createTerminal(elHost);
      terminal = grid;
      elViewport = elPane;
      fit.fitToPane();
      return grid;
    },
    start,
    dispose() {
      isDestroyed = true;
      for (const stopListening of listeners) {
        stopListening();
      }
      activity.dispose();
      delivery.cancel();
      fit.dispose();
      output.dispose();
      screenSampler.dispose();
      terminal?.dispose();
    }
  };
}
