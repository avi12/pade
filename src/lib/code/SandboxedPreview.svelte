<script lang="ts">
  // A rendered markdown/HTML document (lib/code/markdown's `markdownDocument` /
  // `sandboxedHtmlDocument`), quarantined in a sandboxed iframe with NO
  // allow-scripts and NO allow-same-origin: it runs no scripts, has an opaque
  // origin, and can't reach the parent - the web analog of PowerToys'
  // locked-down WebView2. `srcdoc` keeps it fully inline (no external ref under
  // the CSP). The frame is `--preview-height` tall (360px unless the parent says).
  const { document, label }: {
    document: string;
    /** The frame's accessible name. */
    label: string;
  } = $props();
</script>

<iframe class="render" aria-label={label} sandbox="" srcdoc={document}></iframe>

<style>
  /* Themed background shows while the isolated document loads (and behind a
     transparent HTML body); the document itself carries its own light/dark CSS. */
  .render {
    display: block;
    block-size: var(--preview-height, 360px);
    inline-size: 100%;
    border: 0;
    background: var(--surface-1);
  }

  /* Fades and slides up as it replaces the source view - pure CSS on the
     entering element (never a View Transition, which would snapshot and ghost
     the iframe). */
  @media (prefers-reduced-motion: no-preference) {
    .render {
      @starting-style {
        opacity: 0%;
        translate: 0 8px;
      }

      transition:
        opacity 200ms var(--ease),
        translate 200ms var(--ease);
    }
  }
</style>
