<script lang="ts">
  import { firstChangedLine } from "@/lib/code/diff";
  import { PreviewPane } from "@/lib/feed/feed-previews";
  import type { FeedPreviews } from "@/lib/feed/feed-previews";
  import { openAction, openChangedFile } from "@/lib/feed/open-change";
  import { isMediaPath, isRenderablePath } from "@/lib/feed/preview";
  import Icon from "@/lib/Icon.svelte";
  import { windowedEditorFor } from "@/lib/stores/workspace/editors.svelte";
  import { DiffStyle } from "@/lib/types";
  import type { ChangeEvent } from "@/lib/types";
  import { truncationTooltip } from "@/lib/ui/truncation-tooltip";
  import { relativeToRoot } from "@/lib/util/paths";
  import ChangePreview from "@/panels/feed/ChangePreview.svelte";
  import ViewToggles from "@/panels/feed/ViewToggles.svelte";
  import type { z } from "zod";

  // The open card's viewer: a bar (open-in-editor, the view toggles, close)
  // above the card's preview. Only one card is ever open.
  let { event, project, previews, diffMode = $bindable(), onClose }: {
    event: ChangeEvent;
    /** The open project's root dir - lets an editor open the change in the
     *  window that already has this project, at the right line. */
    project: string;
    previews: FeedPreviews;
    /** The feed-wide Unified|Split choice, kept across cards. */
    diffMode: z.infer<typeof DiffStyle>;
    onClose: () => void;
  } = $props();

  // The project's ranked editors come from the shared store (SSOT - the same
  // list the top-bar IdeMenu shows, so a reveal here and the launcher there can
  // never name different editors). The reveal uses the first *windowed* editor:
  // a console editor (Neovim/Vim/Helix) can't run detached - handing it to the
  // OS launcher spawns an invisible orphan that locks the workspace cwd - and
  // the feed has no terminal tab to route it into, so it's skipped here.
  const editor = $derived(windowedEditorFor({ path: project }));
  const action = $derived(
    openAction({
      path: event.path,
      editor
    })
  );
  const lines = $derived(previews.diffCache.get(event.id) ?? []);
  const revealLine = $derived(firstChangedLine(lines));
  const canPreview = $derived(isRenderablePath(event.path));
  const canSwitchDiffMode = $derived(
    !isMediaPath(event.path) && previews.paneOf(event.id) === PreviewPane.Code && lines.length > 0
  );

  // With its path clipped the tooltip carries action + full path; readable,
  // just the action hint - the thunks keep the ranked editor's name live past
  // the attachment's mount.
  function revealTooltip(path: string) {
    return truncationTooltip({
      tooltip: () => (action ? `${action} · ${path}` : path),
      restingTooltip: () => action,
      measureSelector: ".file-path"
    });
  }
</script>

<div class="bar">
  <button
    class="file-button"
    {@attach revealTooltip(event.path)}
    disabled={!action}
    onclick={() => openChangedFile({
      path: event.path,
      line: revealLine,
      project,
      editor
    })}
  >
    <Icon name="external" size={14} />
    <!-- Usually clipped in this narrow panel; the tooltip then carries
         the full path with the open-in action, and just the action when
         the path already reads in full - the shared CSS `[data-tooltip]`
         bubble, which caps at 320px + wraps and anchor-positions with a
         flip-up fallback. -->
    <span class="file-path">{relativeToRoot({
      path: event.path,
      root: project
    })}</span>
  </button>
  <span class="spacer"></span>
  {#if canPreview || canSwitchDiffMode}
    <ViewToggles {canPreview} {canSwitchDiffMode} {event} {previews} bind:diffMode />
  {/if}
  <button
    class="close"
    aria-label="Close diff"
    data-tooltip="Close"
    onclick={onClose}
  >
    <Icon name="close" size={14} />
  </button>
</div>

<ChangePreview
  {editor}
  {event}
  isSplit={diffMode === DiffStyle.enum.split}
  {lines}
  {previews}
  {project}
  {revealLine}
/>

<style>
  .bar {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding-block: 7px;
    padding-inline: 12px 8px;
    background: var(--surface-2);
  }

  .file-path {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-button {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    overflow: hidden;
    min-inline-size: 0;
    color: var(--primary);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.75rem;
    white-space: nowrap;
    cursor: pointer;

    &:hover:not(:disabled) .file-path {
      text-decoration: underline;
    }

    &:disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }
  }

  .spacer {
    flex: 1;
  }

  .close {
    display: inline-flex;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    block-size: 26px;
    inline-size: 26px;
    border-radius: var(--radius-full);
    color: var(--on-surface-variant);
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }
</style>
