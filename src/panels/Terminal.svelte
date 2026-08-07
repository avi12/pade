<script lang="ts">
  // Read docs/terminal-rendering.md BEFORE changing resize/replay behavior here.
  // A terminal has TWO screens and they invert every rule: the normal screen
  // (scrollback document; never send height, debounce width) vs the alternate
  // screen (fullscreen framebuffer; send both sizes, serialize refits, ask the
  // program to repaint). `onAlternateScreen` is the flag; the doc is the policy.
  import { AgentId } from "@/lib/agent-icon";
  import { clipboard, os, pty } from "@/lib/bridge";
  import { afterDelay } from "@/lib/delay";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import type { DragHint } from "@/lib/drag-reorder";
  import { isEditingText } from "@/lib/focus";
  import Icon from "@/lib/Icon.svelte";
  import { isTrustGate, promptEchoed } from "@/lib/initial-prompt";
  import { appearance, effective } from "@/lib/prefs.svelte";
  import SessionBadge from "@/lib/SessionBadge.svelte";
  import { observeApiError } from "@/lib/stores/apiErrorRetry.svelte";
  import { dropContext, observeContext, observeContextScreen, seedContextWindow } from "@/lib/stores/context.svelte";
  import { dropMcpReload, observeMcpReload } from "@/lib/stores/mcpReload.svelte";
  import { setSessionStatus } from "@/lib/stores/sessions.svelte";
  import { showToast } from "@/lib/stores/toast.svelte";
  import { observeUsageLimit } from "@/lib/stores/usageResume.svelte";
  import { colorSchemeReport, enablesColorSchemeNotifications } from "@/lib/terminal-color-scheme";
  import { isPromptNewlineShortcut, pastedText, PROMPT_NEWLINE } from "@/lib/terminal-input";
  import { terminalLinkDestination, TerminalLinkTarget } from "@/lib/terminal-link-target";
  import { registerWrappedLinkProvider } from "@/lib/terminal-links";
  import { terminalFlushMode, TerminalFlushMode, wheelScrollsTerminalDocument } from "@/lib/terminal-output";
  import { accumulateWheelNotches } from "@/lib/terminal-scroll";
  import { xtermTheme } from "@/lib/terminal-theme";
  import { SessionStatus } from "@/lib/types";
  import type { AgentSession, PtyChunk } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onDestroy, onMount } from "svelte";

  const {
    session, active = false, shown = true, removable = false,
    onremove, onpopout, onreorder, onexit, ondraghint
  }: {
    session: AgentSession;
    /** The session the keyboard belongs to — the one tab (or split pane) in front. */
    active?: boolean;
    /** This pane is in the current split (laid out and visible). A background
        tab's pane stays mounted at full size but `visibility: hidden`, so only
        `shown` — never geometry — can tell the two apart for cursor blinking. */
    shown?: boolean;
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
  let terminal: Terminal;
  let unlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;
  let resizeObserver: ResizeObserver | undefined;
  // Guards the async onMount against a teardown that runs before its awaits
  // settle: onDestroy sets this, and each awaited step bails so no listener is
  // registered after unmount and no write hits a disposed terminal.
  let destroyed = false;
  // PTY invokes are asynchronous IPC messages. Keep one write in flight and
  // merge input that arrives while it crosses the process boundary: normal
  // keystrokes still start immediately, while paste/repeat bursts avoid a costly
  // invoke per character and cannot overtake one another.
  let ptyWriteInFlight = false;
  let queuedPtyWrites: Array<{
    data: string;
    settle: () => void;
  }> = [];
  // The terminal exists and is attached — reactive, so the focus effect below can
  // wait for it (the terminal is built inside an async onMount, well after the
  // first effects have already run).
  let attached = $state(false);
  let windowFocused = $state(document.hasFocus());
  // xterm's DOM renderer is the remaining output hot path. Join token-sized PTY
  // chunks before writing: one write per display frame in front, and a bounded
  // live cadence in the background so another app never competes with dozens of
  // terminal layouts and paints per second.
  let queuedTerminalOutput = "";
  let terminalOutputFrame: number | undefined;
  let terminalOutputTimer: ReturnType<typeof setTimeout> | undefined;
  const BACKGROUND_TERMINAL_FLUSH_MS = 250;
  const CONTEXT_SCREEN_SAMPLE_MS = 1000;
  let contextScreenTimer: ReturnType<typeof setTimeout> | undefined;
  let lastContextScreenSampleAt = 0;
  const SCROLL_OUTPUT_QUIET_MS = 120;
  const SCROLL_OUTPUT_MAX_DELAY_MS = 1000;
  let activelyScrolling = false;
  let scrollOutputQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let scrollOutputDeadlineTimer: ReturnType<typeof setTimeout> | undefined;

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>(SessionStatus.enum.starting);
  let idleTimer: ReturnType<typeof setTimeout> | undefined;

  // Initial-prompt delivery (see lib/initial-prompt). A fresh agent gates on a
  // "trust this folder?" prompt before its input line is live, so the first
  // prompt can't just be fired on mount — it would land in the menu or sit unsent.
  // Instead we watch the stream: accept the trust gate the moment it appears,
  // then once the agent has settled quiet at its REPL, paste the prompt and
  // submit it. Both steps run at most once.
  let promptDelivered = false;
  let trustAccepted = false;
  // A rolling tail of recent output, so the ready-time check can tell whether the
  // trust gate is still on screen — a backstop for a gate frame that arrived split
  // across chunks and slipped the per-chunk check.
  let recentOutput = "";
  const RECENT_OUTPUT_CAP = 8_000;

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
      columns: number;
      rows: number;
    }
    | undefined;
  let repaintQuietTimer: ReturnType<typeof setTimeout> | undefined;
  let repaintWatchdog: ReturnType<typeof setTimeout> | undefined;
  let missedRepaint = false;
  // A repaint nudge resizes the grid itself, so it comes back through the resize path —
  // this is what stops it queueing another repaint off the back of its own.
  let repainting = false;
  // How long the PTY must fall quiet before a working session settles back to
  // "ready". This is a hysteresis window, not just a debounce: it must comfortably
  // outlast the *slowest periodic repaint* a long-running task emits, or the badge
  // flickers. A dev server started by the agent (e.g. `pnpm netlify` → Vite) keeps
  // Claude repainting its elapsed-time counter ("1m 1s", "1m 2s", …) about once a
  // second and dribbles the odd log line — if this window were shorter than that
  // ~1 s cadence, the status would drop to "ready" in the gap after each tick and
  // snap back to "working" on the next, a ~1 Hz flicker. Sitting above 1 s, one
  // periodic tick re-arms the timer before it fires, so the session stays steadily
  // "working" for the whole command and only settles ~1.5 s after output truly
  // stops. The cost is the "ready" badge on a normal quiet turn appears ~0.8 s
  // later than the old 700 ms — an acceptable trade for no flicker.
  const IDLE_MS = 1500;
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
  // agent never needs to hear about (see terminal.onResize).
  let agentColumns = 0;
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

  // A PTY read can cut a control sequence in half, so retain one complete
  // sequence's worth of tail while looking for the agent's DECSET 2031 handshake
  // (Claude's `auto` theme subscribes). Any agent that enables the channel gets
  // the truthful scheme report — the handshake itself is the opt-in, not the
  // agent's id.
  const COLOR_SCHEME_ENABLE_LENGTH = "\x1b[?2031h".length;
  let colorSchemeNotificationTail = "";
  let colorSchemeNotificationsEnabled = false;

  // Agents whose TUI subscribes to DEC 2031 color-scheme reports at startup
  // (Claude's `auto` theme, opencode's renderer). The re-attach path treats
  // them as subscribed even when the handshake was trimmed out of the replayed
  // history. (The report only carries the scheme; opencode's own light/dark
  // *detection* is spawn-themed instead — see agents.rs — because its mode
  // probe is answered by ConPTY, not by ADE.)
  const SCHEME_SUBSCRIBED_AGENTS = new Set<string>([AgentId.Claude, AgentId.Opencode]);

  // Agents whose Shift+Enter newline must arrive as a raw Ctrl+J (LF, 0x0a)
  // rather than a bracketed paste of it (see the Shift+Enter branch below):
  //  • Claude Code — Ctrl+J is its documented always-works insert-newline; a
  //    bracketed paste of a lone LF does NOT register in its composer.
  //  • OpenCode — reacts to ANY paste event by consulting the OS clipboard (a
  //    lingering image would attach alongside the newline); a raw LF is its own
  //    ctrl+j insert-newline binding, so the newline lands without a paste.
  const NEWLINE_VIA_RAW_LF_AGENTS = new Set<string>([AgentId.Claude, AgentId.Opencode]);

  async function writeSchemeReport() {
    await writeToPty(colorSchemeReport(appearance.scheme));
  }

  // Recover the context window from the session's model when its startup banner
  // isn't in the replay (a long re-attached session). The backend reads the model
  // off disk and sizes it from the live models.dev catalog. `seedContextWindow`
  // never overrides a window the banner did supply, so a fresh session's live
  // reading always wins. On any failure the window simply stays unknown — the
  // gauge reads "measuring…", never a wrong number.
  async function seedContextWindowFromModel() {
    try {
      const windowTokens = await pty.contextWindow({
        command: session.agent.command,
        conversationId: session.conversationId
      });
      if (windowTokens !== null) {
        seedContextWindow({
          id: session.id,
          windowTokens
        });
      }
    } catch {
    // Leave the window unknown; a guessed number could end a session wrongly.
    }
  }

  async function reportSchemeToSubscribedAgent() {
    const listensWithoutHandshake =
      !colorSchemeNotificationsEnabled && SCHEME_SUBSCRIBED_AGENTS.has(session.agent.id);
    if (!listensWithoutHandshake) {
      return;
    }

    colorSchemeNotificationsEnabled = true;
    await writeSchemeReport();
  }

  function relayColorSchemeAfterSubscribe(data: string) {
    if (colorSchemeNotificationsEnabled) {
      return;
    }

    const combined = colorSchemeNotificationTail + data;
    colorSchemeNotificationTail = combined.slice(-(COLOR_SCHEME_ENABLE_LENGTH - 1));

    if (!enablesColorSchemeNotifications(combined)) {
      return;
    }

    colorSchemeNotificationsEnabled = true;
    writeSchemeReport();
  }

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

  // Carriage return — the "Enter" a CLI reads as "submit this line".
  const ENTER = "\r";

  // ^W (ETB) — the line discipline's "erase word backward". Sent for
  // Ctrl+Backspace, whose legacy byte (^H) a TUI can't tell from a bare
  // backspace; see the key-handler comment.
  const ERASE_WORD = "\x17";

  // Focus reports (mode 1004) xterm emits when the pane gains/loses DOM focus.
  // PADE never forwards them: a PADE pane is either front (focused) or hidden
  // (where unfocused chrome is invisible anyway), and every tab switch would
  // otherwise make the outgoing agent repaint its chrome — output the status
  // heuristic above has no way to tell from real work, flashing a ready
  // session's badge to "working". Agents simply always render as focused.
  const FOCUS_IN_FINAL_BYTE = "I";
  const FOCUS_OUT_FINAL_BYTE = "O";
  const FOCUS_IN = `${CONTROL_SEQUENCE_INTRODUCER}${FOCUS_IN_FINAL_BYTE}`;
  const FOCUS_OUT = `${CONTROL_SEQUENCE_INTRODUCER}${FOCUS_OUT_FINAL_BYTE}`;

  // Mouse-tracking mode xterm reports when no program has grabbed the mouse.
  const NO_MOUSE_TRACKING = "none";

  // How soon after a local keystroke an output chunk still reads as the prompt
  // echoing that keystroke back. The echo round-trips in a few ms; the next
  // chunk after typing pauses longer than this is genuinely the agent.
  const KEYSTROKE_ECHO_MS = 250;
  let lastKeystrokeAt = 0;

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

    // Typing at the prompt echoes every keystroke back through the PTY. That
    // echo is the user composing, not the agent working, so a settled session
    // must not flash its indicator per keypress. Enter never stamps the
    // window (see terminal.onData), so a submitted prompt flips to working
    // immediately.
    const isKeystrokeEcho =
      status === SessionStatus.enum.ready && Date.now() - lastKeystrokeAt < KEYSTROKE_ECHO_MS;
    if (isKeystrokeEcho) {
      return;
    }

    status = SessionStatus.enum.working;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      if (status === SessionStatus.enum.working) {
        status = SessionStatus.enum.ready;
        deliverInitialPromptIfReady();
      }
    }, IDLE_MS);
  }

  // Watch a fresh session's boot output for the first-run trust gate and accept
  // its default ("Yes, I trust this folder") the instant it appears, so the
  // prompt delivered next lands at the REPL and not in the menu. Runs only while
  // a first prompt is still pending, and accepts at most once.
  function watchInitialPrompt(data: string) {
    if (!session.initialPrompt || promptDelivered) {
      return;
    }

    recentOutput = (recentOutput + data).slice(-RECENT_OUTPUT_CAP);

    if (!trustAccepted && isTrustGate(recentOutput)) {
      trustAccepted = true;
      recentOutput = ""; // the gate's gone once accepted — don't re-trip on its stale frame
      writeToPty(ENTER);
    }
  }

  // A freshly-spawned TUI can paint its whole splash while not yet READING
  // stdin (OpenCode/Claude in a brand-new workspace do first-run setup after
  // first paint), so a prompt written the moment the output settles can be
  // silently swallowed. Delivery is therefore verified: after each attempt the
  // output is watched for the prompt echoing back in the composer, and until it
  // does the attempt repeats on a short timer — a deliberate poll, since no
  // event announces "the TUI now reads input". Bounded so an agent that renders
  // the prompt unrecognizably can't be pasted at forever.
  // Longer than IDLE_MS (1500): a submit's paste-echo alone flips the session to
  // `working`, but with nothing behind it the idle timer settles it back to
  // `ready` at 1500ms. Verifying at 2000ms therefore tells the two apart — still
  // `working` here means the agent is genuinely processing the prompt, not just
  // echoing the paste.
  const PROMPT_ECHO_VERIFY_MS = 2_000;
  // Let a fresh bracketed paste land in the composer before the submitting Enter,
  // so the TUI's post-paste guard doesn't fold the CR into the paste burst and
  // leave the prompt sitting unsent (a combined paste+CR is exactly what stranded
  // a handoff successor's prompt, typed but never submitted).
  const PROMPT_SUBMIT_SETTLE_MS = 1_200;
  const PROMPT_MAX_ATTEMPTS = 8;
  let promptAttempts = 0;
  // A submit is written and waiting for its 2000ms verdict. The idle handler
  // fires its own re-check at IDLE_MS (1500) — before the verdict — so it must
  // not re-enter delivery meanwhile, or it would re-paste (or prematurely latch)
  // while the verify timer is still deciding. The verify callback owns the retry.
  let awaitingVerify = false;
  let promptVerifyTimer: ReturnType<typeof setTimeout> | undefined;

  // Once the agent has settled quiet — done booting, past the trust gate — paste
  // the first prompt, then submit it and re-attempt until the agent is seen
  // working. The bracketed paste keeps the prompt's own newlines soft; the
  // submitting ENTER is sent separately (a raw write folds the CR into the paste
  // and leaves the prompt unsent) and re-sent on its own if the TUI's post-paste
  // guard swallowed the first one.
  async function deliverInitialPromptIfReady() {
    if (!session.initialPrompt || promptDelivered || awaitingVerify) {
      return;
    }

    // Backstop: a gate frame split across chunks can slip watchInitialPrompt but
    // still sits in the rolling tail — accept it here and wait for the next settle
    // rather than pasting into the menu.
    if (isTrustGate(recentOutput)) {
      if (!trustAccepted) {
        trustAccepted = true;
        await writeToPty(ENTER);
      }

      recentOutput = "";
      return;
    }

    if (promptAttempts >= PROMPT_MAX_ATTEMPTS) {
      promptDelivered = true;
      clearTimeout(promptVerifyTimer);
      return;
    }

    // The prompt echoing back means the PASTE reached the composer — NOT that it
    // was submitted. A TUI's post-paste guard (Claude Code's especially) swallows
    // the ENTER that rides in the same burst as the bracketed paste, leaving the
    // text sitting unsent; latching on the echo alone is what stranded a handoff
    // successor with its prompt typed but never sent, so the user had to press
    // Enter by hand. A prompt already in the composer is therefore submitted with a
    // SEPARATE ENTER — a standalone keystroke the guard can't fold into the paste —
    // re-sent until the agent is seen working; only a prompt that never echoed is
    // (re)pasted. `working` past the paste-echo settle (the verify below) is the
    // one signal it truly went in.
    const pasteLanded = promptEchoed({
      output: recentOutput,
      prompt: session.initialPrompt
    });
    promptAttempts += 1;
    awaitingVerify = true;

    if (pasteLanded) {
      // The paste already reached the composer; only its submitting Enter was
      // swallowed, so re-send that alone — never re-paste over the standing text.
      await writeToPty(ENTER);
    } else {
      // Fresh delivery: paste, let the composer SETTLE, then submit with a
      // SEPARATE Enter. A one-shot paste+CR rides a single burst and the TUI's
      // post-paste guard eats the CR; the settle is what makes the Enter land on
      // this first attempt instead of leaning on the retry/verify to recover.
      await writeToPty(pastedText(session.initialPrompt));
      await afterDelay(PROMPT_SUBMIT_SETTLE_MS);
      await writeToPty(ENTER);
    }

    // Verify shortly. Still `working` at 2000ms means the agent took the prompt
    // up and is running it — delivery is done, so latch it (this is what stops a
    // handoff successor, whose first turn scrolls the echo out of `recentOutput`,
    // from being re-prompted). Back at `ready` means it wasn't submitted — the
    // bundled Enter was swallowed, or the TUI wasn't reading yet — so try again
    // (a separate Enter when the paste already landed, else a fresh paste).
    clearTimeout(promptVerifyTimer);
    promptVerifyTimer = setTimeout(() => {
      awaitingVerify = false;

      if (status === SessionStatus.enum.working) {
        promptDelivered = true;
        return;
      }

      deliverInitialPromptIfReady();
    }, PROMPT_ECHO_VERIFY_MS);
  }

  // Publish status to the shared store so the top-bar tab shows a matching dot.
  $effect(() => {
    setSessionStatus({
      id: session.id,
      status
    });
  });

  // xterm's font-size option is in px, so the CSS --ui-scale does not reach it.
  const TERMINAL_FONT_SIZE = 13;

  // WCAG AA for body text — the floor xterm holds every foreground to against
  // the themed background (see the Terminal options).
  const MINIMUM_CONTRAST_RATIO = 4.5;

  // Live-update the terminal font (family + zoom) when the preference changes.
  $effect(() => {
    const family = effective.monospaceFamily;
    const fontSize = Math.round(TERMINAL_FONT_SIZE * effective.uiScale);
    if (!terminal) {
      return;
    }

    terminal.options.fontFamily = family;
    terminal.options.fontSize = fontSize;
    fitToPane();
  });

  // Re-theme xterm whenever the app scheme changes. The terminal palette changes
  // in place, so it is safe for every live session. An agent that subscribed to
  // DECSET 2031 color-scheme reports (Claude's `auto` theme, opencode's `system`
  // theme) also gets the standard report relayed through its PTY after
  // repainting: xterm does not emit one merely because `options.theme` changed.
  // It reaches the already-running process and redraws its own TUI without a
  // restart or context loss.
  $effect(() => {
    // Reading the scheme is what subscribes this effect to it, so a light/dark
    // flip re-runs and re-reads the palette; readXtermTheme pulls the live CSS
    // tokens the flipped scheme installed.
    const { scheme } = appearance;
    if (!terminal || !scheme) {
      return;
    }

    terminal.options.theme = readXtermTheme();

    if (colorSchemeNotificationsEnabled) {
      writeSchemeReport();
    }
  });

  // Hand the keyboard to the session in front — the moment it launches, and again
  // whenever the user switches to it. Nothing else claims focus for a terminal, so
  // without this the keystrokes go to whatever the user last clicked (the tab, the
  // agent button in onboarding) and the agent looks like it is ignoring you: you
  // have to click into the pane before it will hear a single key.
  $effect(() => {
    if (active && attached) {
      terminal.focus();
    }
  });

  // Returning to this window — the Ctrl+Alt+[ / ] window cycle, an alt-tab, a
  // taskbar click — puts the OS focus on the window but not back on the terminal,
  // so the agent would ignore your first keystrokes until you click the pane.
  // Re-grab the keyboard for the front session on every window focus, so a cycled
  // window is immediately promptable. Skipped while the user is mid-typing in a
  // real field (a rename box, the picker) — that focus must be left alone; the
  // xterm helper textarea deliberately doesn't count as editing (see lib/focus).
  function handleWindowFocus() {
    windowFocused = true;
    scheduleTerminalOutputFlush();

    if (active && attached && !isEditingText(document.activeElement)) {
      terminal.focus();
    }
  }

  function handleWindowBlur() {
    windowFocused = false;
  }

  $effect(() => {
    addEventListener("focus", handleWindowFocus);
    addEventListener("blur", handleWindowBlur);
    return () => {
      removeEventListener("focus", handleWindowFocus);
      removeEventListener("blur", handleWindowBlur);
    };
  });

  // A hidden or unfocused terminal keeps consuming every PTY chunk, but it does
  // not need a cursor-blink paint loop or foreground output cadence until the
  // user can see it again.
  $effect(() => {
    if (!attached) {
      return;
    }

    terminal.options.cursorBlink = shown && windowFocused;
    scheduleTerminalOutputFlush();
  });

  function flushTerminalOutput() {
    terminalOutputFrame = undefined;
    terminalOutputTimer = undefined;

    if (!queuedTerminalOutput || destroyed || !terminal) {
      return;
    }

    const output = queuedTerminalOutput;
    queuedTerminalOutput = "";
    terminal.write(output, scheduleContextScreenObservation);
    noteRepaintProgress();
  }

  function scheduleTerminalOutputFlush() {
    if (!queuedTerminalOutput) {
      return;
    }

    const mode = terminalFlushMode({
      shown,
      windowFocused,
      readingScrollback: isReadingScrollback(),
      scrolling: activelyScrolling
    });
    if (mode === TerminalFlushMode.Deferred) {
      cancelScheduledTerminalOutput();
      return;
    }

    if (mode === TerminalFlushMode.Background) {
      scheduleBackgroundTerminalOutput();
      return;
    }

    scheduleForegroundTerminalOutput();
  }

  function cancelScheduledTerminalOutput() {
    if (terminalOutputFrame !== undefined) {
      cancelAnimationFrame(terminalOutputFrame);
      terminalOutputFrame = undefined;
    }

    clearTimeout(terminalOutputTimer);
    terminalOutputTimer = undefined;
  }

  function finishTerminalScroll() {
    activelyScrolling = false;
    scrollOutputQuietTimer = undefined;
    clearTimeout(scrollOutputDeadlineTimer);
    scrollOutputDeadlineTimer = undefined;
    flushTerminalOutput();
  }

  function flushLongTerminalScroll() {
    scrollOutputDeadlineTimer = undefined;
    flushTerminalOutput();
  }

  function noteTerminalScroll() {
    activelyScrolling = true;
    cancelScheduledTerminalOutput();
    clearTimeout(scrollOutputQuietTimer);
    scrollOutputQuietTimer = setTimeout(finishTerminalScroll, SCROLL_OUTPUT_QUIET_MS);
    scrollOutputDeadlineTimer ??= setTimeout(
      flushLongTerminalScroll,
      SCROLL_OUTPUT_MAX_DELAY_MS
    );
  }

  function isReadingScrollback(): boolean {
    if (!terminal || terminal.buffer.active.type === Screen.Alternate) {
      return false;
    }

    const buffer = terminal.buffer.active;
    return buffer.viewportY < buffer.baseY;
  }

  function scheduleForegroundTerminalOutput() {
    clearTimeout(terminalOutputTimer);
    terminalOutputTimer = undefined;
    terminalOutputFrame ??= requestAnimationFrame(flushTerminalOutput);
  }

  function scheduleBackgroundTerminalOutput() {
    if (terminalOutputFrame !== undefined) {
      cancelAnimationFrame(terminalOutputFrame);
      terminalOutputFrame = undefined;
    }

    terminalOutputTimer ??= setTimeout(flushTerminalOutput, BACKGROUND_TERMINAL_FLUSH_MS);
  }

  function queueTerminalOutput(output: string) {
    queuedTerminalOutput += output;
    scheduleTerminalOutputFlush();
  }

  // Context is derived from agent output, not viewport movement. Sampling after
  // xterm has processed a write keeps mouse-wheel/scrollbar renders presentation-
  // only while still reading the complete screen that cursor-motion TUIs paint.
  function observeContextScreenAfterWrite() {
    contextScreenTimer = undefined;

    if (destroyed || !terminal) {
      return;
    }

    lastContextScreenSampleAt = Date.now();
    const buffer = terminal.buffer.active;
    const rows: string[] = [];
    for (let row = 0; row < terminal.rows; row += 1) {
      const line = buffer.getLine(buffer.viewportY + row);
      if (line) {
        rows.push(line.translateToString(true));
      }
    }

    observeContextScreen({
      id: session.id,
      text: rows.join("\n")
    });
  }

  function scheduleContextScreenObservation() {
    const elapsed = Date.now() - lastContextScreenSampleAt;
    if (elapsed >= CONTEXT_SCREEN_SAMPLE_MS) {
      clearTimeout(contextScreenTimer);
      observeContextScreenAfterWrite();
      return;
    }

    contextScreenTimer ??= setTimeout(
      observeContextScreenAfterWrite,
      CONTEXT_SCREEN_SAMPLE_MS - elapsed
    );
  }

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
    anchorBottom = !onAlternateScreen && (terminal?.buffer.active.baseY ?? 0) > 0;
  }

  // How much a grid that is momentarily too tall for its pane has to be scaled by to fit
  // inside it — 1 whenever it fits, which is every settled state.
  //
  // It has to be recomputed both when the PANE moves (the drag) and when the GRID does
  // (the agent catching up, which lands later, on its own schedule): either one closes
  // the gap, and only redoing it on both is what brings the scale back to exactly 1 when
  // they meet.
  function updateSqueeze() {
    const cell = terminal?.dimensions?.css.cell;
    if (!terminal || !viewport || !cell || !(cell.height > 0)) {
      return;
    }

    const gridHeight = terminal.rows * cell.height;
    const paneHeight = viewport.clientHeight;
    squeeze = gridHeight > paneHeight && gridHeight > 0 ? paneHeight / gridHeight : 1;
  }

  // The one place that tells the PTY how big it is (DRY — both resize paths and the
  // screen switch go through here). Remembering the width we sent is what lets a
  // height-only change on the normal screen be dropped entirely. Stamp the time so the
  // repaint the agent sends back isn't counted as activity (see markActivity).
  function sizeAgent({
    columns,
    rows
  }: {
    columns: number;
    rows: number;
  }) {
    lastResizeAt = Date.now();
    agentColumns = columns;
    resizePty({
      columns,
      rows
    });
  }

  // Fire-and-forget resize: the session may have exited between measuring and
  // resizing, and a dropped resize on a dead PTY is harmless.
  async function resizePty({
    columns,
    rows
  }: {
    columns: number;
    rows: number;
  }) {
    try {
      await pty.resize({
        id: session.id,
        cols: columns,
        rows
      });
    } catch {
      // A failed IPC write means this WebView can no longer reach the session;
      // reflect that rather than pretending the grid is still live.
      status = SessionStatus.enum.exited;
    }
  }

  // Serializes input across the IPC boundary. The first event in a burst begins
  // immediately; events that land while it is in flight share the next invoke.
  // Each caller's promise settles after the backend accepted its bytes (or the
  // endpoint is known gone), so the boot trust-gate and initial-prompt sequence
  // remains ordered without leaving an awaiter stranded on a failed transport.
  function writeToPty(data: string): Promise<void> {
    return new Promise(settle => {
      queuedPtyWrites.push({
        data,
        settle
      });

      if (!ptyWriteInFlight) {
        flushPtyWrites();
      }
    });
  }

  async function flushPtyWrites(): Promise<void> {
    ptyWriteInFlight = true;
    while (queuedPtyWrites.length > 0) {
      const writes = queuedPtyWrites;
      queuedPtyWrites = [];
      try {
        await pty.write({
          id: session.id,
          data: writes.map(write => write.data).join("")
        });
      } catch {
        // No later queued input can reach an unavailable transport. Resolve every
        // waiter and publish the terminal's terminal state instead of retaining
        // invisible promises or retrying an already-disconnected IPC channel.
        status = SessionStatus.enum.exited;
        const pendingWrites = queuedPtyWrites;
        queuedPtyWrites = [];
        for (const write of pendingWrites) {
          write.settle();
        }
      } finally {
        for (const write of writes) {
          write.settle();
        }
      }
    }
    ptyWriteInFlight = false;
  }

  // Send the current selection to the clipboard. Copy is best-effort — a
  // clipboard the platform withholds must not throw into a key handler.
  async function copySelectionToClipboard() {
    try {
      await clipboard.writeText(terminal.getSelection());
    } catch {
      showToast("Couldn't copy the terminal selection.");
    }
  }

  // Open a terminal hyperlink in the appropriate OS surface. Codex uses OSC-8
  // `file:` links for paths it reports; those are local folders/files, not URLs
  // the browser bridge accepts. Web links retain the browser path.
  async function openTerminalLink(uri: string) {
    const destination = terminalLinkDestination(uri);
    if (!destination) {
      showToast("Couldn't open that link.");
      return;
    }

    try {
      if (destination.kind === TerminalLinkTarget.explorer) {
        await os.explorer(destination.value);
      } else {
        await os.openUrl(destination.value);
      }
    } catch {
      showToast("Couldn't open that link.");
    }
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
  // frame a row short. Resizing the grid drives `terminal.onResize`, which sends the
  // SIGWINCH — terminal and program move together, exactly as in a real resize.
  function repaintAgent() {
    if (!terminal || repainting) {
      return;
    }

    const grid = terminal;
    const { cols: columns, rows } = grid;
    // Both halves of the nudge drive terminal.onResize, which would otherwise queue another
    // repaint off the back of this one, forever.
    repainting = true;
    grid.resize(columns, Math.max(1, rows - 1));
    setTimeout(() => {
      grid.resize(columns, rows);
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
    columns,
    rows
  }: {
    columns: number;
    rows: number;
  }) {
    if (!terminal) {
      return;
    }

    const sinceLastFit = Date.now() - lastAltFitAt;
    if (awaitingRepaint || sinceLastFit < ALT_FIT_MIN_INTERVAL_MS) {
      pendingFit = {
        columns,
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

    if (columns === terminal.cols && rows === terminal.rows) {
      return;
    }

    lastAltFitAt = Date.now();
    awaitingRepaint = true;
    terminal.resize(columns, rows);
    clearTimeout(repaintWatchdog);
    repaintWatchdog = setTimeout(() => {
      // It never answered. Whatever is on screen may be torn, so the gesture owes a full
      // repaint once it stops (see terminal.onResize).
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
  // `terminal.dimensions.css.cell` is the font metric, independent of the current grid,
  // so there's no circular measurement.
  function fitToPane() {
    if (!terminal || !viewport) {
      return;
    }

    const liveCell = terminal.dimensions?.css.cell;
    const liveCellIsUsable = liveCell !== undefined && liveCell.width > 0 && liveCell.height > 0;
    const cell = liveCellIsUsable ? liveCell : undefined;
    if (!cell) {
      return;
    }

    const availableWidth = viewport.clientWidth - SCROLLBAR_WIDTH;
    const columns = Math.floor(availableWidth / cell.width);
    const rows = Math.floor(viewport.clientHeight / cell.height);
    // A dock/panel can briefly leave its flex sibling with no measured space
    // while the browser resolves the new layout. Never clamp that transient to
    // a 2x1 terminal: Codex treats it as a real resize and its TUI can become
    // permanently desynchronised. Keep the last truthful grid until the pane
    // has enough cells to be usable again.
    const MIN_USABLE_COLS = 20;
    const MIN_USABLE_ROWS = 4;
    if (columns < MIN_USABLE_COLS || rows < MIN_USABLE_ROWS) {
      return;
    }

    const grid = terminal;
    // The normal screen reflows every frame: xterm owns the document there, so it can
    // rewrap the text itself as fast as the drag moves.
    if (!onAlternateScreen) {
      if (columns !== grid.cols || rows !== grid.rows) {
        grid.resize(columns, rows);
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
      columns,
      rows
    });
    updateAnchor();
  }

  onMount(async () => {
    terminal = new Terminal({
      fontFamily: effective.monospaceFamily,
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
        activate(_event, uri) {
          openTerminalLink(uri);
        }
      }
    });
    terminal.open(host);
    attached = true;

    // Make URLs in the output clickable — the agent's OAuth sign-in links, docs
    // pointers. xterm's stock web-links addon only rejoins soft-wrapped rows, so
    // a URL a fullscreen agent hard-wraps at the edge would open truncated; this
    // provider stitches those rows too (see terminal-links). The default handler
    // is window.open, which a Tauri WebView won't turn into a browser tab, so
    // route the whole URL through the bridge to the system browser instead.
    registerWrappedLinkProvider({
      terminal,
      openUrl: openTerminalLink
    });

    fitToPane();

    // The context gauge must parse the SCREEN after output, not just the stream.
    // Scroll renders are unrelated viewport movement and deliberately do no work.
    // A fullscreen agent's renderer skips cells that are already painted with a
    // cursor-forward, so the wire can carry "97% contex" + CSI 1C + " used"
    // and the phrase the parser needs never arrives intact. The rendered rows
    // always hold the full text. The output-write callback above samples the
    // complete viewport at a bounded cadence once xterm has applied those edits.

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
      if (destroyed || !terminal) {
        return;
      }

      queueTerminalOutput(chunk.data);
      relayColorSchemeAfterSubscribe(chunk.data);
      markActivity();
      // Track how full this agent's context window is (drives auto-handoff).
      // Only on the normal screen: a fullscreen agent repaints its whole frame
      // on every spinner tick, so counting those bytes balloons the estimate
      // to a meaningless 100% within minutes (a freshly handed-off successor
      // read "≈100%"). On the alternate screen the onRender scan above carries
      // the parsed percent, and no estimate beats a saturated one.
      const onNormalScreen = terminal.buffer.active.type !== "alternate";
      if (onNormalScreen) {
        observeContext({
          id: session.id,
          chunk: chunk.data
        });
      } else {
        // Parse-only on the alternate screen: the window banner ("(1M
        // context)") paints once at spawn and then scrolls out of the visible
        // frame, so the render scan alone would never learn the window size
        // and the gauge would sit on "measuring…" forever. Split-word parses
        // simply miss here; the render scan still carries those.
        observeContextScreen({
          id: session.id,
          text: chunk.data
        });
      }

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
      observeMcpReload({
        id: session.id,
        chunk: chunk.data
      });
      // Accept the first-run trust gate so the pending first prompt can land.
      watchInitialPrompt(chunk.data);
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
      clearTimeout(promptVerifyTimer);
      status = SessionStatus.enum.exited;
      onexit?.(session.id);
    });
    if (destroyed) {
      exitListener();
      return;
    }

    exitUnlisten = exitListener;

    // Send keystrokes to this session's PTY.
    terminal.onData(data => {
      const isFocusReport = data === FOCUS_IN || data === FOCUS_OUT;
      if (isFocusReport) {
        return;
      }

      // Composing keystrokes arm the echo window so their round-trip doesn't
      // flash the status; the submitting Enter deliberately does not — the
      // burst it triggers is the agent starting real work.
      if (data !== ENTER) {
        lastKeystrokeAt = Date.now();
      }

      writeToPty(data);
    });

    async function pasteClipboard() {
      try {
        // An image on the clipboard beats text: the backend saves it as a PNG
        // and the pasted *path* is what agent composers attach. The trailing
        // space makes the path a complete token for the agent's parser.
        const imagePath = await clipboard.saveImage();
        if (imagePath) {
          terminal.paste(`${imagePath} `);
          return;
        }

        const text = await clipboard.readText();
        if (text) {
          // paste (not write) so xterm wraps it in bracketed-paste markers when the
          // agent has that mode on — it then treats it as pasted text, not typing.
          terminal.paste(text);
        }
      } catch {
        showToast("Couldn't read the clipboard.");
      }
    }

    // Keyboard overrides layered on xterm's own handling; returning false stops
    // xterm from also sending the key's control code.
    //  • Shift+Enter → paste a protected prompt newline instead of submitting.
    //  • Ctrl+C → copy the selection; with nothing selected it falls through so
    //    xterm still sends ^C (SIGINT) to interrupt the agent.
    //  • Ctrl+V → paste the clipboard (xterm would otherwise send a raw ^V, and
    //    only the WebView's right-click menu pasted).
    //  • Ctrl+Backspace → ^W (erase word). xterm's legacy encoding for the
    //    chord is ^H, indistinguishable from a bare backspace to a TUI reading
    //    plain bytes (Windows Terminal gets away with it via win32-input-mode,
    //    which xterm doesn't speak); ^W is the erase-word every line editor
    //    and agent already binds.
    terminal.attachCustomKeyEventHandler(event => {
      if (event.type !== "keydown") {
        return true;
      }

      if (isPromptNewlineShortcut(event)) {
        // preventDefault stops the browser inserting a newline into xterm's hidden
        // textarea, which xterm would forward to the PTY as a submit.
        event.preventDefault();

        if (NEWLINE_VIA_RAW_LF_AGENTS.has(session.agent.id)) {
          // A raw Ctrl+J (LF) is the reliable insert-newline for these agents —
          // Claude Code's always-works binding, and OpenCode's, without tripping
          // its clipboard-on-paste. A bracketed paste of a lone LF is silently
          // dropped by Claude's composer, which is why Shift+Enter did nothing.
          writeToPty(PROMPT_NEWLINE);
          return false;
        }

        // `paste` is deliberate: when any agent's TUI enables bracketed paste,
        // xterm marks this as pasted text and the newline stays in its composer.
        // Unlike CSI-u, this does not depend on an individual agent decoding a
        // particular modified-key protocol.
        terminal.paste(PROMPT_NEWLINE);
        return false;
      }

      const isPlainCtrl = event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey;

      const isCopyChord = isPlainCtrl && (event.key === "c" || event.key === "C");
      if (isCopyChord && terminal.hasSelection()) {
        event.preventDefault();
        copySelectionToClipboard();
        return false;
      }

      const isPasteChord = isPlainCtrl && (event.key === "v" || event.key === "V");
      if (isPasteChord) {
        event.preventDefault();
        pasteClipboard();
        return false;
      }

      const isDeleteWordChord = isPlainCtrl && event.key === "Backspace";
      if (isDeleteWordChord) {
        event.preventDefault();
        writeToPty(ERASE_WORD);
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
    terminal.attachCustomWheelEventHandler(e => {
      const agentOwnsMouse = terminal.modes.mouseTrackingMode !== NO_MOUSE_TRACKING;
      const hasNativeScrollback = terminal.buffer.active.baseY > 0;
      // Only a tick that moves xterm's own document may hold output back; a tick
      // the agent receives as input must not, or the repaint that IS the scroll
      // is what gets deferred (see wheelScrollsTerminalDocument).
      if (wheelScrollsTerminalDocument({
        agentOwnsMouse,
        hasNativeScrollback
      })) {
        noteTerminalScroll();
      }

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
        writeToPty(scrollKey.repeat(Math.abs(notches)));
      }

      e.preventDefault();
      return false;
    });

    // The document can outgrow the pane with no resize involved — the agent simply
    // prints past the last row — and that is the moment the grid must re-pin.
    terminal.onScroll(() => {
      updateAnchor();
      scheduleTerminalOutputFlush();
    });

    // A program that takes over the alternate screen has to be told the moment the
    // grid changes, and told the height too — it is painting the whole framebuffer,
    // and nobody else can. Switching screens also makes the grid re-pin, and squares
    // the agent's idea of the size with ours, since on the normal screen we deliberately
    // let its height go stale (below).
    terminal.buffer.onBufferChange(() => {
      onAlternateScreen = terminal.buffer.active.type === Screen.Alternate;
      updateAnchor();

      if (onAlternateScreen) {
        sizeAgent({
          columns: terminal.cols,
          rows: terminal.rows
        });
        reportSchemeToSubscribedAgent();
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
    terminal.onResize(({ cols: columns, rows }) => {
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
          columns,
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

      if (columns === agentColumns) {
        return;
      }

      clearTimeout(sigwinchTimer);
      sigwinchTimer = setTimeout(
        () =>
          sizeAgent({
            columns,
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
    resizeObserver = new ResizeObserver(() => {
      if (fitFrame !== undefined) {
        return;
      }

      fitFrame = requestAnimationFrame(() => {
        fitFrame = undefined;
        fitToPane();
      });
    });
    resizeObserver.observe(viewport);

    // Spawn the chosen agent in a real PTY.
    if (destroyed) {
      return;
    }

    agentColumns = terminal.cols;
    await pty.spawn({
      id: session.id,
      command: session.agent.command,
      cwd: session.cwd ?? null,
      cols: agentColumns,
      rows: terminal.rows,
      args: session.args,
      scheme: appearance.scheme,
      conversationId: session.conversationId
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
    if (destroyed || !terminal) {
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
        terminal.write(ENTER_ALTERNATE_SCREEN);
      }

      terminal.write(history.data);
      relayColorSchemeAfterSubscribe(history.data);
      // The replayed history holds the spawn-time banner (window size) and the
      // latest counters a fresh attach would otherwise never see — parse-only,
      // never the byte estimate: these are replayed bytes, not new output.
      observeContextScreen({
        id: session.id,
        text: history.data
      });

      // A session with history was already running before this terminal
      // attached — any handshake happened at its own startup, usually trimmed
      // out of the replay, so the watcher above never sees it. Report the
      // current scheme to the agents known to listen: their spawn-time
      // COLORFGBG (or a spawn that predates it) may disagree with the palette
      // this app is painting.
      await reportSchemeToSubscribedAgent();

      // A session with history to replay was already running before this
      // terminal attached — it sits at its prompt, not "starting". A
      // normal-screen session at rest says nothing more after the replay
      // (replayed bytes never pass through markActivity), so left unseeded the
      // status would read "starting" forever and the graceful leave gating on
      // idleness would treat a perfectly quiet session as busy. Fresh output
      // still flips it to "working" through the live-chunk path.
      if (status === SessionStatus.enum.starting) {
        status = SessionStatus.enum.ready;
      }
    }

    for (const chunk of pendingChunks) {
      if (chunk.seq > history.seq) {
        consume(chunk);
      }
    }

    pendingChunks.length = 0;
    replayed = true;

    // Fill the context window from the model if the replay's banner didn't (a
    // long re-attached session); the seed no-ops when the banner already set it.
    seedContextWindowFromModel();

    if (history.alternate) {
      repaintAgent();
    }

  // A new-project first prompt is NOT sent here: a fresh agent gates on a
    // "trust this folder?" prompt before its input is live, so it's delivered by
    // watchInitialPrompt / deliverInitialPromptIfReady once the agent has
    // accepted that gate and settled at its REPL (see lib/initial-prompt).
  });

  onDestroy(() => {
    destroyed = true;
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);
    clearTimeout(promptVerifyTimer);
    clearTimeout(sigwinchTimer);
    clearTimeout(repaintQuietTimer);
    clearTimeout(repaintWatchdog);
    clearTimeout(altFitTimer);
    clearTimeout(terminalOutputTimer);
    clearTimeout(contextScreenTimer);
    clearTimeout(scrollOutputQuietTimer);
    clearTimeout(scrollOutputDeadlineTimer);

    if (fitFrame !== undefined) {
      cancelAnimationFrame(fitFrame);
    }

    if (terminalOutputFrame !== undefined) {
      cancelAnimationFrame(terminalOutputFrame);
    }

    resizeObserver?.disconnect();
    dropContext(session.id);
    dropMcpReload(session.id);
    terminal?.dispose();
  });

  // The DOM read lives here; the token→slot mapping and the parse-safe color
  // conversion (xterm silently drops formats its parser rejects) are the pure
  // module's job — lib/terminal-theme, which also documents the app↔agent
  // theme-sync decision.
  function readXtermTheme() {
    const style = getComputedStyle(document.documentElement);
    return xtermTheme({ readToken: name => style.getPropertyValue(name).trim() });
  }
</script>

<div class="terminal-wrapper">
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
  <div class="terminal-padding">
    <div bind:this={viewport} class="terminal-viewport" class:anchor-bottom={anchorBottom}>
      <div bind:this={host} style:scale={`1 ${squeeze}`} class="terminal-host"></div>
    </div>
  </div>
</div>

<style>
  .terminal-wrapper {
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
  .terminal-padding {
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

    /* xterm mounts here at its natural whole-cell size, scaled only while a grid that is
       momentarily too tall for the pane is being squeezed to fit (see `squeeze`) — from
       the top, so the squeeze pulls the overflowing bottom up into view rather than
       moving the text you are reading. At rest the scale is exactly 1: text stays crisp
       and clicks map at native cell size. */
    .terminal-host {
      flex: none;
      transform-origin: top left;
    }
  }
</style>
