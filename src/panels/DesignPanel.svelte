<script lang="ts">
  import { design } from "../lib/bridge";
  import { onDestroy, onMount } from "svelte";

  // Docks a design tool's live web UI in a native Tauri child webview that the
  // Rust side positions over this host div. This element is just a placeholder:
  // it reserves the layout box and reports its screen rect; the real UI is the
  // native webview stacked on top (iframes are impossible — the tools all send
  // X-Frame-Options). A ResizeObserver + window resize keep the webview aligned
  // to the pane, and onDestroy parks it off-screen so the session survives.
  const { url }: { url: string } = $props();

  let host: HTMLDivElement;

  // The tool's host name, shown in the loading hint under the (soon-covering)
  // webview — e.g. "claude.ai".
  const hostName = $derived.by(() => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  });

  // Map the host div's box to logical bounds for the native webview. Rounded so
  // the webview lands on whole pixels and never leaks a seam past the pane.
  function rect() {
    const box = host.getBoundingClientRect();
    return {
      x: Math.round(box.left),
      y: Math.round(box.top),
      width: Math.round(box.width),
      height: Math.round(box.height)
    };
  }

  // (Re)dock at the current url. Measure after a frame so layout has settled
  // before the first embed — positioning is an independent side effect.
  $effect(() => {
    const target = url;
    requestAnimationFrame(() => void design.embed({
      url: target,
      ...rect()
    }));
  });

  let resizeObs: ResizeObserver | undefined;
  function sync() {
    void design.setBounds(rect());
  }

  onMount(() => {
    resizeObs = new ResizeObserver(sync);
    resizeObs.observe(host);
    window.addEventListener("resize", sync);
  });

  onDestroy(() => {
    resizeObs?.disconnect();
    window.removeEventListener("resize", sync);
    void design.close();
  });
</script>

<div bind:this={host} class="design-host">
  <p class="hint">Loading {hostName}…</p>
</div>

<style>
  .design-host {
    display: grid;
    place-items: center;
    block-size: 100%;
    inline-size: 100%;
    background: var(--surface);
  }

  .hint {
    margin: 0;
    color: var(--on-surface-var);
    font-size: 13px;
    letter-spacing: 0.02em;
  }
</style>
