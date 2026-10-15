<script lang="ts">
  import { pty } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { pastedText, referencedSnippet } from "@/lib/terminal/terminal-input";
  import { focusTerminalPane } from "@/lib/ui/focus";
  import { z } from "zod";

  // Highlight -> agent bridge: text selected in the side panel shows this
  // button, which types it into the active session's input. A selection inside
  // one code snippet is sent as a path:line reference to it.
  const { activeId }: {
    /** The session the selection is sent to. */
    activeId: string | null;
  } = $props();

  const SIDE_PANEL_SELECTOR = ".side-pane";
  const SNIPPET_LINE_SELECTOR = "[data-snippet-line]";
  const SNIPPET_PATH_SELECTOR = "[data-snippet-path]";
  const AFTER_PATH_SEPARATOR = /(?<=[\\/])/;
  const WHITESPACE = /\s/;
  // A selection that starts and ends on numbered lines of one snippet.
  const SnippetRange = z.object({
    path: z.string(),
    anchorLine: z.coerce.number().int(),
    focusLine: z.coerce.number().int()
  });

  let selection = $state("");
  const selectionSegments = $derived(
    selection.split(AFTER_PATH_SEPARATOR).map(text => ({
      text,
      isPathPart: !WHITESPACE.test(text)
    }))
  );

  function snippetLineElement(node: Node | null) {
    const element = node instanceof Element ? node : node?.parentElement;
    return element?.closest<HTMLElement>(SNIPPET_LINE_SELECTOR) ?? null;
  }

  function snippetReference({ selected, text }: {
    selected: Selection;
    text: string;
  }) {
    const elAnchor = snippetLineElement(selected.anchorNode);
    const elFocus = snippetLineElement(selected.focusNode);
    const elContainer = elAnchor?.closest<HTMLElement>(SNIPPET_PATH_SELECTOR);
    const isSameSnippet = elFocus?.closest<HTMLElement>(SNIPPET_PATH_SELECTOR) === elContainer;
    const parsed = SnippetRange.safeParse({
      path: elContainer?.dataset.snippetPath,
      anchorLine: elAnchor?.dataset.snippetLine,
      focusLine: elFocus?.dataset.snippetLine
    });
    if (!isSameSnippet || !parsed.success) {
      return text;
    }

    return referencedSnippet({
      ...parsed.data,
      snippet: text
    });
  }

  // What the current selection would send: empty unless it is non-blank text
  // inside the side panel.
  function isInSidePanel(node: Node | null) {
    return node !== null && document.querySelector(SIDE_PANEL_SELECTOR)?.contains(node) === true;
  }

  function sendableSelection() {
    const selected = getSelection();
    if (!selected || !isInSidePanel(selected.anchorNode)) {
      return "";
    }

    const text = selected.toString().trim();
    return text === "" ? "" : snippetReference({
      selected,
      text
    });
  }

  async function sendToAgent({ id, text }: {
    id: string;
    text: string;
  }) {
    await pty.write({
      id,
      data: pastedText(text)
    });
    getSelection()?.removeAllRanges();
    // The click moved focus to this button; hand it back to the agent so the
    // sent text can be submitted (or edited) without a click into the pane.
    focusTerminalPane(id);
  }
</script>

<svelte:document onselectionchange={() => (selection = sendableSelection())} />

{#if selection}
  <button
    class="send-fab"
    onclick={async () => {
      if (!activeId) {
        return;
      }

      await sendToAgent({
        id: activeId,
        text: selection
      });
      selection = "";
    }}
  >
    <Icon name="send" />
    <span class="label">Send to agent</span>
    <!-- A long path wraps only after a separator - each folder name stays
         whole (no split at a hyphen); the line cap is pure CSS. -->
    <span class="preview"
    >{#each selectionSegments as segment, i (i)}<span class:path-part={segment.isPathPart}
    >{segment.text}</span><wbr />{/each}</span
    >
  </button>
{/if}

<style>
  /* FAB entrance: drops in from below and bounces up into place. Bakes the
     -50% centering into every step since it animates `translate`. */
  @keyframes send-pop {
    0% {
      opacity: 0%;
      scale: 0.88;
      translate: -50% 12px;
    }

    65% {
      opacity: 100%;
      scale: 1.03;
      translate: -50% -4px;
    }

    100% {
      opacity: 100%;
      scale: 1;
      translate: -50% 0;
    }
  }

  .send-fab {
    position: fixed;
    inset-block-end: 26px;
    inset-inline-start: 50%;
    z-index: 80;
    display: inline-flex;
    gap: 10px;
    align-items: center;
    max-inline-size: min(880px, 100vw - 48px);
    padding: 12px 20px;
    border: none;
    border-radius: var(--radius-large);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 14px;
    text-align: start;
    box-shadow: 0 10px 30px var(--primary-shadow);
    cursor: pointer;
    translate: -50% 0;
    animation: send-pop 220ms var(--ease);

    .label {
      flex: none;
      white-space: nowrap;
    }

    /* Whole path on as many lines as it needs; a long code selection is capped
       at whole lines so the button never towers over the terminal. */
    .preview {
      overflow: hidden;
      max-block-size: 6lh;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 500;
      font-size: 12px;
      line-height: 1.45;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      opacity: 85%;

      .path-part {
        white-space: nowrap;
      }
    }
  }
</style>
