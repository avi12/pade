<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { observeProjectBranch, projectBranch } from "@/lib/stores/workspace/projectBranches.svelte";
  import { truncationTooltip } from "@/lib/ui/truncation-tooltip";
  import { displayName, isTemporaryWorkspace } from "@/lib/util/paths";
  import LastAccessed from "@/lib/workspace/LastAccessed.svelte";
  import ProjectKindIcon from "@/lib/workspace/ProjectKindIcon.svelte";
  import TempBadge from "@/lib/workspace/TempBadge.svelte";

  // A pinned/recent switcher row's main button: kind icon, name, branch chip,
  // path and last-accessed age, with a check on the window's current project.
  const { project, labels, isCurrent, openedAt, onpick }: {
    project: string;
    labels: Record<string, string>;
    /** This window's own project - wears the primary rail and the check. */
    isCurrent: boolean;
    /** When the switcher opened - the instant the "last accessed" age is measured against. */
    openedAt: number;
    /** The row was clicked - the click carries the modifier keys. */
    onpick: (e: MouseEvent) => void;
  } = $props();

  const branch = $derived(projectBranch(project));
</script>

<button
  class="project-row-main"
  class:current={isCurrent}
  {@attach observeProjectBranch({ path: project })}
  aria-checked={isCurrent}
  data-arrow-stop
  onclick={onpick}
  role="menuitemradio"
  type="button"
>
  <ProjectKindIcon path={project} />
  <span class="project-row-body">
    <span class="project-row-name-line">
      <span class="project-row-name">{displayName({
        path: project,
        labels
      })}</span>
      {#if isTemporaryWorkspace(project)}
        <TempBadge />
      {/if}
    </span>
    <span class="project-row-metadata">
      {#if branch}
        <span class="branch">
          <span class="branch-icon" aria-hidden="true"><Icon name="branch" size={11} /></span>
          {branch}
        </span>
      {/if}
      <span
        class="project-row-path" {@attach truncationTooltip({
          tooltip: project
        })}>{project}</span>
      <LastAccessed now={openedAt} path={project} />
    </span>
  </span>
  {#if isCurrent}
    <span class="project-row-check" aria-hidden="true"><Icon name="check" size={15} /></span>
  {/if}
</button>

<style>
  .project-row-main {
    display: flex;
    flex: 1;
    gap: 9px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px;
    padding-inline: 8px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    /* The current project keeps a primary rail on its left edge. */
    &.current {
      box-shadow: inset 3px 0 0 0 var(--primary);
    }

    &:hover,
    &:focus-visible {
      /* The kind icon falls back to the muted ink when a project has no
         detected language (a plain folder), and that ink was picked against the
         row's resting surface - on this fill it is dim at best, and invisible
         where the container is dark. Re-point the token it already reads rather
         than overriding its colour, so a real brand mark (Rust's orange, Go's
         cyan) still wears its own. */
      --on-surface-variant: var(--on-primary-container);

      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }
  }

  .project-row-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
    line-height: 1.25;
  }

  .project-row-name-line {
    display: flex;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    .project-row-name {
      overflow: hidden;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .project-row-metadata {
    --last-accessed-font-size: 9px;

    display: flex;
    gap: 7px;
    align-items: center;
    min-inline-size: 0;

    .branch {
      display: inline-flex;
      flex: none;
      gap: 4px;
      align-items: center;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 9px;

      /* The branch glyph leads the branch name, tinted tertiary like the design's
         "vc" mark - it stays that colour on row hover (as the dot did). */
      .branch-icon {
        display: inline-flex;
        flex: none;
        color: var(--tertiary);
      }
    }

    .project-row-path {
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 9px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .project-row-main:hover .project-row-metadata,
  .project-row-main:focus-visible .project-row-metadata,
  .project-row-main:hover .project-row-metadata .branch,
  .project-row-main:focus-visible .project-row-metadata .branch {
    --last-accessed-color: currentColor;

    color: inherit;
  }

  .project-row-check {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .project-row-main:hover .project-row-check,
  .project-row-main:focus-visible .project-row-check {
    color: inherit;
  }
</style>
