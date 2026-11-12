<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { observeProjectBranch, projectBranch } from "@/lib/stores/workspace/project-rows/projectBranches.svelte";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { displayName } from "@/lib/util/paths";
  import LanguageBar from "@/lib/workspace/display/LanguageBar.svelte";
  import LastAccessed from "@/lib/workspace/display/LastAccessed.svelte";
  import ProjectKindIcon from "@/lib/workspace/display/ProjectKindIcon.svelte";
  import ProjectActivityCounts from "@/lib/workspace/switcher/projects/ProjectActivityCounts.svelte";
  import ProjectRowName from "@/lib/workspace/switcher/projects/ProjectRowName.svelte";

  // A project row's main button in the left rail: kind icon, its root and
  // name on one line, branch and last-accessed age under it, then how many of
  // its chats wait on you and how many are working, and a window mark on one
  // another window has open. Its tooltip carries the path, and the full name
  // while the row cuts it short.
  const {
    project,
    labels,
    isCurrent,
    isOpenElsewhere,
    isExpanded,
    needsYouCount,
    workingCount,
    now,
    onpick,
    oncontextmenu
  }: {
    project: string;
    labels: Record<string, string>;
    /** This window's own project - wears the primary rail. */
    isCurrent: boolean;
    /** Another window has this project open - picking it focuses that window. */
    isOpenElsewhere: boolean;
    /** Where a click opens and closes the project's chats (the rail), whether
     *  they are showing; left out where a click picks the project. */
    isExpanded?: boolean;
    /** How many of its chats wait on the user. */
    needsYouCount: number;
    /** How many of its chats are working. */
    workingCount: number;
    /** The instant the "last accessed" age is measured against. */
    now: number;
    /** The row was clicked - the click carries the modifier keys. */
    onpick: (e: MouseEvent) => void;
    /** The row was right-clicked. */
    oncontextmenu: (e: MouseEvent) => void;
  } = $props();

  const branch = $derived(projectBranch(project));
  const name = $derived(
    displayName({
      path: project,
      labels
    })
  );
  const OPEN_ELSEWHERE_NOTE = "Open in another window";
  // What the row never spells out: the full path, and for a project another
  // window holds, where picking it will take you. The full name leads only
  // while the row clamps it.
  const restingTooltip = $derived.by(() => {
    if (isOpenElsewhere) {
      return `${project}\n${OPEN_ELSEWHERE_NOTE}`;
    }

    return project;
  });

  function rowTruncationTooltip(element: HTMLElement) {
    return truncationTooltip({
      tooltip: () => `${name}\n${restingTooltip}`,
      restingTooltip: () => restingTooltip,
      measureSelector: ".project-row-name"
    })(element);
  }
</script>

<button
  class="project-row-main"
  class:current={isCurrent}
  {@attach observeProjectBranch({ path: project })}
  {@attach rowTruncationTooltip}
  aria-current={isCurrent}
  aria-expanded={isExpanded}
  data-sidebar-item
  onclick={onpick}
  {oncontextmenu}
  type="button"
>
  <ProjectKindIcon path={project} />
  <span class="project-row-body">
    <ProjectRowName {name} {project} />
    <span class="project-row-metadata">
      {#if branch}
        <span class="branch">
          <span class="branch-icon" aria-hidden="true"><Icon name="branch" size={11} /></span>
          {branch}
        </span>
      {/if}
      <LastAccessed {now} path={project} />
    </span>
  </span>
  <LanguageBar path={project} />
  <ProjectActivityCounts {needsYouCount} {workingCount} />
  {#if isOpenElsewhere}
    <span class="project-row-mark" aria-hidden="true"><Icon name="external" size={13} /></span>
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

    @media (prefers-reduced-motion: no-preference) {
      :global(:root[data-palette="cyberpunk"]) &:hover {
        animation: var(--cyber-jitter-animation);
      }
    }

    /* The tooltip puts the full name, the path and any note on lines of
       their own. */
    &::after {
      white-space: pre-line;
    }

    /* The current project keeps a primary rail on its leading edge. */
    &.current {
      box-shadow: inset 3px 0 0 0 var(--primary);

      /* Cyberpunk: the red accent rail runs down the whole header row
         instead (SidebarProjectGroup), so the button wears none of its own. */
      :global(:root[data-palette="cyberpunk"]) & {
        box-shadow: none;
      }
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

    /* Cyberpunk: a flat row - tighter, and pointing at it only raises the
       fill a step rather than lighting the cyan plate. */
    :global(:root[data-palette="cyberpunk"]) & {
      --row-gap: 0.4375rem;
      --row-padding-start: 0.375rem;
      --project-kind-icon-size: 0.75rem;

      /* The language bar lines up under the name, past the kind icon. */
      --language-bar-inset-inline:
        calc(var(--row-padding-start) + var(--project-kind-icon-size) + var(--row-gap))
        0.25rem;

      gap: var(--row-gap);
      padding-block: 0.4375rem 0.5625rem;
      padding-inline: var(--row-padding-start) 0.25rem;

      &:is(:hover, :focus-visible) {
        --on-surface-variant: inherit;

        background: var(--surface-3);
        color: var(--on-surface);
      }
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

  .project-row-metadata {
    --last-accessed-font-size: 0.5625rem;

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
      font-size: 0.5625rem;
      white-space: nowrap;

      /* The branch glyph leads the branch name, tinted tertiary like the design's
         "vc" mark - it stays that colour on row hover. */
      .branch-icon {
        display: inline-flex;
        flex: none;
        color: var(--tertiary);
      }

      /* Cyberpunk: the branch is quiet mono writing, its glyph in the same
         muted ink. */
      :global(:root[data-palette="cyberpunk"]) & {
        font-weight: 400;
        letter-spacing: 0.02em;

        .branch-icon {
          color: inherit;
        }
      }
    }
  }

  .project-row-main:hover .project-row-metadata,
  .project-row-main:focus-visible .project-row-metadata,
  .project-row-main:hover .project-row-metadata .branch,
  .project-row-main:focus-visible .project-row-metadata .branch {
    --last-accessed-color: currentColor;

    color: inherit;
  }

  .project-row-mark {
    display: inline-flex;
    flex: none;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
  }

  .project-row-main:hover .project-row-mark,
  .project-row-main:focus-visible .project-row-mark {
    color: inherit;
  }
</style>
