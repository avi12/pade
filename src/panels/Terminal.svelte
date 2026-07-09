<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { pty } from "../lib/bridge";
  import SessionBadge from "../lib/SessionBadge.svelte";
  import type { AgentSession, SessionStatus } from "../lib/types";

  let { session }: { session: AgentSession } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let unlisten: UnlistenFn | undefined;
  let exitUnlisten: UnlistenFn | undefined;
  let resizeObs: ResizeObserver | undefined;

  // Session status. Output flowing = working; a quiet gap while the process is
  // alive = ready (done with its task, waiting for you); exit = done.
  let status = $state<SessionStatus>("starting");
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const IDLE_MS = 700;

  function markActivity() {
    if (status === "exited") return;
    status = "working";
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      if (status === "working") status = "ready";
    }, IDLE_MS);
  }

  onMount(async () => {
    term = new Terminal({
      fontFamily: '"JetBrains Mono", "Cascadia Code", ui-monospace, monospace',
      fontSize: 13,
      cursorBlink: true,
      allowProposedApi: true,
      theme: readXtermTheme(),
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);

    // GPU-accelerated rendering; fall back silently if WebGL is unavailable.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      term.loadAddon(webgl);
    } catch {
      /* CPU renderer is fine as a fallback */
    }

    fit.fit();

    // Stream this session's PTY output into the terminal; each chunk is a sign
    // of life that resets the idle → ready timer. Events are filtered by id so
    // sibling sessions don't cross-write.
    unlisten = await pty.onData((id, chunk) => {
      if (id !== session.id) return;
      term.write(chunk);
      markActivity();
    });
    exitUnlisten = await pty.onExit((id) => {
      if (id !== session.id) return;
      clearTimeout(idleTimer);
      status = "exited";
    });

    // Send keystrokes to this session's PTY.
    term.onData((data) => void pty.write(session.id, data));

    // Keep the PTY's window size in sync with the visible grid.
    term.onResize(({ cols, rows }) => void pty.resize(session.id, cols, rows));

    resizeObs = new ResizeObserver(() => fit.fit());
    resizeObs.observe(host);

    // Spawn the chosen agent in a real PTY.
    await pty.spawn(session.id, session.agent.command, term.cols, term.rows);

    // Seed a new-project first prompt into the input (typed, not submitted —
    // the user reviews and presses Enter).
    if (session.initialPrompt) await pty.write(session.id, session.initialPrompt);
  });

  onDestroy(() => {
    unlisten?.();
    exitUnlisten?.();
    clearTimeout(idleTimer);
    resizeObs?.disconnect();
    term?.dispose();
  });

  function readXtermTheme() {
    const s = getComputedStyle(document.documentElement);
    const v = (n: string) => s.getPropertyValue(n).trim();
    return {
      background: v("--code-bg"),
      foreground: v("--code-fg"),
      cursor: v("--primary"),
    };
  }
</script>

<div class="term-wrap">
  <header class="session-bar">
    <SessionBadge {status} label={session.agent.label} />
  </header>
  <div class="term-host" bind:this={host}></div>
</div>

<style>
  .term-wrap { display: flex; flex-direction: column; block-size: 100%; }
  .session-bar {
    display: flex;
    align-items: center;
    padding-block: 8px;
    padding-inline: 14px;
    background: var(--surface-1);
    border-block-end: 1px solid var(--outline);
  }
  .term-host {
    flex: 1;
    min-block-size: 0;
    inline-size: 100%;
    padding: 8px 10px;
    background: var(--code-bg);
  }
</style>
