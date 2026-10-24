<script lang="ts">
  import Logo from "@/lib/ui/Logo.svelte";
  import { openRepositoryOnModifiedClick } from "@/lib/vcs/repository-links";
  import TempBadge from "@/lib/workspace/TempBadge.svelte";

  // The top bar's "Project" pill naming this window's project; it opens the
  // window menu (save a temp workspace, the open windows), and when the
  // repository has a remote, a Ctrl-click opens it instead.
  const { label, isTemp, remoteRepository, menuId, menuAnchor }: {
    label: string;
    isTemp: boolean;
    /** The repository a Ctrl-click opens on its remote; null when it has none. */
    remoteRepository: string | null;
    /** The switcher popover this button toggles. */
    menuId: string;
    /** The anchor name the popover positions itself against. */
    menuAnchor: string;
  } = $props();

  const MENU_TOOLTIP = "Open windows";

  const tooltip = $derived.by(() => {
    if (remoteRepository) {
      return `${MENU_TOOLTIP} · Ctrl-click opens repository`;
    }

    return MENU_TOOLTIP;
  });

  function openRemoteOnModifiedClick(element: HTMLElement) {
    if (!remoteRepository) {
      return;
    }

    return openRepositoryOnModifiedClick({ project: remoteRepository })(element);
  }
</script>

<button
  style:anchor-name={menuAnchor}
  class="trigger menu-trigger"
  {@attach openRemoteOnModifiedClick}
  aria-haspopup="menu"
  aria-label={`${tooltip} - ${label}`}
  data-tooltip={tooltip}
  popovertarget={menuId}
>
  <Logo size={18} />
  <span class="stack">
    <span class="eyebrow">Project</span>
    <span class="name">
      {label}
      {#if isTemp}
        <TempBadge />
      {/if}
    </span>
  </span>
  <span class="caret" aria-hidden="true">▾</span>
</button>

<style>
  .trigger {
    display: inline-flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 11px 10px;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface);
    white-space: nowrap;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    .stack {
      display: inline-flex;
      flex-direction: column;
      align-items: flex-start;
      line-height: 1.1;
    }

    .eyebrow {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .name {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 14px;
    }

    .caret {
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 10px;
      opacity: 70%;
    }
  }
</style>
