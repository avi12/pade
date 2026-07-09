<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { pty } from "../lib/bridge";

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let unlisten: UnlistenFn | undefined;
  let resizeObs: ResizeObserver | undefined;

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

    // Stream PTY output from the Rust core into the terminal.
    unlisten = await pty.onData((chunk) => term.write(chunk));

    // Send keystrokes to the PTY.
    term.onData((data) => void pty.write(data));

    // Keep the PTY's window size in sync with the visible grid.
    term.onResize(({ cols, rows }) => void pty.resize(cols, rows));

    resizeObs = new ResizeObserver(() => fit.fit());
    resizeObs.observe(host);

    // Spawn the agent CLI (defaults to the platform shell) in a real PTY.
    await pty.spawn(term.cols, term.rows);
  });

  onDestroy(() => {
    unlisten?.();
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

<div class="term-host" bind:this={host}></div>

<style>
  .term-host {
    height: 100%;
    width: 100%;
    padding: 8px 10px;
    background: var(--code-bg);
  }
</style>
