<script lang="ts">
  import type { DiffLine } from "@/lib/code/diff";
  import DiffView from "@/lib/code/DiffView.svelte";
  import { openChangedFile } from "@/lib/feed/open-change";
  import type { Ide } from "@/lib/types";
  import { Key } from "@/lib/ui/keyboard/keys";

  // An open card's diff. Clicking it (or Enter on it) opens the file in the
  // editor, jumped to the clicked - else the first changed - line. The launcher
  // hands the file to the already-open editor when one is running, so it
  // navigates there in place.
  const { path, project, editor, lines, revealLine, isSplit }: {
    path: string;
    project: string;
    editor: Ide | undefined;
    lines: DiffLine[];
    /** Where a reveal lands when no particular line was clicked. */
    revealLine: number | undefined;
    isSplit: boolean;
  } = $props();

  const revealTip = $derived.by(() => {
    if (!editor) {
      return "No editor detected";
    }

    return `Reveal in ${editor.label}`;
  });

  // The new-file line a click landed on, read from the diff row it hit.
  function clickedLine(target: EventTarget | null) {
    const lineElement = target instanceof Element ? target.closest<HTMLElement>("[data-newline]") : null;
    return lineElement ? Number(lineElement.dataset.newline) : undefined;
  }
</script>

<div
  class="preview"
  data-snippet-path={path}
  data-tooltip={revealTip}
  onclick={async e => {
    // A drag to select text (for send-to-agent) must not also open the file.
    const isSelectingText = getSelection()?.isCollapsed === false;
    if (isSelectingText) {
      return;
    }

    await openChangedFile({
      path,
      line: clickedLine(e.target) ?? revealLine,
      project,
      editor
    });
  }}
  onkeydown={async e => {
    if (e.key === Key.Enter) {
      await openChangedFile({
        path,
        line: revealLine,
        project,
        editor
      });
    }
  }}
  role="button"
  tabindex="0"
>
  <DiffView diffLines={lines} split={isSplit} />
</div>

<style>
  /* Scroll container around the shared DiffView; the click-to-reveal cursor
     and cap live here, the line rendering in DiffView. */
  .preview {
    overflow: auto;
    max-block-size: 300px;
    padding-block: 8px;
    background: var(--code-background);
    cursor: pointer;

    /* Scopes the unified↔split View Transition to the diff body (only one card
       is ever open, so the name is unique) - the rest of the app stays still. */
    view-transition-name: diff-body;
  }

  /* Fades and slides up as it replaces the rendered preview - pure CSS on the
     entering element (never a View Transition, which would snapshot and ghost
     the sandboxed preview iframe). */
  @media (prefers-reduced-motion: no-preference) {
    .preview {
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
