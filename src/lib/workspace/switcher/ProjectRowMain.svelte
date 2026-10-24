<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { observeProjectBranch, projectBranch } from "@/lib/stores/workspace/projectBranches.svelte";
  import { truncationTooltip } from "@/lib/ui/truncation-tooltip";
  import { displayName, isTemporaryWorkspace } from "@/lib/util/paths";
  import LanguageBar from "@/lib/workspace/LanguageBar.svelte";
  import LastAccessed from "@/lib/workspace/LastAccessed.svelte";
  import ProjectKindIcon from "@/lib/workspace/ProjectKindIcon.svelte";
  import TempBadge from "@/lib/workspace/TempBadge.svelte";

  // A project row's main button in the left rail: kind icon, name (up to two
  // lines), branch chip, path and last-accessed age, with a check on this
  // window's project and a window mark on one another window has open. Its
  // tooltip carries the full name and the path, whatever the row could show.
  const { project, labels, isCurrent, isOpenElsewhere, chatCount, now, onpick }: {
    project: string;
    labels: Record<string, string>;
    /** This window's own project - wears the primary rail and the check. */
    isCurrent: boolean;
    /** Another window has this project open - picking it focuses that window. */
    isOpenElsewhere: boolean;
    /** How many chats the project holds. */
    chatCount: number;
    /** The instant the "last accessed" age is measured against. */
    now: number;
    /** The row was clicked - the click carries the modifier keys. */
    onpick: (e: MouseEvent) => void;
  } = $props();

  const branch = $derived(projectBranch(project));
  const name = $derived(
    displayName({
      path: project,
      labels
    })
  );
  const OPEN_ELSEWHERE_NOTE = "Open in another window";
  // The full name on its own line, then the path - shown only while the row
  // cuts either short - and, for a project another window holds, where
  // picking it will take you, which the row never spells out.
  const rowTooltip = $derived.by(() => {
    const nameAndPath = `${name}\n${project}`;
    if (isOpenElsewhere) {
      return `${nameAndPath}\n${OPEN_ELSEWHERE_NOTE}`;
    }

    return nameAndPath;
  });

  function rowTruncationTooltip(element: HTMLElement) {
    return truncationTooltip({
      tooltip: () => rowTooltip,
      restingTooltip: () => isOpenElsewhere ? OPEN_ELSEWHERE_NOTE : "",
      measureSelector: ".project-row-name, .project-row-path"
    })(element);
  }
  // The trailing mark: a check on this window's project, a window glyph on one
  // another window has open, nothing otherwise.
  const mark = $derived.by(() => {
    if (isCurrent) {
      return {
        icon: "check",
        size: 15
      } as const;
    }

    if (isOpenElsewhere) {
      return {
        icon: "external",
        size: 13
      } as const;
    }

    return null;
  });
</script>

<button
  class="project-row-main"
  class:current={isCurrent}
  {@attach observeProjectBranch({ path: project })}
  {@attach rowTruncationTooltip}
  aria-current={isCurrent}
  data-sidebar-item
  onclick={onpick}
  type="button"
>
  <ProjectKindIcon path={project} />
  <span class="project-row-body">
    <span class="project-row-name-line">
      <span class="project-row-name">{name}</span>
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
      <span class="project-row-path">{project}</span>
      <LastAccessed {now} path={project} />
    </span>
  </span>
  <LanguageBar path={project} />
  <span class="project-row-count">{chatCount}</span>
  {#if mark}
    <span class="project-row-mark" class:current-mark={isCurrent} aria-hidden="true">
      <Icon name={mark.icon} size={mark.size} />
    </span>
  {/if}
</button>

<style>
  .project-row-main {
    position: relative;
    display: flex;
    flex: 1;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px 10px;
    padding-inline: 8px 6px;
    border: none;
    border-radius: var(--radius-small);
    background: none;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

    /* The tooltip puts the full name, the path and any note on lines of
       their own. */
    &::after {
      white-space: pre-line;
    }

    /* The current project keeps a primary rail on its leading edge. */
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
    gap: 2px;
    min-inline-size: 0;
    line-height: 1.25;
  }

  /* A long name (a temp workspace's sentence-long label) wraps onto a second
     line before it clips, so the rail's narrow width still shows most of it;
     the tooltip has the rest. */
  .project-row-name-line {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    min-inline-size: 0;

    .project-row-name {
      overflow: hidden;
      max-block-size: 2lh;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      overflow-wrap: anywhere;
      line-clamp: 2;
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
      flex: 0 1 auto;
      gap: 4px;
      align-items: center;
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 9px;
      white-space: nowrap;

      /* The branch glyph leads the branch name, tinted tertiary like the design's
         "vc" mark - it stays that colour on row hover. */
      .branch-icon {
        display: inline-flex;
        flex: none;
        color: var(--tertiary);
      }
    }

    .project-row-path {
      flex: 1;
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

  .project-row-count {
    flex: none;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 11px;
  }

  .project-row-mark {
    display: inline-flex;
    flex: none;
    color: var(--on-surface-variant);

    &.current-mark {
      color: var(--primary);
    }
  }

  .project-row-main:hover :is(.project-row-mark, .project-row-count),
  .project-row-main:focus-visible :is(.project-row-mark, .project-row-count) {
    color: inherit;
  }
</style>
